use crate::candid_types::CandidBlockTag;
use crate::contract_logs::types::{ReceivedErc20Event, ReceivedNativeEvent};
use crate::contract_logs::{EventSource, LedgerSubaccount};
use crate::erc20::ERC20TokenSymbol;
use crate::eth_types::Address;
use crate::evm_config::EvmNetwork;
use crate::lifecycle::InitArg;
use crate::lifecycle::UpgradeArg;
use crate::map::DedupMultiKeyMap;
use crate::numeric::{
    wei_from_milli_ether, BlockNumber, Erc20TokenAmount, Erc20Value, GasAmount, LedgerBurnIndex,
    LedgerMintIndex, LogIndex, TransactionNonce, Wei, WeiPerGas,
};
use crate::rpc_declarations::BlockTag;
use crate::rpc_declarations::{TransactionReceipt, TransactionStatus};
use crate::state::audit::apply_state_transition;
use crate::state::event::{Event, EventType};
use crate::state::transactions::{Erc20WithdrawalRequest, ReimbursementIndex};
use crate::state::{Erc20Balances, State};
use crate::test_fixtures::arb::{arb_address, arb_checked_amount_of, arb_hash};
use crate::tx::gas_fees::GasFeeEstimate;
use crate::tx::{
    AccessList, AccessListItem, Eip1559Signature, Eip1559TransactionRequest, ResubmissionStrategy,
    SignedEip1559TransactionRequest, StorageKey,
};
use candid::{Nat, Principal};
use ethnum::u256;
use ic_management_canister_types::EcdsaPublicKeyResult;
use proptest::array::uniform32;
use proptest::collection::vec as pvec;
use proptest::prelude::*;
use std::collections::BTreeMap;

pub fn initial_state() -> State {
    State::try_from(InitArg {
        evm_network: Default::default(),
        ecdsa_key_name: "test_key_1".to_string(),
        helper_contract_address: None,
        native_ledger_id: Principal::from_text("apia6-jaaaa-aaaar-qabma-cai")
            .expect("BUG: invalid principal"),
        native_index_id: Principal::from_text("eysav-tyaaa-aaaap-akqfq-cai")
            .expect("BUG: invalid principal"),
        block_height: Default::default(),
        native_minimum_withdrawal_amount: wei_from_milli_ether(20).into(),
        next_transaction_nonce: Default::default(),
        last_scraped_block_number: Default::default(),
        native_symbol: "icMatic".to_string(),
        native_ledger_transfer_fee: wei_from_milli_ether(10).into(),
        min_max_priority_fee_per_gas: Wei::new(15000).into(),
        ledger_suite_manager_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai")
            .expect("BUG: invalid principal"),
        deposit_native_fee: wei_from_milli_ether(1).into(),
        withdrawal_native_fee: 5_000_000_u128.into(),
    })
    .expect("init args should be valid")
}

mod mint_transaction {
    use crate::contract_logs::types::ReceivedNativeEvent;
    use crate::contract_logs::EventSourceError;
    use crate::erc20::ERC20Token;
    use crate::evm_config::EvmNetwork;
    use crate::numeric::{LedgerMintIndex, LogIndex};
    use crate::state::tests::{initial_state, received_deposit_event, received_erc20_event};
    use crate::state::{InvalidEventReason, MintedEvent};

    #[test]
    fn should_record_mint_task_from_event() {
        let mut state = initial_state();
        let event = received_deposit_event();

        state.record_contract_events(&event.clone().into());

        assert!(state.events_to_mint.contains_key(&event.source()));

        let block_index = LedgerMintIndex::new(1u64);

        let minted_event = MintedEvent {
            event: event.clone().into(),
            mint_block_index: block_index,
            token_symbol: "icETH".to_string(),
            erc20_contract_address: None,
        };

        state.record_successful_mint(event.source(), "icETH", block_index, None);

        assert!(!state.events_to_mint.contains_key(&event.source()));
        assert_eq!(
            state.minted_events.get(&event.source()),
            Some(&minted_event)
        );
    }

    #[test]
    fn should_record_erc20_mint_task_from_event() {
        let mut state = initial_state();
        state.evm_network = EvmNetwork::Sepolia;
        let token: ERC20Token = super::erc20::record_add_erc20_token::ic_usdc();
        state.record_add_erc20_token(token.clone());

        let erc20_contract_address = token.erc20_contract_address;
        let mut event = received_erc20_event();
        event.erc20_contract_address = erc20_contract_address;

        state.record_contract_events(&event.clone().into());

        assert!(state.events_to_mint.contains_key(&event.source()));

        let block_index = LedgerMintIndex::new(1u64);

        let minted_event = MintedEvent {
            event: event.clone().into(),
            mint_block_index: block_index,
            token_symbol: token.erc20_token_symbol.to_string(),
            erc20_contract_address: Some(erc20_contract_address),
        };

        state.record_successful_mint(
            event.source(),
            &token.erc20_token_symbol.to_string(),
            block_index,
            Some(token.erc20_contract_address),
        );

        assert!(!state.events_to_mint.contains_key(&event.source()));
        assert_eq!(
            state.minted_events.get(&event.source()),
            Some(&minted_event)
        );
    }

    #[test]
    fn should_allow_minting_events_with_equal_txhash() {
        let mut state = initial_state();
        let event_1 = ReceivedNativeEvent {
            log_index: LogIndex::from(1u8),
            ..received_deposit_event()
        }
        .into();
        let event_2 = ReceivedNativeEvent {
            log_index: LogIndex::from(2u8),
            ..received_deposit_event()
        }
        .into();

        assert_ne!(event_1, event_2);

        state.record_contract_events(&event_1);

        assert!(state.events_to_mint.contains_key(&event_1.source()));

        state.record_contract_events(&event_2);

        assert!(state.events_to_mint.contains_key(&event_2.source()));

        assert_eq!(2, state.events_to_mint.len());
    }

    #[test]
    #[should_panic = "unknown event"]
    fn should_not_allow_unknown_mints() {
        let mut state = initial_state();
        let event = received_deposit_event();

        assert!(!state.events_to_mint.contains_key(&event.source()));
        state.record_successful_mint(event.source(), "icETH", LedgerMintIndex::new(1), None);
    }

    #[test]
    #[should_panic = "invalid"]
    fn should_not_record_invalid_event_already_recorded_as_valid() {
        let mut state = initial_state();
        let event = received_deposit_event().into();

        state.record_contract_events(&event);

        assert!(state.events_to_mint.contains_key(&event.source()));

        state.record_invalid_event(
            event.source(),
            EventSourceError::InvalidEvent("bad".to_string()).to_string(),
        );
    }

    #[test]
    fn should_not_update_already_recorded_invalid_deposit() {
        let mut state = initial_state();
        let event = received_deposit_event();
        let error = EventSourceError::InvalidEvent("first".to_string());
        let other_error = EventSourceError::InvalidEvent("second".to_string());
        assert_ne!(error, other_error);

        assert!(state.record_invalid_event(event.source(), error.to_string()));
        assert_eq!(
            state.invalid_events[&event.source()],
            InvalidEventReason::InvalidEvent(error.to_string())
        );

        assert!(!state.record_invalid_event(event.source(), other_error.to_string()));
        assert_eq!(
            state.invalid_events[&event.source()],
            InvalidEventReason::InvalidEvent(error.to_string())
        );
    }

    #[test]
    fn should_quarantine_deposit() {
        let mut state = initial_state();
        let event = received_deposit_event();
        state.record_contract_events(&event.clone().into());
        assert_eq!(state.events_to_mint.len(), 1);

        state.record_quarantined_deposit(event.source());

        assert!(state.events_to_mint.is_empty());
        assert!(state.invalid_events.contains_key(&event.source()));
    }

