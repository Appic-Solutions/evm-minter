use crate::evm_config::EvmNetwork;
use crate::guard::TimerGuard;
use crate::logs::{DEBUG, INFO};
use crate::numeric::{Erc20TokenAmount, GasAmount, LedgerBurnIndex, LedgerMintIndex};
use crate::rpc_client::{MultiCallError, RpcClient};
use crate::rpc_declarations::{SendRawTransactionResult, TransactionReceipt};
use crate::state::audit::{process_event, EventType};
use crate::state::transactions::{
    create_transaction, CreateTransactionError, Reimbursed, ReimbursementIndex,
    ReimbursementRequest, WithdrawalRequest,
};
use crate::state::{mutate_state, State, TaskType};
use crate::tx::gas_fees::{lazy_refresh_gas_fee_estimate, GasFeeEstimate};
use crate::{numeric::TransactionCount, state::read_state};
use candid::Nat;
use futures::future::join_all;
use ic_canister_log::log;
use icrc_ledger_client_cdk::{CdkRuntime, ICRC1Client};
use icrc_ledger_types::icrc1::account::Account;
use icrc_ledger_types::icrc1::transfer::TransferArg;
use num_traits::ToPrimitive;
use scopeguard;
use scopeguard::ScopeGuard;
use std::collections::{BTreeMap, BTreeSet};
use std::iter::zip;

const WITHDRAWAL_REQUESTS_BATCH_SIZE: usize = 5;
const TRANSACTIONS_TO_SIGN_BATCH_SIZE: usize = 5;
const TRANSACTIONS_TO_SEND_BATCH_SIZE: usize = 5;

// 21000 is fixed for native tokens, however 65000 is idle for ERC20s but some ERC20 contracts have
// more complicated logic that requires maximum of 100000 Gas.
pub const NATIVE_WITHDRAWAL_TRANSACTION_GAS_LIMIT: GasAmount = GasAmount::new(21_000);
pub const ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT: GasAmount = GasAmount::new(66_000);

// used for mining wrapped icrc transactions
pub const ERC20_MINT_TRANSACTION_GAS_LIMIT: GasAmount = GasAmount::new(100_000);

