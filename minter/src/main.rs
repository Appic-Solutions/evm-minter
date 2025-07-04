use candid::{Nat, Principal};
use evm_minter::address::{validate_address_as_destination, AddressValidationError};
use evm_minter::candid_types::chain_data::ChainData;
use evm_minter::candid_types::events::{
    Event as CandidEvent, EventSource as CandidEventSource, GetEventsArg, GetEventsResult,
};
use evm_minter::candid_types::wrapped_icrc::{
    RetrieveWrapIcrcRequest, WrapIcrcArg, WrapIcrcError, WrappedIcrcToken,
};
use evm_minter::contract_logs::types::{
    ReceivedBurnEvent, ReceivedErc20Event, ReceivedNativeEvent, ReceivedWrappedIcrcDeployedEvent,
};
use evm_minter::contract_logs::EventSource;
use evm_minter::deposit::{
    apply_safe_threshold_to_latest_block_numner, scrape_logs, validate_log_scraping_request,
};

use evm_minter::candid_types::{
    self, AddErc20Token, DepositStatus, Icrc28TrustedOriginsResponse, IcrcBalance,
    RequestScrapingError,
};
use evm_minter::candid_types::{
    withdraw_erc20::RetrieveErc20Request, withdraw_erc20::WithdrawErc20Arg,
    withdraw_erc20::WithdrawErc20Error,
};
use evm_minter::candid_types::{
    withdraw_native::WithdrawalArg, withdraw_native::WithdrawalDetail,
    withdraw_native::WithdrawalError, withdraw_native::WithdrawalSearchParameter,
    Eip1559TransactionPrice, Eip1559TransactionPriceArg, Erc20Balance, GasFeeEstimate, MinterInfo,
    RetrieveNativeRequest, RetrieveWithdrawalStatus,
};

use evm_minter::erc20::ERC20Token;
use evm_minter::eth_types::fee_hisotry_parser::parse_fee_history;
use evm_minter::evm_config::EvmNetwork;
use evm_minter::guard::retrieve_withdraw_guard;
use evm_minter::ledger_client::{LedgerBurnError, LedgerClient};
use evm_minter::lifecycle::MinterArg;
use evm_minter::logs::INFO;
use evm_minter::lsm_client::lazy_add_native_ls_to_lsm_canister;
use evm_minter::memo::BurnMemo;
use evm_minter::numeric::{BlockNumber, Erc20Value, LedgerBurnIndex, Wei};
use evm_minter::rpc_client::providers::Provider;
use evm_minter::rpc_declarations::Hash;
use evm_minter::state::audit::{process_event, EventType};
use evm_minter::state::event::Event;
use evm_minter::state::transactions::{
    Erc20WithdrawalRequest, NativeWithdrawalRequest, Reimbursed, ReimbursementIndex,
    ReimbursementRequest,
};
use evm_minter::state::{
    lazy_call_ecdsa_public_key, mutate_state, read_state, transactions, State, STATE,
};
use evm_minter::storage::set_rpc_api_key;
use evm_minter::tx::gas_fees::{
    estimate_transaction_fee, lazy_refresh_gas_fee_estimate, DEFAULT_L1_BASE_GAS_FEE,
};
use evm_minter::withdraw::{
    process_reimbursement, process_retrieve_tokens_requests, ERC20_MINT_TRANSACTION_GAS_LIMIT,
    ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT, NATIVE_WITHDRAWAL_TRANSACTION_GAS_LIMIT,
};
use evm_minter::{
    state, storage, PROCESS_REIMBURSEMENT, PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL,
    RPC_HELPER_PRINCIPAL, SCRAPING_CONTRACT_LOGS_INTERVAL,
};
use ic_canister_log::log;
use ic_cdk::{init, post_upgrade, pre_upgrade, query, update};
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::str::FromStr;
use std::time::Duration;

// Set api_keys for rpc providers
const ANKR_API_KEY: Option<&'static str> = option_env!("Ankr_Api_Key");
const LLAMA_API_KEY: Option<&'static str> = option_env!("Llama_Api_Key");
const DRPC_API_KEY: Option<&'static str> = option_env!("DRPC_Api_Key");
const ALCHEMY_API_KEY: Option<&'static str> = option_env!("Alchemy_Api_Key");

fn validate_caller_not_anonymous() -> candid::Principal {
    let principal = ic_cdk::caller();
    if principal == candid::Principal::anonymous() {
        panic!("anonymous principal is not allowed");
    }
    principal
}

fn setup_timers() {
    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        // Initialize the minter's public key to make the address known.
        ic_cdk::spawn(async {
            let _ = lazy_call_ecdsa_public_key().await;
        })
    });

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        // Initialize the Gas fee estimate for eip1559 transaction price
        ic_cdk::spawn(async {
            let _ = lazy_refresh_gas_fee_estimate().await;
        })
    });

    // Start scraping logs immediately after the install, then repeat with the interval.
    ic_cdk_timers::set_timer(Duration::from_secs(0), || ic_cdk::spawn(scrape_logs()));
    ic_cdk_timers::set_timer_interval(SCRAPING_CONTRACT_LOGS_INTERVAL, || {
        ic_cdk::spawn(scrape_logs())
    });
    ic_cdk_timers::set_timer_interval(PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL, || {
        ic_cdk::spawn(process_retrieve_tokens_requests())
    });
    ic_cdk_timers::set_timer_interval(PROCESS_REIMBURSEMENT, || {
        ic_cdk::spawn(process_reimbursement())
    });
}

#[init]
async fn init(arg: MinterArg) {
    match arg {
        MinterArg::InitArg(init_arg) => {
            log!(INFO, "[init]: initialized minter with arg: {:?}", init_arg);
            STATE.with(|cell| {
                storage::record_event(EventType::Init(init_arg.clone()));
                *cell.borrow_mut() = Some(
                    State::try_from(init_arg.clone()).expect("BUG: failed to initialize minter"),
                )
            });
        }

        MinterArg::UpgradeArg(_) => {
            ic_cdk::trap("cannot init canister state with upgrade args");
        }
    }

    let ankr_api_key = ANKR_API_KEY.unwrap();
    let llama_api_key = LLAMA_API_KEY.unwrap();
    let drpc_api_key = DRPC_API_KEY.unwrap();
    let alchemy_api_key = ALCHEMY_API_KEY.unwrap();

    set_rpc_api_key(Provider::Ankr, ankr_api_key.to_string());
    set_rpc_api_key(Provider::LlamaNodes, llama_api_key.to_string());
    set_rpc_api_key(Provider::DRPC, drpc_api_key.to_string());
    set_rpc_api_key(Provider::Alchemy, alchemy_api_key.to_string());

    // Add native ledger suite to the lsm canister.
    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(async {
            let _ = lazy_add_native_ls_to_lsm_canister().await;
        })
    });

    setup_timers();
}

