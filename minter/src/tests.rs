#[cfg(test)]
pub mod appic_helper_types;
#[cfg(test)]
pub mod ledger_arguments;
#[cfg(test)]
pub mod lsm_types;
#[cfg(test)]
mod minter_flow_tets;
#[cfg(test)]
pub mod pocket_ic_helpers;

#[cfg(test)]
pub mod lock_release;

#[cfg(test)]
pub mod swap;

#[cfg(test)]
pub mod dex_types;

use crate::{
    contract_logs::{types::ReceivedNativeEvent, EventSource},
    erc20::ERC20TokenSymbol,
    evm_config::EvmNetwork,
    map::DedupMultiKeyMap,
    numeric::{BlockNumber, LedgerMintIndex, LogIndex, Wei, WeiPerGas},
    rpc_declarations::BlockTag,
    state::{
        balances::GasTank, transactions::WithdrawalTransactions, InvalidEventReason, MintedEvent,
        State,
    },
};
use evm_rpc_client::{address::ecdsa_public_key_to_address, eth_types::Address};
use ic_management_canister_types::EcdsaPublicKeyResult;
use maplit::btreemap;

#[test]
fn deserialize_block_spec() {
    use crate::numeric::BlockNumber;
    use crate::rpc_declarations::*;

    assert_eq!(
        BlockSpec::Number(BlockNumber::new(0xffff)),
        serde_json::from_str("\"0xffff\"").unwrap()
    );

    assert_eq!(
        BlockSpec::Tag(BlockTag::Latest),
        serde_json::from_str("\"latest\"").unwrap()
    );
    assert_eq!(
        BlockSpec::Tag(BlockTag::Safe),
        serde_json::from_str("\"safe\"").unwrap()
    );
    assert_eq!(
        BlockSpec::Tag(BlockTag::Finalized),
        serde_json::from_str("\"finalized\"").unwrap()
    );
}
mod get_contract_logs {
    use crate::candid_types::RequestScrapingError;
    use crate::contract_logs::parser::{LogParser, ReceivedEventsLogParser};
    use crate::contract_logs::swap::swap_logs::ReceivedSwapEvent;
    use crate::contract_logs::types::{ReceivedBurnEvent, ReceivedErc20Event, ReceivedNativeEvent};
    use crate::contract_logs::{LedgerSubaccount, ReceivedContractEvent};
    use crate::deposit::validate_log_scraping_request;
    use crate::erc20::ERC20TokenSymbol;
    use crate::numeric::{BlockNumber, Erc20Value, LogIndex, Wei};
    use crate::rpc_declarations::Data;
    use crate::rpc_declarations::{FixedSizeData, LogEntry};
    use crate::state::STATE;
    use crate::tests::test_state;
    use candid::Principal;
    use evm_rpc_client::eth_types::Address;
    use ic_sha3::Keccak256;
    use std::str::FromStr;

    #[test]
    fn deserialize_get_logs() {
        use crate::rpc_declarations::*;

        fn hash_from_hex(s: &str) -> Hash {
            Hash(hex::decode(s).unwrap().try_into().unwrap())
        }

        let logs: Vec<LogEntry> = serde_json::from_str(r#"[
 {
    "address": "0x7e41257f7b5c3dd3313ef02b1f4c864fe95bec2b",
    "topics": [
      "0x2a2607d40f4a6feb97c36e0efd57e0aa3e42e0332af4fceb78f21b7dffcbd657"
    ],
    "data": "0x00000000000000000000000055654e7405fcb336386ea8f36954a211b2cda764000000000000000000000000000000000000000000000000002386f26fc100000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000003f62327071372d71677a7a692d74623564622d72357363692d637736736c2d6e646f756c2d666f7435742d347a7732702d657a6677692d74616a32792d76716500",
    "blockNumber": "0x3aa4f4",
    "transactionHash": "0x5618f72c485bd98a3df58d900eabe9e24bfaa972a6fe5227e02233fad2db1154",
    "transactionIndex": "0x6",
    "blockHash": "0x908e6b84d26d71421bfaa08e7966e0afcef3883a28a53a0a7a31104caf1e94c2",
    "logIndex": "0x8",
    "removed": false
  }]"#).unwrap();
        assert_eq!(
            logs,
            vec![LogEntry {
                address: Address::from_str("0x7e41257f7b5c3dd3313ef02b1f4c864fe95bec2b").unwrap(),
                topics: vec![
                   FixedSizeData::from_str("0x2a2607d40f4a6feb97c36e0efd57e0aa3e42e0332af4fceb78f21b7dffcbd657").unwrap(),
                ],
                data: Data(hex::decode("00000000000000000000000055654e7405fcb336386ea8f36954a211b2cda764000000000000000000000000000000000000000000000000002386f26fc100000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000003f62327071372d71677a7a692d74623564622d72357363692d637736736c2d6e646f756c2d666f7435742d347a7732702d657a6677692d74616a32792d76716500").unwrap()),
                block_number: Some(BlockNumber::new(0x3aa4f4)),
                transaction_hash: Some(hash_from_hex("5618f72c485bd98a3df58d900eabe9e24bfaa972a6fe5227e02233fad2db1154")),
                transaction_index: Some(Quantity::new(0x06)),
                block_hash: Some(hash_from_hex("908e6b84d26d71421bfaa08e7966e0afcef3883a28a53a0a7a31104caf1e94c2")),
                log_index: Some(LogIndex::from(0x08_u8)),
                removed: false,
            }]
        );
    }

    #[test]
    fn should_have_correct_topic() {
        use crate::contract_logs::types::RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC_OLD_CONTRACT;

        //must match event signature in minter.sol
        let event_signature = "DepositLog(address,address,uint256,bytes32,bytes32)";

        let topic = Keccak256::hash(event_signature);
        assert_eq!(topic, RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC_OLD_CONTRACT)
    }