pub async fn process_reimbursement() {
    let _guard = match TimerGuard::new(TaskType::Reimbursement) {
        Ok(guard) => guard,
        Err(e) => {
            log!(DEBUG, "Failed retrieving reimbursement guard: {e:?}",);
            return;
        }
    };

    let reimbursements: Vec<(ReimbursementIndex, ReimbursementRequest)> = read_state(|s| {
        s.withdrawal_transactions
            .reimbursement_requests_iter()
            .map(|(index, request)| (index.clone(), request.clone()))
            .collect()
    });
    if reimbursements.is_empty() {
        return;
    }

    let mut error_count = 0;

    for (index, reimbursement_request) in reimbursements {
        // Ensure that even if we were to panic in the callback, after having contacted the ledger to mint the tokens,
        // this reimbursement request will not be processed again.
        let prevent_double_minting_guard = scopeguard::guard(index.clone(), |index| {
            mutate_state(|s| process_event(s, EventType::QuarantinedReimbursement { index }));
        });
        let (ledger_canister_id, should_transfer_fetch_fee) = match index {
            ReimbursementIndex::Native { .. } => read_state(|s| (s.native_ledger_id, false)),
            ReimbursementIndex::Erc20 { ledger_id, .. } => (ledger_id, false),
            ReimbursementIndex::IcrcWrap {
                native_ledger_burn_index: _,
                icrc_token,
                icrc_ledger_lock_index: _,
            } => (icrc_token, true),
        };
        let client = ICRC1Client {
            runtime: CdkRuntime,
            ledger_canister_id,
        };
        let transfer_fee = if should_transfer_fetch_fee {
            match client.fee().await {
                Ok(fee) => Some(Erc20TokenAmount::try_from(fee).unwrap_or(Erc20TokenAmount::MAX)),
                Err(err) => {
                    log!(
                    INFO,
                    "[process_reimbursement] Failed send a message to the ledger ({ledger_canister_id}): {err:?}"
                );
                    error_count += 1;
                    // minting failed, defuse guard
                    ScopeGuard::into_inner(prevent_double_minting_guard);
                    continue;
                }
            }
        } else {
            None
        };

        let amount = match transfer_fee {
            Some(fee) => Nat::from(
                reimbursement_request
                    .reimbursed_amount
                    .checked_sub(fee)
                    .unwrap_or(Erc20TokenAmount::ZERO),
            ),
            None => Nat::from(reimbursement_request.reimbursed_amount),
        };

        let args = TransferArg {
            from_subaccount: None,
            to: Account {
                owner: reimbursement_request.to,
                subaccount: reimbursement_request
                    .to_subaccount
                    .as_ref()
                    .map(|subaccount| subaccount.0),
            },
            fee: transfer_fee.map(|fee| Nat::from(fee)),
            created_at_time: None,
            memo: Some(reimbursement_request.clone().into()),
            amount: amount.clone(),
        };
        let block_index = if amount != Nat::from(Erc20TokenAmount::ZERO) {
            match client.transfer(args).await {
                Ok(Ok(block_index)) => block_index
                    .0
                    .to_u64()
                    .expect("block index should fit into u64"),
                Ok(Err(err)) => {
                    log!(
                        INFO,
                        "[process_reimbursement] Failed to mint native token {err}"
                    );
                    error_count += 1;
                    // minting failed, defuse guard
                    ScopeGuard::into_inner(prevent_double_minting_guard);
                    continue;
                }
                Err(err) => {
                    log!(
                    INFO,
                    "[process_reimbursement] Failed to send a message to the ledger ({ledger_canister_id}): {err:?}"
                );
                    error_count += 1;
                    // minting failed, defuse guard
                    ScopeGuard::into_inner(prevent_double_minting_guard);
                    continue;
                }
            }
        } else {
            0_u64
        };
        let reimbursed = Reimbursed {
            burn_in_block: reimbursement_request.ledger_burn_index,
            reimbursed_in_block: LedgerMintIndex::new(block_index),
            reimbursed_amount: reimbursement_request.reimbursed_amount,
            transaction_hash: reimbursement_request.transaction_hash,
            transfer_fee,
        };
        let event = match index {
            ReimbursementIndex::Native {
                ledger_burn_index: _,
            } => EventType::ReimbursedNativeWithdrawal(reimbursed),
            ReimbursementIndex::Erc20 {
                native_ledger_burn_index,
                ledger_id,
                erc20_ledger_burn_index: _,
            } => EventType::ReimbursedErc20Withdrawal {
                native_ledger_burn_index,
                erc20_ledger_id: ledger_id,
                reimbursed,
            },
            ReimbursementIndex::IcrcWrap {
                native_ledger_burn_index,
                icrc_token,
                icrc_ledger_lock_index: _,
            } => EventType::ReimbursedIcrcWrap {
                native_ledger_burn_index,
                reimbursed_icrc_token: icrc_token,
                reimbursed,
            },
        };
        mutate_state(|s| process_event(s, event));
        // minting succeeded, defuse guard
        ScopeGuard::into_inner(prevent_double_minting_guard);
    }
    if error_count > 0 {
        log!(
            INFO,
            "[process_reimbursement] Failed to reimburse {error_count} users, retrying later."
        );
    }
}

pub async fn process_retrieve_tokens_requests() {
    let _guard = match TimerGuard::new(TaskType::RetrieveEth) {
        Ok(guard) => guard,
        Err(e) => {
            log!(
                DEBUG,
                "Failed retrieving timer guard to process ETH requests: {e:?}",
            );
            return;
        }
    };

    if read_state(|s| !s.withdrawal_transactions.has_pending_requests()) {
        return;
    }

    let gas_fee_estimate = match lazy_refresh_gas_fee_estimate().await {
        Some(gas_fee_estimate) => gas_fee_estimate,
        None => {
            log!(
                INFO,
                "Failed retrieving gas fee estimate to process ETH requests",
            );
            return;
        }
    };

    let latest_transaction_count = latest_transaction_count().await;
    resubmit_transactions_batch(latest_transaction_count, &gas_fee_estimate).await;
    create_transactions_batch(gas_fee_estimate);
    sign_transactions_batch().await;
    send_transactions_batch(latest_transaction_count).await;
    finalize_transactions_batch().await;

    if read_state(|s| s.withdrawal_transactions.has_pending_requests()) {
        ic_cdk_timers::set_timer(
            crate::PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_RETRY_INTERVAL,
            || ic_cdk::spawn(process_retrieve_tokens_requests()),
        );
    }
}

async fn latest_transaction_count() -> Option<TransactionCount> {
    match read_state(RpcClient::from_state_all_providers)
        .get_latest_transaction_count(crate::state::minter_address().await)
        .await
    {
        Ok(transaction_count) => Some(transaction_count),
        Err(e) => {
            log!(INFO, "Failed to get the latest transaction count: {e:?}");
            None
        }
    }
}