fn emit_preupgrade_events() {
    read_state(|s| {
        storage::record_event(EventType::SyncedToBlock {
            block_number: s.last_scraped_block_number,
        });
    });
}

#[pre_upgrade]
fn pre_upgrade() {
    emit_preupgrade_events();
}

#[post_upgrade]
fn post_upgrade(minter_arg: Option<MinterArg>) {
    use evm_minter::lifecycle;
    match minter_arg {
        Some(MinterArg::InitArg(_)) => {
            ic_cdk::trap("cannot upgrade canister state with init args");
        }
        Some(MinterArg::UpgradeArg(upgrade_args)) => lifecycle::post_upgrade(Some(upgrade_args)),
        None => lifecycle::post_upgrade(None),
    }

    let ankr_api_key = ANKR_API_KEY.unwrap();
    let llama_api_key = LLAMA_API_KEY.unwrap();
    let drpc_api_key = DRPC_API_KEY.unwrap();
    let alchemy_api_key = ALCHEMY_API_KEY.unwrap();

    set_rpc_api_key(Provider::Ankr, ankr_api_key.to_string());
    set_rpc_api_key(Provider::LlamaNodes, llama_api_key.to_string());
    set_rpc_api_key(Provider::DRPC, drpc_api_key.to_string());
    set_rpc_api_key(Provider::Alchemy, alchemy_api_key.to_string());

    setup_timers();
}

#[update]
async fn minter_address() -> String {
    state::minter_address().await.to_string()
}

#[query]
async fn smart_contract_address() -> Option<Vec<String>> {
    read_state(|s| {
        s.helper_contract_addresses.as_ref().map(|addresses| {
            addresses
                .into_iter()
                .map(|address| address.to_string())
                .collect()
        })
    })
}

/// Estimate price of EIP-1559 transaction based on the
/// `base_fee_per_gas` included in the last Latest block.
#[query]
async fn eip_1559_transaction_price(
    token: Option<Eip1559TransactionPriceArg>,
) -> Eip1559TransactionPrice {
    let gas_limit = match token {
        None => NATIVE_WITHDRAWAL_TRANSACTION_GAS_LIMIT,
        Some(Eip1559TransactionPriceArg { erc20_ledger_id }) => {
            match read_state(|s| s.find_erc20_token_by_ledger_id(&erc20_ledger_id)) {
                Some(_) => ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT,
                None => {
                    if erc20_ledger_id == read_state(|s| s.native_ledger_id) {
                        NATIVE_WITHDRAWAL_TRANSACTION_GAS_LIMIT
                    } else {
                        ic_cdk::trap(&format!(
                            "ERROR: Unsupported ckERC20 token ledger {}",
                            erc20_ledger_id
                        ))
                    }
                }
            }
        }
    };
    match read_state(|s| s.last_transaction_price_estimate.clone()) {
        Some((ts, estimate)) => {
            let mut result = Eip1559TransactionPrice::from(estimate.to_price(gas_limit));
            result.timestamp = Some(ts);
            result
        }
        None => ic_cdk::trap("ERROR: last transaction price estimate is not available"),
    }
}

/// Returns the current parameters used by the minter.
/// This includes information that can be retrieved form other endpoints as well.
/// To retain some flexibility in the API all fields in the return value are optional.
#[allow(deprecated)]
#[query]
async fn get_minter_info() -> MinterInfo {
    read_state(|s| {
        let erc20_balances = Some(
            s.supported_erc20_tokens()
                .map(|token| Erc20Balance {
                    erc20_contract_address: token.erc20_contract_address.to_string(),
                    balance: s
                        .erc20_balances
                        .balance_of(&token.erc20_contract_address)
                        .into(),
                })
                .collect(),
        );
        let supported_erc20_tokens = Some(
            s.supported_erc20_tokens()
                .map(|token| candid_types::Erc20Token::from(token))
                .collect(),
        );

        let icrc_balances = Some(
            s.icrc_balances
                .balance_by_icrc_ledger
                .iter()
                .map(|(token, balance)| IcrcBalance {
                    icrc_token: *token,
                    balance: (*balance).into(),
                })
                .collect(),
        );

        let wrapped_icrc_tokens = Some(
            s.wrapped_icrc_tokens
                .iter()
                .map(|(token, erc20_address, _)| WrappedIcrcToken {
                    base_token: *token,
                    deployed_wrapped_erc20: erc20_address.to_string(),
                })
                .collect(),
        );

        MinterInfo {
            minter_address: s.minter_address().map(|a| a.to_string()),
            helper_smart_contract_address: s
                .helper_contract_addresses
                .as_ref()
                .and_then(|addresses| addresses.first().map(|address| address.to_string())),

            helper_smart_contract_addresses: s.helper_contract_addresses.as_ref().map(
                |addresses| {
                    addresses
                        .into_iter()
                        .map(|address| address.to_string())
                        .collect()
                },
            ),
            supported_erc20_tokens,
            minimum_withdrawal_amount: Some(s.native_minimum_withdrawal_amount.into()),
            deposit_native_fee: None,
            withdrawal_native_fee: s.withdrawal_native_fee.map(|fee| fee.into()),
            block_height: Some(s.block_height.into()),
            last_observed_block_number: s.last_observed_block_number.map(|n| n.into()),
            native_balance: Some(s.native_balance.native_balance().into()),
            last_gas_fee_estimate: s.last_transaction_price_estimate.as_ref().map(
                |(timestamp, estimate)| GasFeeEstimate {
                    max_fee_per_gas: estimate.estimate_max_fee_per_gas().into(),
                    max_priority_fee_per_gas: estimate.max_priority_fee_per_gas.into(),
                    timestamp: *timestamp,
                },
            ),
            erc20_balances,
            last_scraped_block_number: Some(s.last_scraped_block_number.into()),
            native_twin_token_ledger_id: Some(s.native_ledger_id),
            ledger_suite_manager_id: s.ledger_suite_manager_id,
            swap_canister_id: s.swap_canister_id,
            total_collected_operation_fee: Some(
                s.native_balance.total_collected_operation_native_fee.into(),
            ),
            icrc_balances,
            wrapped_icrc_tokens,
        }
    })
}

