use std::collections::VecDeque;
use std::time::Duration;

use candid::Nat;
use ic_canister_log::log;
use icrc_ledger_types::icrc1::account::Account;
use scopeguard::ScopeGuard;

use crate::candid_types::RequestScrapingError;
use crate::contract_logs::parser::{LogParser, ReceivedEventsLogParser};
use crate::contract_logs::scraping::{LogScraping, ReceivedEventsLogScraping};
use crate::contract_logs::{
    report_transaction_error, ReceivedContractEvent, ReceivedContractEventError,
};
use crate::dex_client::types::ReceivedSwapOrderEvent;
use crate::dex_client::DexClient;
use crate::eth_types::Address;
use crate::evm_config::EvmNetwork;
use crate::guard::TimerGuard;
use crate::icrc_client::runtime::IcrcBoundedRuntime;
use crate::logs::{DEBUG, INFO};
use crate::numeric::{BlockNumber, BlockRangeInclusive, IcrcValue, LedgerMintIndex};
use crate::rpc_client::providers::Provider;
use crate::rpc_client::{is_response_too_large, MultiCallError, RpcClient};
use crate::rpc_declarations::LogEntry;
use crate::rpc_declarations::Topic;
use crate::rpc_declarations::{BlockSpec, GetLogsParam};
use crate::state::audit::{process_event, EventType};
use crate::state::{mutate_state, read_state, State, TaskType};
use crate::tx_id::SwapTxId;
use icrc_ledger_client::ICRC1Client;
use icrc_ledger_types::icrc1::transfer::TransferArg;
use num_traits::ToPrimitive;

pub(crate) const TEN_SEC: u64 = 10_000_000_000_u64; // 10 seconds