async fn resubmit_transactions_batch(
    latest_transaction_count: Option<TransactionCount>,
    gas_fee_estimate: &GasFeeEstimate,
) {
    if read_state(|s| s.withdrawal_transactions.is_sent_tx_empty()) {
        return;
    }
    let latest_transaction_count = match latest_transaction_count {
        Some(latest_transaction_count) => latest_transaction_count,
        None => {
            return;
        }
    };
    let transactions_to_resubmit = read_state(|s| {
        s.withdrawal_transactions
            .create_resubmit_transactions(latest_transaction_count, gas_fee_estimate.clone())
    });
    for result in transactions_to_resubmit {
        match result {
            Ok((withdrawal_id, transaction)) => {
                log!(
                    INFO,
                    "[resubmit_transactions_batch]: transactions to resubmit {transaction:?}"
                );
                mutate_state(|s| {
                    process_event(
                        s,
                        EventType::ReplacedTransaction {
                            withdrawal_id,
                            transaction,
                        },
                    )
                });
            }
            Err(e) => {
                log!(INFO, "Failed to resubmit transaction: {e:?}");
            }
        }
    }
}

fn create_transactions_batch(gas_fee_estimate: GasFeeEstimate) {
    for request in read_state(|s| {
        s.withdrawal_transactions
            .withdrawal_requests_batch(WITHDRAWAL_REQUESTS_BATCH_SIZE)
    }) {
        log!(DEBUG, "[create_transactions_batch]: processing {request:?}",);
        let evm_network = read_state(State::evm_network);
        let nonce = read_state(|s| s.withdrawal_transactions.next_transaction_nonce());
        let gas_limit = estimate_gas_limit(&request);
        match create_transaction(
            &request,
            nonce,
            gas_fee_estimate.clone(),
            gas_limit,
            evm_network,
        ) {
            Ok(transaction) => {
                log!(
                    DEBUG,
                    "[create_transactions_batch]: created transaction {transaction:?}",
                );

                mutate_state(|s| {
                    process_event(
                        s,
                        EventType::CreatedTransaction {
                            withdrawal_id: request.native_ledger_burn_index(),
                            transaction,
                        },
                    );
                });
            }
            Err(CreateTransactionError::InsufficientTransactionFee {
                native_ledger_burn_index: ledger_burn_index,
                allowed_max_transaction_fee: withdrawal_amount,
                actual_max_transaction_fee: max_transaction_fee,
            }) => {
                log!(
                    INFO,
                    "[create_transactions_batch]: Withdrawal request with burn index {ledger_burn_index} has insufficient amount {withdrawal_amount:?} to cover transaction fees: {max_transaction_fee:?}. Request moved back to end of queue."
                );
                mutate_state(|s| {
                    s.withdrawal_transactions
                        .reschedule_withdrawal_request(request)
                });
            }
        };
    }
}