    #[test]
    fn shoulf_parse_received_icrc_wrapp_event() {
        let state = test_state();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{    
    "address": "0x7e41257f7b5c3dd3313ef02b1f4c864fe95bec2b",
    "topics": [
      "0x37199deebd336af9013dbddaaf9a68e337707bb4ed64cb45ed12841af85e0377",
      "0x0000000000000000000000001234567890abcdef1234567890abcdef12345678",
      "0x09efcdab00000000000100000000000000000000000000000000000000000000",
      "0x0000000000000000000000009876543210fedcba9876543210fedcba98765432"
    ],
    "data": "0x0000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000000000000000000",
    "blockNumber": "0x3aa4f4",
    "transactionHash": "0x5618f72c485bd98a3df58d900eabe9e24bfaa972a6fe5227e02233fad2db1154",
    "transactionIndex": "0x6",
    "blockHash": "0x908e6b84d26d71421bfaa08e7966e0afcef3883a28a53a0a7a31104caf1e94c2",
    "logIndex": "0x8",
    "removed": false

        }"#;

        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap());
        println!("{parsed_event:?}");
        let burn_event = ReceivedBurnEvent {
            transaction_hash: "0x5618f72c485bd98a3df58d900eabe9e24bfaa972a6fe5227e02233fad2db1154"
                .parse()
                .unwrap(),
            block_number: 3_843_316_u32.into(),
            log_index: 8_u32.into(),
            from_address: "0x1234567890AbcdEF1234567890aBcdef12345678"
                .parse()
                .unwrap(),
            value: 1_000_000_000_000_000_000_u64.into(),
            principal: "2chl6-4hpzw-vqaaa-aaaaa-c".parse().unwrap(),
            wrapped_erc20_contract_address: "0x9876543210FeDcba9876543210FEdCba98765432"
                .parse()
                .unwrap(),
            icrc_token_principal: "ryjl3-tyaaa-aaaaa-aaaba-cai".parse().unwrap(),
            subaccount: None,
        }
        .into();
        assert_eq!(parsed_event.unwrap(), burn_event);
    }

    #[test]
    fn should_parse_received_eth_event() {
        let state = test_state();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{
            "address": "0xF199c1779706fE7Fe636B9897043F51235295E96",
            "topics": [
                "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                "0x0000000000000000000000000000000000000000000000000000000000000000",
                "0x00000000000000000000000000000000000000000000000000005af3107a4000",
                "0x09efcdab00000000000100000000000000000000000000000000000000000000"
            ],
            "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
            "blockNumber": "0x3ca487",
            "transactionHash": "0x705f826861c802b407843e99af986cfde8749b669e5e0a5a150f4350bcaa9bc3",
            "transactionIndex": "0x22",
            "blockHash": "0x8436209a391f7bc076123616ecb229602124eb6c1007f5eae84df8e098885d3c",
            "logIndex": "0x27",
            "removed": false
        }"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap())
                .unwrap();
        let expected_event = ReceivedNativeEvent {
            transaction_hash: "0x705f826861c802b407843e99af986cfde8749b669e5e0a5a150f4350bcaa9bc3"
                .parse()
                .unwrap(),
            block_number: BlockNumber::new(3974279),
            log_index: LogIndex::from(39_u8),
            from_address: "0x5d737F982696Fe2fE4eF1c7584E914C3A8e44D54"
                .parse()
                .unwrap(),
            value: Wei::from(100_000_000_000_000_u128),
            principal: Principal::from_str("2chl6-4hpzw-vqaaa-aaaaa-c").unwrap(),
            subaccount: None,
        }
        .into();

        assert_eq!(parsed_event, expected_event);
    }

    #[test]
    fn should_parse_received_native_event_with_subaccount() {
        let state = test_state();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{
            "address": "0x11d7c426eedc044b21066d2be9480d4b99e7cc1a",
            "topics": [
                "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                "0x0000000000000000000000000000000000000000000000000000000000000000",
                "0x00000000000000000000000000000000000000000000000000005af3107a4000",
                "0x09efcdab00000000000100000000000000000000000000000000000000000000"
            ],
            "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d54ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "blockNumber": "0x698ab3",
            "transactionHash": "0x037305b461a7c69bf65d4e143262fc038b39d5e46da79de1539e3a90e91b9b37",
            "transactionIndex": "0x12",
            "blockHash": "0x92d629a73b6e94c799e940868e4961e2674b0ffd28796102add19a89402e03dd",
            "logIndex": "0x14",
            "removed": false
        }"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap())
                .unwrap();
        let expected_event = ReceivedNativeEvent {
            transaction_hash: "0x037305b461a7c69bf65d4e143262fc038b39d5e46da79de1539e3a90e91b9b37"
                .parse()
                .unwrap(),
            block_number: BlockNumber::new(6916787),
            log_index: LogIndex::from(20_u8),
            from_address: "0x5d737F982696Fe2fE4eF1c7584E914C3A8e44D54"
                .parse()
                .unwrap(),
            value: Wei::from(100_000_000_000_000_u128),
            principal: Principal::from_str("2chl6-4hpzw-vqaaa-aaaaa-c").unwrap(),
            subaccount: LedgerSubaccount::from_bytes([0xff; 32]),
        }
        .into();
        assert_eq!(parsed_event, expected_event);
    }

    #[test]
    fn should_parse_received_erc20_event() {
        let state = test_state();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{
            "address": "0xF199c1779706fE7Fe636B9897043F51235295E96",
            "topics": [
                "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                "0x000000000000000000000000779877a7b0d9e8603169ddbd7836e478b4624789",
                "0x0000000000000000000000000000000000000000000000004563918244f40000",
                "0x1d9facb184cbe453de4841b6b9d9cc95bfc065344e485789b550544529020000"
            ],
            "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
            "blockNumber": "0x5146a4",
            "transactionHash": "0x44d8e93a8f4bbc89ad35fc4fbbdb12cb597b4832da09c0b2300777be180fde87",
            "transactionIndex": "0x22",
            "blockHash": "0x0cbfb260e2e589ef110e63314279eb3ef2e307e46fa5409f08c101976858f80a",
            "logIndex": "0x27",
            "removed": false
        }"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap())
                .unwrap();
        let expected_event = ReceivedErc20Event {
            transaction_hash: "0x44d8e93a8f4bbc89ad35fc4fbbdb12cb597b4832da09c0b2300777be180fde87"
                .parse()
                .unwrap(),
            block_number: BlockNumber::new(5326500),
            log_index: LogIndex::from(39_u8),
            from_address: "0x5d737F982696Fe2fE4eF1c7584E914C3A8e44D54"
                .parse()
                .unwrap(),
            value: Erc20Value::from(5_000_000_000_000_000_000_u128),
            principal: Principal::from_str(
                "hkroy-sm7vs-yyjs7-ekppe-qqnwx-hm4zf-n7ybs-titsi-k6e3k-ucuiu-uqe",
            )
            .unwrap(),
            erc20_contract_address: "0x779877A7B0D9E8603169DdbD7836e478b4624789"
                .parse()
                .unwrap(),
            subaccount: None,
        }
        .into();

        assert_eq!(parsed_event, expected_event);
    }

    #[test]
    fn should_parse_received_erc20_event_with_subaccount() {
        let state = test_state();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{
            "address": "0x11d7c426eedc044b21066d2be9480d4b99e7cc1a",
            "topics": [
               "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                "0x000000000000000000000000779877a7b0d9e8603169ddbd7836e478b4624789",
                "0x00000000000000000000000000000000000000000000000000005af3107a4000",
                "0x09efcdab00000000000100000000000000000000000000000000000000000000"
            ],
            "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d54ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "blockNumber": "0x698adb",
            "transactionHash": "0xf353e17cbcfea236a8b03d2d800205074e1f5014a3ce0f6dedcf128addb6bea4",
            "transactionIndex": "0x15",
            "blockHash": "0xeee67434b62fe62182ee51cdaf2693f112994fd3aa4d043c7e4a16fe775c37e3",
            "logIndex": "0x45",
            "removed": false
        }"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap())
                .unwrap();
        let expected_event = ReceivedErc20Event {
            transaction_hash: "0xf353e17cbcfea236a8b03d2d800205074e1f5014a3ce0f6dedcf128addb6bea4"
                .parse()
                .unwrap(),
            block_number: BlockNumber::new(6916827),
            log_index: LogIndex::from(69_u8),
            from_address: "0x5d737F982696Fe2fE4eF1c7584E914C3A8e44D54"
                .parse()
                .unwrap(),
            value: Erc20Value::from(100_000_000_000_000_u128),
            principal: Principal::from_str("2chl6-4hpzw-vqaaa-aaaaa-c").unwrap(),
            erc20_contract_address: "0x779877a7b0d9e8603169ddbd7836e478b4624789"
                .parse()
                .unwrap(),
            subaccount: LedgerSubaccount::from_bytes([0xff; 32]),
        }
        .into();

        assert_eq!(parsed_event, expected_event);
    }

    #[test]
    #[should_panic]
    fn should_panic_parsing_swap_event_if_swapping_is_not_active() {
        let state = test_state();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{"address":"0x98fff5f36c0cf12ae16d3d80f67b5e8ab5e1ffb1","topics":["0xc33dada04354dd803ea44b93af35ba61d4bfa477f5f06c86b6a00cfc0c261bea","0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace","0x0000000000000000000000000000000000000000000000000000000000000000","0x00000000000000000000000055d398326f99059ff775485246999027b3197955"],"data":"0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace00000000000000000000000000000000000000000000000000038d7ea4c680000000000000000000000000000000000000000000000000000cd201850b14077f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000116273635f626e625f757364745f74657374000000000000000000000000000000","blockNumber":"0x3a565a0","transactionHash":"0x99842464fc055be0582006a497356e363d47b87be7cfcfc57e58f3b7624b60cc","transactionIndex":"0x38","blockHash":"0xc8b61d02becbe0f0d90239fba679e5b80f9f063e0ba2f6475d04cd68fc4b8b6e","logIndex":"0xf4","removed":false}"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap());

        println!("{parsed_event:?}");
    }

    #[test]
    fn should_not_parse_log_if_bridge_to_minter_false() {
        let mut state = test_state();

        let _ = state.erc20_tokens.try_insert(
            Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap(),
            Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(),
            ERC20TokenSymbol("icUSDC.base".to_string()),
        );
        state.is_swapping_active = true;
        state.activate_swap_feature(
            (
                Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(),
                Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap(),
            ),
            Address::from_str("0x98fff5F36C0cF12AE16d3D80F67B5E8ab5E1FfB1").unwrap(),
            6,
            Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap(),
            Erc20Value::from(30_000_u32),
        );

        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{"address":"0x98fff5f36c0cf12ae16d3d80f67b5e8ab5e1ffb1","topics":["0xc33dada04354dd803ea44b93af35ba61d4bfa477f5f06c86b6a00cfc0c261bea","0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace","0x0000000000000000000000000000000000000000000000000000000000000000","0x00000000000000000000000055d398326f99059ff775485246999027b3197955"],"data":"0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace00000000000000000000000000000000000000000000000000038d7ea4c680000000000000000000000000000000000000000000000000000cd201850b14077f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000116273635f626e625f757364745f74657374000000000000000000000000000000","blockNumber":"0x3a565a0","transactionHash":"0x99842464fc055be0582006a497356e363d47b87be7cfcfc57e58f3b7624b60cc","transactionIndex":"0x38","blockHash":"0xc8b61d02becbe0f0d90239fba679e5b80f9f063e0ba2f6475d04cd68fc4b8b6e","logIndex":"0xf4","removed":false}"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap());

        assert!(parsed_event.is_err());
    }

    #[test]
    fn should_parse_swap_log() {
        let mut state = test_state();

        let _ = state.erc20_tokens.try_insert(
            Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap(),
            Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(),
            ERC20TokenSymbol("icUSDC.base".to_string()),
        );
        state.is_swapping_active = true;
        state.activate_swap_feature(
            (
                Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(),
                Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap(),
            ),
            Address::from_str("0x98fff5F36C0cF12AE16d3D80F67B5E8ab5E1FfB1").unwrap(),
            6,
            Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap(),
            Erc20Value::from(30_000_u32),
        );

        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{"address":"0xa72ab997ccd4c55a7adc049df8057d577f5322a8","topics":["0xc33dada04354dd803ea44b93af35ba61d4bfa477f5f06c86b6a00cfc0c261bea","0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace","0x0000000000000000000000004200000000000000000000000000000000000006","0x000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda02913"],"data":"0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace00000000000000000000000000000000000000000000000000037235b96ea0000000000000000000000000000000000000000000000000000000000000435d8a000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000800000000000000000000000008ac76a51cc950d9822d68b83fe1ad97b32cd580d00000000000000000000000055d398326f99059ff775485246999027b31979550000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000052c3d6e0738861c","blockNumber":"0x21eb744","transactionHash":"0x374994a3848087112f992c8f587399cb13e3a2e53fd4614c7d6f58f45feeed92","transactionIndex":"0x124","blockHash":"0xd3c268add935a28bd2d94cd0984de06c6c60650973d16b1bb45b11c43de8cebf","logIndex":"0x2d4","removed":false}"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap())
                .unwrap();

        let expected_event=ReceivedContractEvent::ReceivedSwapOrder(ReceivedSwapEvent{ transaction_hash: parsed_event.transaction_hash(), block_number:parsed_event.block_number(), log_index:parsed_event.log_index(), from_address: Address::from_str("0xdAf40D6d8FCFBbFfd1deBA15990B7e08780F7ACe").unwrap(), recipient:FixedSizeData::from_str("0x000000000000000000000000DAF40D6D8FCFBBFFD1DEBA15990B7E08780F7ACE").unwrap() ,token_in:Address::from_str("0x4200000000000000000000000000000000000006").unwrap(), token_out: Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(), amount_in: Erc20Value::from(970000000000000_u128), amount_out: Erc20Value::from(4414858_u128), bridged_to_minter:true, encoded_swap_data: Data::from_str("0x0000000000000000000000008ac76a51cc950d9822d68b83fe1ad97b32cd580d00000000000000000000000055d398326f99059ff775485246999027b31979550000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000052c3d6e0738861c").unwrap() });

        assert_eq!(expected_event, parsed_event);
    }

    #[test]
    fn should_parse_swap_log_second() {
        let mut state = test_state();

        let _ = state.erc20_tokens.try_insert(
            Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap(),
            Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(),
            ERC20TokenSymbol("icUSDC.base".to_string()),
        );
        state.is_swapping_active = true;
        state.activate_swap_feature(
            (
                Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(),
                Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap(),
            ),
            Address::from_str("0x98fff5F36C0cF12AE16d3D80F67B5E8ab5E1FfB1").unwrap(),
            6,
            Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap(),
            Erc20Value::from(30_000_u32),
        );

        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        let event = r#"{
"address": "0xa72ab997ccd4c55a7adc049df8057d577f5322a8",
"topics": [
"0xc33dada04354dd803ea44b93af35ba61d4bfa477f5f06c86b6a00cfc0c261bea",
"0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace",
"0x0000000000000000000000004200000000000000000000000000000000000006",
"0x000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda02913"
],
"data": "0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace00000000000000000000000000000000000000000000000000037235b96ea0000000000000000000000000000000000000000000000000000000000000423ce6000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000497f904948f3937303030303030303030303030309334323534303032363739383931383236333238933431393837303036343530353332333236303084312e3325f90454f901b784383435338f3937303030303030303030303030308734333430393636873433313932363184302e35258633343938373088313136393736323388302e30313933343730f85cf85aaa307834323030303030303030303030303030303030303030303030303030303030303030303030303036aa30783833333538396643443665446236453038663463374333324434663731623534626441303239313383313030c131f90105b901023078303030303030303030303030303030303030303030303030343230303030303030303030303030303030303030303030303030303030303030303030303030363030303030303030303030303030303030303030303030303833333538396663643665646236653038663463376333326434663731623534626461303239313330303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303431653831648a31373538343638323431f8bf8369637087343334303936369334333331323335383335383832333333393631933432393635383539343931393532373533303084302e382530303030f87df83c9b716b7277702d7a696161612d61616161672d6175656d712d6361699b7865766e6d2d67616161612d61616161722d7161666e712d63616983313030f83d9b7865766e6d2d67616161612d61616161722d7161666e712d6361699b7a326979652d66796161612d61616161672d61743270612d6361698431303030c0c030f901d682353693343235373038333833353838323333333936319334323534303032363739383931383236333238933431393837303036343530353332333236303084312e3325863334393837308931303030303030303088302e30333637363084302e3035f85cf85aaa307838414337366135316363393530643938323244363862383366453141643937423332436435383064aa30783535643339383332366639393035396646373735343835323436393939303237423331393739353583313030c131f90105b901023078303030303030303030303030303030303030303030303030386163373661353163633935306439383232643638623833666531616439376233326364353830643030303030303030303030303030303030303030303030303535643339383332366639393035396666373735343835323436393939303237623331393739353530303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030336162646166656632373738333039388a31373538343638323432000000000000000000",
"blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
"blockNumber": "0x1",
"logIndex": "0x0",
"removed": false,
"transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
"transactionIndex": "0x0"
}"#;
        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap())
                .unwrap();

        let expected_event=ReceivedContractEvent::ReceivedSwapOrder(ReceivedSwapEvent{ transaction_hash: parsed_event.transaction_hash(), block_number:parsed_event.block_number(), log_index:parsed_event.log_index(), from_address: Address::from_str("0xdAf40D6d8FCFBbFfd1deBA15990B7e08780F7ACe").unwrap(), recipient:FixedSizeData::from_str("0x000000000000000000000000DAF40D6D8FCFBBFFD1DEBA15990B7E08780F7ACE").unwrap() ,token_in:Address::from_str("0x4200000000000000000000000000000000000006").unwrap(), token_out: Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap(), amount_in: Erc20Value::from(970000000000000_u128), amount_out: Erc20Value::from(4_340_966_u128), bridged_to_minter:true, encoded_swap_data: Data::from_str("0xf904948f3937303030303030303030303030309334323534303032363739383931383236333238933431393837303036343530353332333236303084312e3325f90454f901b784383435338f3937303030303030303030303030308734333430393636873433313932363184302e35258633343938373088313136393736323388302e30313933343730f85cf85aaa307834323030303030303030303030303030303030303030303030303030303030303030303030303036aa30783833333538396643443665446236453038663463374333324434663731623534626441303239313383313030c131f90105b901023078303030303030303030303030303030303030303030303030343230303030303030303030303030303030303030303030303030303030303030303030303030363030303030303030303030303030303030303030303030303833333538396663643665646236653038663463376333326434663731623534626461303239313330303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303431653831648a31373538343638323431f8bf8369637087343334303936369334333331323335383335383832333333393631933432393635383539343931393532373533303084302e382530303030f87df83c9b716b7277702d7a696161612d61616161672d6175656d712d6361699b7865766e6d2d67616161612d61616161722d7161666e712d63616983313030f83d9b7865766e6d2d67616161612d61616161722d7161666e712d6361699b7a326979652d66796161612d61616161672d61743270612d6361698431303030c0c030f901d682353693343235373038333833353838323333333936319334323534303032363739383931383236333238933431393837303036343530353332333236303084312e3325863334393837308931303030303030303088302e30333637363084302e3035f85cf85aaa307838414337366135316363393530643938323244363862383366453141643937423332436435383064aa30783535643339383332366639393035396646373735343835323436393939303237423331393739353583313030c131f90105b901023078303030303030303030303030303030303030303030303030386163373661353163633935306439383232643638623833666531616439376233326364353830643030303030303030303030303030303030303030303030303535643339383332366639393035396666373735343835323436393939303237623331393739353530303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030336162646166656632373738333039388a31373538343638323432").unwrap() });

        assert_eq!(expected_event, parsed_event);
    }

    #[test]
    fn should_not_parse_removed_event() {
        let state = test_state();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));

        use crate::contract_logs::{EventSource, EventSourceError, ReceivedContractEventError};
        let event = r#"{
            "address": "0xb44b5e756a894775fc32eddf3314bb1b1944dc34",
            "topics": [
                "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                "0x000000000000000000000000779877a7b0d9e8603169ddbd7836e478b4624789",
                "0x0000000000000000000000000000000000000000000000004563918244f40000",
                "0x09efcdab00000000000100000000000000000000000000000000000000000000"
            ],
            "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d54ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "blockNumber": "0x3ca487",
            "transactionHash": "0x705f826861c802b407843e99af986cfde8749b669e5e0a5a150f4350bcaa9bc3",
            "transactionIndex": "0x22",
            "blockHash": "0x8436209a391f7bc076123616ecb229602124eb6c1007f5eae84df8e098885d3c",
            "logIndex": "0x27",
            "removed": true
        }"#;

        let parsed_event =
            ReceivedEventsLogParser::parse_log(serde_json::from_str::<LogEntry>(event).unwrap());
        let expected_error = Err(ReceivedContractEventError::InvalidEventSource {
            source: EventSource {
                transaction_hash:
                    "0x705f826861c802b407843e99af986cfde8749b669e5e0a5a150f4350bcaa9bc3"
                        .parse()
                        .unwrap(),
                log_index: LogIndex::from(39_u8),
            },
            error: EventSourceError::InvalidEvent(
                "this event has been removed from the chain".to_string(),
            ),
        });
        assert_eq!(parsed_event, expected_error);
    }

    #[test]
    fn should_not_allow_log_scraping() {
        let validation_result_observed_block = validate_log_scraping_request(
            1_732_638_362_000_000_000_u64,
            2_845_738_362_000_000_000_u64,
        )
        .is_ok();

        assert!(validation_result_observed_block);

        let validation_result_not_enough_gap_between_two_requests = validate_log_scraping_request(
            1_732_638_362_000_000_000_u64,
            1_732_638_362_000_000_000_u64.saturating_add(5_000_000_000_u64),
        );

        assert_eq!(
            validation_result_not_enough_gap_between_two_requests,
            Err(RequestScrapingError::CalledTooManyTimes)
        );
    }

    #[test]
    fn should_allow_log_scrapping() {
        let validation_result = validate_log_scraping_request(
            1_732_638_362_000_000_000_u64,
            1_732_638_362_000_000_000_u64.saturating_add(60_000_000_001_u64),
        );

        assert_eq!(validation_result, Ok(()));
    }
}