    #[test]
    fn should_have_readable_eth_debug_representation() {
        let expected = "ReceivedNativeEvent { \
          transaction_hash: 0xf1ac37d920fa57d9caeebc7136fea591191250309ffca95ae0e8a7739de89cc2, \
          block_number: 3_960_623, \
          log_index: 29, \
          from_address: 0xdd2851Cdd40aE6536831558DD46db62fAc7A844d, \
          value: 10_000_000_000_000_000, \
          principal: k2t6j-2nvnp-4zjm3-25dtz-6xhaa-c7boj-5gayf-oj3xs-i43lp-teztq-6ae, \
          subaccount: None \
        }";
        assert_eq!(format!("{:?}", received_deposit_event()), expected);
    }

    #[test]
    fn should_have_readable_erc20_debug_representation() {
        let expected = "ReceivedErc20Event { \
          transaction_hash: 0xd9335910102c08a9dc16f8cc1a42a0bf8ca93666d11dc3194c6ee1bd30d19686, \
          block_number: 5_539_903, \
          log_index: 87, \
          from_address: 0xdd2851Cdd40aE6536831558DD46db62fAc7A844d, \
          value: 5_000_000, \
          principal: hkroy-sm7vs-yyjs7-ekppe-qqnwx-hm4zf-n7ybs-titsi-k6e3k-ucuiu-uqe, \
          contract_address: 0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238, \
          subaccount: None \
        }";
        assert_eq!(format!("{:?}", received_erc20_event()), expected);
    }
}

fn received_deposit_event() -> ReceivedNativeEvent {
    ReceivedNativeEvent {
        transaction_hash: "0xf1ac37d920fa57d9caeebc7136fea591191250309ffca95ae0e8a7739de89cc2"
            .parse()
            .unwrap(),
        block_number: BlockNumber::new(3960623u128),
        log_index: LogIndex::from(29u8),
        from_address: "0xdd2851cdd40ae6536831558dd46db62fac7a844d"
            .parse()
            .unwrap(),
        value: Wei::from(10_000_000_000_000_000_u128),
        principal: "k2t6j-2nvnp-4zjm3-25dtz-6xhaa-c7boj-5gayf-oj3xs-i43lp-teztq-6ae"
            .parse()
            .unwrap(),

        subaccount: None,
    }
}

// https://sepolia.etherscan.io/tx/0xd9335910102c08a9dc16f8cc1a42a0bf8ca93666d11dc3194c6ee1bd30d19686
fn received_erc20_event() -> ReceivedErc20Event {
    ReceivedErc20Event {
        transaction_hash: "0xd9335910102c08a9dc16f8cc1a42a0bf8ca93666d11dc3194c6ee1bd30d19686"
            .parse()
            .unwrap(),
        block_number: BlockNumber::new(5539903),
        log_index: LogIndex::from(0x57_u32),
        from_address: "0xdd2851Cdd40aE6536831558DD46db62fAc7A844d"
            .parse()
            .unwrap(),
        value: Erc20Value::from(5_000_000_u64),
        principal: "hkroy-sm7vs-yyjs7-ekppe-qqnwx-hm4zf-n7ybs-titsi-k6e3k-ucuiu-uqe"
            .parse()
            .unwrap(),
        erc20_contract_address: "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238"
            .parse()
            .unwrap(),
        subaccount: None,
    }
}

mod upgrade {
    use crate::eth_types::Address;
    use crate::evm_config::EvmNetwork;
    use crate::lifecycle::UpgradeArg;
    use crate::numeric::{TransactionNonce, Wei};
    use crate::rpc_declarations::BlockTag;
    use crate::state::tests::initial_state;
    use crate::state::InvalidStateError;
    use assert_matches::assert_matches;
    use candid::Nat;
    use num_bigint::BigUint;
    use std::str::FromStr;

    #[test]
    fn should_fail_when_upgrade_args_invalid() {
        let mut state = initial_state();
        assert_matches!(
            state.upgrade(UpgradeArg {
                next_transaction_nonce: Some(Nat(BigUint::from_bytes_be(
                    &ethnum::u256::MAX.to_be_bytes(),
                ) + 1_u8)),
                ..Default::default()
            }),
            Err(InvalidStateError::InvalidTransactionNonce(_))
        );

        let mut state = initial_state();
        assert_matches!(
            state.upgrade(UpgradeArg {
                native_minimum_withdrawal_amount: Some(Nat::from(0_u8)),
                ..Default::default()
            }),
            Err(InvalidStateError::InvalidMinimumWithdrawalAmount(_))
        );

        let mut state = initial_state();
        state.evm_network = EvmNetwork::Sepolia;
        assert_matches!(
            state.upgrade(UpgradeArg {
                native_minimum_withdrawal_amount: Some(Nat::from(2_000_000_000_000_u64 - 1)),
                ..Default::default()
            }),
            Err(InvalidStateError::InvalidMinimumWithdrawalAmount(_))
        );

        let mut state = initial_state();
        state.evm_network = EvmNetwork::Sepolia;
        assert_matches!(
            state.upgrade(UpgradeArg {
                native_minimum_withdrawal_amount: Some(Nat::from(10_000_000_000_u64 - 1)),
                ..Default::default()
            }),
            Err(InvalidStateError::InvalidMinimumWithdrawalAmount(_))
        );

        let mut state = initial_state();
        assert_matches!(
            state.upgrade(UpgradeArg {
                helper_contract_address: Some("invalid".to_string()),
                ..Default::default()
            }),
            Err(InvalidStateError::InvalidHelperContractAddress(_))
        );

        let mut state = initial_state();
        assert_matches!(
            state.upgrade(UpgradeArg {
                helper_contract_address: Some(
                    "0x0000000000000000000000000000000000000000".to_string(),
                ),
                ..Default::default()
            }),
            Err(InvalidStateError::InvalidHelperContractAddress(_))
        );
    }

    #[test]
    fn should_succeed() {
        use crate::candid_types::CandidBlockTag;
        let mut state = initial_state();
        let upgrade_arg = UpgradeArg {
            next_transaction_nonce: Some(Nat::from(15_u8)),
            native_minimum_withdrawal_amount: Some(Nat::from(20_000_000_000_000_000_u64)),
            helper_contract_address: Some("0xb44B5e756A894775FC32EDdf3314Bb1B1944dC34".to_string()),
            block_height: Some(CandidBlockTag::Safe),
            native_ledger_transfer_fee: Some(Nat::from(10_000_000_000_000_000_u64)),
            ..Default::default()
        };

        state.upgrade(upgrade_arg).expect("valid upgrade args");

        assert_eq!(
            state.withdrawal_transactions.next_transaction_nonce(),
            TransactionNonce::from(15_u64)
        );
        assert_eq!(
            state.native_minimum_withdrawal_amount,
            Wei::new(20_000_000_000_000_000)
        );
        assert_eq!(
            state.helper_contract_addresses,
            Some(vec![Address::from_str(
                "0xb44B5e756A894775FC32EDdf3314Bb1B1944dC34"
            )
            .unwrap()])
        );
        assert_eq!(state.block_height, BlockTag::Safe);
    }
}

mod erc20 {
    pub mod record_add_erc20_token {
        use crate::erc20::ERC20Token;
        use crate::evm_config::EvmNetwork;
        use crate::state::tests::initial_state;
        use crate::test_fixtures::expect_panic_with_message;

        #[test]
        fn should_panic_when_evm_network_mismatch() {
            let mut state = initial_state();
            state.evm_network = EvmNetwork::Ethereum;

            expect_panic_with_message(
                || state.record_add_erc20_token(ic_usdc()),
                "ERROR: Expected Ethereum",
            );
        }

        #[test]
        fn should_record_ckerc20_token() {
            let mut state = initial_state();
            state.evm_network = EvmNetwork::Sepolia;
            let ckerc20 = ic_usdc();

            state.record_add_erc20_token(ckerc20.clone());

            assert_eq!(
                state.supported_erc20_tokens().collect::<Vec<_>>(),
                vec![ERC20Token {
                    chain_id: EvmNetwork::Sepolia,
                    erc20_contract_address: ckerc20.erc20_contract_address,
                    erc20_token_symbol: ckerc20.erc20_token_symbol,
                    erc20_ledger_id: ckerc20.erc20_ledger_id,
                }]
            );
        }

