use std::collections::VecDeque;
use std::time::Duration;

use candid::Nat;
use ic_canister_log::log;
use icrc_ledger_types::icrc1::account::Account;
use scopeguard::ScopeGuard;

use crate::deposit_logs::{
    report_transaction_error, ReceivedDepositEvent, ReceivedDepositEventError,
};
use crate::deposit_logs::{LogParser, ReceivedDepositLogParser};
use crate::deposit_logs::{LogScraping, ReceivedDepositLogScraping};
use crate::endpoints::RequestScrapingError;
use crate::eth_types::Address;
use crate::evm_config::EvmNetwork;
use crate::guard::TimerGuard;
use crate::logs::{DEBUG, INFO};
use crate::numeric::{BlockNumber, BlockRangeInclusive, LedgerMintIndex};
use crate::rpc_client::{is_response_too_large, MultiCallError, RpcClient};
use crate::rpc_declarations::LogEntry;
use crate::rpc_declarations::Topic;
use crate::rpc_declarations::{BlockSpec, GetLogsParam};
use crate::state::audit::{process_event, EventType};
use crate::state::{mutate_state, read_state, State, TaskType};
use crate::FEES_SUBACCOUNT;
use num_traits::ToPrimitive;

async fn mint() {
    use icrc_ledger_client_cdk::{CdkRuntime, ICRC1Client};
    use icrc_ledger_types::icrc1::transfer::TransferArg;

    let _guard = match TimerGuard::new(TaskType::Mint) {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let (native_ledger_canister_id, events) =
        read_state(|s| (s.native_ledger_id, s.events_to_mint()));

    let deposit_native_fee = read_state(|s| s.deposit_native_fee).map(|fee| Nat::from(fee));

    let mut error_count = 0;

    for event in events {
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
        let (token_symbol, ledger_canister_id, is_native_deposit) = match &event {
            ReceivedDepositEvent::Native(_) => {
                ("Native".to_string(), native_ledger_canister_id, true)
            }
            ReceivedDepositEvent::Erc20(event) => {
                if let Some(result) = read_state(|s| {
                    s.erc20_tokens
                        .get_entry_alt(&event.erc20_contract_address)
                        .map(|(principal, symbol)| (symbol.to_string(), *principal, false))
                }) {
                    result
                } else {
                    panic!(
                        "Failed to mint ERC20: {event:?} Unsupported ERC20 contract address. (This should have already been filtered out by process_event)"
                    )
                }
            }
        };
        let client = ICRC1Client {
            runtime: CdkRuntime,
            ledger_canister_id,
        };

        // If deposit type is native_deposit and the fees are not set to zero,
        // The minted amount for user should be as follow
        // event.value() - deposit_native_fee
        let amount = calculate_deposit_amount_deducting_fee(
            event.value(),
            &deposit_native_fee,
            is_native_deposit,
        );

        // Mint tokens for the user
        let block_index = match client
            .transfer(TransferArg {
                from_subaccount: None,
                to: Account {
                    owner: event.principal(),
                    subaccount: event.subaccount(),
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

        // In case the deposit is native and there is deposit fee,
        // Mint the fees for the fee collector account
        // If minting tokens is successful and minting deposit fees fails,
        // The minting process will be considered as successful and will not go for another iteration,
        // To prevent double minting
        if is_native_deposit {
            match deposit_native_fee {
                Some(ref deposit_fee) => {
                    // Minting deposit fees in minters fee collector subaccount
                    match client
                        .transfer(TransferArg {
                            from_subaccount: None,
                            to: Account {
                                owner: ic_cdk::id(),
                                subaccount: Some(FEES_SUBACCOUNT),
                            },
                            fee: None,
                            created_at_time: None,
                            memo: None,
                            amount: deposit_fee.clone(),
                        })
                        .await
                    {
                        Ok(Ok(block_index)) => {
                            log!(
                                INFO,
                                "Minted deposit fee {} in fee collecting account in block {}",
                                deposit_fee,
                                block_index
                            );
                        }
                        Ok(Err(err)) => {
                            log!(
                                INFO,
                                "Failed to minter fees {}: {err:?}",
                                deposit_fee.clone()
                            );
                        }
                        Err(err) => {
                            log!(INFO,"Failed to send a message to the ledger for minting fees({ledger_canister_id}): {err:?}");
                        }
                    };
                }
                None => {}
            }
        };

        // Record event
        mutate_state(|s| {
            process_event(
                s,
                match &event {
                    ReceivedDepositEvent::Native(event) => EventType::MintedNative {
                        event_source: event.source(),
                        mint_block_index: LedgerMintIndex::new(block_index),
                    },

                    ReceivedDepositEvent::Erc20(event) => EventType::MintedErc20 {
                        event_source: event.source(),
                        mint_block_index: LedgerMintIndex::new(block_index),
                        erc20_contract_address: event.erc20_contract_address,
                        erc20_token_symbol: token_symbol.clone(),
                    },
                },
            )
        });
        log!(
            INFO,
            "Minted {} {token_symbol} to {} in block {block_index} ",
            amount,
            event.principal(),
        );
        // minting succeeded, defuse guard
        ScopeGuard::into_inner(prevent_double_minting_guard);
    }

    if error_count > 0 {
        log!(
            INFO,
            "Failed to mint {error_count} events, rescheduling the minting"
        );
        ic_cdk_timers::set_timer(crate::MINT_RETRY_DELAY, || ic_cdk::spawn(mint()));
    }
}

pub async fn scrape_logs() {
    let _guard = match TimerGuard::new(TaskType::ScrapLogs) {
        Ok(guard) => guard,
        Err(_) => return,
    };

    mutate_state(|s| s.last_observed_block_time = Some(ic_cdk::api::time()));

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

    let max_block_spread = read_state(|s| s.max_block_spread_for_logs_scraping());
    scrape_until_block(last_block_number, max_block_spread).await;
}

// Updates last_observed_block_number in the state.
pub async fn update_last_observed_block_number() -> Option<BlockNumber> {
    let block_height = read_state(State::block_height);
    let network = read_state(|state| state.evm_network);
    match read_state(RpcClient::from_state_one_provider_public_node)
        .get_block_by_number(BlockSpec::Tag(block_height))
        .await
    {
        Ok(latest_block) => {
            let mut block_number = Some(latest_block.number);
            match network {
                EvmNetwork::BSC => {
                    // Waiting for 20 blocks means the transaction is practically safe on BSC
                    // So we go 15 blocks before the latest block
                    block_number = latest_block.number.checked_sub(
                        BlockNumber::try_from(20_u32)
                            .expect("Removing 20 blocks from latest block should never fail"),
                    )
                }
                EvmNetwork::ArbitrumOne => {
                    // it's generally recommended to wait for at least 6-12 blocks after a block is initially produced before
                    // considering it to be finalized and safe from reorgs. This waiting period provides a buffer to account for potential fork scenarios
                    //  or other unexpected events.
                    block_number = latest_block.number.checked_sub(
                        BlockNumber::try_from(12_u32)
                            .expect("Removing 12 blocks from latest block should never fail"),
                    )
                }
                EvmNetwork::Base => {
                    // like Arbitrum, it's recommended to wait for a few blocks after a transaction is included in a block
                    // to ensure finality and minimize the risk of reorgs. A waiting period of 6-12 blocks is
                    // typically considered sufficient for most applications.

                    block_number = latest_block.number.checked_sub(
                        BlockNumber::try_from(12_u32)
                            .expect("Removing 12 blocks from latest block should never fail"),
                    )
                }
                EvmNetwork::Optimism => {
                    // Similar to the other layer-2 networks, it's recommended to wait for a few blocks after a transaction is included in a block to
                    // ensure finality and minimize the risk of reorgs. A waiting period of 6-12 blocks is typically considered sufficient.

                    block_number = latest_block.number.checked_sub(
                        BlockNumber::try_from(12_u32)
                            .expect("Removing 12 blocks from latest block should never fail"),
                    )
                }
                EvmNetwork::Avalanche => {
                    // If your application deals with extremely high-value transactions or sensitive data,
                    // you might want to consider waiting for a slightly longer period, such as 12 blocks.
                    // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.

                    block_number = latest_block.number.checked_sub(
                        BlockNumber::try_from(12_u32)
                            .expect("Removing 12 blocks from latest block should never fail"),
                    )
                }

                EvmNetwork::Fantom => {
                    // If your application deals with extremely high-value transactions or sensitive data,
                    // you might want to consider waiting for a slightly longer period, such as 12 blocks.
                    // This can provide an additional layer of security, especially if you're dealing with particularly critical transactions.

                    block_number = latest_block.number.checked_sub(
                        BlockNumber::try_from(12_u32)
                            .expect("Removing 12 blocks from latest block should never fail"),
                    )
                }

                // For the rest of the networks we rely on BlockTag::Finalized, So we can make sure that there wont be any reorgs
                _ => {}
            }
            mutate_state(|s| s.last_observed_block_number = block_number);
            block_number
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
    let scrape = match read_state(|state| ReceivedDepositLogScraping::next_scrape(state)) {
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
            scrape.contract_address,
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
    contract_address: Address,
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
            address: vec![contract_address],
            topics: topics.clone(),
        };

        let result = rpc_client
            .get_logs(request)
            .await
            .map(ReceivedDepositLogParser::parse_all_logs);

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
    transaction_events: Vec<ReceivedDepositEvent>,
    errors: Vec<ReceivedDepositEventError>,
) {
    for event in transaction_events {
        log!(
            INFO,
            "Received event {event:?}; will mint {} to {}",
            event.value(),
            event.principal()
        );

        mutate_state(|s| process_event(s, event.into_deposit()));
    }
    if read_state(State::has_events_to_mint) {
        ic_cdk_timers::set_timer(Duration::from_secs(0), || ic_cdk::spawn(mint()));
    }
    for error in errors {
        if let ReceivedDepositEventError::InvalidEventSource { source, error } = &error {
            mutate_state(|s| {
                process_event(
                    s,
                    EventType::InvalidDeposit {
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
    pub(crate) const ONE_MIN_NS: u64 = 60_000_000_000_u64; // 60 seconds

    if now_ns < last_observed_block_time.saturating_add(ONE_MIN_NS) {
        return Err(RequestScrapingError::CalledTooManyTimes);
    }

    Ok(())
}

pub fn calculate_deposit_amount_deducting_fee(
    amount: Nat,
    deposit_native_fee: &Option<Nat>,
    is_native_deposit: bool,
) -> candid::Nat {
    // Calculate Amount - Deposit fee
    if is_native_deposit {
        // If there is a deposit fee
        match deposit_native_fee {
            Some(fee) => amount - fee.clone(),
            None => amount,
        }
    } else {
        amount
    }
}