#[test]
fn address_from_pubkey() {
    use libsecp256k1::{PublicKey, PublicKeyFormat};

    // Examples come from https://mycrypto.tools/sample_ethaddresses.html
    const EXAMPLES: &[(&str, &str)] = &[
        (
            "04e68acfc0253a10620dff706b0a1b1f1f5833ea3beb3bde2250d5f271f3563606672ebc45e0b7ea2e816ecb70ca03137b1c9476eec63d4632e990020b7b6fba39",
            "0x90F8bf6A479f320ead074411a4B0e7944Ea8c9C1",
        ),
        (
            "04bbe06c9dd095cdf0aded667ea17621e8c1fdcd36ffe112a9c94e47aa6be1406a666e1001cf0067d0f9a541043dfc5438ead7be3ecbcdc328b67d8f966bceea63",
            "0x721B68fA152a930F3df71F54aC1ce7ed3ac5f867",
        ),
    ];
    for (pk_bytes, address) in EXAMPLES {
        let sec1_bytes = hex::decode(pk_bytes).unwrap();
        let pk = PublicKey::parse_slice(&sec1_bytes, Some(PublicKeyFormat::Full)).unwrap();
        assert_eq!(&ecdsa_public_key_to_address(&pk).to_string(), address);
    }
}

mod rlp_encoding {
    use crate::numeric::{GasAmount, TransactionNonce, Wei, WeiPerGas};
    use crate::rpc_declarations::Hash;
    use crate::tx::{
        determine_signature_y_parity, AccessList, Eip1559Signature, Eip1559TransactionRequest,
        SignedEip1559TransactionRequest,
    };
    use ethnum::u256;
    use evm_rpc_client::eth_types::Address;
    use libsecp256k1::{PublicKey, PublicKeyFormat};
    use rlp::Encodable;
    use std::str::FromStr;