        #[test]
        fn should_panic_when_duplicate_ledger_id() {
            let mut state = initial_state();
            state.evm_network = EvmNetwork::Sepolia;
            let icusdc = ic_usdc();
            state.record_add_erc20_token(icusdc.clone());

            let ckusdt_with_wrong_ledger_id = ERC20Token {
                erc20_ledger_id: icusdc.erc20_ledger_id,
                ..ic_usdt()
            };
            expect_panic_with_message(
                || state.record_add_erc20_token(ckusdt_with_wrong_ledger_id),
                "same ERC20 ledger ID",
            );
        }

        #[test]
        fn should_panic_when_duplicate_erc20_smart_contract_address() {
            let mut state = initial_state();
            state.evm_network = EvmNetwork::Sepolia;
            let icusdc = ic_usdc();
            state.record_add_erc20_token(icusdc.clone());

            let ckusdt_with_wrong_address = ERC20Token {
                erc20_contract_address: icusdc.erc20_contract_address,
                ..ic_usdt()
            };
            expect_panic_with_message(
                || state.record_add_erc20_token(ckusdt_with_wrong_address),
                "ERC-20 address",
            );
        }

        #[test]
        fn should_panic_when_duplicate_erc20_token_symbol() {
            let mut state = initial_state();
            state.evm_network = EvmNetwork::Sepolia;
            let icusdc = ic_usdc();
            state.record_add_erc20_token(icusdc.clone());

            let ckusdt_with_wrong_symbol = ERC20Token {
                erc20_token_symbol: icusdc.erc20_token_symbol,
                ..ic_usdt()
            };
            expect_panic_with_message(
                || state.record_add_erc20_token(ckusdt_with_wrong_symbol),
                "symbol",
            );
        }

        pub fn ic_usdc() -> ERC20Token {
            ERC20Token {
                chain_id: EvmNetwork::Sepolia,
                erc20_contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse()
                    .unwrap(),
                erc20_token_symbol: "icUSDC".parse().unwrap(),
                erc20_ledger_id: "mxzaz-hqaaa-aaaar-qaada-cai".parse().unwrap(),
            }
        }

        pub fn ic_usdt() -> ERC20Token {
            ERC20Token {
                chain_id: EvmNetwork::Sepolia,
                erc20_contract_address: "0xdac17f958d2ee523a2206206994597c13d831ec7"
                    .parse()
                    .unwrap(),
                erc20_token_symbol: "ckUSDT".parse().unwrap(),
                erc20_ledger_id: "nbsys-saaaa-aaaar-qaaga-cai".parse().unwrap(),
            }
        }
    }
}

fn arb_principal() -> impl Strategy<Value = Principal> {
    pvec(any::<u8>(), 0..=29).prop_map(|bytes| Principal::from_slice(&bytes))
}

fn arb_ledger_subaccount() -> impl Strategy<Value = Option<LedgerSubaccount>> {
    uniform32(any::<u8>()).prop_map(LedgerSubaccount::from_bytes)
}

fn arb_u256() -> impl Strategy<Value = u256> {
    uniform32(any::<u8>()).prop_map(u256::from_be_bytes)
}

fn arb_event_source() -> impl Strategy<Value = EventSource> {
    (arb_hash(), arb_checked_amount_of()).prop_map(|(transaction_hash, log_index)| EventSource {
        transaction_hash,
        log_index,
    })
}

fn arb_block_tag() -> impl Strategy<Value = CandidBlockTag> {
    prop_oneof![
        Just(CandidBlockTag::Safe),
        Just(CandidBlockTag::Latest),
        Just(CandidBlockTag::Finalized),
    ]
}

fn arb_nat() -> impl Strategy<Value = Nat> {
    any::<u128>().prop_map(Nat::from)
}

fn arb_storage_key() -> impl Strategy<Value = StorageKey> {
    uniform32(any::<u8>()).prop_map(StorageKey)
}

fn arb_access_list_item() -> impl Strategy<Value = AccessListItem> {
    (arb_address(), pvec(arb_storage_key(), 0..100)).prop_map(|(address, storage_keys)| {
        AccessListItem {
            address,
            storage_keys,
        }
    })
}

fn arb_access_list() -> impl Strategy<Value = AccessList> {
    pvec(arb_access_list_item(), 0..100).prop_map(AccessList)
}

prop_compose! {
    fn arb_init_arg()(
        contract_address in proptest::option::of(arb_address()),
        block_height in arb_block_tag(),
        native_minimum_withdrawal_amount in arb_nat(),
        next_transaction_nonce in arb_nat(),
        native_ledger_id in arb_principal(),
        native_index_id in arb_principal(),
        ecdsa_key_name in "[a-z_]*",
        last_scraped_block_number in arb_nat(),
        min_max_priority_fee_per_gas in arb_nat(),
        native_ledger_transfer_fee in arb_nat(),
        native_symbol in "[a-z_]*",
        ledger_suite_manager_id in arb_principal(),
        deposit_native_fee in arb_nat(),
        withdrawal_native_fee in arb_nat()

    ) -> InitArg {
        InitArg {evm_network:EvmNetwork::Sepolia,ecdsa_key_name,helper_contract_address:contract_address.map(|addr|addr.to_string()),native_ledger_id,native_index_id,block_height,native_minimum_withdrawal_amount,next_transaction_nonce,last_scraped_block_number,min_max_priority_fee_per_gas,native_ledger_transfer_fee,native_symbol,ledger_suite_manager_id, deposit_native_fee, withdrawal_native_fee }
    }
}

prop_compose! {
    fn arb_upgrade_arg()(
        contract_address in proptest::option::of(arb_address()),
        block_height in proptest::option::of(arb_block_tag()),
        native_minimum_withdrawal_amount in proptest::option::of(arb_nat()),
        next_transaction_nonce in proptest::option::of(arb_nat()),
        last_scraped_block_number in proptest::option::of(arb_nat()),
        evm_rpc_id in proptest::option::of(arb_principal()),
        native_ledger_transfer_fee in proptest::option::of(arb_nat()),
        min_max_priority_fee_per_gas in proptest::option::of(arb_nat()),
        deposit_native_fee in proptest::option::of(arb_nat()),
        withdrawal_native_fee in proptest::option::of(arb_nat())
    ) -> UpgradeArg {
        UpgradeArg {helper_contract_address:contract_address.map(|addr|addr.to_string()),block_height,native_minimum_withdrawal_amount,next_transaction_nonce,last_scraped_block_number,evm_rpc_id,native_ledger_transfer_fee,min_max_priority_fee_per_gas, deposit_native_fee, withdrawal_native_fee }
    }
}

prop_compose! {
    fn arb_received_deposit_event()(
        transaction_hash in arb_hash(),
        block_number in arb_checked_amount_of(),
        log_index in arb_checked_amount_of(),
        from_address in arb_address(),
        value in arb_checked_amount_of(),
        principal in arb_principal(),
        subaccount in arb_ledger_subaccount()
    ) -> ReceivedNativeEvent {
        ReceivedNativeEvent {
            transaction_hash,
            block_number,
            log_index,
            from_address,
            value,
            principal,
            subaccount
        }
    }
}

prop_compose! {
    fn arb_received_erc20_event()(
        transaction_hash in arb_hash(),
        block_number in arb_checked_amount_of(),
        log_index in arb_checked_amount_of(),
        from_address in arb_address(),
        value in arb_checked_amount_of(),
        principal in arb_principal(),
        erc20_contract_address in arb_address(),
        subaccount in arb_ledger_subaccount()
    ) -> ReceivedErc20Event {
        ReceivedErc20Event {
            transaction_hash,
            block_number,
            log_index,
            from_address,
            value,
            principal,
            erc20_contract_address,
            subaccount
        }
    }
}

