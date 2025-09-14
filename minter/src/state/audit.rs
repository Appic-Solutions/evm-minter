pub use super::event::{Event, EventType};
use super::{
    transactions::{Reimbursed, ReimbursementIndex},
    State,
};
use crate::{
    contract_logs::ReceivedContractEvent,
    storage::{record_event, with_event_iter},
};

/// Updates the state to reflect the given state transition.
// public because it's used in tests since process_event
// requires canister infrastructure to retrieve time
pub fn apply_state_transition(state: &mut State, payload: &EventType) {
    match payload {
        EventType::Init(init_arg) => {
            panic!("state re-initialization is not allowed: {init_arg:?}");
        }
        EventType::Upgrade(upgrade_arg) => {
            state
                .upgrade(upgrade_arg.clone())
                .expect("applying upgrade event should succeed");
        }
        EventType::AcceptedDeposit(native_event) => {
            state.record_contract_events(&native_event.clone().into());
        }
        EventType::AcceptedErc20Deposit(erc20_event) => {
            state.record_contract_events(&erc20_event.clone().into());
        }
        EventType::InvalidDeposit {
            event_source,
            reason,
        } => {
            let _ = state.record_invalid_event(*event_source, reason.clone());
        }
        EventType::MintedNative {
            event_source,
            mint_block_index,
        } => {
            state.record_successful_mint(
                *event_source,
                &state.native_symbol.to_string(),
                *mint_block_index,
                None,
            );
        }
        EventType::MintedErc20 {
            event_source,
            mint_block_index,
            erc20_token_symbol,
            erc20_contract_address,
        } => {
            state.record_successful_mint(
                *event_source,
                erc20_token_symbol,
                *mint_block_index,
                Some(*erc20_contract_address),
            );
        }
        EventType::SyncedToBlock { block_number } => {
            state.last_scraped_block_number = *block_number;
        }
        EventType::AcceptedNativeWithdrawalRequest(request) => {
            state.record_native_withdrawal_request(request.clone());
        }
        EventType::CreatedTransaction {
            withdrawal_id,
            transaction,
        } => {
            state
                .withdrawal_transactions
                .record_created_transaction(*withdrawal_id, transaction.clone());
        }
        EventType::SignedTransaction {
            withdrawal_id: _,
            transaction,
        } => {
            state
                .withdrawal_transactions
                .record_signed_transaction(transaction.clone());
        }
        EventType::ReplacedTransaction {
            withdrawal_id: _,
            transaction,
        } => {
            state
                .withdrawal_transactions
                .record_resubmit_transaction(transaction.clone());
        }
        EventType::FinalizedTransaction {
            withdrawal_id,
            transaction_receipt,
        } => {
            state.record_finalized_transaction(withdrawal_id, transaction_receipt);
        }
        EventType::ReimbursedNativeWithdrawal(Reimbursed {
            burn_in_block: withdrawal_id,
            reimbursed_in_block,
            reimbursed_amount: _,
            transaction_hash: _,
            transfer_fee: _,
        }) => {
            state
                .withdrawal_transactions
                .record_finalized_reimbursement(
                    ReimbursementIndex::Native {
                        ledger_burn_index: *withdrawal_id,
                    },
                    *reimbursed_in_block,
                    None,
                );
        }
        EventType::SkippedBlock { block_number } => {
            state.record_skipped_block(*block_number);
        }
        EventType::AddedErc20Token(erc20_token) => {
            state.record_add_erc20_token(erc20_token.clone());
        }
        EventType::AcceptedErc20WithdrawalRequest(request) => {
            state.record_erc20_withdrawal_request(request.clone())
        }
        EventType::ReimbursedErc20Withdrawal {
            native_ledger_burn_index,
            erc20_ledger_id,
            reimbursed,
        } => {
            state
                .withdrawal_transactions
                .record_finalized_reimbursement(
                    ReimbursementIndex::Erc20 {
                        native_ledger_burn_index: *native_ledger_burn_index,
                        ledger_id: *erc20_ledger_id,
                        erc20_ledger_burn_index: reimbursed.burn_in_block,
                    },
                    reimbursed.reimbursed_in_block,
                    None,
                );
        }
        EventType::FailedErc20WithdrawalRequest(native_reimbursement_request) => {
            state.withdrawal_transactions.record_reimbursement_request(
                ReimbursementIndex::Native {
                    ledger_burn_index: native_reimbursement_request.ledger_burn_index,
                },
                native_reimbursement_request.clone(),
            )
        }
        EventType::QuarantinedDeposit { event_source } => {
            state.record_quarantined_deposit(*event_source);
        }
        EventType::QuarantinedReimbursement { index } => {
            state
                .withdrawal_transactions
                .record_quarantined_reimbursement(index.clone());
        }
        EventType::AcceptedWrappedIcrcBurn(received_burn_event) => {
            state.record_contract_events(&received_burn_event.clone().into());
        }
        EventType::InvalidEvent {
            event_source,
            reason,
        } => {
            state.record_invalid_event(*event_source, reason.clone());
        }
        EventType::DeployedWrappedIcrcToken(received_wrapped_icrc_deployed_event) => {
            state.record_contract_events(&received_wrapped_icrc_deployed_event.clone().into());
        }
        EventType::QuarantinedRelease {
            event_source,
            release_event,
        } => {
            state.record_quarantined_release(
                *event_source,
                ReceivedContractEvent::WrappedIcrcBurn(release_event.clone()),
            );
        }
        EventType::ReleasedIcrcToken {
            event_source,
            release_block_index,
            released_icrc_token,
            wrapped_erc20_contract_address,
            transfer_fee,
        } => {
            state.record_successful_release(
                *event_source,
                *transfer_fee,
                *release_block_index,
                *wrapped_erc20_contract_address,
                *released_icrc_token,
            );
        }
        EventType::FailedIcrcLockRequest(native_reimbursement_request) => {
            state.withdrawal_transactions.record_reimbursement_request(
                ReimbursementIndex::Native {
                    ledger_burn_index: native_reimbursement_request.ledger_burn_index,
                },
                native_reimbursement_request.clone(),
            )
        }
        EventType::ReimbursedIcrcWrap {
            native_ledger_burn_index,
            reimbursed_icrc_token,
            reimbursed,
        } => {
            state
                .withdrawal_transactions
                .record_finalized_reimbursement(
                    ReimbursementIndex::IcrcWrap {
                        native_ledger_burn_index: *native_ledger_burn_index,
                        icrc_token: *reimbursed_icrc_token,
                        icrc_ledger_lock_index: reimbursed.burn_in_block,
                    },
                    reimbursed.reimbursed_in_block,
                    reimbursed.transfer_fee,
                );
        }
        EventType::AcceptedSwapActivationRequest(erc20_approve) => {
            state
                .withdrawal_transactions
                .record_withdrawal_request(erc20_approve.clone());
        }
        EventType::SwapContractActivated {
            swap_contract_address,
            usdc_contract_address,
            twin_usdc_ledger_id,
            twin_usdc_decimals,
            canister_signing_fee_twin_usdc_value,
        } => {
            state.activate_erc20_contract_address(
                (*usdc_contract_address, *twin_usdc_ledger_id),
                *swap_contract_address,
                *twin_usdc_decimals,
                *canister_signing_fee_twin_usdc_value,
            );
        }
        EventType::ReceivedSwapOrder(received_swap_event) => {
            state.record_contract_events(&received_swap_event.clone().into());
        }
        EventType::ReleasedGasFromGasTankWithUsdc {
            usdc_amount,
            gas_amount,
            swap_tx_id: _,
        } => state.release_gas_from_tank_with_usdc(*usdc_amount, *gas_amount),
        EventType::AcceptedSwapRequest(execute_swap_request) => {
            state.record_swap_request(execute_swap_request.clone())
        }
        EventType::QuarantinedDexOrder(dex_order_args) => {
            state.record_quarantined_dex_order(dex_order_args.clone())
        }
        EventType::QuarantinedSwapRequest(execute_swap_request) => {
            state
                .withdrawal_transactions
                .record_quarantined_swap_request(execute_swap_request.clone());
        }
        EventType::MintedToAppicDex {
            event_source,
            mint_block_index,
            minted_token,
            erc20_contract_address,
            tx_id,
        } => {
            state.record_successful_mint_to_dex(
                *event_source,
                *mint_block_index,
                *minted_token,
                *erc20_contract_address,
                tx_id.clone(),
            );
        }
        EventType::NotifiedSwapEventOrderToAppicDex {
            event_source,
            tx_id,
        } => {
            state.record_notified_swap_event_to_appic_dex(*event_source, tx_id.clone());
        }
        EventType::GasTankUpdate {
            usdc_withdrawn,
            native_deposited,
        } => {
            state.update_gas_tank_balance(*usdc_withdrawn, *native_deposited);
        }
    }
}

/// Records the given event payload in the event log and updates the state to reflect the change.
pub fn process_event(state: &mut State, payload: EventType) {
    apply_state_transition(state, &payload);
    record_event(payload);
}

/// Recomputes the minter state from the event log.
///
/// # Panics
///
/// This function panics if:
///   * The event log is empty.
///   * The first event in the log is not an Init event.
///   * One of the events in the log invalidates the minter's state invariants.
pub fn replay_events() -> State {
    with_event_iter(|iter| replay_events_internal(iter))
}

fn replay_events_internal<T: IntoIterator<Item = Event>>(events: T) -> State {
    let mut events_iter = events.into_iter();
    let mut state = match events_iter
        .next()
        .expect("the event log should not be empty")
    {
        Event {
            payload: EventType::Init(init_arg),
            ..
        } => State::try_from(init_arg).expect("state initialization should succeed"),
        other => panic!("the first event must be an Init event, got: {other:?}"),
    };
    for event in events_iter {
        apply_state_transition(&mut state, &event.payload);
    }
    state
}