    const SEPOLIA_TEST_CHAIN_ID: u64 = 11155111;

    #[test]
    fn test_rlp_encoding() {
        use crate::tx::{AccessList, Eip1559TransactionRequest};
        use ethers_core::abi::ethereum_types::H160;
        use ethers_core::types::transaction::eip1559::Eip1559TransactionRequest as EthersCoreEip1559TransactionRequest;
        use ethers_core::types::transaction::eip2930::AccessList as EthersCoreAccessList;
        use ethers_core::types::Signature as EthersCoreSignature;
        use ethers_core::types::{Bytes, U256};
        use ethnum::u256;

        let address_bytes: [u8; 20] = [
            180, 75, 94, 117, 106, 137, 71, 117, 252, 50, 237, 223, 51, 20, 187, 27, 25, 68, 220,
            52,
        ];

        let ethers_core_tx = EthersCoreEip1559TransactionRequest {
            from: None,
            to: Some(ethers_core::types::NameOrAddress::Address(H160::from(
                address_bytes,
            ))),
            gas: Some(1.into()),
            value: Some(2.into()),
            data: Some(Bytes::new()),
            nonce: Some(0.into()),
            access_list: EthersCoreAccessList::from(vec![]),
            max_priority_fee_per_gas: Some(3.into()),
            max_fee_per_gas: Some(4.into()),
            chain_id: Some(1.into()),
        };
        let minter_tx = Eip1559TransactionRequest {
            chain_id: 1,
            destination: Address::new(address_bytes),
            nonce: 0_u64.into(),
            gas_limit: 1_u32.into(),
            max_fee_per_gas: 4_u64.into(),
            amount: 2_u64.into(),
            data: vec![],
            access_list: AccessList::new(),
            max_priority_fee_per_gas: 3_u64.into(),
        };
        assert_eq!(
            minter_tx.rlp_bytes().to_vec(),
            ethers_core_tx.rlp().to_vec()
        );

        let signature = Eip1559Signature {
            signature_y_parity: true,
            r: u256::from_str_radix(
                "b92224ecdb5295f3b889059621909c6b7a2308ccd0e5f13812409d80706b13cd",
                16,
            )
            .unwrap(),
            s: u256::from_str_radix(
                "0bec9da278e6388a9d6934c911684234e16db1610c2227545c7b192db277c4b1",
                16,
            )
            .unwrap(),
        };

        assert_eq!(
            SignedEip1559TransactionRequest::from((minter_tx, signature))
                .rlp_bytes()
                .to_vec(),
            ethers_core_tx
                .rlp_signed(&EthersCoreSignature {
                    v: 1,
                    r: U256::from_str_radix(
                        "b92224ecdb5295f3b889059621909c6b7a2308ccd0e5f13812409d80706b13cd",
                        16
                    )
                    .unwrap(),
                    s: U256::from_str_radix(
                        "0bec9da278e6388a9d6934c911684234e16db1610c2227545c7b192db277c4b1",
                        16
                    )
                    .unwrap(),
                })
                .to_vec()
        );
    }

    #[test]
    fn should_compute_correct_rlp_encoding_of_signed_transaction() {
        // see https://sepolia.etherscan.io/getRawTx?tx=0x66a9a218ea720ac6d2c9e56f7e44836c1541c186b7627bda220857ce34e2df7f
        let signature = Eip1559Signature {
            signature_y_parity: true,
            r: u256::from_str_hex(
                "0x7d097b81dc8bf5ad313f8d6656146d4723d0e6bb3fb35f1a709e6a3d4426c0f3",
            )
            .unwrap(),
            s: u256::from_str_hex(
                "0x4f8a618d959e7d96e19156f0f5f2ed321b34e2004a0c8fdb7f02bc7d08b74441",
            )
            .unwrap(),
        };
        let transaction = Eip1559TransactionRequest {
            chain_id: SEPOLIA_TEST_CHAIN_ID,
            nonce: TransactionNonce::from(6_u8),
            max_priority_fee_per_gas: WeiPerGas::new(0x59682f00),
            max_fee_per_gas: WeiPerGas::new(0x598653cd),
            gas_limit: GasAmount::new(56_511),
            destination: Address::from_str("0xb44B5e756A894775FC32EDdf3314Bb1B1944dC34").unwrap(),
            amount: Wei::new(1_000_000_000_000_000),
            data: hex::decode(
                "b214faa51d882d15b09f8e81e29606305f5fefc5eff3e2309620a3557ecae39d62020000",
            )
            .unwrap(),
            access_list: AccessList::new(),
        };
        let tx_hash = transaction.hash();
        assert_eq!(
            tx_hash.to_string(),
            "0x2d9e6453d9864cff7453ca35dcab86be744c641ba4891c2fe9aeaa2f767b9758"
        );

        let signed_transaction = SignedEip1559TransactionRequest::from((transaction, signature));
        assert_eq!(signed_transaction.raw_transaction_hex(), "0x02f89883aa36a7068459682f0084598653cd82dcbf94b44b5e756a894775fc32eddf3314bb1b1944dc3487038d7ea4c68000a4b214faa51d882d15b09f8e81e29606305f5fefc5eff3e2309620a3557ecae39d62020000c001a07d097b81dc8bf5ad313f8d6656146d4723d0e6bb3fb35f1a709e6a3d4426c0f3a04f8a618d959e7d96e19156f0f5f2ed321b34e2004a0c8fdb7f02bc7d08b74441");
        assert_eq!(
            signed_transaction.hash().to_string(),
            "0x66a9a218ea720ac6d2c9e56f7e44836c1541c186b7627bda220857ce34e2df7f"
        );
    }