// The logs are scraped automatically every 10 minutes, however if a user deposits some funds in the smart contract they can all this function
// with the block number that deposit transaction is located at, and the minter would scrape the logs after necessary validation.
// Validation factors:
// 1: The provided block number should be greater than last observed block number.
// 2: There should be at least a minute of gap between the last time this function was called and now.
// Meaning that this function can only be called onces in a minute due to cycle drain attacks.
#[update]
async fn request_scraping_logs() -> Result<(), RequestScrapingError> {
    let last_log_scraping_time = read_state(|s| s.last_log_scraping_time)
        .expect("The block time should not be null at the time of this function call");

    let now_ns = ic_cdk::api::time();

    validate_log_scraping_request(last_log_scraping_time, now_ns)?;

    ic_cdk_timers::set_timer(Duration::from_secs(0), || ic_cdk::spawn(scrape_logs()));

    Ok(())
}

#[query]
async fn retrieve_deposit_status(tx_hash: String) -> Option<DepositStatus> {
    read_state(|s| {
        s.get_deposit_status(Hash::from_str(&tx_hash).expect("Invalid transaction hash"))
    })
}

#[update]
async fn withdraw_native_token(
    WithdrawalArg { amount, recipient }: WithdrawalArg,
) -> Result<RetrieveNativeRequest, WithdrawalError> {
    let caller = validate_caller_not_anonymous();
    let _guard = retrieve_withdraw_guard(caller).unwrap_or_else(|e| {
        println!("{:?}", e);
        ic_cdk::trap(&format!(
            "Failed retrieving guard for principal {}: {:?}",
            caller, e
        ))
    });

    let destination = validate_address_as_destination(&recipient).map_err(|e| match e {
        AddressValidationError::Invalid { .. } | AddressValidationError::NotSupported(_) => {
            WithdrawalError::InvalidDestination("Invalid destination entered".to_string())
        }
    })?;

    let amount = Wei::try_from(amount).expect("failed to convert Nat to u256");

    // If withdrawal_native_fee is some, the total transaction value should be as follow
    // amount - withdrawal_native_fee
    let (withdrawal_native_fee, minimum_withdrawal_amount) =
        read_state(|s| (s.withdrawal_native_fee, s.native_minimum_withdrawal_amount));

    if amount < minimum_withdrawal_amount {
        return Err(WithdrawalError::AmountTooLow {
            min_withdrawal_amount: minimum_withdrawal_amount.into(),
        });
    }

    // Check if l1_fee is required for this network
    let l1_fee = match read_state(|s| s.evm_network) {
        EvmNetwork::Base => Some(DEFAULT_L1_BASE_GAS_FEE),
        _ => None,
    };

    let client = read_state(LedgerClient::native_ledger_from_state);
    let now = ic_cdk::api::time();
    log!(INFO, "[withdraw]: burning {:?}", amount);
    match client
        .burn_from(
            caller.into(),
            amount,
            BurnMemo::Convert {
                to_address: destination,
            },
            None,
        )
        .await
    {
        Ok(ledger_burn_index) => {
            let withdrawal_request = NativeWithdrawalRequest {
                withdrawal_amount: amount,
                destination,
                ledger_burn_index,
                from: caller,
                from_subaccount: None,
                created_at: Some(now),
                l1_fee,
                withdrawal_fee: withdrawal_native_fee,
            };

            log!(
                INFO,
                "[withdraw]: queuing withdrawal request {:?}",
                withdrawal_request,
            );

            mutate_state(|s| {
                process_event(
                    s,
                    EventType::AcceptedNativeWithdrawalRequest(withdrawal_request.clone()),
                );
            });

            ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                ic_cdk::spawn(process_retrieve_tokens_requests())
            });

            Ok(RetrieveNativeRequest::from(withdrawal_request))
        }
        Err(e) => Err(WithdrawalError::from(e)),
    }
}

#[update]
async fn retrieve_withdrawal_status(block_index: u64) -> RetrieveWithdrawalStatus {
    let ledger_burn_index = LedgerBurnIndex::new(block_index);
    read_state(|s| {
        s.withdrawal_transactions
            .transaction_status(&ledger_burn_index)
    })
}

#[query]
async fn withdrawal_status(parameter: WithdrawalSearchParameter) -> Vec<WithdrawalDetail> {
    use transactions::WithdrawalRequest::*;
    let parameter = transactions::WithdrawalSearchParameter::try_from(parameter).unwrap();
    read_state(|s| {
        s.withdrawal_transactions
            .withdrawal_status(&parameter)
            .into_iter()
            .map(|(request, status, tx)| WithdrawalDetail {
                withdrawal_id: *request.native_ledger_burn_index().as_ref(),
                recipient_address: request.payee().to_string(),
                token_symbol: match request {
                    Native(_) => s.native_symbol.to_string(),
                    Erc20(r) => s
                        .erc20_tokens
                        .get_alt(&r.erc20_contract_address)
                        .unwrap()
                        .to_string(),
                },
                withdrawal_amount: match request {
                    Native(r) => r.withdrawal_amount.into(),
                    Erc20(r) => r.withdrawal_amount.into(),
                },
                max_transaction_fee: match (request, tx) {
                    (Native(_), None) => None,
                    (Native(r), Some(tx)) => {
                        r.withdrawal_amount.checked_sub(tx.amount).map(|x| x.into())
                    }
                    (Erc20(r), _) => Some(r.max_transaction_fee.into()),
                },
                from: request.from(),
                from_subaccount: request
                    .from_subaccount()
                    .clone()
                    .map(|subaccount| subaccount.0),
                status,
            })
            .collect()
    })
}