prop_compose! {
    fn arb_unsigned_tx()(
        chain_id in any::<u64>(),
        nonce in arb_checked_amount_of(),
        max_priority_fee_per_gas in arb_checked_amount_of(),
        max_fee_per_gas in arb_checked_amount_of(),
        gas_limit in arb_checked_amount_of(),
        destination in arb_address(),
        amount in arb_checked_amount_of(),
        data in pvec(any::<u8>(), 0..20),
        access_list in arb_access_list(),
    ) -> Eip1559TransactionRequest {
         Eip1559TransactionRequest {
            chain_id,
            nonce,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            gas_limit,
            destination,
            amount,
            data,
            access_list,
        }
    }
}

prop_compose! {
    fn arb_signed_tx()(
        unsigned_tx in arb_unsigned_tx(),
        r in arb_u256(),
        s in arb_u256(),
        signature_y_parity in any::<bool>(),
    ) -> SignedEip1559TransactionRequest {
        SignedEip1559TransactionRequest::from((
            unsigned_tx,
            Eip1559Signature {
                r,
                s,
                signature_y_parity,
            }
        ))
    }
}

fn arb_transaction_status() -> impl Strategy<Value = TransactionStatus> {
    prop_oneof![
        Just(TransactionStatus::Success),
        Just(TransactionStatus::Failure),
    ]
}

prop_compose! {
    fn arb_tx_receipt()(
        block_hash in arb_hash(),
        block_number in arb_checked_amount_of(),
        effective_gas_price in arb_checked_amount_of(),
        gas_used in arb_checked_amount_of(),
        status in arb_transaction_status(),
        transaction_hash in arb_hash(),
    ) -> TransactionReceipt {
        TransactionReceipt {
            block_hash,
            block_number,
            effective_gas_price,
            gas_used,
            status,
            transaction_hash,
        }
    }
}

fn arb_event_type() -> impl Strategy<Value = EventType> {
    prop_oneof![
        arb_init_arg().prop_map(EventType::Init),
        arb_upgrade_arg().prop_map(EventType::Upgrade),
        arb_received_deposit_event().prop_map(EventType::AcceptedDeposit),
        arb_received_erc20_event().prop_map(EventType::AcceptedErc20Deposit),
        arb_event_source().prop_map(|event_source| EventType::InvalidEvent {
            event_source,
            reason: "bad principal".to_string()
        }),
        (arb_event_source(), any::<u64>()).prop_map(|(event_source, index)| {
            EventType::MintedNative {
                event_source,
                mint_block_index: index.into(),
            }
        }),
        arb_checked_amount_of().prop_map(|block_number| EventType::SyncedToBlock { block_number }),
        (any::<u64>(), arb_unsigned_tx()).prop_map(|(withdrawal_id, transaction)| {
            EventType::CreatedTransaction {
                withdrawal_id: withdrawal_id.into(),
                transaction,
            }
        }),
        (any::<u64>(), arb_signed_tx()).prop_map(|(withdrawal_id, transaction)| {
            EventType::SignedTransaction {
                withdrawal_id: withdrawal_id.into(),
                transaction,
            }
        }),
        (any::<u64>(), arb_unsigned_tx()).prop_map(|(withdrawal_id, transaction)| {
            EventType::ReplacedTransaction {
                withdrawal_id: withdrawal_id.into(),
                transaction,
            }
        }),
        (any::<u64>(), arb_tx_receipt()).prop_map(|(withdrawal_id, transaction_receipt)| {
            EventType::FinalizedTransaction {
                withdrawal_id: withdrawal_id.into(),
                transaction_receipt,
            }
        }),
    ]
}

fn arb_event() -> impl Strategy<Value = Event> {
    (any::<u64>(), arb_event_type()).prop_map(|(timestamp, payload)| Event { timestamp, payload })
}

proptest! {
    #[test]
    fn event_encoding_roundtrip(event in arb_event()) {
        use ic_stable_structures::storable::Storable;
        let bytes = event.to_bytes();
        prop_assert_eq!(&event, &Event::from_bytes(bytes.clone()), "failed to decode bytes {}", hex::encode(bytes));
    }
}

