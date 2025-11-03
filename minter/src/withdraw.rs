use crate::evm_config::EvmNetwork;
use crate::guard::TimerGuard;
use crate::icrc_client::runtime::IcrcBoundedRuntime;
use crate::logs::{DEBUG, INFO};
use crate::numeric::{
    Erc20TokenAmount, Erc20Value, GasAmount, LedgerBurnIndex, LedgerMintIndex, Wei,
};
use crate::rpc_client::providers::Provider;
use crate::rpc_client::{MultiCallError, RpcClient};
use crate::rpc_declarations::{SendRawTransactionResult, TransactionReceipt};
use crate::state::audit::{process_event, EventType};
use crate::state::balances::release_gas_from_tank_with_usdc;
use crate::state::transactions::{
    create_transaction, CreateTransactionError, ExecuteSwapRequest, Reimbursed, ReimbursementIndex,
    ReimbursementRequest, WithdrawalRequest,
};
use crate::state::{mutate_state, State, TaskType};
use crate::swap::build_dex_swap_refund_request;
use crate::tx::gas_fees::{lazy_refresh_gas_fee_estimate, GasFeeEstimate, DEFAULT_L1_BASE_GAS_FEE};
use crate::tx::gas_usd::MaxFeeUsd;
use crate::{numeric::TransactionCount, state::read_state};
use candid::Nat;
use futures::future::join_all;
use ic_canister_log::log;
use icrc_ledger_client::ICRC1Client;
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

pub const ERC20_APPROVAL_TRANSACTION_GAS_LIMIT: GasAmount = GasAmount::new(70_000);

// used for mining wrapped icrc transactions
pub const ERC20_MINT_TRANSACTION_GAS_LIMIT: GasAmount = GasAmount::new(100_000);

pub const REFUND_FAILED_SWAP_GAS_LIMIT: GasAmount = GasAmount::new(120_000);

// the deadline is valid for 20 years and it is used for the the failed swaps that will be
// converted to usdc transfer
pub const UNLIMITED_DEADLINE: Erc20Value = Erc20Value::new(2388441600);

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
            runtime: IcrcBoundedRuntime,
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
            fee: transfer_fee.map(Nat::from),
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