#[update]
async fn withdraw_erc20(
    WithdrawErc20Arg {
        amount,
        erc20_ledger_id,
        recipient,
    }: WithdrawErc20Arg,
) -> Result<RetrieveErc20Request, WithdrawErc20Error> {
    let caller = validate_caller_not_anonymous();
    let _guard = retrieve_withdraw_guard(caller).unwrap_or_else(|e| {
        ic_cdk::trap(&format!(
            "Failed retrieving guard for principal {}: {:?}",
            caller, e
        ))
    });

    let destination = validate_address_as_destination(&recipient).map_err(|e| match e {
        AddressValidationError::Invalid { .. } | AddressValidationError::NotSupported(_) => {
            WithdrawErc20Error::InvalidDestination("Invalid destination entered".to_string())
        }
    })?;

    let erc20_withdrawal_amount =
        Erc20Value::try_from(amount).expect("ERROR: failed to convert Nat to u256");

    let erc20_token = read_state(|s| s.find_erc20_token_by_ledger_id(&erc20_ledger_id))
        .ok_or_else(|| {
            let supported_erc20_tokens: BTreeSet<_> = read_state(|s| {
                s.supported_erc20_tokens()
                    .map(|token| token.into())
                    .collect()
            });
            WithdrawErc20Error::TokenNotSupported {
                supported_tokens: Vec::from_iter(supported_erc20_tokens),
            }
        })?;

    let (withdrawal_native_fee, native_ledger, native_transfer_fee) = read_state(|s| {
        (
            s.withdrawal_native_fee,
            LedgerClient::native_ledger_from_state(s),
            s.native_ledger_transfer_fee,
        )
    });

    let erc20_tx_fee = estimate_erc20_transaction_fee().await.ok_or_else(|| {
        WithdrawErc20Error::TemporarilyUnavailable("Failed to retrieve current gas fee".to_string())
    })?;

    // Check if l1_fee is required for this network
    let l1_fee = match read_state(|s| s.evm_network) {
        EvmNetwork::Base => Some(DEFAULT_L1_BASE_GAS_FEE),
        _ => None,
    };

    let now = ic_cdk::api::time();

    // amount that will be burnt to cover transaction_fees plus transaction_signing
    // cost(native_withdrawal_fee)
    let native_burn_amount = erc20_tx_fee
        .checked_add(l1_fee.unwrap_or(Wei::ZERO))
        .expect("Bug: Tx_fee plus l1_fee should fit in u256")
        .checked_add(withdrawal_native_fee.unwrap_or(Wei::ZERO))
        .unwrap_or(Wei::MAX);

    log!(
        INFO,
        "[withdraw_erc20]: burning {:?} native",
        native_burn_amount
    );

    match native_ledger
        .burn_from(
            caller.into(),
            native_burn_amount,
            BurnMemo::Erc20GasFee {
                erc20_token_symbol: erc20_token.erc20_token_symbol.clone(),
                erc20_withdrawal_amount,
                to_address: destination,
            },
            None,
        )
        .await
    {
        Ok(native_ledger_burn_index) => {
            log!(
                INFO,
                "[withdraw_erc20]: burning {} {}",
                erc20_withdrawal_amount,
                erc20_token.erc20_token_symbol
            );
            match LedgerClient::erc20_ledger(&erc20_token)
                .burn_from(
                    caller.into(),
                    erc20_withdrawal_amount,
                    BurnMemo::Erc20Convert {
                        erc20_withdrawal_id: native_ledger_burn_index.get(),
                        to_address: destination,
                    },
                    None,
                )
                .await
            {
                Ok(erc20_ledger_burn_index) => {
                    let withdrawal_request = Erc20WithdrawalRequest {
                        max_transaction_fee: erc20_tx_fee,
                        withdrawal_amount: erc20_withdrawal_amount,
                        destination,
                        native_ledger_burn_index,
                        erc20_ledger_id: erc20_token.erc20_ledger_id,
                        erc20_ledger_burn_index,
                        erc20_contract_address: erc20_token.erc20_contract_address,
                        from: caller,
                        from_subaccount: None,
                        created_at: now,
                        l1_fee,
                        is_wrapped_mint: Some(false),
                        withdrawal_fee: withdrawal_native_fee,
                    };
                    log!(
                        INFO,
                        "[withdraw_erc20]: queuing withdrawal request {:?}",
                        withdrawal_request
                    );
                    mutate_state(|s| {
                        process_event(
                            s,
                            EventType::AcceptedErc20WithdrawalRequest(withdrawal_request.clone()),
                        );
                    });

                    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                        ic_cdk::spawn(process_retrieve_tokens_requests())
                    });

                    Ok(RetrieveErc20Request::from(withdrawal_request))
                }
                Err(erc20_burn_error) => {
                    let reimbursed_amount = match &erc20_burn_error {
                        LedgerBurnError::TemporarilyUnavailable { .. } => native_burn_amount, //don't penalize user in case of an error outside of their control
                        LedgerBurnError::InsufficientFunds { .. }
                        | LedgerBurnError::AmountTooLow { .. }
                        | LedgerBurnError::InsufficientAllowance { .. } => native_burn_amount
                            .checked_sub(native_transfer_fee)
                            .unwrap_or(Wei::ZERO),
                    };

                    if reimbursed_amount > Wei::ZERO {
                        let reimbursement_request = ReimbursementRequest {
                            ledger_burn_index: native_ledger_burn_index,
                            reimbursed_amount: reimbursed_amount.change_units(),
                            to: caller,
                            to_subaccount: None,
                            transaction_hash: None,
                        };
                        mutate_state(|s| {
                            process_event(
                                s,
                                EventType::FailedErc20WithdrawalRequest(reimbursement_request),
                            );
                        });
                    }

                    Err(WithdrawErc20Error::Erc20LedgerError {
                        native_block_index: Nat::from(native_ledger_burn_index.get()),
                        error: erc20_burn_error.into(),
                    })
                }
            }
        }
        Err(native_burn_error) => Err(WithdrawErc20Error::NativeLedgerError {
            error: native_burn_error.into(),
        }),
    }
}