    #[test]
    fn test_determine_signature_y_parity() {
        let public_key = PublicKey::parse_slice(
            &[
                0x02, 0xc6, 0x6e, 0x7d, 0x89, 0x66, 0xb5, 0xc5, 0x55, 0xaf, 0x58, 0x05, 0x98, 0x9d,
                0xa9, 0xfb, 0xf8, 0xdb, 0x95, 0xe1, 0x56, 0x31, 0xce, 0x35, 0x8c, 0x3a, 0x17, 0x10,
                0xc9, 0x62, 0x67, 0x90, 0x63,
            ],
            Some(PublicKeyFormat::Compressed),
        )
        .expect("public keys must be 33 or 65 bytes, serialized according to SEC 2");

        let message = Hash([
            0xaa, 0xdf, 0x7d, 0xe7, 0x82, 0x03, 0x4f, 0xbe, 0x3d, 0x3d, 0xb2, 0xcb, 0x13, 0xc0,
            0xcd, 0x91, 0xbf, 0x41, 0xcb, 0x08, 0xfa, 0xc7, 0xbd, 0x61, 0xd5, 0x44, 0x53, 0xcf,
            0x6e, 0x82, 0xb4, 0x50,
        ]);

        let sig = &[
            0xdc, 0x4d, 0xc2, 0x64, 0xa9, 0xfe, 0xf1, 0x7a, 0x3f, 0x25, 0x34, 0x49, 0xcf, 0x8c,
            0x39, 0x7a, 0xb6, 0xf1, 0x6f, 0xb3, 0xd6, 0x3d, 0x86, 0x94, 0x0b, 0x55, 0x86, 0x82,
            0x3d, 0xfd, 0x02, 0xae, 0x3b, 0x46, 0x1b, 0xb4, 0x33, 0x6b, 0x5e, 0xcb, 0xae, 0xfd,
            0x66, 0x27, 0xaa, 0x92, 0x2e, 0xfc, 0x04, 0x8f, 0xec, 0x0c, 0x88, 0x1c, 0x10, 0xc4,
            0xc9, 0x42, 0x8f, 0xca, 0x69, 0xc1, 0x32, 0xa2,
        ];

        assert_eq!(
            determine_signature_y_parity(&public_key, &message, sig),
            Some(true)
        )
    }
}

mod eth_get_block_by_number {
    use crate::numeric::{BlockNumber, Wei};
    use crate::rpc_declarations::{into_nat, Block, Quantity};