async fn process_failed_swaps(gas_fee_estimate: GasFeeEstimate) {
    if read_state(|s| {
        (s.withdrawal_transactions.is_failed_swaps_requests_empty()
            && s.quarantined_dex_orders.is_empty())
            || !s.is_swapping_active
    }) {
        return;
    }

    let (
        evm_network,
        last_native_token_usd_price_estimate,
        twin_usdc_info,
        signing_fee,
        swap_contract_address,
        dex_canister_id,
    ) = read_state(|s| {
        (
            s.evm_network(),
            s.last_native_token_usd_price_estimate,
            s.twin_usdc_info.clone(),
            s.canister_signing_fee_twin_usdc_amount,
            s.swap_contract_address,
            s.dex_canister_id,
        )
    });

    let twin_usdc_info =
        twin_usdc_info.expect("BUG: twin USDC info should be available if swapping is active");

    let last_native_token_usd_price_estimate = last_native_token_usd_price_estimate
        .expect("BUG: native token USD price should be available if swapping is active");

    let signing_fee = signing_fee.expect(
        "BUG: canister signing fee twin USDC amount should be available if swapping is active",
    );

    let swap_contract_address = swap_contract_address
        .expect("BUG: swap contract address should be available if swapping is active");

    let dex_canister_id =
        dex_canister_id.expect("BUG: dex canister id should be available if swapping is active");

    let erc20_tx_fee = gas_fee_estimate
        .to_price(REFUND_FAILED_SWAP_GAS_LIMIT)
        .max_transaction_fee();

    let l1_fee = match evm_network {
        EvmNetwork::Base => Some(DEFAULT_L1_BASE_GAS_FEE),
        _ => None,
    };

    let fee_to_be_deducted = erc20_tx_fee
        .checked_add(l1_fee.unwrap_or(Wei::ZERO))
        .expect("Bug: Tx_fee plus l1_fee should fit in u256");

    let max_gas_fee_twin_usdc = match MaxFeeUsd::twin_usdc_from_native_wei(
        fee_to_be_deducted,
        last_native_token_usd_price_estimate.1,
        twin_usdc_info.decimals,
    ) {
        Ok(usdc_amount) => usdc_amount,
        Err(_) => return,
    };

    let all_twin_usdc_fees = max_gas_fee_twin_usdc
        .checked_add(signing_fee)
        .unwrap_or(Erc20Value::MAX);

    // process failed swaps
    for (_previous_native_ledger_burn_index, request) in
        read_state(|s| s.withdrawal_transactions.failed_swap_requests())
    {
        let amount_in = request
            .erc20_amount_in
            .checked_sub(all_twin_usdc_fees)
            .unwrap_or(Erc20Value::ZERO);

        if amount_in == Erc20Value::ZERO {
            log!(
                INFO,
                "[create_refund_swap_erquest]: Failed to create refund swap request the request will be Quarantined with swap tx id {:?}",
                request.swap_tx_id
            );

            mutate_state(|s| process_event(s, EventType::QuarantinedSwapRequest(request.clone())));
            continue;
        }

        let native_ledger_burn_index = match release_gas_from_tank_with_usdc(
            all_twin_usdc_fees,
            fee_to_be_deducted,
            request.swap_tx_id.clone(),
        ) {
            Ok(native_ledger_burn_index) => native_ledger_burn_index,
            Err(err) => {
                log!(
                    INFO,
                    "[create_refund_swap_erquest]: Failed to release gas from gas tank low balance {:?}",
                    err
                );
                continue;
            }
        };

        let now = ic_cdk::api::time();

        let request = ExecuteSwapRequest {
            max_transaction_fee: erc20_tx_fee,
            native_ledger_burn_index,
            erc20_ledger_id: twin_usdc_info.ledger_id,
            erc20_ledger_burn_index: request.erc20_ledger_burn_index,
            from: request.from,
            from_subaccount: None,
            created_at: now,
            l1_fee,
            withdrawal_fee: None,
            swap_tx_id: request.swap_tx_id,
            erc20_token_in: twin_usdc_info.address,
            erc20_amount_in: amount_in,
            min_amount_out: amount_in,
            recipient: request.recipient,
            deadline: UNLIMITED_DEADLINE,
            commands: vec![],
            commands_data: vec![],
            swap_contract: swap_contract_address,
            gas_estimate: REFUND_FAILED_SWAP_GAS_LIMIT,
            is_refund: true,
        };

        log!(
            INFO,
            "[create_refund_swap_erquest]: Successfully created the refund request for swap {:?} with request {:?}",
            request.swap_tx_id,
            request
        );

        mutate_state(|s| process_event(s, EventType::AcceptedSwapRequest(request)))
    }

    // process quarantined_dex_orders
    for (tx_id, dex_order) in read_state(|s| s.quarantined_dex_orders.clone().into_iter()) {
        log!(
            INFO,
            "[dex_order]: Building swap request for Quarantined Dex Order with tx_id: {:?}",
            tx_id
        );
        if let Ok(refund_swap_request) = build_dex_swap_refund_request(
            &dex_order,
            &twin_usdc_info,
            last_native_token_usd_price_estimate.1,
            signing_fee,
            evm_network,
            dex_canister_id,
            swap_contract_address,
        )
        .await
        {
            log!(
                INFO,
                "[dex_order]: Successfully built refund request for tx_id: {:?} {:?}",
                tx_id,
                refund_swap_request
            );
            mutate_state(|s| process_event(s, EventType::AcceptedSwapRequest(refund_swap_request)));
        };
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

    if read_state(|s| {
        !s.withdrawal_transactions.has_pending_requests() && s.quarantined_dex_orders.is_empty()
    }) {
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
    create_transactions_batch(gas_fee_estimate.clone());
    sign_transactions_batch().await;
    send_transactions_batch(latest_transaction_count).await;
    finalize_transactions_batch().await;
    process_failed_swaps(gas_fee_estimate).await;

    if read_state(|s| s.withdrawal_transactions.has_pending_requests()) {
        ic_cdk_timers::set_timer(
            crate::PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_RETRY_INTERVAL,
            || ic_cdk::futures::spawn_017_compat(process_retrieve_tokens_requests()),
        );
    }
}

async fn latest_transaction_count() -> Option<TransactionCount> {
    match read_state(|s| RpcClient::from_state_custom_providers(s, vec![Provider::Alchemy]))
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
    let rpc_client =
        read_state(|s| RpcClient::from_state_custom_providers(s, vec![Provider::Alchemy]));
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
            let rpc_client =
                read_state(|s| RpcClient::from_state_custom_providers(s, vec![Provider::Alchemy]));

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
        EvmNetwork::Polygon => {
            read_state(|s| RpcClient::from_state_custom_providers(s, vec![Provider::Alchemy]))
                .get_finalized_transaction_count(crate::state::minter_address().await)
                .await
        }
        _ => {
            read_state(|s| RpcClient::from_state_custom_providers(s, vec![Provider::Alchemy]))
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
        WithdrawalRequest::Erc20Approve(_) => ERC20_APPROVAL_TRANSACTION_GAS_LIMIT,
        WithdrawalRequest::Swap(request) => request.gas_estimate,
    }
}