// mints wrapped tokens on the evm side corresponding to the locked tokens on the icp side
#[update]
async fn wrap_icrc(
    WrapIcrcArg {
        amount,
        icrc_ledger_id,
        recipient,
    }: WrapIcrcArg,
) -> Result<RetrieveWrapIcrcRequest, WrapIcrcError> {
    let caller = validate_caller_not_anonymous();
    let _guard = retrieve_withdraw_guard(caller).unwrap_or_else(|e| {
        ic_cdk::trap(&format!(
            "Failed retrieving guard for principal {}: {:?}",
            caller, e
        ))
    });

    let destination = validate_address_as_destination(&recipient).map_err(|e| match e {
        AddressValidationError::Invalid { .. } | AddressValidationError::NotSupported(_) => {
            WrapIcrcError::InvalidDestination("Invalid destination entered".to_string())
        }
    })?;

    let lock_amount = Erc20Value::try_from(amount).expect("ERROR: failed to convert Nat to u256");

    let erc20_token = read_state(|s| s.find_wrapped_erc20_token_by_icrc_ledger_id(&icrc_ledger_id))
        .ok_or_else(|| {
            let supported_wrapped_icrc_tokens: BTreeSet<_> = read_state(|s| {
                s.supported_wrapped_icrc_tokens()
                    .map(|(ledger_id, address)| WrappedIcrcToken {
                        base_token: ledger_id,
                        deployed_wrapped_erc20: address.to_string(),
                    })
                    .collect()
            });
            WrapIcrcError::TokenNotSupported {
                supported_tokens: Vec::from_iter(supported_wrapped_icrc_tokens),
            }
        })?;

    let (withdrawal_native_fee, native_ledger, native_transfer_fee) = read_state(|s| {
        (
            s.withdrawal_native_fee,
            LedgerClient::native_ledger_from_state(s),
            s.native_ledger_transfer_fee,
        )
    });

    let erc20_tx_fee = estimate_icrc_wrap_transaction_fee().await.ok_or_else(|| {
        WrapIcrcError::TemporarilyUnavailable("Failed to retrieve current gas fee".to_string())
    })?;

    // Check if l1_fee is required for this network
    let l1_fee = match read_state(|s| s.evm_network) {
        EvmNetwork::Base => Some(DEFAULT_L1_BASE_GAS_FEE),
        _ => None,
    };

    let now = ic_cdk::api::time();

    // amount that will be burnt to cover transaction_fees plus transaction_signing
    // cost(native_withdrawal_fee)
    let native_burn_amount = erc20_tx_fee
        .checked_add(l1_fee.unwrap_or(Wei::ZERO))
        .expect("Bug: Tx_fee plus l1_fee should fit in u256")
        .checked_add(withdrawal_native_fee.unwrap_or(Wei::ZERO))
        .unwrap_or(Wei::MAX);

    let icrc_ledger_client = LedgerClient::icrc_ledger(icrc_ledger_id);

    log!(INFO, "[wrap_icrc]: burning {:?} native", native_burn_amount);
    match native_ledger
        .burn_from(
            caller.into(),
            native_burn_amount,
            BurnMemo::WrapIcrcGasFee {
                wrapped_icrc_base: icrc_ledger_id,
                wrap_amount: lock_amount,
                to_address: destination,
            },
            None,
        )
        .await
    {
        Ok(native_ledger_burn_index) => {
            log!(INFO, "[wrap_icrc]: locking {}", icrc_ledger_id,);
            match icrc_ledger_client
                .burn_from(
                    caller.into(),
                    lock_amount,
                    BurnMemo::IcrcLocked {
                        to_address: destination,
                    },
                    None,
                )
                .await
            {
                Ok(erc20_ledger_burn_index) => {
                    let withdrawal_request = Erc20WithdrawalRequest {
                        max_transaction_fee: erc20_tx_fee,
                        withdrawal_amount: lock_amount,
                        destination,
                        native_ledger_burn_index,
                        erc20_ledger_id: icrc_ledger_id,
                        erc20_ledger_burn_index,
                        erc20_contract_address: erc20_token,
                        from: caller,
                        from_subaccount: None,
                        created_at: now,
                        l1_fee,
                        is_wrapped_mint: Some(true),
                        withdrawal_fee: withdrawal_native_fee,
                    };
                    log!(
                        INFO,
                        "[wrap_icrc]: queuing withdrawal request {:?}",
                        withdrawal_request
                    );
                    mutate_state(|s| {
                        process_event(
                            s,
                            EventType::AcceptedErc20WithdrawalRequest(withdrawal_request.clone()),
                        );
                    });

                    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                        ic_cdk::spawn(process_retrieve_tokens_requests())
                    });

                    Ok(RetrieveWrapIcrcRequest::from(withdrawal_request))
                }
                Err(icrc_lock_error) => {
                    let reimbursed_amount = match &icrc_lock_error {
                        LedgerBurnError::TemporarilyUnavailable { .. } => native_burn_amount, //don't penalize user in case of an error outside of their control
                        LedgerBurnError::InsufficientFunds { .. }
                        | LedgerBurnError::AmountTooLow { .. }
                        | LedgerBurnError::InsufficientAllowance { .. } => native_burn_amount
                            .checked_sub(native_transfer_fee)
                            .unwrap_or(Wei::ZERO),
                    };

                    if reimbursed_amount > Wei::ZERO {
                        let reimbursement_request = ReimbursementRequest {
                            ledger_burn_index: native_ledger_burn_index,
                            reimbursed_amount: reimbursed_amount.change_units(),
                            to: caller,
                            to_subaccount: None,
                            transaction_hash: None,
                        };
                        mutate_state(|s| {
                            process_event(
                                s,
                                EventType::FailedIcrcLockRequest(reimbursement_request),
                            );
                        });
                    }

                    Err(WrapIcrcError::IcrcLedgerError {
                        native_block_index: Nat::from(native_ledger_burn_index.get()),
                        error: icrc_lock_error.into(),
                    })
                }
            }
        }
        Err(native_burn_error) => Err(WrapIcrcError::NativeLedgerError {
            error: native_burn_error.into(),
        }),
    }
}

async fn estimate_erc20_transaction_fee() -> Option<Wei> {
    lazy_refresh_gas_fee_estimate()
        .await
        .map(|gas_fee_estimate| {
            gas_fee_estimate
                .to_price(ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT)
                .max_transaction_fee()
        })
}

async fn estimate_icrc_wrap_transaction_fee() -> Option<Wei> {
    lazy_refresh_gas_fee_estimate()
        .await
        .map(|gas_fee_estimate| {
            gas_fee_estimate
                .to_price(ERC20_MINT_TRANSACTION_GAS_LIMIT)
                .max_transaction_fee()
        })
}

#[update]
async fn add_erc20_token(erc20_token: AddErc20Token) {
    let orchestrator_id = read_state(|s| s.ledger_suite_manager_id)
        .unwrap_or_else(|| ic_cdk::trap("ERROR: ERC-20 feature is not activated"));
    if orchestrator_id != ic_cdk::caller() {
        ic_cdk::trap(&format!(
            "ERROR: only the orchestrator {} can add ERC-20 tokens",
            orchestrator_id
        ));
    }
    let erc20_token = ERC20Token::try_from(erc20_token)
        .unwrap_or_else(|e| ic_cdk::trap(&format!("ERROR: {}", e)));
    mutate_state(|s| process_event(s, EventType::AddedErc20Token(erc20_token)));
}