    #[test]
    fn should_deserialize_block() {
        const ETHEREUM_BLOCK: &str = r#"{
        "number": "0x10eb3c6",
        "hash": "0x85db6d6ad071d127795df4c5f1b04863629d7c2832c89550aa2771bf81c40c85",
        "transactions": [
            "0x62b791a3dd0d4af9a08ee216ae026f07cae1e80223b4d18002d3534ff87cd96d",
            "0x44df0fd08656db3243db02859a3086b4d5325366b747f222b5609f22052ff130",
            "0x6d4bf1167d28fc7d75185df120fc5688723521bdc6474668435dfa1a85e806aa",
            "0x84bb81777861de66b6ce0032b1fe3327fe4bd841aff2cc207910c730b93b2da7",
            "0xfac980b08a46c31a9f58acba52fd1459f6c7c9ec98e5e5414fc61ff2cbfa7ba1",
            "0x5b52da11ebbc46c83abe5bb187e7926362a06de92d898cecdf27bdfa4d321b08",
            "0xa12d8bec9a257c7e75429bf84fd355bc0d81ca40d7c2e937ae0aa5ad4b4be91b",
            "0x8174af141ce5ea8b13e43cc1bec2460d9b695ad2ebdbea8537fdcb27ae2b9d87",
            "0x43cafd8a89cd79be8856e7660134673f6c03f64df96f346e98ffc788cffc079f",
            "0x19616c135014db3f1c4ac756ed2e62368ad28ff36080e78644c2aaec543e9f47",
            "0xd3d96454d4fa9b9fd8af6160f0b1e5084f66d3c0182623e1bfca4b9cc5db70fc",
            "0xd654aaf9e47508096bc308a489e7bfb850633a5dcde6d545d07d285b2bcf0b92",
            "0x62f0d116bdc1ae05598b724c4ed412abcbf8b4ab1f1b7bb4d8acbd6ffe8f99a2",
            "0x685d40c7cf00c136cbe6748eb7dd096150dc4e1c80b273854623f0affe3f6e05",
            "0x3bb42e6bfd1d7199d3fe4931385f7ad29a21c7d86e63cdbd4d9b7fc63d04d22e",
            "0x0eeeb99bf26d429507ddc3787479445c7f0230fb4dfbb040896601ed0bd6a1fc",
            "0xb86c4956cce31bb7b41febcb720ff33f389462424f6ab0c058ce0007568c01f3",
            "0xe908b796c883d66bb5f521d91b4f05f2f83233e40dfc3eb785625897939ccb8c",
            "0xc32577489bac670888793ae3f705a94778bb6250ca005cbfeed947cdb114cfc0",
            "0xd7f02026282f19d117a2e8ba613c9256b0a82f0efd9dbfa3a5ef80bd0361ebbb",
            "0x4d383fd836f9af8f323c077e98df7614e5123aeedd1f11b746e484e8a21c86f4",
            "0xfb68fb20070408bf44aeba79fc2727ed6d8c049d54f09f8b269c80604ed0d493",
            "0xcba32003d860cfc685afd20c7396956c6369b8cbde60ce915d13d61a74738fa2",
            "0xe7bee7180020c01d3076ea7d525c57c0f686efe75a6bfeb53cc89b2eecfbcf56",
            "0xa85f3c4b11c91b0c286926cc758957cb87ab0c64f3fff04f92aa1295ec0c46f7",
            "0xee5f0c2e19ce0edd09c9f529815a92adf6fc209645ef841ddab0d977eea1b44e",
            "0x7066c558c0cdaa3384ca8098369067a6355bda1e8cffc8f8eae12509695b12bc",
            "0x5b6652796186cd9191fcadb2603759163910c1567a534257019f46aa38be240c",
            "0xb4a04c3e9b0c9b811d9b6a4dce0c4a03bcd71b990aed57aaa5e712dc641fb0ee",
            "0x1e9fbe872e0674ecc6d8f77aefcfa1ca5f4f719d1dd06df4c493978cd3f7d5e8",
            "0x4526d24fb35fa917bcad8d1f9c90b5678a1f9edecb3b4f755af766f24666c47a",
            "0xaf66a1c077a39bcf0cd43d43dbdaf1489b4d197f204dda3951216aa5db18ac2d",
            "0xe327f5d9beb074b7c4f82f0959f4a06a525417224fee26656d01edc2cdd05b9b",
            "0x156f7e9a92686a8bfb6daa5f13829a0f66df95afb443a1407b12f7c99eb4e7d7",
            "0x1c3353a7e896799c67d065e63688444949a70dfd3aeee7c1e936a36b3bf66fbb",
            "0xdcc6e041c3f7c7eadfeba232489fff50ae5916b4d4a7710ad8388005ebd21f71",
            "0x19adefeddd5db0eb1060da38c3c9e336a067fb791abbbe678b1d125e41047e68",
            "0x5a4bc5e1504a50d51832e58843e5ff0bb671e04152383f7023c7a2e1834a171d",
            "0x36b2c6986c8e937e87bd16e78f38e258441c9343eab4b1c3117b0bc56cc440c6",
            "0xae77bb110e850c5e243e209e8d56395d3e0fdb55a4520516c1637658205b434d",
            "0x144009d3c9c09e8b6950344bd5a6632e07173722471158c6a790c4c644c6936c",
            "0xa4fdf738ba7114689c4758362a8db1821f4f01f95d367861c95b7d79b59278f4",
            "0xd559f94c9b9cbb5f49e707a7ba4f944ea4027eabba7b0be36e1e8edbdbefd188",
            "0x92e58727292c4e88401218e28fcb736b21913322c424478de9aa1df61e0d6a01",
            "0xd83b4dbef3c1a27b997e0b8d5aa0245b29428f179d8f0d956e7ac218d004c901",
            "0xa7ad7f8291548f103a7bf542d2798cb5d2df4e66cb84b7fe90503f93920f2e92",
            "0x5b1584d243ae00ae0379de4c4af138a1814524da3219c5438c8170d92471875d",
            "0x6cac4f6e997b9c387f26bce0b7ff1cef140285cabd46812c8057466f33e9e9f3",
            "0x5420a43bb314f615c891bec93f19965238d5f8e6ed1c69c564edbe4fd2c4838d",
            "0x1fa594218737f6affadf0df0847e6281f2e251296fc2e09ef15c28e6b0299fee",
            "0x3f5ef12aa954f74afb30903bbb23fd3c0e60548c57a4a107beedbe8d34cd378b",
            "0xf4c7cde96177692fa28614df13693ff875d2dcca6e00007a07481e774a80a2b0",
            "0x82290b459874c0832f75b7c9923ab1bf8d3df5b68237419195e5f43fd204556b",
            "0x760dfe88e16a96847b9175a0a3321810c0b1b40393277cd598df3f44cc0a24a2",
            "0x596acfc21638a9401a5d251994d33a6a72c28ac43376af2f6085be003121e197",
            "0x798ba88f869412eac757a28b5d3e757178550b7796babd6f3864200a67d77338",
            "0x4c0b7494bb350955e4eb3bd014b54b96619e3e5ffcb0522af1f0df2484816668",
            "0x6e4a5ad3a496f891283bbad30cf6df6d0294a4a7ebe4b8306baaadb94558943f",
            "0x0bfe13a16a07454f905898eed38404ab3da5ede8eaf4476ee084e6c753c367d0",
            "0xa4c7c2950d38f0844bc0575e78cdf3dc510e11b20e2468e20f2121e39357a8c2",
            "0xabd0fefb3ea616d97bf75c38e95912f9d38ab14f7013cc53384b087e357e3057",
            "0x0b707a4b62cc8a45ee3ec635012fa1598c6561bbe648fea21b326415853f97d3",
            "0x43402a84d4857b4436ba6fe9a4d3953d9f3b514407c6051ba711f5912ec288be",
            "0xe304a0c0c6ce85084c5f4069af9e42a2ab79320f8faddecef897c4ca05aaf3a5",
            "0x98e354944d0e7d8a2f9fd549a597fe2cd87bac24a636cbd4323d718d77681d16",
            "0x3d5df7e021e3551799896052038c98ea1e513410180596f9ced84f5e4e2bfc34",
            "0xb5f07cf00b4f017b1c5c64871ef871a6fe0275c43d275d3753838ac71075e136",
            "0xc367d657ea9690d7c74744b4558b3411dab4ed1bd5c7e7437fbe2aa546370e15",
            "0x8f18d22d167750502790b52061c94d2ca447cdfa96eaa3c21ea55248a562e859",
            "0x8250d0df06f6d705c0829e3f6ce90f9b1a98436672ce7879469193f7581375d6",
            "0xdfe83e4446b89d8d2f3aa0c4bf3b5e9023818787315d54a28846e20ed48cc318",
            "0x0dce8c294656cdf1db610b3a5c287b6432443e76acf142602a4afedecfbe7f5f",
            "0xb3eb4710ea1e329f54ddf8c44501605ba4929ebc6b3f5ec519eefe5eb881264b",
            "0x9269bdf56daea54c93965278abb121425339ddad516059e63efd37ebead2b6bb",
            "0x334de6ac3deeb8530ca46a836502d8c74396ee4452d53aca85970cdd93112de9",
            "0xa6573280917fe5229fc38e9261e8d7708d8ca16123c5d9057ae34c1e84d46b15",
            "0x4f809c9d31e7a70eba98c9038beb1798e2a75da2595b3b4ca6f9d0d779efada8",
            "0x3664f09f724db034d3de364785c355d47854ace9a530814d63d522f95e69b01f",
            "0x38fd23df69ada8063a8016648474b187c0561c2d0832386fa7be9bc78b6bcb10",
            "0x54f30ef019541a329916c8161908381aaffd6cedf95a3f9b041ac9be5914be2c",
            "0x5593cd9b94ffbfcd38cc8bc52aeb8f361663c839c3f047f1008ab3b4139953a9",
            "0x54b82b949d340078cf88d9e6d937f655678b47eaf3490fca39d5ea75d5caa9fa",
            "0x5a04b1a0b29fab99861e02aae11704602fb27a8ce756901d37dd89cd08181dd8",
            "0x36e270a038bb87f1628154513c40f85f787e0d32275f73ed1b47d7c4fcd0e1ee",
            "0x5d324597d2156795402b0eecdcdf6d94add668100bc67768b59eb97d87523418",
            "0x96d0392d953bb9ef1a9c4f372ed844f8f03c73a30c52b522820d4d09a77baa86",
            "0xf1bbf23f813cc8161f18ef1015fa07425b0e43a7e0b95f6ba728e07d764738ca",
            "0x016cc3d2a4d953277f709d406556373cc0381b4c77e226d3293f51156e3e0bf0",
            "0xdd730d8d5a2fe0320a65dc027d38ea16b8f03890192560ed34b245c976651577",
            "0xb4c5651d82081528896121b36c9d2dcf73ddd88146f650292bb324570c8f2171",
            "0x8288b04bfc3ea4ce52fabccec23f388e3ad00a14b139799c6f0ab3c4ad0b938b",
            "0x532a5c61c4f69fc2f425140cf70cc218bf2b3a7216fc6b7313d8d464e62a5748",
            "0x2ada6c883d082dbecbf349b452578bd0a286a2560484a4fcc88b353c0b81d676",
            "0x287447d7d206b46d88e65ade12780468f700f80557f4ef1c22182a6236c7bf30",
            "0xd6fdfc6fe73f0ed9cfd94b6676e627abee2313586c349327eceacae1cf5e36bd",
            "0xd2b83ecab206d94ce46455df46c1470db6405600b41935b1f0d52df5ebc9ba97",
            "0x50f7ba81889644d40770a9c6616671d4ddede9489537fa6e2a43bd03c4d4ae1e",
            "0x0392f98e481256d85c49d13e62b6e74469a983f8e0ce9485527b6fa70c9390fc",
            "0x9330393697e32400b8af310b5883fb9d660362ab158a7cb9657bb2ace83e5829",
            "0x3253b9a59c8b730139b87b56238cc99a83fb923045b2b46824fee0e3c451baaa",
            "0x07783fd1779b0fa3ab4c963e534ed90ada682434de7e469472166fa849a2858f",
            "0xe93851b360b27c737d3b2e3c9eee6224860085093947d9ca7354cc9e562ae4c4",
            "0x770497791659d958f60e518c5b6bb3861c36530489fdc477de8e43ea557e16ea",
            "0x58cdf8c150c14bfa163ff44407fc662d16bf471362f76bf68025bd856549375c",
            "0xf8c17408cfdb3250d1158e41db18d4f500c75aebb5b5ae3229876c83df9befde",
            "0xce6a1efb6d872fff826a20e5940e6403e192ae6657f62f82fe44a42721ecd992",
            "0xda13b6a78ee57589268ec34cee19d2376cbed8bcd129dd9db7037da8cde16cff",
            "0x8479a242cfac20255d850e15d32fe2f4437407a3760bb7c4931742ab4c54e131",
            "0xc77453f8a7c1b5a77ead2cbaca5f1b23a855910166f012dcbf1651294dd39c94",
            "0x34caaa155e8e4d5cc34befa7001847ca6dd436caff08ee3363dbb12b9d36034d",
            "0x961a6fed9f96d6337a73dc8f5800a627abd11d34d71705b6ea36ab9cb9646b65",
            "0xa4a30487560b41cee6a1081e8998f3ba5187f0fac6e7bb6a207dfb4d1081ba87",
            "0x907cacc34b377875207a3bec0adedd04955a36822338f4f952b74c113cf0c752",
            "0xc7ad96cf911a17e4c63e07f2f518139c0610969684f588815a881a0efd174125",
            "0x25da212672495c57d7884264b9a3062e5f5b77bc7db6eba349ed3ddb3ed30bce",
            "0x0c62b7b4a990b4cbca60e09be90c7cd1f123d5581f62498173e90a529c3f94c8",
            "0x5de05145b8799900245241c127d27ef684a367de376985907a97fd1b03bb6d32",
            "0x095750ec64c090c9ae6f0c68d372b5150b1ecdbec13200db210b3eba0e4534fb",
            "0x36247d43ccfe2e421fecc2b3aacba1d5215875881f3257171728800cb0ca9e7a",
            "0xb1375c4f9869dfa97d35fc69f5a6a012614d1b70e8877308d10835dbd3499e35",
            "0x8d98cf91cdaf1a2380ecb72a5dc10491bbddc134d9b5d18f2d4ed45c5f44f7b1",
            "0xbca977f8a3788c4a2bc1c2e9169a513bec5d5525a99f950e72122294e2f80804",
            "0x993b04d6509dd5653b4b050ed50b2c4df1c0ab778e154c59831cb9fa7b026c01",
            "0xc81c6099fc96506098270a22ff082840e06b37ff1d73e777658174192df8d183",
            "0xcca9e8021b71ad27794de26e4d09457c6c4c0d88fcfaf5539e12bec2c8e57a98",
            "0xeb0d657dc818ee0f7110ff124b0b81b911db7a117514c9599143c81f9d694ba8",
            "0x8b3e2167958f59f576154cc60fa08f0e16b585d4aae31eeeb345fd969dce4402",
            "0x9fb1693ed00151d4de33424ca78bbc9d284b20ac1be89ce497a182b8c31b94e0",
            "0x7f6cdf3939aeeb3c05a45bd401adffffc890d06d288af3429c355c90e820ee60",
            "0xd5cc040c1d65e575c2547db1949d94f3b813613022fef243903a0fca5af19c0c",
            "0x0f04f089a244cdae05fedd43d8a0000bcfbb1941eb177031e50b061f6fe31f3c",
            "0x03fdd0961c36be705e547a15564bd87c6c0b5233b42c3236e41ac1dd0f77a14e",
            "0x2ef3ae2a1fb31ae2a7c3dd0d9666b9150f829a60c8cf05961062f6b030e68253",
            "0xa237468fc9b57904cfc7756c711c68c457468290efa6be38b2f3938643cdfe28",
            "0x17decde3c88030e58144177a4d1701dbfc7a3651599ddba83096adb9f33dc0b4",
            "0xb7b10c7a1d487a7670037c663f05278e38ea3dd41a78323a8775a39a5ad2c30d",
            "0x9fbbcec87717c40d3623ca43d62618ad59d16bf8b01845bc329c830fcc7a2e3e",
            "0xacdfa67d39691abf4b673d194e5b02413ceec0e8afda3bad429ff19a716f43db",
            "0x948e3846bf6134637b24721bc3f2dcba749fd8af35100ef7760ac1e0280944de",
            "0x22d309af1b225bc2b56eab356433d8a27b007f5b8313b210fca8f3a4394f664e",
            "0x003ae399e22355dec73c16e8522b14548cfddbab6e45f4696de3a12b9c140f5a",
            "0xa411dae68f38b015f833647d27f59fe22e17d9bde8175816a36dfb9da42c4208",
            "0xa9552ab8ae7f55473f27c1b5575874831531a764d375ebed35f9369834347949",
            "0xe4e58ca2bf41a8eb909132ac6911dae611565d34063e90f0872122c8cf549b64",
            "0x04aedeb0d0b7fcc1abe123a5c8ff21e821f136509c61259562ce7157971f7414",
            "0x7998d83ac9c2cde47f9bed245c47cbd61bc53d0dece88bf14f4041992e41752b",
            "0xf8733b5ce7cb87e41aac9445f7c87ebb9ea0e6e467ec95f16d5806650864f073",
            "0xabf9e3dbd51c6f2990c245b8061d6222f0ce722b34583062a90193a4330b4b05",
            "0xb6ec489105d66142c15ec3e390ef22014b741f308688d3cdf0aaaacb2f74c68b",
            "0xee8c0b6e00203e88e95386b59a5566352208acf54e1401c3c9111f8ec98648f6",
            "0x5c5e1e50b83074b0f4bd26bb1f302855909c71ff50aedd935ad088209d9b8e24",
            "0xce9c2b8f9a60d0886b85a4577bfaab1f87a6c36b24476ee9ba6622e293daf921",
            "0x6b29f094c7120022eec3b3171807b2a540f780afb4807c2bf3b74cfc5801ba3d",
            "0x56bc2e289ac59c6172ca8b0841d44e7a016d1622be812be1d0ab096facb8b0ff",
            "0x3702307e7ea240a600bb76277b3a1657a4f9e657ca7f14840b28c21724e6c8fc",
            "0xbe8f0fa8247317ffb26c98e322adf37147dd4795e34913a77dffe11b13e10475",
            "0xe6ee7f24903a005ec53224ae4afeacc626e2304cdd2e5e4ae0e6deaa718e5e80",
            "0x2fcec74388113371a8dca6ffcaff44db3d42d119254050c6fa3475c0d3915f34",
            "0x75d4200515493982e1e846022692cbee1000e5f63b9875f48c0101203429e25e",
            "0xc5365dfcf91ef8bcd55e971237b211ca53430ee2d597a04999a395c0fc6db0aa",
            "0xea3d83e41a9d962b0cc9595a2b55e10448098181f67d5cfb55a6b4a76c447fbd",
            "0x91083e71babe1b481b92dab78048b039ddd6080b07893223eee6a033756f425f",
            "0x5962cbdca2e6eb9664cde5844706bbcd29d5e6493c2ff349f218a20fa60a09a5",
            "0xe4ab7d0325d4d9b7ebb6983431189f6e294274ecdb50ab5892e20c5020a02b44",
            "0x72319c14c994353f193a6cc5b1ad5a0efc25f49a88526a00d38473cbee715360",
            "0x469001fbc0db7460cd8f75868542f4c8f582d51866bb6f117b6940b8b2d8c6c3",
            "0xb8be42cd522de6fb9de53618356e3b59ebdecad22667098c3393ccc75c2b9028",
            "0x5b39ce71fa3c572ea559fdf1640417412fb6a337e9617dfa5c97399119a7d5c4",
            "0xd17bbce51d9f32c01a1c6f2e272bfba9ba18c5554c984fa740e9350b6a3046bf",
            "0x6056769bedb0cb3f3e245014f5ca6c3d9e58b1609fc48d9297c81f2b91d1e3f3",
            "0xa36f9dd0b7446b99c1b9de2b430994a3e75a9c1190082470fd9561e53b909f8f",
            "0x92c77300454154640fc672d010c077bcff89ba0811a4bcb6705591765e5839b5",
            "0xa993c8d109bc4f6166ecfd28a43542028f17704727badfe60265540b6fc4f131",
            "0xd746eeb1b20f66af84cde400d53c5c0710bd09e67ca2ab15ef671c20603f027a",
            "0x6715e947733d7603a212a78ebd5b4511d5a17b0743bb69e77fb0094641d65dab",
            "0x18ad0ca5c4c80863e7a2942eb6a8a2a0f8f9dee5e56a6e5920a35b589bbb27e2",
            "0xa854adb509b151557cccabb3bcf773ee4dd150df6099831b9b997deaf2d3ca66",
            "0x136beb59a04fbdc2d1018a77ba123b767d45b6c56d26fb50b29dc830b8c2061e",
            "0xfe51b67789651dc814020b9f7dfddbb8d140dddb027cf44e93a0d22d713abe81",
            "0x0ac593b42d17924cda625932202fdf9ed06a23f9cb7728eb85bab29fe69f8b9b",
            "0xb958fd1a0d916030ee38da38e6da8dbb7d16224db4bcc0279e94e67e2757bc8a",
            "0x1972bbdf825bbb0604beda81df6a16ad158e559d855d25387914ea66e759b0be",
            "0xf1fbc8937a04325034efe4ed9a93be92ab4bc3d0602ba8924dc6b650e075bf30",
            "0xa99578c9f499dca6075ead96fe35e46aa81bfee217afa3ffeb9240c92878d113",
            "0xd1ceaf440eae405793aa9eb89d0f23c591b878d0fc179883d3131d2b5849d273",
            "0xd542cbe096cf71ca8a0c97abc7de94f012c1d6c87fafe9c643d73a0f57338afd",
            "0x08d4f0affbf448fbe44d15a1788d81a38ca7c17c33a9dfa0d442b9dded649d4e",
            "0xaba624b0bc5b38fff847ae2ed3bf48e235b62ec91dc7d0d4263e7956f40e7430",
            "0x042ff663ad0a8ce7e189b94be28d1702d3fab965197f3d0d61d51c837f5bd104",
            "0xf73d96a5259d491a161e8e108c6c393990534cc696ef4a38c898f49dfe2b22ef",
            "0x63523befdb85e5016e2755f3c7fbf75656ca456d828c02c0d14e6857d5066f77",
            "0x448a6f0e4ebb4b187a410b9a0491d8c9a6c7fda1ec20751c32739c0395a2fc98",
            "0xf56507339221f3a4d607bd8b5db7b0ebdfd43f751648afc71a8587d5aa4b6521",
            "0x3829ea8f4312fc3c69fea37003cbe43f7745c616bc3fd5bff8fef99e35bad75b"
        ],
        "difficulty": "0x0",
        "extraData": "0x6275696c64657230783639",
        "gasLimit": "0x1c9c380",
        "gasUsed": "0xd447a0",
        "logsBloom": "0xcdb111024104125e7188052bbd09fb21d8b08419130094a16401d7a6b605df8060b5f29682d5e7b072303f06c3299750de01e29aea01e9b75e70c4cd752f6d60381244097518a92c5974c28b8389202aa12a738008641e05ed45d5f498668eb47a12ed8a2a62dd03a75d39f938e17c4fa3f7066c30001d45f20a3cdd008854222a3cff6e860cf993c26d9521834e77aea0c5209109435088ec85fd4703107cacfee407e909b1b1a72a1957d19b9e440484061401a11260ea906b9326ae5a92e8591e74b6008062532f8c842037b0ac8480e51222268d72d68efac0226815e0cc3f58600c3be8a0f80e853eefa3216baa850f779a99fc87d60421384150a3a483",
        "miner": "0x690b9a9e9aa1c9db991c7721a92d351db4fac990",
        "mixHash": "0x4dd122a99169327413ec6533fd70a9a9a9cbfad627d356d9b1dc67a47f61b936",
        "nonce": "0x0000000000000000",
        "parentHash": "0xeb080e615e8d1583a5e5cbe3daaed23cf408ae64da2c7352691e00b6e1ffdf89",
        "receiptsRoot": "0xb07ebab433f52fd6dc24297a7804a40578ae0201060aa5938a5a57f4a3a05e03",
        "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
        "size": "0x29ade",
        "stateRoot": "0x675aa943df0011c3b47038b8365db65ce2d41fcdf3e4bcfb2076f1dfd2dabca4",
        "timestamp": "0x64ba5557",
        "totalDifficulty": "0xc70d815d562d3cfa955",
        "transactionsRoot": "0x42bdb666db19f89d6b6d16e125c49bd15143e062665e00287da5fda10e0d95c0",
        "uncles": [],
        "baseFeePerGas": "0x4b85a0fcd",
        "withdrawalsRoot": "0xedaa8043cdce8101ef827863eb0d808277d200a7a0ee77961934bd235dcb82c6",
        "withdrawals": [
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdbdc02",
                "index": "0xac512e",
                "validatorIndex": "0x932ef"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdbf4a4",
                "index": "0xac512f",
                "validatorIndex": "0x932f0"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xddd100",
                "index": "0xac5130",
                "validatorIndex": "0x932f1"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdc4122",
                "index": "0xac5131",
                "validatorIndex": "0x932f2"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0x30720f2",
                "index": "0xac5132",
                "validatorIndex": "0x932f3"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdbf545",
                "index": "0xac5133",
                "validatorIndex": "0x932f4"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdcfd43",
                "index": "0xac5134",
                "validatorIndex": "0x932f5"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xddb901",
                "index": "0xac5135",
                "validatorIndex": "0x932f6"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdceb8f",
                "index": "0xac5136",
                "validatorIndex": "0x932f7"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdc457b",
                "index": "0xac5137",
                "validatorIndex": "0x932f8"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdbfe3f",
                "index": "0xac5138",
                "validatorIndex": "0x932f9"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdd04c5",
                "index": "0xac5139",
                "validatorIndex": "0x932fa"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xda6118",
                "index": "0xac513a",
                "validatorIndex": "0x932fb"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdb5124",
                "index": "0xac513b",
                "validatorIndex": "0x932fc"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdd057f",
                "index": "0xac513c",
                "validatorIndex": "0x932fd"
            },
            {
                "address": "0x80b2886b8ef418cce2564ad16ffec4bfbff13787",
                "amount": "0xdcdc5b",
                "index": "0xac513d",
                "validatorIndex": "0x932fe"
            }
        ]
    }"#;

        let block: Block = serde_json::from_str(ETHEREUM_BLOCK).unwrap();

        assert_eq!(
            block,
            Block {
                number: BlockNumber::new(0x10eb3c6),
                base_fee_per_gas: Wei::new(0x4b85a0fcd),
            }
        )
    }

    #[test]
    fn should_convert_quantity_to_nat() {
        let quantity = Quantity::new(0x4b85a0fcd); //20_272_779_213 wei
        let nat = into_nat(quantity);
        assert_eq!(nat.to_string(), "20_272_779_213")
    }
}