async fn sign_transactions_batch() {
    let transactions_batch: Vec<_> = read_state(|s| {
        s.withdrawal_transactions
            .transactions_to_sign_batch(TRANSACTIONS_TO_SIGN_BATCH_SIZE)
    });
    log!(DEBUG, "Signing transactions {transactions_batch:?}");
    let results = join_all(
        transactions_batch
            .into_iter()
            .map(|(withdrawal_id, tx)| async move { (withdrawal_id, tx.sign().await) }),
    )
    .await;
    let mut errors = Vec::new();
    for (withdrawal_id, result) in results {
        match result {
            Ok(transaction) => mutate_state(|s| {
                process_event(
                    s,
                    EventType::SignedTransaction {
                        withdrawal_id,
                        transaction,
                    },
                )
            }),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        // At this point there might be a gap in transaction nonces between signed transactions, e.g.,
        // transactions 1,2,4,5 were signed, but 3 was not due to some unexpected error.
        // This means that transactions 4 and 5 are currently stuck until transaction 3 is signed.
        // However, we still proceed with transactions 4 and 5 since that way they might be mined faster
        // once transaction 3 is sent on the next iteration. Otherwise, we would need to re-sign transactions 4 and 5
        // and send them (together with transaction 3) on the next iteration.
        log!(INFO, "Errors encountered during signing: {errors:?}");
    }
}

async fn send_transactions_batch(latest_transaction_count: Option<TransactionCount>) {
    let latest_transaction_count = match latest_transaction_count {
        Some(latest_transaction_count) => latest_transaction_count,
        None => {
            return;
        }
    };
    let transactions_to_send: Vec<_> = read_state(|s| {
        s.withdrawal_transactions
            .transactions_to_send_batch(latest_transaction_count, TRANSACTIONS_TO_SEND_BATCH_SIZE)
    });

    log!(INFO, "Transactions to send {:?}", transactions_to_send);
    let rpc_client = read_state(RpcClient::from_state_all_providers);
    let results = join_all(
        transactions_to_send
            .iter()
            .map(|tx| rpc_client.send_raw_transaction(tx.raw_transaction_hex())),
    )
    .await;

    for (signed_tx, result) in zip(transactions_to_send, results) {
        log!(DEBUG, "Sent transaction {signed_tx:?}: {result:?}");
        match result {
            Ok(SendRawTransactionResult::Ok) | Ok(SendRawTransactionResult::NonceTooLow) => {
                // In case of resubmission we may hit the case of SendRawTransactionResult::NonceTooLow
                // if the stuck transaction was mined in the meantime.
                // It will be cleaned-up once the transaction is finalized.
            }
            Ok(SendRawTransactionResult::InsufficientFunds)
            | Ok(SendRawTransactionResult::NonceTooHigh) => log!(
                INFO,
                "Failed to send transaction {signed_tx:?}: {result:?}. Will retry later.",
            ),
            Err(e) => {
                log!(
                    INFO,
                    "Failed to send transaction {signed_tx:?}: {e:?}. Will retry later."
                )
            }
        };
    }
}

async fn finalize_transactions_batch() {
    if read_state(|s| s.withdrawal_transactions.is_sent_tx_empty()) {
        return;
    }

    match finalized_transaction_count().await {
        Ok(finalized_tx_count) => {
            let txs_to_finalize = read_state(|s| {
                s.withdrawal_transactions
                    .sent_transactions_to_finalize(&finalized_tx_count)
            });

            let expected_finalized_withdrawal_ids: BTreeSet<_> =
                txs_to_finalize.values().cloned().collect();
            let rpc_client = read_state(RpcClient::from_state_all_providers);

            let results = join_all(
                txs_to_finalize
                    .keys()
                    .map(|hash| rpc_client.get_transaction_receipt(*hash)),
            )
            .await;
            let mut receipts: BTreeMap<LedgerBurnIndex, TransactionReceipt> = BTreeMap::new();
            for ((hash, withdrawal_id), result) in zip(txs_to_finalize, results) {
                match result {
                    Ok(Some(receipt)) => {
                        log!(DEBUG, "Received transaction receipt {receipt:?} for transaction {hash} and withdrawal ID {withdrawal_id}");
                        match receipts.get(&withdrawal_id) {
                            // by construction we never query twice the same transaction hash, which is a field in TransactionReceipt.
                            Some(existing_receipt) => {
                                log!(INFO, "ERROR: received different receipts for transaction {hash} with withdrawal ID {withdrawal_id}: {existing_receipt:?} and {receipt:?}. Will retry later");
                                return;
                            }
                            None => {
                                receipts.insert(withdrawal_id, receipt);
                            }
                        }
                    }
                    Ok(None) => {
                        log!(
                            DEBUG,
                            "Transaction {hash} for withdrawal ID {withdrawal_id} was not mined, it's probably a resubmitted transaction",
                        )
                    }
                    Err(e) => {
                        log!(
                            INFO,
                            "Failed to get transaction receipt for {hash} and withdrawal ID {withdrawal_id}: {e:?}. Will retry later",
                        );
                        return;
                    }
                }
            }
            let actual_finalized_withdrawal_ids: BTreeSet<_> = receipts.keys().cloned().collect();
            assert_eq!(
                expected_finalized_withdrawal_ids, actual_finalized_withdrawal_ids,
                "ERROR: unexpected transaction receipts for some withdrawal IDs"
            );
            for (withdrawal_id, transaction_receipt) in receipts {
                mutate_state(|s| {
                    process_event(
                        s,
                        EventType::FinalizedTransaction {
                            withdrawal_id,
                            transaction_receipt,
                        },
                    );
                });
            }
        }

        Err(e) => {
            log!(INFO, "Failed to get finalized transaction count: {e:?}");
        }
    }
}
async fn finalized_transaction_count() -> Result<TransactionCount, MultiCallError<TransactionCount>>
{
    let evm_netowrk = read_state(|s| s.evm_network());
    match evm_netowrk {
        EvmNetwork::Ethereum | EvmNetwork::Sepolia | EvmNetwork::BSC => {
            read_state(RpcClient::from_state_all_providers)
                .get_finalized_transaction_count(crate::state::minter_address().await)
                .await
        }
        _ => {
            read_state(RpcClient::from_state_all_providers)
                .get_latest_transaction_count(crate::state::minter_address().await)
                .await
        }
    }
}

pub fn estimate_gas_limit(withdrawal_request: &WithdrawalRequest) -> GasAmount {
    match withdrawal_request {
        WithdrawalRequest::Native(_) => NATIVE_WITHDRAWAL_TRANSACTION_GAS_LIMIT,
        WithdrawalRequest::Erc20(request) => {
            if request.is_wrapped_mint.unwrap_or_default() {
                ERC20_MINT_TRANSACTION_GAS_LIMIT
            } else {
                ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT
            }
        }
    }
}
