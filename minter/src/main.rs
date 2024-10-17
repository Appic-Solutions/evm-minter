use candid::{Nat, Principal};
use evm_minter::address::{validate_address_as_destination, AddressValidationError};
use evm_minter::deposit::scrape_logs;
use evm_minter::deposit_logs::{EventSource, ReceivedErc20Event, ReceivedNativeEvent};
use evm_minter::endpoints::events::{
    Event as CandidEvent, EventSource as CandidEventSource, GetEventsArg, GetEventsResult,
};
use evm_minter::endpoints::{self, AddErc20Token};
use evm_minter::endpoints::{
    Eip1559TransactionPrice, Eip1559TransactionPriceArg, Erc20Balance, GasFeeEstimate, MinterInfo,
    RetrieveNativeRequest, RetrieveNativeStatus, WithdrawalArg, WithdrawalDetail, WithdrawalError,
    WithdrawalSearchParameter,
};
use evm_minter::endpoints::{RetrieveErc20Request, WithdrawErc20Arg, WithdrawErc20Error};
use evm_minter::erc20::ERC20Token;
use evm_minter::guard::retrieve_withdraw_guard;
use evm_minter::ledger_client::{LedgerBurnError, LedgerClient};
use evm_minter::lifecycle::MinterArg;
use evm_minter::logs::INFO;
use evm_minter::memo::BurnMemo;
use evm_minter::numeric::{Erc20Value, LedgerBurnIndex, Wei};
use evm_minter::state::audit::{process_event, Event, EventType};
use evm_minter::state::transactions::{
    Erc20WithdrawalRequest, NativeWithdrawalRequest, Reimbursed, ReimbursementIndex,
    ReimbursementRequest,
};
use evm_minter::state::{
    lazy_call_ecdsa_public_key, mutate_state, read_state, transactions, State, STATE,
};
use evm_minter::tx::lazy_refresh_gas_fee_estimate;
use evm_minter::withdraw::{
    process_reimbursement, process_retrieve_tokens_requests,
    ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT, NATIVE_WITHDRAWAL_TRANSACTION_GAS_LIMIT,
};
use evm_minter::{
    state, storage, PROCESS_REIMBURSEMENT, PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL,
    SCRAPING_DEPOSIT_LOGS_INTERVAL,
};
use ic_canister_log::log;
// use ic_canisters_http_types::{HttpRequest, HttpResponse, HttpResponseBuilder};
use ic_cdk::{init, post_upgrade, pre_upgrade, query, update};
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::time::Duration;

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
    // Start scraping logs immediately after the install, then repeat with the interval.
    ic_cdk_timers::set_timer(Duration::from_secs(0), || ic_cdk::spawn(scrape_logs()));
    ic_cdk_timers::set_timer_interval(SCRAPING_DEPOSIT_LOGS_INTERVAL, || {
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
fn init(arg: MinterArg) {
    match arg {
        MinterArg::InitArg(init_arg) => {
            log!(INFO, "[init]: initialized minter with arg: {:?}", init_arg);
            STATE.with(|cell| {
                storage::record_event(EventType::Init(init_arg.clone()));
                *cell.borrow_mut() =
                    Some(State::try_from(init_arg).expect("BUG: failed to initialize minter"))
            });
        }
        MinterArg::UpgradeArg(_) => {
            ic_cdk::trap("cannot init canister state with upgrade args");
        }
    }
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
    setup_timers();
}

#[update]
async fn minter_address() -> String {
    state::minter_address().await.to_string()
}

#[query]
async fn smart_contract_address() -> String {
    read_state(|s| s.helper_contract_address)
        .map(|a| a.to_string())
        .unwrap_or("N/A".to_string())
}

/// Estimate price of EIP-1559 transaction based on the
/// `base_fee_per_gas` included in the last finalized block.
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
                .map(|token| endpoints::Erc20Token::from(token))
                .collect(),
        );

        MinterInfo {
            minter_address: s.minter_address().map(|a| a.to_string()),
            helper_smart_contract_address: s.helper_contract_address.map(|a| a.to_string()),
            supported_erc20_tokens,
            minimum_withdrawal_amount: Some(s.native_minimum_withdrawal_amount.into()),
            block_height: Some(s.block_height.into()),
            last_observed_block_number: s.last_observed_block_number.map(|n| n.into()),
            eth_balance: Some(s.native_balance.native_balance().into()),
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
        }
    })
}

#[update]
async fn withdraw_native_token(
    WithdrawalArg { amount, recipient }: WithdrawalArg,
) -> Result<RetrieveNativeRequest, WithdrawalError> {
    let caller = validate_caller_not_anonymous();
    let _guard = retrieve_withdraw_guard(caller).unwrap_or_else(|e| {
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

    let minimum_withdrawal_amount = read_state(|s| s.native_minimum_withdrawal_amount);
    if amount < minimum_withdrawal_amount {
        return Err(WithdrawalError::AmountTooLow {
            min_withdrawal_amount: minimum_withdrawal_amount.into(),
        });
    }

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
            Ok(RetrieveNativeRequest::from(withdrawal_request))
        }
        Err(e) => Err(WithdrawalError::from(e)),
    }
}