mod eth_fee_history {
    use crate::numeric::{BlockNumber, WeiPerGas};
    use crate::rpc_declarations::{BlockSpec, BlockTag, FeeHistory, FeeHistoryParams, Quantity};

    #[test]
    fn should_serialize_fee_history_params_as_tuple() {
        let params = FeeHistoryParams {
            block_count: Quantity::from(5_u8),
            highest_block: BlockSpec::Tag(BlockTag::Finalized),
            reward_percentiles: vec![10, 20, 30],
        };
        let serialized_params = serde_json::to_string(&params).unwrap();
        assert_eq!(serialized_params, r#"["0x5","finalized",[10,20,30]]"#);
    }

    #[test]
    fn should_deserialize_eth_fee_history_response_base() {
        const BASE_FEE_HISOTRY: &str = r#"{"baseFeePerGas":["0x11535c","0x114116","0x114768","0x11393d","0x1141b3","0x1151fa","0x114dcb","0x113f39","0x112bdf","0x114e6f","0x116881"],"gasUsedRatio":[0.26466441333333335,0.35718779333333334,0.27994958,0.36532474,0.39474598,0.31759408666666666,0.27850063333333336,0.2602731933333333,0.46438367333333336,0.41186010666666667],"baseFeePerBlobGas":["0x1","0x1","0x1","0x1","0x1","0x1","0x1","0x1","0x1","0x1","0x1"],"blobGasUsedRatio":[0,0,0,0,0,0,0,0,0,0],"oldestBlock":"0x1e85374","reward":[["0xf5a3e"],["0x127685"],["0x110ea7"],["0x182276"],["0xf47f5"],["0xf4240"],["0xf466f"],["0x116739"],["0xf685b"],["0x12732f"]]
    }"#;
        let fee_history: Result<FeeHistory, _> = serde_json::from_str(BASE_FEE_HISOTRY);

        assert!(fee_history.is_ok())
    }