#[test]
pub fn state_equivalence() {
    use crate::map::MultiKeyMap;
    use crate::rpc_declarations::{TransactionReceipt, TransactionStatus};
    use crate::state::transactions::{
        NativeWithdrawalRequest, Reimbursed, ReimbursementRequest, WithdrawalTransactions,
    };
    use crate::state::{InvalidEventReason, MintedEvent};
    use crate::tx::{
        Eip1559Signature, Eip1559TransactionRequest, SignedTransactionRequest, TransactionRequest,
    };
    use maplit::{btreemap, btreeset};

    pub fn source(txhash: &str, index: u64) -> EventSource {
        EventSource {
            transaction_hash: txhash.parse().unwrap(),
            log_index: LogIndex::from(index),
        }
    }

    fn singleton_map<T: std::fmt::Debug>(
        nonce: u128,
        burn_index: u64,
        value: T,
    ) -> MultiKeyMap<TransactionNonce, LedgerBurnIndex, T> {
        let mut map = MultiKeyMap::new();
        map.try_insert(
            TransactionNonce::new(nonce),
            LedgerBurnIndex::new(burn_index),
            value,
        )
        .unwrap();
        map
    }

    let withdrawal_request1 = NativeWithdrawalRequest {
        withdrawal_amount: Wei::new(10_999_968_499_999_664_000),
        destination: "0xA776Cc20DFdCCF0c3ba89cB9Fb0f10Aba5b98f52"
            .parse()
            .unwrap(),
        ledger_burn_index: LedgerBurnIndex::new(10),
        from: "2chl6-4hpzw-vqaaa-aaaaa-c".parse().unwrap(),
        from_subaccount: None,
        created_at: Some(1699527697000000000),
        l1_fee: Some(Wei::new(1_000_000_000_000)),
        withdrawal_fee: None,
    };
    let withdrawal_request2 = NativeWithdrawalRequest {
        ledger_burn_index: LedgerBurnIndex::new(20),
        ..withdrawal_request1.clone()
    };
    let withdrawal_transactions = WithdrawalTransactions {
        pending_withdrawal_requests: vec![
            withdrawal_request1.clone().into(),
            withdrawal_request2.clone().into(),
        ]
        .into_iter()
        .collect(),
        processed_withdrawal_requests: btreemap! {
            LedgerBurnIndex::new(4) => NativeWithdrawalRequest {
                withdrawal_amount: Wei::new(1_000_000_000_000),
                ledger_burn_index: LedgerBurnIndex::new(4),
                destination: "0xA776Cc20DFdCCF0c3ba89cB9Fb0f10Aba5b98f52".parse().unwrap(),
                from: "ezu3d-2mifu-k3bh4-oqhrj-mbrql-5p67r-pp6pr-dbfra-unkx5-sxdtv-rae"
                    .parse()
                    .unwrap(),
                from_subaccount: None,
                created_at: Some(1699527697000000000),
                l1_fee:Some(Wei::new(4_000_000_000_000)),
                withdrawal_fee:None
            }.into(),
           withdrawal_request1.ledger_burn_index  => withdrawal_request1.clone().into(),
        },
        created_tx: singleton_map(
            2,
            4,
            TransactionRequest {
                transaction: Eip1559TransactionRequest {
                    chain_id: 1,
                    nonce: TransactionNonce::new(2),
                    max_priority_fee_per_gas: WeiPerGas::new(100_000_000),
                    max_fee_per_gas: WeiPerGas::new(100_000_000),
                    gas_limit: GasAmount::new(21_000),
                    destination: "0xA776Cc20DFdCCF0c3ba89cB9Fb0f10Aba5b98f52"
                        .parse()
                        .unwrap(),
                    amount: Wei::new(1_000_000_000_000),
                    data: vec![],
                    access_list: Default::default(),
                },
                resubmission: ResubmissionStrategy::ReduceEthAmount {
                    withdrawal_amount: Wei::new(1_000_000_000_000),
                },
            },
        ),
        sent_tx: singleton_map(
            1,
            3,
            vec![SignedTransactionRequest {
                transaction: SignedEip1559TransactionRequest::from((
                    Eip1559TransactionRequest {
                        chain_id: 1,
                        nonce: TransactionNonce::new(1),
                        max_priority_fee_per_gas: WeiPerGas::new(100_000_000),
                        max_fee_per_gas: WeiPerGas::new(100_000_000),
                        gas_limit: GasAmount::new(21_000),
                        destination: "0xA776Cc20DFdCCF0c3ba89cB9Fb0f10Aba5b98f52"
                            .parse()
                            .unwrap(),
                        amount: Wei::new(1_000_000_000_000),
                        data: vec![],
                        access_list: Default::default(),
                    },
                    Eip1559Signature {
                        signature_y_parity: true,
                        r: Default::default(),
                        s: Default::default(),
                    },
                )),
                resubmission: ResubmissionStrategy::ReduceEthAmount {
                    withdrawal_amount: Wei::new(1_000_000_000_000),
                },
            }],
        ),
        finalized_tx: singleton_map(
            0,
            2,
            SignedEip1559TransactionRequest::from((
                Eip1559TransactionRequest {
                    chain_id: 1,
                    nonce: TransactionNonce::new(0),
                    max_priority_fee_per_gas: WeiPerGas::new(100_000_000),
                    max_fee_per_gas: WeiPerGas::new(100_000_000),
                    gas_limit: GasAmount::new(21_000),
                    destination: "0xA776Cc20DFdCCF0c3ba89cB9Fb0f10Aba5b98f52"
                        .parse()
                        .unwrap(),
                    amount: Wei::new(1_000_000_000_000),
                    data: vec![],
                    access_list: Default::default(),
                },
                Eip1559Signature {
                    signature_y_parity: true,
                    r: Default::default(),
                    s: Default::default(),
                },
            ))
            .try_finalize(TransactionReceipt {
                block_hash: "0x9e1e2124a453e7b5afaabe42fb66fffb12d4b1053403d2f487d250007f3cb550"
                    .parse()
                    .unwrap(),
                block_number: BlockNumber::new(400_000),
                effective_gas_price: WeiPerGas::new(100_000_000),
                gas_used: GasAmount::new(21_000),
                status: TransactionStatus::Success,
                transaction_hash:
                    "0x06afc3c693dc2ba2c19b5c287c4dddce040d766bea5fd13c8a7268b04aa94f2d"
                        .parse()
                        .unwrap(),
            })
            .expect("valid receipt"),
        ),
        next_nonce: TransactionNonce::new(3),
        maybe_reimburse: btreeset! { LedgerBurnIndex::new(4) },
        reimbursement_requests: btreemap! {
            ReimbursementIndex::Native { ledger_burn_index: LedgerBurnIndex::new(3) } => ReimbursementRequest {
                transaction_hash: Some("0x06afc3c693dc2ba2c19b5c287c4dddce040d766bea5fd13c8a7268b04aa94f2d"
                .parse()
                .unwrap()),
                ledger_burn_index: LedgerBurnIndex::new(3),
                reimbursed_amount: Erc20TokenAmount::new(100_000_000_000),
                to: "ezu3d-2mifu-k3bh4-oqhrj-mbrql-5p67r-pp6pr-dbfra-unkx5-sxdtv-rae".parse().unwrap(),
                to_subaccount: None,
            }
        },
        reimbursed: btreemap! {
           ReimbursementIndex::Native { ledger_burn_index: LedgerBurnIndex::new(6) } =>
            Ok(Reimbursed {transaction_hash:Some("0x06afc3c693dc2ba2c19b5c287c4dddce040d766bea5fd13c8a7268b04aa94f2d".parse().unwrap())
                ,reimbursed_in_block:LedgerMintIndex::new(150),reimbursed_amount:Erc20TokenAmount::new(10_000_000_000_000),burn_in_block:LedgerBurnIndex::new(6), transfer_fee: None }),
        },
    };
    let mut erc20_tokens = DedupMultiKeyMap::default();
    erc20_tokens
        .try_insert(
            "mxzaz-hqaaa-aaaar-qaada-cai".parse().unwrap(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                .parse()
                .unwrap(),
            "ckUSDC".parse().unwrap(),
        )
        .unwrap();
    let state = State {
        evm_network: EvmNetwork::Sepolia,
        ecdsa_key_name: "test_key".to_string(),
        native_ledger_id: "apia6-jaaaa-aaaar-qabma-cai".parse().unwrap(),
        native_index_id: "eysav-tyaaa-aaaap-akqfq-cai".parse().unwrap(),
        helper_contract_addresses: Some(vec!["0xb44B5e756A894775FC32EDdf3314Bb1B1944dC34"
            .parse()
            .unwrap()]),
        ecdsa_public_key: Some(EcdsaPublicKeyResult {
            public_key: vec![1; 32],
            chain_code: vec![2; 32],
        }),
        native_minimum_withdrawal_amount: Wei::new(1_000_000_000_000_000),
        block_height: BlockTag::Finalized,
        first_scraped_block_number: BlockNumber::new(1_000_001),
        last_scraped_block_number: BlockNumber::new(1_000_000),
        last_observed_block_number: Some(BlockNumber::new(2_000_000)),
        events_to_mint: btreemap! {
            source("0xac493fb20c93bd3519a4a5d90ce72d69455c41c5b7e229dafee44344242ba467", 100) => ReceivedNativeEvent {
                transaction_hash: "0xac493fb20c93bd3519a4a5d90ce72d69455c41c5b7e229dafee44344242ba467".parse().unwrap(),
                block_number: BlockNumber::new(500_000),
                log_index: LogIndex::new(100),
                from_address: "0x9d68bd6F351bE62ed6dBEaE99d830BECD356Ed25".parse().unwrap(),
                value: Wei::new(500_000_000_000_000_000),
                principal: "lsywz-sl5vm-m6tct-7fhwt-6gdrw-4uzsg-ibknl-44d6d-a2oyt-c2cxu-7ae".parse().unwrap(),
                subaccount:None
            }.into()
        },
        minted_events: btreemap! {
            source("0x705f826861c802b407843e99af986cfde8749b669e5e0a5a150f4350bcaa9bc3", 1) => MintedEvent {
            event: ReceivedNativeEvent {
                    transaction_hash: "0x705f826861c802b407843e99af986cfde8749b669e5e0a5a150f4350bcaa9bc3".parse().unwrap(),
                    block_number: BlockNumber::new(450_000),
                    log_index: LogIndex::new(1),
                    from_address: "0x9d68bd6F351bE62ed6dBEaE99d830BECD356Ed25".parse().unwrap(),
                    value: Wei::new(10_000_000_000_000_000),
                    principal: "2chl6-4hpzw-vqaaa-aaaaa-c".parse().unwrap(),
                    subaccount:None
                }.into(),
                mint_block_index: LedgerMintIndex::new(1),
                erc20_contract_address: None,
                token_symbol: "icUSDT".to_string(),
            }
        },
        invalid_events: btreemap! {
            source("0x05c6ec45699c9a6a4b1a4ea2058b0cee852ea2f19b18fb8313c04bf8156efde4", 11) => InvalidEventReason::InvalidEvent("failed to decode principal from bytes 0x00333c125dc9f41abaf2b8b85d49fdc7ff75b2a4000000000000000000000000".to_string()),
        },
        withdrawal_transactions: withdrawal_transactions.clone(),
        pending_withdrawal_principals: Default::default(),
        active_tasks: Default::default(),
        native_balance: Default::default(),
        erc20_balances: Default::default(),
        skipped_blocks: Default::default(),
        last_transaction_price_estimate: None,
        evm_canister_id: "sosge-5iaaa-aaaag-alcla-cai".parse().unwrap(),
        erc20_tokens,
        native_symbol: ERC20TokenSymbol::new("icSepoliaETH".to_string()),
        native_ledger_transfer_fee: Wei::new(2_000_000_000_000_000),
        min_max_priority_fee_per_gas: WeiPerGas::new(1000),
        ledger_suite_manager_id: None,
        swap_canister_id: None,
        last_observed_block_time: None,
        withdrawal_native_fee: None,
        events_to_release: Default::default(),
        released_events: Default::default(),
        quarantined_releases: Default::default(),
        icrc_balances: Default::default(),
        wrapped_icrc_tokens: Default::default(),
        last_log_scraping_time: None,
    };

    assert_eq!(
        Ok(()),
        state.is_equivalent_to(&State {
            ecdsa_public_key: None,
            last_observed_block_number: None,
            ..state.clone()
        }),
        "changing only computed/transient fields should result in an equivalent state",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            first_scraped_block_number: BlockNumber::new(100_000_000_000),
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            last_scraped_block_number: BlockNumber::new(100_000_000_000),
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            ecdsa_key_name: "".to_string(),
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            helper_contract_addresses: None,
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            block_height: BlockTag::Latest,
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            events_to_mint: Default::default(),
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            minted_events: Default::default(),
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            invalid_events: Default::default(),
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );

    assert_eq!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                pending_withdrawal_requests: vec![
                    withdrawal_request2.clone().into(),
                    withdrawal_request1.clone().into()
                ]
                .into_iter()
                .collect(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        },),
        "changing the order of withdrawal requests should result in an equivalent state",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                pending_withdrawal_requests: vec![withdrawal_request1.into()].into_iter().collect(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the withdrawal requests should break equivalence"
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                sent_tx: Default::default(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the transactions should break equivalence"
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                created_tx: Default::default(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the transactions should break equivalence"
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                finalized_tx: Default::default(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the transactions should break equivalence"
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                maybe_reimburse: Default::default(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the reimbursement data should break equivalence"
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                reimbursement_requests: Default::default(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the reimbursement data should break equivalence"
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                reimbursed: Default::default(),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the reimbursement data should break equivalence"
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            withdrawal_transactions: WithdrawalTransactions {
                next_nonce: TransactionNonce::new(1000),
                ..withdrawal_transactions.clone()
            },
            ..state.clone()
        }),
        "changing the next nonce should break equivalence"
    );

    assert_eq!(
        Ok(()),
        state.is_equivalent_to(&State {
            last_transaction_price_estimate: Some((
                0,
                GasFeeEstimate {
                    base_fee_per_gas: WeiPerGas::new(10_000_000),
                    max_priority_fee_per_gas: WeiPerGas::new(1_000_000),
                }
            )),
            ..state.clone()
        }),
        "changing the last transaction price estimate should result in an equivalent state",
    );

    assert_ne!(
        Ok(()),
        state.is_equivalent_to(&State {
            erc20_tokens: Default::default(),
            ..state.clone()
        }),
        "changing essential fields should break equivalence",
    );
}

mod native_balance {
    use super::*;
    use crate::contract_logs::ReceivedContractEvent;
    use crate::evm_config::EvmNetwork;
    use crate::numeric::{
        BlockNumber, GasAmount, LedgerBurnIndex, TransactionNonce, Wei, WeiPerGas,
    };
    use crate::rpc_declarations::{TransactionReceipt, TransactionStatus};
    use crate::state::audit::{apply_state_transition, EventType};
    use crate::state::tests::checked_sub;
    use crate::state::tests::{initial_state, received_deposit_event};
    use crate::state::transactions::{
        create_transaction, NativeWithdrawalRequest, WithdrawalRequest,
    };
    use crate::state::{NativeBalance, State};
    use crate::tx::{Eip1559Signature, SignedEip1559TransactionRequest};
    use maplit::btreemap;

    #[test]
    fn should_add_deposit_to_native_balance() {
        let mut state = initial_state();
        let balance_before = state.native_balance.clone();

        let deposit_event = received_deposit_event();
        apply_state_transition(
            &mut state,
            &ReceivedContractEvent::from(deposit_event.clone()).into_event_type(),
        );
        let balance_after = state.native_balance.clone();

        assert_eq!(
            balance_after,
            NativeBalance {
                native_balance: deposit_event.value,
                ..balance_before
            }
        )
    }

    #[test]
    fn should_ignore_erc20_deposit_for_native_balance() {
        let mut state = initial_erc20_state();
        let balance_before = state.native_balance.clone();

        let deposit_event = received_erc20_event();
        apply_state_transition(
            &mut state,
            &ReceivedContractEvent::from(deposit_event.clone()).into_event_type(),
        );
        let balance_after = state.native_balance.clone();

        assert_eq!(balance_before, balance_after);
    }

    #[test]
    fn should_ignore_rejected_deposit() {
        let mut state = initial_state();
        let balance_before = state.native_balance.clone();

        let deposit_event = received_deposit_event();
        apply_state_transition(
            &mut state,
            &EventType::InvalidEvent {
                event_source: deposit_event.source(),
                reason: "invalid principal".to_string(),
            },
        );
        let balance_after = state.native_balance.clone();
        assert_eq!(balance_after, balance_before);

        add_erc20_token(&mut state);
        let deposit_event = received_erc20_event();
        apply_state_transition(
            &mut state,
            &EventType::InvalidEvent {
                event_source: deposit_event.source(),
                reason: "invalid principal".to_string(),
            },
        );
        let balance_after_erc20_deposit = state.native_balance.clone();

        assert_eq!(balance_after_erc20_deposit, balance_before);
    }

    #[test]
    fn should_update_after_successful_and_failed_withdrawal() {
        let mut state_before_withdrawal = initial_state();
        apply_state_transition(
            &mut state_before_withdrawal,
            &EventType::AcceptedDeposit(received_deposit_event()),
        );

        let withdrawal_native_fee = state_before_withdrawal.withdrawal_native_fee.unwrap();

        let mut state_after_successful_withdrawal = state_before_withdrawal.clone();
        let native_balance_before_withdrawal = state_before_withdrawal.native_balance.clone();
        let erc20_balance_before_withdrawal = state_before_withdrawal.erc20_balances.clone();

        let l1_fee = Wei::new(10_000_000);

        //Values from https://sepolia.etherscan.io/tx/0xef628b8f45984bdf386f5b765b665a2e584295e1190d21c6acdfabe17c27e1bb
        let withdrawal_request = NativeWithdrawalRequest {
            withdrawal_amount: Wei::new(10_000_000_000_000_000),
            destination: "0xb44B5e756A894775FC32EDdf3314Bb1B1944dC34"
                .parse()
                .unwrap(),
            ledger_burn_index: LedgerBurnIndex::new(0),
            from: "k2t6j-2nvnp-4zjm3-25dtz-6xhaa-c7boj-5gayf-oj3xs-i43lp-teztq-6ae"
                .parse()
                .unwrap(),
            from_subaccount: None,
            created_at: Some(1699527697000000000),
            l1_fee: Some(l1_fee),
            withdrawal_fee: Some(withdrawal_native_fee),
        };

        let withdrawal_flow = WithdrawalFlow {
            tx_fee: GasFeeEstimate {
                base_fee_per_gas: WeiPerGas::from(0xbc9998d1_u64),
                max_priority_fee_per_gas: WeiPerGas::from(1_500_000_000_u64),
            },
            gas_limit: GasAmount::from(21_000_u32),
            effective_gas_price: WeiPerGas::from(0x1176e9eb9_u64),
            tx_status: TransactionStatus::Success,
            ..WithdrawalFlow::for_request(withdrawal_request)
        };
        withdrawal_flow
            .clone()
            .apply(&mut state_after_successful_withdrawal);
        let native_balance_after_successful_withdrawal =
            state_after_successful_withdrawal.native_balance.clone();
        let erc20_balance_after_successful_withdrawal =
            state_after_successful_withdrawal.erc20_balances.clone();

        assert_eq!(
            native_balance_after_successful_withdrawal,
            NativeBalance {
                native_balance: native_balance_before_withdrawal
                    .native_balance
                    .checked_sub(Wei::from(9_934_054_260_043_000_u64))
                    .unwrap()
                    .checked_sub(l1_fee)
                    .unwrap(),
                total_effective_tx_fees: native_balance_before_withdrawal
                    .total_effective_tx_fees
                    .checked_add(Wei::from(98_449_949_997_000_u64))
                    .unwrap(),
                total_unspent_tx_fees: native_balance_before_withdrawal
                    .total_unspent_tx_fees
                    .checked_add(Wei::from(65_945_724_957_000_u64))
                    .unwrap(),
                total_collected_operation_native_fee: native_balance_before_withdrawal
                    .total_collected_operation_native_fee
                    .checked_add(withdrawal_native_fee)
                    .unwrap(),
            }
        );

        assert_eq!(
            erc20_balance_before_withdrawal,
            erc20_balance_after_successful_withdrawal
        );

        let mut state_after_failed_withdrawal = state_before_withdrawal.clone();
        let receipt_failed = WithdrawalFlow {
            tx_status: TransactionStatus::Failure,
            ..withdrawal_flow
        }
        .apply(&mut state_after_failed_withdrawal);
        let native_balance_after_failed_withdrawal =
            state_after_failed_withdrawal.native_balance.clone();
        let erc20_balance_after_failed_withdrawal =
            state_after_failed_withdrawal.erc20_balances.clone();

        assert_eq!(
            native_balance_after_failed_withdrawal.native_balance,
            native_balance_before_withdrawal
                .native_balance
                .checked_sub(receipt_failed.effective_transaction_fee())
                .unwrap()
                .checked_sub(l1_fee)
                .unwrap()
        );
        assert_eq!(
            native_balance_after_successful_withdrawal.total_effective_tx_fees,
            native_balance_after_failed_withdrawal.total_effective_tx_fees
        );
        assert_eq!(
            native_balance_after_successful_withdrawal.total_unspent_tx_fees,
            native_balance_after_failed_withdrawal.total_unspent_tx_fees()
        );
        assert_eq!(
            erc20_balance_before_withdrawal,
            erc20_balance_after_failed_withdrawal
        );
    }

    #[test]
    fn should_update_after_successful_and_failed_erc20_withdrawal() {
        let mut state_before_withdrawal = initial_erc20_state();
        apply_state_transition(
            &mut state_before_withdrawal,
            &EventType::AcceptedErc20Deposit(received_erc20_event()),
        );
        apply_state_transition(
            &mut state_before_withdrawal,
            &EventType::AcceptedDeposit(received_deposit_event()),
        );

        let withdrawal_fee = state_before_withdrawal
            .withdrawal_native_fee
            .clone()
            .unwrap();

        let mut state_after_successful_withdrawal = state_before_withdrawal.clone();
        let native_balance_before_withdrawal = state_before_withdrawal.native_balance.clone();
        let erc20_balance_before_withdrawal = state_before_withdrawal.erc20_balances.clone();

        let withdrawal_request = erc20_withdrawal_request();
        let tx_fee = GasFeeEstimate {
            base_fee_per_gas: WeiPerGas::from(0x4ce9a_u64),
            max_priority_fee_per_gas: WeiPerGas::from(1_500_000_000_u64),
        };

        let l1_fee = withdrawal_request.l1_fee.unwrap();

        let gas_limit = GasAmount::from(66_000_u64);
        let effective_gas_price = WeiPerGas::from(0x596cfd9a_u64);
        let effective_gas_used = GasAmount::from(0xb003_u32);
        let withdrawal_flow = WithdrawalFlow {
            tx_fee: tx_fee.clone(),
            gas_limit,
            effective_gas_price,
            effective_gas_used,
            tx_status: TransactionStatus::Success,
            ..WithdrawalFlow::for_request(withdrawal_request.clone())
        };
        withdrawal_flow
            .clone()
            .apply(&mut state_after_successful_withdrawal);
        let native_balance_after_successful_withdrawal =
            state_after_successful_withdrawal.native_balance.clone();
        let erc20_balance_after_successful_withdrawal =
            state_after_successful_withdrawal.erc20_balances.clone();

        let charged_transaction_fee = withdrawal_request.max_transaction_fee;
        let effective_transaction_fee = effective_gas_price
            .transaction_cost(effective_gas_used)
            .unwrap();
        let unspent_tx_fee = charged_transaction_fee
            .checked_sub(effective_transaction_fee)
            .unwrap();
        assert_eq!(
            native_balance_after_successful_withdrawal,
            NativeBalance {
                native_balance: native_balance_before_withdrawal
                    .native_balance
                    .checked_sub(effective_transaction_fee)
                    .unwrap()
                    .checked_sub(l1_fee)
                    .unwrap(),
                total_effective_tx_fees: native_balance_before_withdrawal
                    .total_effective_tx_fees
                    .checked_add(effective_transaction_fee)
                    .unwrap(),
                total_unspent_tx_fees: native_balance_before_withdrawal
                    .total_unspent_tx_fees
                    .checked_add(unspent_tx_fee)
                    .unwrap(),
                total_collected_operation_native_fee: native_balance_before_withdrawal
                    .total_collected_operation_native_fee
                    .checked_add(withdrawal_fee)
                    .unwrap()
            }
        );
        assert_eq!(
            checked_sub(
                erc20_balance_before_withdrawal.clone(),
                erc20_balance_after_successful_withdrawal
            ),
            btreemap! { withdrawal_request.erc20_contract_address => withdrawal_request.withdrawal_amount }
        );

        let mut state_after_failed_withdrawal = state_before_withdrawal.clone();
        WithdrawalFlow {
            tx_status: TransactionStatus::Failure,
            ..withdrawal_flow
        }
        .apply(&mut state_after_failed_withdrawal);
        let native_balance_after_failed_withdrawal =
            state_after_failed_withdrawal.native_balance.clone();
        let erc20_balance_after_failed_withdrawal =
            state_after_failed_withdrawal.erc20_balances.clone();
        assert_eq!(
            native_balance_after_successful_withdrawal,
            native_balance_after_failed_withdrawal
        );
        assert_eq!(
            erc20_balance_before_withdrawal,
            erc20_balance_after_failed_withdrawal
        );
    }

    #[derive(Clone)]
    struct WithdrawalFlow {
        withdrawal_request: WithdrawalRequest,
        nonce: TransactionNonce,
        tx_fee: GasFeeEstimate,
        gas_limit: GasAmount,
        effective_gas_price: WeiPerGas,
        effective_gas_used: GasAmount,
        tx_status: TransactionStatus,
    }

    impl WithdrawalFlow {
        fn for_request<T: Into<WithdrawalRequest>>(withdrawal_request: T) -> Self {
            Self {
                withdrawal_request: withdrawal_request.into(),
                nonce: TransactionNonce::ZERO,
                tx_fee: GasFeeEstimate {
                    base_fee_per_gas: WeiPerGas::ONE,
                    max_priority_fee_per_gas: WeiPerGas::ONE,
                },
                gas_limit: GasAmount::from(21_000_u32),
                effective_gas_price: WeiPerGas::ONE,
                effective_gas_used: GasAmount::from(21_000_u32),
                tx_status: TransactionStatus::Success,
            }
        }

        fn apply(self, state: &mut State) -> TransactionReceipt {
            let accepted_withdrawal_request_event = match &self.withdrawal_request {
                WithdrawalRequest::Native(eth_request) => {
                    EventType::AcceptedNativeWithdrawalRequest(eth_request.clone())
                }
                WithdrawalRequest::Erc20(erc20_request) => {
                    EventType::AcceptedErc20WithdrawalRequest(erc20_request.clone())
                }
            };
            apply_state_transition(state, &accepted_withdrawal_request_event);

            let transaction = create_transaction(
                &self.withdrawal_request,
                self.nonce,
                self.tx_fee,
                self.gas_limit,
                EvmNetwork::Sepolia,
            )
            .expect("BUG: failed to create transaction");
            apply_state_transition(
                state,
                &EventType::CreatedTransaction {
                    withdrawal_id: self.withdrawal_request.native_ledger_burn_index(),
                    transaction: transaction.clone(),
                },
            );

            let dummy_signature = Eip1559Signature {
                signature_y_parity: false,
                r: Default::default(),
                s: Default::default(),
            };
            let signed_tx =
                SignedEip1559TransactionRequest::from((transaction.clone(), dummy_signature));
            apply_state_transition(
                state,
                &EventType::SignedTransaction {
                    withdrawal_id: self.withdrawal_request.native_ledger_burn_index(),
                    transaction: signed_tx.clone(),
                },
            );

            let tx_receipt = TransactionReceipt {
                block_hash: "0xce67a85c9fb8bc50213815c32814c159fd75160acf7cb8631e8e7b7cf7f1d472"
                    .parse()
                    .unwrap(),
                block_number: BlockNumber::new(4190269),
                effective_gas_price: self.effective_gas_price,
                gas_used: self.effective_gas_used,
                status: self.tx_status,
                transaction_hash: signed_tx.hash(),
            };
            apply_state_transition(
                state,
                &EventType::FinalizedTransaction {
                    withdrawal_id: self.withdrawal_request.native_ledger_burn_index(),
                    transaction_receipt: tx_receipt.clone(),
                },
            );
            tx_receipt
        }
    }

    fn initial_erc20_state() -> State {
        let mut state = initial_state();
        add_erc20_token(&mut state);
        state
    }

    fn add_erc20_token(state: &mut State) {
        use crate::state::ERC20Token;
        apply_state_transition(
            state,
            &EventType::AddedErc20Token(ERC20Token {
                chain_id: Default::default(),
                erc20_contract_address: "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238"
                    .parse()
                    .unwrap(),
                erc20_token_symbol: "ckSepoliaUSDC".parse().unwrap(),
                erc20_ledger_id: Principal::from_text("3sgad-taaaa-aaaar-qaedq-cai").unwrap(),
            }),
        );
    }
}

mod erc20_balance {
    use crate::contract_logs::types::ReceivedErc20Event;
    use crate::contract_logs::ReceivedContractEvent;
    use crate::eth_types::Address;
    use crate::state::audit::EventType::AcceptedErc20WithdrawalRequest;
    use crate::state::audit::{apply_state_transition, EventType};
    use crate::state::tests::{
        checked_sub, erc20_withdrawal_request, initial_erc20_state, received_deposit_event,
        received_erc20_event,
    };
    use crate::state::transactions::Erc20WithdrawalRequest;
    use crate::test_fixtures::expect_panic_with_message;
    use maplit::btreemap;

    #[test]
    fn should_panic_when_deposited_erc20_not_supported() {
        let mut state = initial_erc20_state();
        let unsupported_erc20_address: Address = "0x6b175474e89094c44da98b954eedeac495271d0f"
            .parse()
            .unwrap();
        assert!(!state.erc20_tokens.contains_alt(&unsupported_erc20_address));

        let deposit_event = ReceivedErc20Event {
            erc20_contract_address: unsupported_erc20_address,
            ..received_erc20_event()
        };
        expect_panic_with_message(
            || {
                apply_state_transition(
                    &mut state,
                    &ReceivedContractEvent::from(deposit_event.clone()).into_event_type(),
                )
            },
            "BUG: unsupported ERC-20",
        );
    }

    #[test]
    fn should_panic_when_withdrawn_erc20_not_supported() {
        let mut state = initial_erc20_state();
        let unsupported_erc20_address: Address = "0x6b175474e89094c44da98b954eedeac495271d0f"
            .parse()
            .unwrap();
        assert!(!state.erc20_tokens.contains_alt(&unsupported_erc20_address));
        apply_state_transition(
            &mut state,
            &EventType::AcceptedErc20Deposit(received_erc20_event()),
        );
        apply_state_transition(
            &mut state,
            &EventType::AcceptedDeposit(received_deposit_event()),
        );
        let erc20_withdrawal = Erc20WithdrawalRequest {
            erc20_contract_address: unsupported_erc20_address,
            ..erc20_withdrawal_request()
        };
        expect_panic_with_message(
            || {
                apply_state_transition(
                    &mut state,
                    &AcceptedErc20WithdrawalRequest(erc20_withdrawal.clone()),
                )
            },
            "BUG: unsupported ERC-20",
        );
    }

    #[test]
    fn should_add_deposit_to_erc20_balance() {
        let mut state = initial_erc20_state();
        let balance_before = state.erc20_balances.clone();

        let deposit_event = received_erc20_event();
        apply_state_transition(
            &mut state,
            &ReceivedContractEvent::from(deposit_event.clone()).into_event_type(),
        );
        let balance_after = state.erc20_balances.clone();

        assert_eq!(
            checked_sub(balance_after, balance_before),
            btreemap! {
                deposit_event.erc20_contract_address => deposit_event.value
            }
        );
    }
    #[test]
    fn should_ignore_rejected_deposit() {
        let mut state = initial_erc20_state();
        let balance_before = state.erc20_balances.clone();

        let deposit_event = received_erc20_event();
        apply_state_transition(
            &mut state,
            &EventType::InvalidEvent {
                event_source: deposit_event.source(),
                reason: "invalid principal".to_string(),
            },
        );
        let balance_after = state.erc20_balances.clone();

        assert_eq!(balance_after, balance_before);
    }
}
fn initial_erc20_state() -> State {
    let mut state = initial_state();
    add_erc20_token(&mut state);
    state
}

fn add_erc20_token(state: &mut State) {
    use crate::state::ERC20Token;
    apply_state_transition(
        state,
        &EventType::AddedErc20Token(ERC20Token {
            chain_id: Default::default(),
            erc20_contract_address: "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238"
                .parse()
                .unwrap(),
            erc20_token_symbol: "ckSepoliaUSDC".parse().unwrap(),
            erc20_ledger_id: Principal::from_text("3sgad-taaaa-aaaar-qaedq-cai").unwrap(),
        }),
    );
}

fn erc20_withdrawal_request() -> Erc20WithdrawalRequest {
    Erc20WithdrawalRequest {
        max_transaction_fee: Wei::from(30_000_000_000_000_000_u64),
        withdrawal_amount: Erc20Value::from(4_996_000_u64),
        destination: "0xdd2851Cdd40aE6536831558DD46db62fAc7A844d"
            .parse()
            .unwrap(),
        native_ledger_burn_index: LedgerBurnIndex::new(5),
        erc20_contract_address: "0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238"
            .parse()
            .unwrap(),
        erc20_ledger_id: Principal::from_text("3sgad-taaaa-aaaar-qaedq-cai").unwrap(),
        erc20_ledger_burn_index: LedgerBurnIndex::new(2),
        from: Principal::from_text(
            "hkroy-sm7vs-yyjs7-ekppe-qqnwx-hm4zf-n7ybs-titsi-k6e3k-ucuiu-uqe",
        )
        .unwrap(),
        from_subaccount: None,
        created_at: 1_711_138_972_460_345_032,
        l1_fee: Some(Wei::new(10_000_000)),
        is_wrapped_mint: Some(false),
        withdrawal_fee: Some(Wei::new(5_000_000)),
    }
}

fn checked_sub(lhs: Erc20Balances, rhs: Erc20Balances) -> BTreeMap<Address, Erc20Value> {
    assert!(rhs
                .balance_by_erc20_contract
                .keys()
                .all(|rhs_erc20_contract| {
                    lhs.balance_by_erc20_contract
                        .contains_key(rhs_erc20_contract)
                }), "BUG: Cannot subtract rhs {:?} to lhs {:?} since some ERC-20 contracts are missing in the lhs", rhs, lhs);
    let mut result = lhs.balance_by_erc20_contract.clone();
    for (erc20_contract, rhs_value) in rhs.balance_by_erc20_contract.into_iter() {
        match lhs.balance_by_erc20_contract.get(&erc20_contract).unwrap() {
            lhs_value if lhs_value == &rhs_value => {
                result.remove(&erc20_contract);
            }
            lhs_value if lhs_value > &rhs_value => {
                result.insert(erc20_contract, lhs_value.checked_sub(rhs_value).unwrap());
            }
            lhs_value => panic!(
                "BUG: Cannot subtract rhs {:?} to lhs {:?} since it would underflow",
                rhs_value, lhs_value
            ),
        }
    }
    result
}