#[update]
async fn retrieve_native_status(block_index: u64) -> RetrieveNativeStatus {
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
    let native_transfer_fee = read_state(|s| s.native_ledger_transfer_fee);
    let native_ledger = read_state(LedgerClient::native_ledger_from_state);
    let erc20_tx_fee = estimate_erc20_transaction_fee().await.ok_or_else(|| {
        WithdrawErc20Error::TemporarilyUnavailable("Failed to retrieve current gas fee".to_string())
    })?;
    let now = ic_cdk::api::time();
    log!(INFO, "[withdraw_erc20]: burning {:?} ckETH", erc20_tx_fee);
    match native_ledger
        .burn_from(
            caller.into(),
            erc20_tx_fee,
            BurnMemo::Erc20GasFee {
                erc20_token_symbol: erc20_token.erc20_token_symbol.clone(),
                erc20_withdrawal_amount,
                to_address: destination,
            },
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
            match LedgerClient::ckerc20_ledger(&erc20_token)
                .burn_from(
                    caller.into(),
                    erc20_withdrawal_amount,
                    BurnMemo::Erc20Convert {
                        erc20_withdrawal_id: native_ledger_burn_index.get(),
                        to_address: destination,
                    },
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
                    Ok(RetrieveErc20Request::from(withdrawal_request))
                }
                Err(erc20_burn_error) => {
                    let reimbursed_amount = match &erc20_burn_error {
                        LedgerBurnError::TemporarilyUnavailable { .. } => erc20_tx_fee, //don't penalize user in case of an error outside of their control
                        LedgerBurnError::InsufficientFunds { .. }
                        | LedgerBurnError::AmountTooLow { .. }
                        | LedgerBurnError::InsufficientAllowance { .. } => erc20_tx_fee
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

async fn estimate_erc20_transaction_fee() -> Option<Wei> {
    lazy_refresh_gas_fee_estimate()
        .await
        .map(|gas_fee_estimate| {
            gas_fee_estimate
                .to_price(ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT)
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
    use evm_minter::endpoints::events::{
        AccessListItem, ReimbursementIndex as CandidReimbursementIndex,
        TransactionReceipt as CandidTransactionReceipt,
        TransactionStatus as CandidTransactionStatus, UnsignedTransaction,
    };
    use evm_minter::rpc_declrations::TransactionReceipt;
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
        use evm_minter::rpc_declrations::TransactionStatus;
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
        use evm_minter::endpoints::events::EventPayload as EP;
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
                }) => EP::AcceptedDeposit {
                    transaction_hash: transaction_hash.to_string(),
                    block_number: block_number.into(),
                    log_index: log_index.into(),
                    from_address: from_address.to_string(),
                    value: value.into(),
                    principal,
                },
                EventType::AcceptedErc20Deposit(ReceivedErc20Event {
                    transaction_hash,
                    block_number,
                    log_index,
                    from_address,
                    value,
                    principal,
                    erc20_contract_address,
                }) => EP::AcceptedErc20Deposit {
                    transaction_hash: transaction_hash.to_string(),
                    block_number: block_number.into(),
                    log_index: log_index.into(),
                    from_address: from_address.to_string(),
                    value: value.into(),
                    principal,
                    erc20_contract_address: erc20_contract_address.to_string(),
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
                }) => EP::AcceptedNativeWithdrawalRequest {
                    withdrawal_amount: withdrawal_amount.into(),
                    destination: destination.to_string(),
                    ledger_burn_index: ledger_burn_index.get().into(),
                    from,
                    from_subaccount: from_subaccount.map(|s| s.0),
                    created_at,
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

#[cfg(feature = "debug_checks")]
#[query]
fn check_audit_log() {
    use evm_minter::state::audit::replay_events;

    emit_preupgrade_events();

    read_state(|s| {
        replay_events()
            .is_equivalent_to(s)
            .expect("replaying the audit log should produce an equivalent state")
    })
}

/// Returns the amount of heap memory in bytes that has been allocated.
#[cfg(target_arch = "wasm32")]
pub fn heap_memory_size_bytes() -> usize {
    const WASM_PAGE_SIZE_BYTES: usize = 65536;
    core::arch::wasm32::memory_size(0) * WASM_PAGE_SIZE_BYTES
}

#[cfg(not(any(target_arch = "wasm32")))]
pub fn heap_memory_size_bytes() -> usize {
    0
}

fn main() {}

// Enable Candid export
ic_cdk::export_candid!();