#[update]
async fn get_canister_status() -> ic_cdk::api::management_canister::main::CanisterStatusResponse {
    ic_cdk::api::management_canister::main::canister_status(
        ic_cdk::api::management_canister::main::CanisterIdRecord {
            canister_id: ic_cdk::id(),
        },
    )
    .await
    .expect("failed to fetch canister status")
    .0
}

// Only the swap canister can call this function to make the process of swapping faster
#[update]
async fn check_new_deposits() {
    let swap_canister_id = read_state(|s| s.swap_canister_id)
        .unwrap_or_else(|| ic_cdk::trap("ERROR: swap feature not activated"));
    if swap_canister_id != ic_cdk::caller() {
        ic_cdk::trap(&format!(
            "ERROR: only the swap canister id {} can add request for early deposit check",
            swap_canister_id
        ));
    }
    scrape_logs().await;
}

#[query]
fn get_events(arg: GetEventsArg) -> GetEventsResult {
    use evm_minter::candid_types::events::{
        AccessListItem, ReimbursementIndex as CandidReimbursementIndex,
        TransactionReceipt as CandidTransactionReceipt,
        TransactionStatus as CandidTransactionStatus, UnsignedTransaction,
    };
    //use crate::candid_types::
    use evm_minter::rpc_declarations::TransactionReceipt;
    use evm_minter::tx::Eip1559TransactionRequest;
    use serde_bytes::ByteBuf;

    const MAX_EVENTS_PER_RESPONSE: u64 = 100;

    fn map_event_source(
        EventSource {
            transaction_hash,
            log_index,
        }: EventSource,
    ) -> CandidEventSource {
        CandidEventSource {
            transaction_hash: transaction_hash.to_string(),
            log_index: log_index.into(),
        }
    }

    fn map_reimbursement_index(index: ReimbursementIndex) -> CandidReimbursementIndex {
        match index {
            ReimbursementIndex::Native { ledger_burn_index } => CandidReimbursementIndex::Native {
                ledger_burn_index: ledger_burn_index.get().into(),
            },
            ReimbursementIndex::Erc20 {
                native_ledger_burn_index,
                ledger_id,
                erc20_ledger_burn_index,
            } => CandidReimbursementIndex::Erc20 {
                native_ledger_burn_index: native_ledger_burn_index.get().into(),
                ledger_id,
                erc20_ledger_burn_index: erc20_ledger_burn_index.get().into(),
            },
            ReimbursementIndex::IcrcWrap {
                native_ledger_burn_index,
                icrc_token,
                icrc_ledger_lock_index,
            } => CandidReimbursementIndex::IcrcWrap {
                native_ledger_burn_index: native_ledger_burn_index.get().into(),
                icrc_token,
                icrc_ledger_lock_index: icrc_ledger_lock_index.get().into(),
            },
        }
    }

    fn map_unsigned_transaction(tx: Eip1559TransactionRequest) -> UnsignedTransaction {
        UnsignedTransaction {
            chain_id: tx.chain_id.into(),
            nonce: tx.nonce.into(),
            max_priority_fee_per_gas: tx.max_priority_fee_per_gas.into(),
            max_fee_per_gas: tx.max_fee_per_gas.into(),
            gas_limit: tx.gas_limit.into(),
            destination: tx.destination.to_string(),
            value: tx.amount.into(),
            data: ByteBuf::from(tx.data),
            access_list: tx
                .access_list
                .0
                .iter()
                .map(|item| AccessListItem {
                    address: item.address.to_string(),
                    storage_keys: item
                        .storage_keys
                        .iter()
                        .map(|key| ByteBuf::from(key.0.to_vec()))
                        .collect(),
                })
                .collect(),
        }
    }

    fn map_transaction_receipt(receipt: TransactionReceipt) -> CandidTransactionReceipt {
        use evm_minter::rpc_declarations::TransactionStatus;
        CandidTransactionReceipt {
            block_hash: receipt.block_hash.to_string(),
            block_number: receipt.block_number.into(),
            effective_gas_price: receipt.effective_gas_price.into(),
            gas_used: receipt.gas_used.into(),
            status: match receipt.status {
                TransactionStatus::Success => CandidTransactionStatus::Success,
                TransactionStatus::Failure => CandidTransactionStatus::Failure,
            },
            transaction_hash: receipt.transaction_hash.to_string(),
        }
    }

    fn map_event(Event { timestamp, payload }: Event) -> CandidEvent {
        use evm_minter::candid_types::events::EventPayload as EP;
        CandidEvent {
            timestamp,
            payload: match payload {
                EventType::Init(args) => EP::Init(args),
                EventType::Upgrade(args) => EP::Upgrade(args),
                EventType::AcceptedDeposit(ReceivedNativeEvent {
                    transaction_hash,
                    block_number,
                    log_index,
                    from_address,
                    value,
                    principal,
                    subaccount,
                }) => EP::AcceptedDeposit {
                    transaction_hash: transaction_hash.to_string(),
                    block_number: block_number.into(),
                    log_index: log_index.into(),
                    from_address: from_address.to_string(),
                    value: value.into(),
                    principal,
                    subaccount: subaccount.map(|s| s.to_bytes()),
                },
                EventType::AcceptedErc20Deposit(ReceivedErc20Event {
                    transaction_hash,
                    block_number,
                    log_index,
                    from_address,
                    value,
                    principal,
                    erc20_contract_address,
                    subaccount,
                }) => EP::AcceptedErc20Deposit {
                    transaction_hash: transaction_hash.to_string(),
                    block_number: block_number.into(),
                    log_index: log_index.into(),
                    from_address: from_address.to_string(),
                    value: value.into(),
                    principal,
                    erc20_contract_address: erc20_contract_address.to_string(),
                    subaccount: subaccount.map(|s| s.to_bytes()),
                },
                EventType::InvalidDeposit {
                    event_source,
                    reason,
                } => EP::InvalidDeposit {
                    event_source: map_event_source(event_source),
                    reason,
                },
                EventType::MintedNative {
                    event_source,
                    mint_block_index,
                } => EP::MintedNative {
                    event_source: map_event_source(event_source),
                    mint_block_index: mint_block_index.get().into(),
                },
                EventType::SyncedToBlock { block_number } => EP::SyncedToBlock {
                    block_number: block_number.into(),
                },
                EventType::AcceptedNativeWithdrawalRequest(NativeWithdrawalRequest {
                    withdrawal_amount,
                    destination,
                    ledger_burn_index,
                    from,
                    from_subaccount,
                    created_at,
                    l1_fee,
                    withdrawal_fee,
                }) => EP::AcceptedNativeWithdrawalRequest {
                    withdrawal_amount: withdrawal_amount.into(),
                    destination: destination.to_string(),
                    ledger_burn_index: ledger_burn_index.get().into(),
                    from,
                    from_subaccount: from_subaccount.map(|s| s.0),
                    created_at,
                    l1_fee: l1_fee.map(|fee| fee.into()),
                    withdrawal_fee: withdrawal_fee.map(|fee| fee.into()),
                },
                EventType::CreatedTransaction {
                    withdrawal_id,
                    transaction,
                } => EP::CreatedTransaction {
                    withdrawal_id: withdrawal_id.get().into(),
                    transaction: map_unsigned_transaction(transaction),
                },
                EventType::SignedTransaction {
                    withdrawal_id,
                    transaction,
                } => EP::SignedTransaction {
                    withdrawal_id: withdrawal_id.get().into(),
                    raw_transaction: transaction.raw_transaction_hex(),
                },
                EventType::ReplacedTransaction {
                    withdrawal_id,
                    transaction,
                } => EP::ReplacedTransaction {
                    withdrawal_id: withdrawal_id.get().into(),
                    transaction: map_unsigned_transaction(transaction),
                },
                EventType::FinalizedTransaction {
                    withdrawal_id,
                    transaction_receipt,
                } => EP::FinalizedTransaction {
                    withdrawal_id: withdrawal_id.get().into(),
                    transaction_receipt: map_transaction_receipt(transaction_receipt),
                },
                EventType::ReimbursedNativeWithdrawal(Reimbursed {
                    burn_in_block: withdrawal_id,
                    reimbursed_in_block,
                    reimbursed_amount,
                    transaction_hash,
                    transfer_fee: _,
                }) => EP::ReimbursedNativeWithdrawal {
                    withdrawal_id: withdrawal_id.get().into(),
                    reimbursed_in_block: reimbursed_in_block.get().into(),
                    reimbursed_amount: reimbursed_amount.into(),
                    transaction_hash: transaction_hash.map(|h| h.to_string()),
                },
                EventType::ReimbursedErc20Withdrawal {
                    native_ledger_burn_index,
                    erc20_ledger_id,
                    reimbursed,
                } => EP::ReimbursedErc20Withdrawal {
                    withdrawal_id: native_ledger_burn_index.get().into(),
                    burn_in_block: reimbursed.burn_in_block.get().into(),
                    ledger_id: erc20_ledger_id,
                    reimbursed_in_block: reimbursed.reimbursed_in_block.get().into(),
                    reimbursed_amount: reimbursed.reimbursed_amount.into(),
                    transaction_hash: reimbursed.transaction_hash.map(|h| h.to_string()),
                },
                EventType::SkippedBlock { block_number } => EP::SkippedBlock {
                    block_number: block_number.into(),
                },
                EventType::AddedErc20Token(token) => EP::AddedErc20Token {
                    chain_id: token.chain_id.chain_id().into(),
                    address: token.erc20_contract_address.to_string(),
                    erc20_token_symbol: token.erc20_token_symbol.to_string(),
                    erc20_ledger_id: token.erc20_ledger_id,
                },
                EventType::AcceptedErc20WithdrawalRequest(Erc20WithdrawalRequest {
                    max_transaction_fee,
                    withdrawal_amount,
                    destination,
                    native_ledger_burn_index,
                    erc20_contract_address,
                    erc20_ledger_id,
                    erc20_ledger_burn_index,
                    from,
                    from_subaccount,
                    created_at,
                    l1_fee,
                    withdrawal_fee,
                    is_wrapped_mint,
                }) => EP::AcceptedErc20WithdrawalRequest {
                    max_transaction_fee: max_transaction_fee.into(),
                    withdrawal_amount: withdrawal_amount.into(),
                    erc20_contract_address: erc20_contract_address.to_string(),
                    destination: destination.to_string(),
                    native_ledger_burn_index: native_ledger_burn_index.get().into(),
                    erc20_ledger_id,
                    erc20_ledger_burn_index: erc20_ledger_burn_index.get().into(),
                    from,
                    from_subaccount: from_subaccount.map(|s| s.0),
                    created_at,
                    l1_fee: l1_fee.map(|fee| fee.into()),
                    withdrawal_fee: withdrawal_fee.map(|fee| fee.into()),
                    is_wrapped_mint: is_wrapped_mint.unwrap_or_default(),
                },
                EventType::MintedErc20 {
                    event_source,
                    mint_block_index,
                    erc20_token_symbol,
                    erc20_contract_address,
                } => EP::MintedErc20 {
                    event_source: map_event_source(event_source),
                    mint_block_index: mint_block_index.get().into(),
                    erc20_token_symbol,
                    erc20_contract_address: erc20_contract_address.to_string(),
                },
                EventType::FailedErc20WithdrawalRequest(ReimbursementRequest {
                    ledger_burn_index,
                    reimbursed_amount,
                    to,
                    to_subaccount,
                    transaction_hash: _,
                }) => EP::FailedErc20WithdrawalRequest {
                    withdrawal_id: ledger_burn_index.get().into(),
                    reimbursed_amount: reimbursed_amount.into(),
                    to,
                    to_subaccount: to_subaccount.map(|s| s.0),
                },
                EventType::QuarantinedDeposit { event_source } => EP::QuarantinedDeposit {
                    event_source: map_event_source(event_source),
                },
                EventType::QuarantinedReimbursement { index } => EP::QuarantinedReimbursement {
                    index: map_reimbursement_index(index),
                },
                EventType::AcceptedWrappedIcrcBurn(ReceivedBurnEvent {
                    transaction_hash,
                    block_number,
                    log_index,
                    from_address,
                    value,
                    principal,
                    wrapped_erc20_contract_address,
                    icrc_token_principal,
                    subaccount,
                }) => EP::AcceptedWrappedIcrcBurn {
                    transaction_hash: transaction_hash.to_string(),
                    block_number: block_number.into(),
                    log_index: log_index.into(),
                    from_address: from_address.to_string(),
                    value: value.into(),
                    principal: principal,
                    wrapped_erc20_contract_address: wrapped_erc20_contract_address.to_string(),
                    icrc_token_principal,
                    subaccount: subaccount.map(|s| s.to_bytes()),
                },
                EventType::InvalidEvent {
                    event_source,
                    reason,
                } => EP::InvalidEvent {
                    event_source: map_event_source(event_source),
                    reason: reason.to_string(),
                },
                EventType::DeployedWrappedIcrcToken(ReceivedWrappedIcrcDeployedEvent {
                    transaction_hash,
                    block_number,
                    log_index,
                    base_token,
                    deployed_wrapped_erc20,
                }) => EP::DeployedWrappedIcrcToken {
                    transaction_hash: transaction_hash.to_string(),
                    block_number: block_number.into(),
                    log_index: log_index.into(),
                    base_token: base_token,
                    deployed_wrapped_erc20: deployed_wrapped_erc20.to_string(),
                },
                EventType::QuarantinedRelease {
                    event_source,
                    release_event: _,
                } => EP::QuarantinedRelease {
                    event_source: map_event_source(event_source),
                },
                EventType::ReleasedIcrcToken {
                    event_source,
                    release_block_index,
                    released_icrc_token: _,
                    wrapped_erc20_contract_address: _,
                    transfer_fee,
                } => EP::ReleasedIcrcToken {
                    event_source: map_event_source(event_source),
                    release_block_index: release_block_index.get().into(),
                    transfer_fee: transfer_fee.into(),
                },
                EventType::FailedIcrcLockRequest(ReimbursementRequest {
                    ledger_burn_index,
                    reimbursed_amount,
                    to,
                    to_subaccount,
                    transaction_hash: _,
                }) => EP::FailedIcrcLockRequest {
                    withdrawal_id: ledger_burn_index.get().into(),
                    reimbursed_amount: reimbursed_amount.into(),
                    to,
                    to_subaccount: to_subaccount.map(|s| s.0),
                },
                EventType::ReimbursedIcrcWrap {
                    native_ledger_burn_index,
                    reimbursed_icrc_token,
                    reimbursed,
                } => EP::ReimbursedIcrcWrap {
                    native_ledger_burn_index: native_ledger_burn_index.get().into(),
                    lock_in_block: reimbursed.burn_in_block.get().into(),
                    reimbursed_in_block: reimbursed.reimbursed_in_block.get().into(),
                    reimbursed_icrc_token,
                    reimbursed_amount: reimbursed.reimbursed_amount.into(),
                    transaction_hash: reimbursed.transaction_hash.map(|hash| hash.to_string()),
                    transfer_fee: reimbursed.transfer_fee.map(|fee| fee.into()),
                },
            },
        }
    }

    let events = storage::with_event_iter(|it| {
        it.skip(arg.start as usize)
            .take(arg.length.min(MAX_EVENTS_PER_RESPONSE) as usize)
            .map(map_event)
            .collect()
    });

    GetEventsResult {
        events,
        total_event_count: storage::total_event_count(),
    }
}