async fn mint_and_release() {
    let _guard = match TimerGuard::new(TaskType::Mint) {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let (native_ledger_canister_id, events_to_mint, events_to_release) = read_state(|s| {
        (
            s.native_ledger_id,
            s.events_to_mint(),
            s.events_to_release(),
        )
    });

    let mut error_count = 0;

    for event in events_to_mint {
        // Ensure that even if we were to panic in the callback, after having contacted the ledger to mint the tokens,
        // this event will not be processed again.
        let prevent_double_minting_guard = scopeguard::guard(event.clone(), |event| {
            mutate_state(|s| {
                process_event(
                    s,
                    EventType::QuarantinedDeposit {
                        event_source: event.source(),
                    },
                )
            });
        });
        let (token_symbol, ledger_canister_id, amount, recepient, subaccount) = match &event {
            ReceivedContractEvent::NativeDeposit(event) => (
                "Native".to_string(),
                native_ledger_canister_id,
                Nat::from(event.value),
                event.principal,
                event.subaccount.clone(),
            ),
            ReceivedContractEvent::Erc20Deposit(event) => {
                if let Some(result) = read_state(|s| {
                    s.erc20_tokens
                        .get_entry_alt(&event.erc20_contract_address)
                        .map(|(principal, symbol)| {
                            (
                                symbol.to_string(),
                                *principal,
                                Nat::from(event.value),
                                event.principal,
                                event.subaccount.clone(),
                            )
                        })
                }) {
                    result
                } else {
                    panic!("Failed to mint ERC20: {event:?} Unsupported ERC20 contract address. (This should have already been filtered out by process_event)");
                }
            }
            _ => panic!("BUG: Only deposit events should be in the minting list"),
        };

        let client = ICRC1Client {
            runtime: IcrcBoundedRuntime,
            ledger_canister_id,
        };

        // Mint tokens for the user
        let block_index = match client
            .transfer(TransferArg {
                from_subaccount: None,
                to: Account {
                    owner: recepient,
                    subaccount: subaccount.map(|subaccount| subaccount.to_bytes()),
                },
                fee: None,
                created_at_time: None,
                memo: Some((&event).into()),
                amount: amount.clone(),
            })
            .await
        {
            Ok(Ok(block_index)) => block_index.0.to_u64().expect("nat does not fit into u64"),
            Ok(Err(err)) => {
                log!(INFO, "Failed to mint {token_symbol}: {event:?} {err}");
                error_count += 1;
                // minting failed, defuse guard
                ScopeGuard::into_inner(prevent_double_minting_guard);
                continue;
            }
            Err(err) => {
                log!(
                    INFO,
                    "Failed to send a message to the ledger ({ledger_canister_id}): {err:?}"
                );
                error_count += 1;
                // minting failed, defuse guard
                ScopeGuard::into_inner(prevent_double_minting_guard);
                continue;
            }
        };

        // Record event
        mutate_state(|s| {
            process_event(
                s,
                match &event {
                    ReceivedContractEvent::NativeDeposit(event) => EventType::MintedNative {
                        event_source: event.source(),
                        mint_block_index: LedgerMintIndex::new(block_index),
                    },

                    ReceivedContractEvent::Erc20Deposit(event) => EventType::MintedErc20 {
                        event_source: event.source(),
                        mint_block_index: LedgerMintIndex::new(block_index),
                        erc20_contract_address: event.erc20_contract_address,
                        erc20_token_symbol: token_symbol.clone(),
                    },
                    _ => panic!("BUG: Only deposit events should be in the minting list"),
                },
            )
        });
        log!(
            INFO,
            "Minted {} {token_symbol} to {} in block {block_index} ",
            amount,
            recepient.to_text(),
        );
        // minting succeeded, defuse guard
        ScopeGuard::into_inner(prevent_double_minting_guard);
    }

    for event in events_to_release {
        let received_burn_event = match &event {
            ReceivedContractEvent::WrappedIcrcBurn(event) => event,

            _ => panic!("BUG: Only burn events should be in the minting list"),
        };

        let client = ICRC1Client {
            runtime: IcrcBoundedRuntime,
            ledger_canister_id: received_burn_event.icrc_token_principal,
        };

        let fee = match client.fee().await {
            Ok(fee) => fee,
            Err(err) => {
                log!(
                    INFO,
                    "Failed to send a message to the ledger ({}): {err:?}",
                    received_burn_event.icrc_token_principal
                );
                error_count += 1;
                mutate_state(|s| {
                    process_event(
                        s,
                        EventType::QuarantinedRelease {
                            event_source: event.source(),
                            release_event: received_burn_event.clone(),
                        },
                    )
                });
                continue;
            }
        };

        // sub transfer fee from amount
        let transfer_fee = IcrcValue::try_from(fee.clone()).unwrap_or(IcrcValue::MAX);

        let amount = received_burn_event
            .value
            .checked_sub(transfer_fee)
            .unwrap_or(IcrcValue::ZERO);

        let mut block_index = 0_u64;

        // if amount is greater than transfer fee
        if amount != IcrcValue::ZERO {
            // Release tokens for the user
            block_index = match client
                .transfer(TransferArg {
                    from_subaccount: None,
                    to: Account {
                        owner: received_burn_event.principal,
                        subaccount: received_burn_event
                            .subaccount
                            .clone()
                            .map(|subaccount| subaccount.to_bytes()),
                    },
                    fee: Some(fee),
                    created_at_time: None,
                    memo: Some((&event).into()),
                    amount: amount.into(),
                })
                .await
            {
                Ok(Ok(block_index)) => block_index.0.to_u64().expect("nat does not fit into u64"),
                Ok(Err(err)) => {
                    log!(
                        INFO,
                        "Failed to release {}: {event:?} {err}",
                        received_burn_event.icrc_token_principal.to_text()
                    );
                    error_count += 1;
                    // releasing failed
                    mutate_state(|s| {
                        process_event(
                            s,
                            EventType::QuarantinedRelease {
                                event_source: event.source(),
                                release_event: received_burn_event.clone(),
                            },
                        )
                    });
                    continue;
                }
                Err(err) => {
                    log!(
                        INFO,
                        "Failed to send a message to the ledger ({}): {err:?}",
                        received_burn_event.icrc_token_principal
                    );
                    error_count += 1;
                    // releasing failed, defuse guard
                    mutate_state(|s| {
                        process_event(
                            s,
                            EventType::QuarantinedRelease {
                                event_source: event.source(),
                                release_event: received_burn_event.clone(),
                            },
                        )
                    });
                    continue;
                }
            };
        }

        // record event
        mutate_state(|s| {
            process_event(
                s,
                EventType::ReleasedIcrcToken {
                    event_source: event.source(),
                    release_block_index: block_index.into(),
                    released_icrc_token: received_burn_event.icrc_token_principal,
                    wrapped_erc20_contract_address: received_burn_event
                        .wrapped_erc20_contract_address,
                    transfer_fee,
                },
            )
        })
    }

    if error_count > 0 {
        log!(
            INFO,
            "Failed to mint or release {error_count} events, rescheduling the minting and releasing"
        );
        ic_cdk_timers::set_timer(crate::MINT_RETRY_DELAY, || {
            ic_cdk::futures::spawn_017_compat(mint_and_release())
        });
    }
}

pub async fn mint_to_appic_dex_and_swap() {
    let _guard = match TimerGuard::new(TaskType::MintToDexAndSwap) {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let (swap_events_to_mint, dex_canister_id, ledger_canister_id, chain_id) = read_state(|s| {
        (
            s.swap_events_to_mint_to_appic_dex(),
            s.dex_canister_id
                .expect("Bug: This function should not be called if swapping is not active"),
            s.twin_usdc_info
                .clone()
                .expect("Bug: This function should not be called if swapping is not active")
                .ledger_id,
            s.evm_network.chain_id().to_string(),
        )
    });

    let mut error_count = 0;

    for event in swap_events_to_mint {
        // Ensure that even if we were to panic in the callback, after having contacted the ledger to mint the tokens,
        // this event will not be processed again.
        let prevent_double_minting_guard = scopeguard::guard(event.clone(), |event| {
            mutate_state(|s| {
                process_event(
                    s,
                    EventType::QuarantinedDeposit {
                        event_source: event.source(),
                    },
                )
            });
        });
        let amount = match &event {
            ReceivedContractEvent::ReceivedSwapOrder(swap_order) => {
                Nat::from(swap_order.amount_out)
            }

            _ => panic!("BUG: Only deposit events should be in the minting list"),
        };

        let client = ICRC1Client {
            runtime: IcrcBoundedRuntime,
            ledger_canister_id,
        };

        // Mint tokens for the user
        let block_index = match client
            .transfer(TransferArg {
                from_subaccount: None,
                to: Account {
                    owner: dex_canister_id,
                    subaccount: None,
                },
                fee: None,
                created_at_time: None,
                memo: Some((&event).into()),
                amount: amount.clone(),
            })
            .await
        {
            Ok(Ok(block_index)) => block_index.0.to_u64().expect("nat does not fit into u64"),
            Ok(Err(err)) => {
                log!(INFO, "Failed to mint USDC: {event:?} {err}");
                error_count += 1;
                // minting failed, defuse guard
                ScopeGuard::into_inner(prevent_double_minting_guard);
                continue;
            }
            Err(err) => {
                log!(
                    INFO,
                    "Failed to send a message to the ledger ({ledger_canister_id}): {err:?}"
                );
                error_count += 1;
                // minting failed, defuse guard
                ScopeGuard::into_inner(prevent_double_minting_guard);
                continue;
            }
        };

        let time = ic_cdk::api::time();

        let tx_id = SwapTxId::new(&chain_id, Nat::from(block_index), time);

        // Record event
        mutate_state(|s| {
            process_event(
                s,
                match &event {
                    ReceivedContractEvent::ReceivedSwapOrder(swap_order) => {
                        EventType::MintedToAppicDex {
                            event_source: swap_order.source(),
                            mint_block_index: LedgerMintIndex::new(block_index),
                            minted_token: ledger_canister_id,
                            erc20_contract_address: swap_order.token_out,
                            tx_id,
                        }
                    }
                    _ => panic!("BUG: Only deposit events should be in the minting list"),
                },
            )
        });
        log!(
            INFO,
            "Minted {} USDC to appic_dex {} in block {block_index} ",
            amount,
            dex_canister_id.to_text()
        );
        // minting succeeded, defuse guard
        ScopeGuard::into_inner(prevent_double_minting_guard);
    }

    let swap_events_to_be_notified = read_state(|s| s.swap_events_to_be_notified());

    for event in swap_events_to_be_notified {
        // Ensure that even if we were to panic in the callback, after having contacted the ledger to mint the tokens,
        // this event will not be processed again.
        let prevent_double_minting_guard = scopeguard::guard(event.clone(), |event| {
            mutate_state(|s| {
                process_event(
                    s,
                    EventType::QuarantinedDeposit {
                        event_source: event.event.source(),
                    },
                )
            });
        });

        let swap_order = match event.event {
            ReceivedContractEvent::ReceivedSwapOrder(received_swap_event) => received_swap_event,
            _ => panic!("BUG: only swap events should be presented here"),
        };

        let client = DexClient::new(dex_canister_id);

        // Mint tokens for the user
        match client
            .minter_order(&ReceivedSwapOrderEvent {
                from_address: swap_order.from_address.to_string(),
                recipient: swap_order.recipient.to_string(),
                token_in: swap_order.token_in.to_string(),
                token_out: swap_order.token_out.to_string(),
                amount_in: swap_order.amount_in.into(),
                amount_out: swap_order.amount_out.into(),
                encoded_swap_data: swap_order.encoded_swap_data.to_string(),
                tx_id: event.tx_id.0.clone(),
            })
            .await
        {
            Ok(notify_result) => {
                log!(
                    INFO,
                    "Notified appic dex for swap order {:?} with result {:?}",
                    swap_order,
                    notify_result
                );
            }
            Err(err) => {
                log!(INFO, "Failed to send a message to the appic dex: {err:?}");
                error_count += 1;
                // minting failed, defuse guard
                ScopeGuard::into_inner(prevent_double_minting_guard);
                continue;
            }
        };

        // Record event
        mutate_state(|s| {
            process_event(
                s,
                EventType::NotifiedSwapEventOrderToAppicDex {
                    event_source: swap_order.source(),
                    tx_id: event.tx_id.clone(),
                },
            )
        });
        log!(
            INFO,
            "Notified Appic Dex about swap {} with source {}",
            event.tx_id.0,
            swap_order.source()
        );
        // minting succeeded, defuse guard
        ScopeGuard::into_inner(prevent_double_minting_guard);
    }

    if error_count > 0 {
        log!(
            INFO,
            "Failed to mint or release {error_count} events, rescheduling the minting and releasing"
        );
        ic_cdk_timers::set_timer(crate::MINT_RETRY_DELAY, || {
            ic_cdk::futures::spawn_017_compat(mint_to_appic_dex_and_swap())
        });
    }
}

pub async fn scrape_logs() {
    let _guard = match TimerGuard::new(TaskType::ScrapLogs) {
        Ok(guard) => guard,
        Err(_) => return,
    };

    mutate_state(|s| s.last_log_scraping_time = Some(ic_cdk::api::time()));

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 3;

    let last_block_number = loop {
        match update_last_observed_block_number().await {
            Some(block_number) => break block_number, // Exit loop on success
            None => {
                attempts += 1;
                log!(
                    DEBUG,
                    "[scrape_logs]: attempt {}/{} failed: no last observed block number",
                    attempts,
                    MAX_ATTEMPTS
                );

                if attempts >= MAX_ATTEMPTS {
                    log!(
                        DEBUG,
                        "[scrape_logs]: max retries reached. Skipping scrapping logs."
                    );
                    return; // Exit function after maximum retries
                }
            }
        }
    };

    ic_cdk::println!("Last_block_number:{}", last_block_number);

    let max_block_spread = read_state(|s| s.max_block_spread_for_logs_scraping());
    scrape_until_block(last_block_number, max_block_spread).await;
}

// Updates last_observed_block_number in the state.
pub async fn update_last_observed_block_number() -> Option<BlockNumber> {
    let block_height = read_state(State::block_height);
    let network = read_state(|state| state.evm_network);
    let now_ns = ic_cdk::api::time();

    // first we check if the last_observed_block_number is newly updated(it's not older than 10
    // seconds), if the last_observed_block_number is fresh we dont need to request for a new block
    // number, on the opposite the on-chain request has to be sent.
    if let (Some(last_observed_block_number), Some(last_observed_block_time)) =
        read_state(|s| (s.last_observed_block_number, s.last_observed_block_time))
    {
        if now_ns < last_observed_block_time.saturating_add(TEN_SEC) {
            return Some(last_observed_block_number);
        }
    };

    match read_state(|s| RpcClient::from_state_one_provider(s, Provider::DRPC))
        .get_block_by_number(BlockSpec::Tag(block_height))
        .await
    {
        Ok(latest_block) => {
            let block_number = latest_block.number;
            mutate_state(|s| s.last_observed_block_number = Some(block_number));
            mutate_state(|s| s.last_observed_block_time = Some(now_ns));

            Some(apply_safe_threshold_to_latest_block_numner(
                network,
                block_number,
            ))
        }
        Err(e) => {
            log!(
                INFO,
                "Failed to get the latest {block_height} block number: {e:?}"
            );
            None
        }
    }
}

async fn scrape_until_block(last_block_number: BlockNumber, max_block_spread: u16) {
    let scrape = match read_state(ReceivedEventsLogScraping::next_scrape) {
        Some(s) => s,
        None => {
            log!(
                DEBUG,
                "[scrape_contract_logs]: skipping scraping logs: not active",
            );
            return;
        }
    };
    let block_range = BlockRangeInclusive::new(
        scrape
            .last_scraped_block_number
            .checked_increment()
            .unwrap_or(BlockNumber::MAX),
        last_block_number,
    );
    log!(
        DEBUG,
        "[scrape_contract_logs]: Scraping logs in block range {block_range}",
    );
    let rpc_client = read_state(RpcClient::from_state_all_providers);
    for block_range in block_range.into_chunks(max_block_spread) {
        match scrape_block_range(
            &rpc_client,
            scrape.contract_addresses.clone(),
            scrape.topics.clone(),
            block_range.clone(),
        )
        .await
        {
            Ok(()) => {}
            Err(e) => {
                log!(
                    INFO,
                    "[scrape_contract_logs]: Failed to scrape logs in range {block_range}: {e:?}",
                );
                return;
            }
        }
    }
}

async fn scrape_block_range(
    rpc_client: &RpcClient,
    contract_addresses: Vec<Address>,
    topics: Vec<Topic>,
    block_range: BlockRangeInclusive,
) -> Result<(), MultiCallError<Vec<LogEntry>>> {
    let mut subranges = VecDeque::new();
    subranges.push_back(block_range);

    while !subranges.is_empty() {
        let range = subranges.pop_front().unwrap();
        let (from_block, to_block) = range.clone().into_inner();

        let request = GetLogsParam {
            from_block: BlockSpec::from(from_block),
            to_block: BlockSpec::from(to_block),
            address: contract_addresses.clone(),
            topics: topics.clone(),
        };

        let result = rpc_client
            .get_logs(request)
            .await
            .map(ReceivedEventsLogParser::parse_all_logs);

        match result {
            Ok((events, errors)) => {
                register_deposit_events(events, errors);
                mutate_state(|s| s.last_scraped_block_number = to_block);
            }
            Err(e) => {
                log!(INFO, "Failed to get logs in range {range}: {e:?}");
                if e.has_http_outcall_error_matching(is_response_too_large) {
                    if from_block == to_block {
                        mutate_state(|s| {
                            process_event(
                                s,
                                EventType::SkippedBlock {
                                    block_number: to_block,
                                },
                            );
                        });
                        mutate_state(|s| s.last_scraped_block_number = to_block);
                    } else {
                        let (left_half, right_half) = range.partition_into_halves();
                        if let Some(r) = right_half {
                            let upper_range = subranges
                                .pop_front()
                                .map(|current_next| r.clone().join_with(current_next))
                                .unwrap_or(r);
                            subranges.push_front(upper_range);
                        }
                        if let Some(lower_range) = left_half {
                            subranges.push_front(lower_range);
                        }
                        log!(
                            INFO,
                            "Too many logs received. Will retry with ranges {subranges:?}"
                        );
                    }
                } else {
                    log!(INFO, "Failed to get logs in range {range}: {e:?}",);
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

pub fn register_deposit_events(
    transaction_events: Vec<ReceivedContractEvent>,
    errors: Vec<ReceivedContractEventError>,
) {
    for event in transaction_events {
        match &event {
            ReceivedContractEvent::NativeDeposit(received_native_event) => {
                log!(
                    INFO,
                    "Received event {event:?}; will mint {} to {}",
                    received_native_event.value,
                    received_native_event.principal.to_text()
                );
            }
            ReceivedContractEvent::Erc20Deposit(received_erc20_event) => {
                log!(
                    INFO,
                    "Received event {event:?}; will mint {} to {}",
                    received_erc20_event.value,
                    received_erc20_event.principal.to_text()
                );
            }
            ReceivedContractEvent::WrappedIcrcBurn(received_burn_event) => {
                log!(
                    INFO,
                    "Received event {event:?}; will release {} to {}",
                    received_burn_event.value,
                    received_burn_event.principal.to_text()
                );
            }
            ReceivedContractEvent::WrappedIcrcDeployed(wrapped_icrc_deployed) => {
                log!(
                    INFO,
                    "Received event {event:?}, erc20 token {}, was deployed for icrc token {}",
                    wrapped_icrc_deployed.deployed_wrapped_erc20,
                    wrapped_icrc_deployed.base_token.to_text()
                );
            }
            ReceivedContractEvent::ReceivedSwapOrder(received_swap_event) => {
                log!(INFO,
            "Received swap evnet {received_swap_event:?}, will send the event to the appic dex")
            }
        }

        mutate_state(|s| process_event(s, event.into_event_type()));
    }
    if read_state(|s| s.has_events_to_mint() || s.has_events_to_release()) {
        ic_cdk_timers::set_timer(Duration::from_secs(0), || {
            ic_cdk::futures::spawn_017_compat(mint_and_release());
        });
    }

    if read_state(|s| s.is_swapping_active && s.has_events_to_mint_and_notify()) {
        ic_cdk_timers::set_timer(Duration::from_secs(0), || {
            ic_cdk::futures::spawn_017_compat(mint_to_appic_dex_and_swap());
        });
    }

    for error in errors {
        if let ReceivedContractEventError::InvalidEventSource { source, error } = &error {
            mutate_state(|s| {
                process_event(
                    s,
                    EventType::InvalidEvent {
                        event_source: *source,
                        reason: error.to_string(),
                    },
                )
            });
        }
        report_transaction_error(error);
    }
}

// Validate request_log scraping
// Validation factors:
// 1: The provided block number should be greater than last observed block number.
// 2: There should be at least a minute of gap between the last time this function was called and now.
// Meaning that this function can only be called onces in a minute due to cycle drain attacks.
pub fn validate_log_scraping_request(
    last_observed_block_time: u64,
    now_ns: u64,
) -> Result<(), RequestScrapingError> {
    if now_ns < last_observed_block_time.saturating_add(TEN_SEC) {
        return Err(RequestScrapingError::CalledTooManyTimes);
    }

    Ok(())
}

pub fn apply_safe_threshold_to_latest_block_numner(
    network: EvmNetwork,
    latest_block: BlockNumber,
) -> BlockNumber {
    match network {
        EvmNetwork::BSC => {
            // Waiting for 12 blocks means the transaction is practically safe on BSC
            // So we go 12 blocks before the latest block
            latest_block
                .checked_sub(BlockNumber::from(1_u32))
                .expect("Removing 5 blocks from latest block should never fail")
        }
        EvmNetwork::ArbitrumOne => {
            // it's generally recommended to wait for at least 6-12 blocks after a block is initially produced before
            // considering it to be finalized and safe from reorgs. This waiting period provides a buffer to account for potential fork scenarios
            //  or other unexpected events.
            latest_block
                .checked_sub(BlockNumber::from(6_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::Base => {
            // like Arbitrum, it's recommended to wait for a few blocks after a transaction is included in a block
            // to ensure finality and minimize the risk of reorgs. A waiting period of 6-12 blocks is
            // typically considered sufficient for most applications.

            latest_block
                .checked_sub(BlockNumber::from(1_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::Optimism => {
            // Similar to the other layer-2 networks, it's recommended to wait for a few blocks after a transaction is included in a block to
            // ensure finality and minimize the risk of reorgs. A waiting period of 6-12 blocks is typically considered sufficient.

            latest_block
                .checked_sub(BlockNumber::from(12_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::Avalanche => {
            // If your application deals with extremely high-value transactions or sensitive data,
            // you might want to consider waiting for a slightly longer period, such as 12 blocks.
            // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.

            latest_block
                .checked_sub(BlockNumber::from(12_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::Fantom => {
            // If your application deals with extremely high-value transactions or sensitive data,
            // you might want to consider waiting for a slightly longer period, such as 12 blocks.
            // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.

            latest_block
                .checked_sub(BlockNumber::from(12_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::Ethereum =>
        // If your application deals with extremely high-value transactions or sensitive data,
        // you might want to consider waiting for a slightly longer period, such as 12 blocks.
        // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.
        {
            latest_block
                .checked_sub(BlockNumber::from(12_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::Sepolia =>
        // If your application deals with extremely high-value transactions or sensitive data,
        // you might want to consider waiting for a slightly longer period, such as 12 blocks.
        // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.
        {
            latest_block
                .checked_sub(BlockNumber::from(12_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::BSCTestnet =>
        // If your application deals with extremely high-value transactions or sensitive data,
        // you might want to consider waiting for a slightly longer period, such as 12 blocks.
        // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.
        {
            latest_block
                .checked_sub(BlockNumber::from(12_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
        EvmNetwork::Polygon =>
        // If your application deals with extremely high-value transactions or sensitive data,
        // you might want to consider waiting for a slightly longer period, such as 12 blocks.
        // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.
        {
            latest_block
                .checked_sub(BlockNumber::from(12_u32))
                .expect("Removing 12 blocks from latest block should never fail")
        }
    }
}
