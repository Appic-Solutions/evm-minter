use candid::Nat;

use crate::numeric::wei_from_milli_ether;

mod retrieve_eth_guard {
    use crate::eth_types::Address;
    use crate::guard::tests::init_state;
    use crate::guard::{retrieve_withdraw_guard, GuardError, MAX_CONCURRENT, MAX_PENDING};
    use crate::numeric::{LedgerBurnIndex, Wei};
    use crate::state::mutate_state;
    use crate::state::transactions::{NativeWithdrawalRequest, WithdrawalRequest};
    use candid::Principal;

    #[test]
    fn should_error_on_reentrant_principal() {
        init_state();
        let principal = principal_with_id(1);
        let _guard = retrieve_withdraw_guard(principal).unwrap();

        assert_eq!(
            retrieve_withdraw_guard(principal),
            Err(GuardError::AlreadyProcessing)
        )
    }

    #[test]
    fn should_allow_reentrant_principal_after_drop() {
        init_state();
        let principal = principal_with_id(1);
        {
            let _guard = retrieve_withdraw_guard(principal).unwrap();
        }

        assert!(retrieve_withdraw_guard(principal).is_ok());
    }

    #[test]
    fn should_allow_limited_number_of_principals() {
        init_state();
        let mut guards: Vec<_> = (0..MAX_CONCURRENT)
            .map(|i| retrieve_withdraw_guard(principal_with_id(i as u64)).unwrap())
            .collect();

        for additional_principal in MAX_CONCURRENT..2 * MAX_CONCURRENT {
            assert_eq!(
                retrieve_withdraw_guard(principal_with_id(additional_principal as u64)),
                Err(GuardError::TooManyConcurrentRequests)
            );
        }

        {
            let _guard = guards.pop().expect("should have at least one guard");
        }
        assert!(retrieve_withdraw_guard(principal_with_id(MAX_CONCURRENT as u64)).is_ok());
    }

    #[test]
    fn should_allow_limited_number_of_pending_requests() {
        init_state();
        for i in 0..MAX_PENDING {
            let _guard = retrieve_withdraw_guard(principal_with_id(i as u64)).unwrap();
            record_withdrawal_request(LedgerBurnIndex::new(i as u64));
        }

        for additional_principal in MAX_PENDING..2 * MAX_PENDING {
            assert_eq!(
                retrieve_withdraw_guard(principal_with_id(additional_principal as u64)),
                Err(GuardError::TooManyPendingRequests)
            );
        }

        fn record_withdrawal_request(ledger_burn_index: LedgerBurnIndex) {
            mutate_state(|s| {
                s.withdrawal_transactions
                    .record_withdrawal_request(WithdrawalRequest::Native(NativeWithdrawalRequest {
                        withdrawal_amount: Wei::ONE,
                        destination: Address::ZERO,
                        ledger_burn_index,
                        from: Principal::anonymous(),
                        from_subaccount: None,
                        created_at: None,
                        l1_fee: None,
                        withdrawal_fee: None,
                    }))
            })
        }
    }

    fn principal_with_id(id: u64) -> Principal {
        Principal::try_from_slice(&id.to_le_bytes()).unwrap()
    }
}

mod timer_guard {
    use crate::guard::tests::init_state;
    use crate::guard::{TimerGuard, TimerGuardError};
    use crate::state::TaskType;
    use strum::IntoEnumIterator;

    #[test]
    fn should_prevent_concurrent_access() {
        for task_type in TaskType::iter() {
            init_state();
            let _guard = TimerGuard::new(task_type).expect("can retrieve timer guard");

            assert_eq!(
                TimerGuard::new(task_type),
                Err(TimerGuardError::AlreadyProcessing)
            );
        }
    }

    #[test]
    fn should_allow_access_when_guard_dropped() {
        for task_type in TaskType::iter() {
            init_state();
            let _guard = TimerGuard::new(task_type).expect("can retrieve timer guard");

            drop(_guard);

            assert!(TimerGuard::new(task_type).is_ok());
        }
    }

    #[test]
    fn should_be_able_to_get_all_timer_guards() {
        init_state();
        let mut guards = Vec::new();

        for task_type in TaskType::iter() {
            guards.push(TimerGuard::new(task_type).expect("can retrieve timer guard"));
        }
    }
}

fn init_state() {
    use crate::lifecycle::InitArg;
    use crate::state::State;
    use candid::Principal;
    crate::state::STATE.with(|s| {
        *s.borrow_mut() = Some(
            State::try_from(InitArg {
                evm_network: crate::evm_config::EvmNetwork::BSC,
                ecdsa_key_name: "test_key_1".to_string(),
                helper_contract_address: None,
                native_ledger_id: Principal::from_text("apia6-jaaaa-aaaar-qabma-cai")
                    .expect("BUG: invalid principal"),
                native_index_id: Principal::from_text("eysav-tyaaa-aaaap-akqfq-cai")
                    .expect("BUG: invalid principal"),
                block_height: Default::default(),
                native_minimum_withdrawal_amount: wei_from_milli_ether(10).into(),
                next_transaction_nonce: Default::default(),
                last_scraped_block_number: Default::default(),
                native_symbol: "IcBNB".to_string(),
                native_ledger_transfer_fee: candid::Nat::from(1000_u128),
                min_max_priority_fee_per_gas: candid::Nat::from(1000000_u128),
                ledger_suite_manager_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai")
                    .expect("BUG: invalid principal"),
                deposit_native_fee: Nat::from(0_u64),
                withdrawal_native_fee: wei_from_milli_ether(1).into(),
            })
            .expect("init args should be valid"),
        );
    });
}