    #[test]
    fn should_deserialize_eth_fee_history_response() {
        const ETH_FEE_HISTORY: &str = r#"{
        "baseFeePerGas": [
            "0x729d3f3b3",
            "0x766e503ea",
            "0x75b51b620",
            "0x74094f2b4",
            "0x716724f03",
            "0x73b467f76"
        ],
        "gasUsedRatio": [
            0.6332004,
            0.47556506666666665,
            0.4432122666666667,
            0.4092196,
            0.5811903
        ],
        "oldestBlock": "0x10f73fc",
        "reward": [
            [
                "0x5f5e100",
                "0x5f5e100",
                "0x68e7780"
            ],
            [
                "0x55d4a80",
                "0x5f5e100",
                "0x5f5e100"
            ],
            [
                "0x5f5e100",
                "0x5f5e100",
                "0x5f5e100"
            ],
            [
                "0x5f5e100",
                "0x5f5e100",
                "0x5f5e100"
            ],
            [
                "0x5f5e100",
                "0x5f5e100",
                "0x180789e0"
            ]
        ]
    }"#;

        let fee_history: FeeHistory = serde_json::from_str(ETH_FEE_HISTORY).unwrap();

        assert_eq!(
            fee_history,
            FeeHistory {
                oldest_block: BlockNumber::new(0x10f73fc),
                base_fee_per_gas: vec![
                    WeiPerGas::new(0x729d3f3b3),
                    WeiPerGas::new(0x766e503ea),
                    WeiPerGas::new(0x75b51b620),
                    WeiPerGas::new(0x74094f2b4),
                    WeiPerGas::new(0x716724f03),
                    WeiPerGas::new(0x73b467f76)
                ],
                reward: vec![
                    vec![
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x68e7780)
                    ],
                    vec![
                        WeiPerGas::new(0x55d4a80),
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x5f5e100)
                    ],
                    vec![
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x5f5e100)
                    ],
                    vec![
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x5f5e100)
                    ],
                    vec![
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x5f5e100),
                        WeiPerGas::new(0x180789e0)
                    ]
                ],
            }
        )
    }
}

fn test_state() -> State {
    let mut erc20_tokens = DedupMultiKeyMap::default();
    erc20_tokens
        .try_insert(
            "mxzaz-hqaaa-aaaar-qaada-cai".parse().unwrap(),
            "0x779877A7B0D9E8603169DdbD7836e478b4624789"
                .parse()
                .unwrap(),
            "ckUSDC".parse().unwrap(),
        )
        .unwrap();

    let mut wrapped_icrc_tokens = DedupMultiKeyMap::default();

    wrapped_icrc_tokens
        .try_insert(
            "ryjl3-tyaaa-aaaaa-aaaba-cai".parse().unwrap(),
            "0x9876543210fedcba9876543210fedcba98765432"
                .parse()
                .unwrap(),
            None,
        )
        .unwrap();

    State {
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
        withdrawal_transactions: WithdrawalTransactions::new(0_u64.into()),
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
        last_observed_block_time: None,
        withdrawal_native_fee: None,
        events_to_release: Default::default(),
        released_events: Default::default(),
        quarantined_releases: Default::default(),
        icrc_balances: Default::default(),
        wrapped_icrc_tokens,
        last_log_scraping_time: None,
        dex_canister_id: None,
        twin_usdc_info: None,
        swap_contract_address: None,
        is_swapping_active: false,
        swap_events_to_mint_to_appic_dex: Default::default(),
        last_native_token_usd_price_estimate: None,
        canister_signing_fee_twin_usdc_amount: None,
        gas_tank: GasTank::default(),
        next_swap_ledger_burn_index: None,
        quarantined_dex_orders: Default::default(),
        swap_events_to_be_notified: Default::default(),
        notified_swap_events: Default::default(),
    }
}

pub fn source(txhash: &str, index: u64) -> EventSource {
    EventSource {
        transaction_hash: txhash.parse().unwrap(),
        log_index: LogIndex::from(index),
    }
}