#[update]
pub async fn update_chain_data(chain_data: ChainData) {
    let caller = ic_cdk::caller();
    let rpc_helper_identity = Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap();

    if caller != rpc_helper_identity {
        panic!("Access Denied");
    }

    let now = ic_cdk::api::time();
    let network = read_state(|s| s.evm_network());

    let latest_block_number = apply_safe_threshold_to_latest_block_numner(
        network,
        BlockNumber::try_from(chain_data.latest_block_number)
            .expect("Failed to parse block number"),
    );

    let previous_observed_block =
        read_state(|s| s.last_observed_block_number).unwrap_or(BlockNumber::ZERO);

    if previous_observed_block > latest_block_number {
        return;
    }

    let fee_history =
        parse_fee_history(chain_data.fee_history).expect("Failed to parse fee hisotry");

    match estimate_transaction_fee(&fee_history) {
        Ok(estimate) => {
            mutate_state(|s| {
                s.last_transaction_price_estimate = Some((now, estimate.clone()));
                s.last_observed_block_number = Some(latest_block_number);
                s.last_observed_block_time = Some(now);
            });
        }
        Err(e) => {
            log!(
                INFO,
                "[refresh_gas_fee_estimate]: Failed estimating gas fee: {e:?}",
            );
        }
    };
}

/// Returns the amount of heap memory in bytes that has been allocated.
//#[cfg(target_arch = "wasm32")]
//pub fn heap_memory_size_bytes() -> usize {
//    const WASM_PAGE_SIZE_BYTES: usize = 65536;
//    core::arch::wasm32::memory_size(0) * WASM_PAGE_SIZE_BYTES
//}

#[cfg(not(any(target_arch = "wasm32")))]
pub fn heap_memory_size_bytes() -> usize {
    0
}

// list every base URL that users will authenticate to your app from
#[update]
fn icrc28_trusted_origins() -> Icrc28TrustedOriginsResponse {
    let trusted_origins = vec![
        String::from("https://dduc6-3yaaa-aaaal-ai63a-cai.icp0.io"),
        String::from("https://dduc6-3yaaa-aaaal-ai63a-cai.raw.icp0.io"),
        String::from("https://dduc6-3yaaa-aaaal-ai63a-cai.ic0.app"),
        String::from("https://dduc6-3yaaa-aaaal-ai63a-cai.raw.ic0.app"),
        String::from("https://dduc6-3yaaa-aaaal-ai63a-cai.icp0.icp-api.io"),
        String::from("https://dduc6-3yaaa-aaaal-ai63a-cai.icp-api.io"),
        String::from("https://app.appicdao.com"),
        String::from("https://ib67n-yiaaa-aaaao-qjwca-cai.icp0.io"),
        String::from("https://ib67n-yiaaa-aaaao-qjwca-cai.raw.icp0.io"),
        String::from("https://ib67n-yiaaa-aaaao-qjwca-cai.ic0.app"),
        String::from("https://ib67n-yiaaa-aaaao-qjwca-cai.raw.ic0.app"),
        String::from("https://ib67n-yiaaa-aaaao-qjwca-cai.icp0.icp-api.io"),
        String::from("https://ib67n-yiaaa-aaaao-qjwca-cai.icp-api.io"),
        String::from("https://test.appicdao.com"),
    ];

    return Icrc28TrustedOriginsResponse { trusted_origins };
}

fn main() {}

// Enable Candid export
ic_cdk::export_candid!();
