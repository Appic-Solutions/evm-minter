use std::str::FromStr;
use std::time::Duration;

use alloy_primitives::Address;
use candid::{Int, Nat, Principal};
use icrc_ledger_types::icrc1::account::Account;
use icrc_ledger_types::icrc2::approve::{ApproveArgs, ApproveError};
use pocket_ic::PocketIc;

use crate::candid_types::chain_data::ChainData;
use crate::candid_types::{
    ActivateSwapReqest, AddErc20Token, CandidBlockTag, MinterInfo, RequestScrapingError,
};
use crate::evm_config::EvmNetwork;
use crate::lifecycle::{InitArg, MinterArg};
use crate::tests::dex_types::{
    CandidMinter, CandidPathKey, CandidPoolId, CandidPoolState, CreatePoolArgs, CreatePoolError,
    MintPositionArgs, MintPositionError, QuoteArgs, QuoteError, QuoteExactParams,
    UpgradeArgs as DexUpgradeArgs,
};
use crate::tests::lsm_types::{
    AddErc20Arg, AddErc20Error, Erc20Contract, LedgerInitArg, LedgerManagerInfo,
};
use crate::tests::minter_flow_tets::mock_rpc_https_responses::{
    generate_and_submit_mock_http_response, MOCK_BASE_BLOCK_NUMBER, MOCK_BASE_FEE_HISTORY_INNER,
    MOCK_BASE_FEE_HISTORY_RESPONSE, MOCK_BASE_HIGHER_BLOCK_NUMBER, MOCK_BSC_BLOCK_NUMBER,
    MOCK_BSC_FEE_HISTORY_INNER, MOCK_BSC_FEE_HISTORY_RESPONSE, MOCK_BSC_HIGHER_BLOCK_NUMBER,
    MOCK_GET_BASE_LOGS_ERC20, MOCK_GET_BSC_LOGS_ERC20, MOCK_GET_LOGS_EMPTY,
    MOCK_SEND_TRANSACTION_ERROR, MOCK_SEND_TRANSACTION_SUCCESS,
    MOCK_TRANSACTION_COUNT_BASE_FINALIZED, MOCK_TRANSACTION_COUNT_BASE_LATEST,
    MOCK_TRANSACTION_COUNT_BSC_FINALIZED, MOCK_TRANSACTION_COUNT_BSC_LATEST,
    MOCK_TRANSACTION_COUNT_LATEST_ERC20, MOCK_TRANSACTION_RECEIPT_APPROVE_BASE_ERC20,
    MOCK_TRANSACTION_RECEIPT_APPROVE_BSC_ERC20, MOCK_TRANSACTION_RECEIPT_APPROVE_ERC20,
};
use crate::tests::pocket_ic_helpers::{
    create_appic_helper_canister, create_evm_rpc_canister, create_icp_ledger_canister,
    create_lsm_canister, create_pic, encode_call_args, five_ticks, icp_principal,
    install_appic_helper_canister, install_evm_rpc_canister, install_icp_ledger_canister,
    install_lsm_canister, lsm_principal, minter_principal, query_call, sender_principal,
    update_call, DEX_CANISTER_BYTES, INDEX_WAM_BYTES, LEDGER_WASM_BYTES, MINTER_WASM_BYTES,
    PROXY_CANISTER_BYTES, TWENTY_TRILLIONS, TWO_TRILLIONS,
};
use crate::{APPIC_CONTROLLER_PRINCIPAL, RPC_HELPER_PRINCIPAL, SCRAPING_CONTRACT_LOGS_INTERVAL};

use super::super::ledger_arguments::{
    ArchiveOptions, FeatureFlags as LedgerFeatureFlags, IndexArg, IndexInitArg,
    InitArgs as LedgerInitArgs, LedgerArgument, MetadataValue as LedgerMetadataValue,
};

pub fn create_proxy_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("epulg-riaaa-aaaaj-a2erq-cai").unwrap(),
    )
    .unwrap()
}
pub fn install_proxy_canister(pic: &PocketIc, canister_id: Principal) {
    pic.install_canister(
        canister_id,
        PROXY_CANISTER_BYTES.to_vec(),
        vec![],
        Some(sender_principal()),
    );
}

// BNB SMART chain minter
pub fn bsc_minter_principal() -> Principal {
    Principal::from_text("2ztvj-yaaaa-aaaap-ahiza-cai").unwrap()
}

fn create_bsc_minter_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(Some(sender_principal()), None, bsc_minter_principal())
        .expect("Should create the canister")
}

fn install_bsc_minter_canister(pic: &PocketIc, canister_id: Principal) {
    let init_args = MinterArg::InitArg(InitArg {
        evm_network: crate::evm_config::EvmNetwork::BSC,
        ecdsa_key_name: "key_1".to_string(),
        helper_contract_address: Some("0x733a1beef5a02990aad285d7ed93fc1b622eef1d".to_string()),
        native_ledger_id: "n44gr-qyaaa-aaaam-qbuha-cai".parse().unwrap(),
        native_index_id: "eysav-tyaaa-aaaap-akqfq-cai".parse().unwrap(),
        native_symbol: "icBNB".to_string(),
        block_height: CandidBlockTag::Latest,
        native_minimum_withdrawal_amount: Nat::from(200_000_000_000_000_u128),
        native_ledger_transfer_fee: Nat::from(10_000_000_000_000_u128),
        next_transaction_nonce: Nat::from(0_u128),
        last_scraped_block_number: Nat::from(23_402_960_u128),
        min_max_priority_fee_per_gas: Nat::from(100_000_000_u128),
        ledger_suite_manager_id: "kmcdp-4yaaa-aaaag-ats3q-cai".parse().unwrap(),
        deposit_native_fee: Nat::from(0_u8),
        withdrawal_native_fee: Nat::from(100_000_000_000_000_u64),
    });
    let init_bytes = candid::encode_one(init_args).unwrap();

    pic.install_canister(
        canister_id,
        MINTER_WASM_BYTES.to_vec(),
        init_bytes,
        Some(sender_principal()),
    );
}

fn create_bsc_native_ledger_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("n44gr-qyaaa-aaaam-qbuha-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_bsc_native_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: LedgerFeatureFlags = LedgerFeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let minter_id = bsc_minter_principal();

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: Account::from(minter_id),
        fee_collector_account: Some(Account {
            owner: minter_id,
            subaccount: Some(LEDGER_FEE_SUBACCOUNT),
        }),
        initial_balances: vec![],
        transfer_fee: Nat::from(10_000_000_000_000_u128),
        decimals: Some(18_u8),
        token_name: "icBNB".to_string(),
        token_symbol: "icBNB".to_string(),
        metadata: vec![(
            "icrc1:logo".to_string(),
            LedgerMetadataValue::Text("TOKEN_LOGO".to_string()),
        )],
        archive_options: ArchiveOptions {
            trigger_threshold: 2_000,
            num_blocks_to_archive: 1_000,
            node_max_memory_size_bytes: Some(THREE_GIGA_BYTES),
            max_message_size_bytes: None,
            controller_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap(),
            more_controller_ids: Some(vec![sender_principal()]),
            cycles_for_archive_creation: Some(2_000_000_000_000_u64),
            max_transactions_per_response: None,
        },
        max_memo_length: Some(MAX_MEMO_LENGTH),
        feature_flags: Some(ICRC2_FEATURE),
        index_principal: None,
    });

    pic.install_canister(
        canister_id,
        LEDGER_WASM_BYTES.to_vec(),
        encode_call_args(ledger_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

fn create_bsc_index_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("eysav-tyaaa-aaaap-akqfq-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_bsc_index_canister(pic: &PocketIc, canister_id: Principal) {
    let index_arg = Some(IndexArg::Init(IndexInitArg {
        ledger_id: Principal::from_text("n44gr-qyaaa-aaaam-qbuha-cai").unwrap(),
        retrieve_blocks_from_ledger_interval_seconds: None,
    }));

    pic.install_canister(
        canister_id,
        INDEX_WAM_BYTES.to_vec(),
        encode_call_args(index_arg).unwrap(),
        Some(sender_principal()),
    );
}

// BASE minter
pub fn base_minter_principal() -> Principal {
    Principal::from_text("4ati2-naaaa-aaaad-qg6la-cai").unwrap()
}

fn create_base_minter_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(Some(sender_principal()), None, base_minter_principal())
        .expect("Should create the canister")
}

fn install_base_minter_canister(pic: &PocketIc, canister_id: Principal) {
    let init_args = MinterArg::InitArg(InitArg {
        evm_network: crate::evm_config::EvmNetwork::Base,
        ecdsa_key_name: "key_1".to_string(),
        helper_contract_address: Some("0x576849BEA9397fb33a992C7D5a5e1641c94532Fa".to_string()),
        native_ledger_id: "3iven-myaaa-aaaai-q3u5q-cai".parse().unwrap(),
        native_index_id: "cpbhu-5iaaa-aaaad-aalta-cai".parse().unwrap(),
        native_symbol: "icETH.base".to_string(),
        block_height: CandidBlockTag::Latest,
        native_minimum_withdrawal_amount: Nat::from(100_000_000_000_000_u128),
        native_ledger_transfer_fee: Nat::from(5_000_000_000_000_u128),
        next_transaction_nonce: Nat::from(0_u128),
        last_scraped_block_number: Nat::from(572_530_664_u128),
        min_max_priority_fee_per_gas: Nat::from(1_000_000_u128),
        ledger_suite_manager_id: "kmcdp-4yaaa-aaaag-ats3q-cai".parse().unwrap(),
        deposit_native_fee: Nat::from(0_u8),
        withdrawal_native_fee: Nat::from(15_000_000_000_000_u128),
    });
    let init_bytes = candid::encode_one(init_args).unwrap();

    pic.install_canister(
        canister_id,
        MINTER_WASM_BYTES.to_vec(),
        init_bytes,
        Some(sender_principal()),
    );
}

fn create_base_native_ledger_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("3iven-myaaa-aaaai-q3u5q-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_base_native_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: LedgerFeatureFlags = LedgerFeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let minter_id = base_minter_principal();

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: Account::from(minter_id),
        fee_collector_account: Some(Account {
            owner: minter_id,
            subaccount: Some(LEDGER_FEE_SUBACCOUNT),
        }),
        initial_balances: vec![],
        transfer_fee: Nat::from(5_000_000_000_000_u128),
        decimals: Some(18_u8),
        token_name: "icETH.base".to_string(),
        token_symbol: "icETH.base".to_string(),
        metadata: vec![(
            "icrc1:logo".to_string(),
            LedgerMetadataValue::Text("TOKEN_LOGO".to_string()),
        )],
        archive_options: ArchiveOptions {
            trigger_threshold: 2_000,
            num_blocks_to_archive: 1_000,
            node_max_memory_size_bytes: Some(THREE_GIGA_BYTES),
            max_message_size_bytes: None,
            controller_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap(),
            more_controller_ids: Some(vec![sender_principal()]),
            cycles_for_archive_creation: Some(2_000_000_000_000_u64),
            max_transactions_per_response: None,
        },
        max_memo_length: Some(MAX_MEMO_LENGTH),
        feature_flags: Some(ICRC2_FEATURE),
        index_principal: None,
    });

    pic.install_canister(
        canister_id,
        LEDGER_WASM_BYTES.to_vec(),
        encode_call_args(ledger_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

fn create_base_index_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("cpbhu-5iaaa-aaaad-aalta-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_base_index_canister(pic: &PocketIc, canister_id: Principal) {
    let index_arg = Some(IndexArg::Init(IndexInitArg {
        ledger_id: Principal::from_text("3iven-myaaa-aaaai-q3u5q-cai").unwrap(),
        retrieve_blocks_from_ledger_interval_seconds: None,
    }));

    pic.install_canister(
        canister_id,
        INDEX_WAM_BYTES.to_vec(),
        encode_call_args(index_arg).unwrap(),
        Some(sender_principal()),
    );
}

fn dex_principal_id() -> Principal {
    Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap()
}

fn create_dex_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_dex_canister(pic: &PocketIc, canister_id: Principal) {
    pic.install_canister(
        canister_id,
        DEX_CANISTER_BYTES.to_vec(),
        encode_call_args(()).unwrap(),
        Some(sender_principal()),
    );

    five_ticks(pic);
    five_ticks(pic);

    pic.upgrade_canister(
        canister_id,
        DEX_CANISTER_BYTES.to_vec(),
        encode_call_args(DexUpgradeArgs {
            upgrade_minters: Some(vec![
                CandidMinter {
                    id: bsc_minter_principal(),
                    chain_id: 56,
                    twin_usdc_principal: ic_usdc_bsc_principal(),
                    usdc_address: "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d".to_string(),
                },
                CandidMinter {
                    id: base_minter_principal(),
                    chain_id: 8453,
                    twin_usdc_principal: ic_usdc_base_principal(),
                    usdc_address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(),
                },
            ]),
        })
        .unwrap(),
        Some(sender_principal()),
    )
    .unwrap();
}

fn ck_usdc_principal() -> Principal {
    Principal::from_text("xevnm-gaaaa-aaaar-qafnq-cai").unwrap()
}

// ck usdc ledger
fn create_ck_usdc_ledger_cansiter(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("xevnm-gaaaa-aaaar-qafnq-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_ck_usdc_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: LedgerFeatureFlags = LedgerFeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: lsm_principal().into(),
        fee_collector_account: None,
        initial_balances: vec![(
            Principal::from_text(APPIC_CONTROLLER_PRINCIPAL)
                .unwrap()
                .into(),
            Nat::from(3_000_000_000_u128),
        )],
        transfer_fee: Nat::from(10_000_u128),
        decimals: Some(6_u8),
        token_name: "ckUSDC".to_string(),
        token_symbol: "ckUSDC".to_string(),
        metadata: vec![(
            "icrc1:logo".to_string(),
            LedgerMetadataValue::Text("TOKEN_LOGO".to_string()),
        )],
        archive_options: ArchiveOptions {
            trigger_threshold: 2_000,
            num_blocks_to_archive: 1_000,
            node_max_memory_size_bytes: Some(THREE_GIGA_BYTES),
            max_message_size_bytes: None,
            controller_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap(),
            more_controller_ids: Some(vec![sender_principal()]),
            cycles_for_archive_creation: Some(2_000_000_000_000_u64),
            max_transactions_per_response: None,
        },
        max_memo_length: Some(MAX_MEMO_LENGTH),
        feature_flags: Some(ICRC2_FEATURE),
        index_principal: None,
    });

    pic.install_canister(
        canister_id,
        LEDGER_WASM_BYTES.to_vec(),
        encode_call_args(ledger_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

fn ic_usdc_bsc_principal() -> Principal {
    Principal::from_text("z2iye-fyaaa-aaaag-at2pa-cai").unwrap()
}

fn create_ic_usdc_bsc_ledger_cansiter(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("z2iye-fyaaa-aaaag-at2pa-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_ic_usdc_bsc_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: LedgerFeatureFlags = LedgerFeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: bsc_minter_principal().into(),
        fee_collector_account: None,
        initial_balances: vec![(sender_principal().into(), Nat::from(500_000_000_u128))],
        transfer_fee: Nat::from(1_000_000_000_000_000_000_u128),
        decimals: Some(18_u8),
        token_name: "icUSDC.bsc".to_string(),
        token_symbol: "icUSDC.bsc".to_string(),
        metadata: vec![(
            "icrc1:logo".to_string(),
            LedgerMetadataValue::Text("TOKEN_LOGO".to_string()),
        )],
        archive_options: ArchiveOptions {
            trigger_threshold: 2_000,
            num_blocks_to_archive: 1_000,
            node_max_memory_size_bytes: Some(THREE_GIGA_BYTES),
            max_message_size_bytes: None,
            controller_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap(),
            more_controller_ids: Some(vec![sender_principal()]),
            cycles_for_archive_creation: Some(2_000_000_000_000_u64),
            max_transactions_per_response: None,
        },
        max_memo_length: Some(MAX_MEMO_LENGTH),
        feature_flags: Some(ICRC2_FEATURE),
        index_principal: None,
    });

    pic.install_canister(
        canister_id,
        LEDGER_WASM_BYTES.to_vec(),
        encode_call_args(ledger_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

fn ic_usdc_base_principal() -> Principal {
    Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap()
}

fn create_ic_usdc_base_ledger_cansiter(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("qkrwp-ziaaa-aaaag-auemq-cai").unwrap(),
    )
    .expect("Should create the canister")
}

fn install_ic_usdc_base_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: LedgerFeatureFlags = LedgerFeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: base_minter_principal().into(),
        fee_collector_account: None,
        initial_balances: vec![(sender_principal().into(), Nat::from(500_000_000_u128))],
        transfer_fee: Nat::from(10_000u128),
        decimals: Some(18_u8),
        token_name: "icUSDC.base".to_string(),
        token_symbol: "icUSDC.base".to_string(),
        metadata: vec![(
            "icrc1:logo".to_string(),
            LedgerMetadataValue::Text("TOKEN_LOGO".to_string()),
        )],
        archive_options: ArchiveOptions {
            trigger_threshold: 2_000,
            num_blocks_to_archive: 1_000,
            node_max_memory_size_bytes: Some(THREE_GIGA_BYTES),
            max_message_size_bytes: None,
            controller_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap(),
            more_controller_ids: Some(vec![sender_principal()]),
            cycles_for_archive_creation: Some(2_000_000_000_000_u64),
            max_transactions_per_response: None,
        },
        max_memo_length: Some(MAX_MEMO_LENGTH),
        feature_flags: Some(ICRC2_FEATURE),
        index_principal: None,
    });

    pic.install_canister(
        canister_id,
        LEDGER_WASM_BYTES.to_vec(),
        encode_call_args(ledger_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

pub fn create_and_install_minters_plus_dependency_canisters(pic: &PocketIc) {
    // Create and install icp ledger
    let icp_canister_id = create_icp_ledger_canister(pic);
    pic.add_cycles(icp_canister_id, TWO_TRILLIONS.into());
    install_icp_ledger_canister(pic, icp_canister_id);
    five_ticks(pic);

    // create and install proxy canister
    let proxy_id = create_proxy_canister(pic);
    pic.add_cycles(proxy_id, TWO_TRILLIONS.into());
    install_proxy_canister(pic, proxy_id);

    // Create and install appic helper
    let appic_helper_id = create_appic_helper_canister(pic);
    pic.add_cycles(appic_helper_id, TWENTY_TRILLIONS.into());
    install_appic_helper_canister(pic, appic_helper_id);
    five_ticks(pic);
    five_ticks(pic);

    // Create and install lsm canister
    let lsm_canister_id = create_lsm_canister(pic);
    pic.add_cycles(lsm_canister_id, TWENTY_TRILLIONS.wrapping_mul(5).into());
    install_lsm_canister(pic, lsm_canister_id);
    five_ticks(pic);
    five_ticks(pic);

    // Create and install evm rpc canister
    let evm_rpc_canister_id = create_evm_rpc_canister(pic);
    pic.add_cycles(evm_rpc_canister_id, TWO_TRILLIONS.into());
    install_evm_rpc_canister(pic, evm_rpc_canister_id);
    five_ticks(pic);

    // create and install dex canister
    let dex_canister_id = create_dex_canister(pic);
    pic.add_cycles(dex_canister_id, TWENTY_TRILLIONS.into());
    install_dex_canister(pic, dex_canister_id);

    // Create and install native ledgeres canister
    let bsc_native_ledger_canister_id = create_bsc_native_ledger_canister(pic);
    let base_native_ledger_canister_id = create_base_native_ledger_canister(pic);
    pic.add_cycles(bsc_native_ledger_canister_id, TWO_TRILLIONS.into());
    pic.add_cycles(base_native_ledger_canister_id, TWO_TRILLIONS.into());
    install_bsc_native_ledger_canister(pic, bsc_native_ledger_canister_id);
    install_base_native_ledger_canister(pic, base_native_ledger_canister_id);

    five_ticks(pic);

    let ic_usdc_bsc_ledger_id = create_ic_usdc_bsc_ledger_cansiter(pic);
    pic.add_cycles(ic_usdc_bsc_ledger_id, TWENTY_TRILLIONS.into());
    install_ic_usdc_bsc_ledger_canister(pic, ic_usdc_bsc_ledger_id);

    let ic_usdc_base_ledger_id = create_ic_usdc_base_ledger_cansiter(pic);
    pic.add_cycles(ic_usdc_base_ledger_id, TWENTY_TRILLIONS.into());
    install_ic_usdc_base_ledger_canister(pic, ic_usdc_base_ledger_id);

    five_ticks(pic);

    // Create and install native index canister
    let bsc_native_index_canister_id = create_bsc_index_canister(pic);
    let base_native_index_canister_id = create_base_index_canister(pic);
    pic.add_cycles(bsc_native_index_canister_id, TWO_TRILLIONS.into());
    install_base_index_canister(pic, bsc_native_index_canister_id);
    pic.add_cycles(base_native_index_canister_id, TWO_TRILLIONS.into());
    install_base_index_canister(pic, base_native_index_canister_id);

    five_ticks(pic);
    five_ticks(pic);

    install_base_minter_and_setup(pic);

    five_ticks(pic);
    five_ticks(pic);
    five_ticks(pic);

    install_bsc_minter_and_setup(pic);

    five_ticks(pic);
    five_ticks(pic);

    five_ticks(pic);

    let ck_usdc_ledger_id = create_ck_usdc_ledger_cansiter(pic);
    pic.add_cycles(ck_usdc_ledger_id, TWENTY_TRILLIONS.into());
    install_ck_usdc_ledger_canister(pic, ck_usdc_ledger_id);

    create_pools_and_provide_liquidty(pic);
}

#[test]
fn add_usdc_tokens_and_create_usdc_pools() {
    let pic = create_pic();
    create_and_install_minters_plus_dependency_canisters(&pic);

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    print!("{:?}", canister_http_requests);

    assert!(false)
}

pub fn install_bsc_minter_and_setup(pic: &PocketIc) {
    // Create and install minter canister for bsc test net
    let bsc_minter_id = create_bsc_minter_canister(pic);
    pic.add_cycles(bsc_minter_id, 1_000_000_000_000);
    install_bsc_minter_canister(pic, bsc_minter_id);

    five_ticks(pic);
    five_ticks(pic);

    // At this time there should be 2 http requests:
    // [0] is for eth_getBlockByNumber
    // [1] is for eth_feeHistory
    let canister_http_requests = pic.get_canister_http();

    // 1st Generating mock response for eth_feehistory
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_BSC_FEE_HISTORY_RESPONSE,
    );

    // 2nd Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 1, MOCK_BSC_BLOCK_NUMBER);

    five_ticks(pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 0, MOCK_GET_LOGS_EMPTY);

    // Ankr mock submission
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 1, MOCK_GET_LOGS_EMPTY);

    // Drpc mock submission
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 2, MOCK_GET_LOGS_EMPTY);

    // Alchemy mock submissios
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 3, MOCK_GET_LOGS_EMPTY);

    five_ticks(pic);
    five_ticks(pic);

    update_call::<AddErc20Token, ()>(
        pic,
        bsc_minter_id,
        "add_erc20_token",
        AddErc20Token {
            chain_id: Nat::from(56_u8),
            address: "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d".to_string(),
            erc20_token_symbol: "icUSDC.bsc".to_string(),
            erc20_ledger_id: ic_usdc_bsc_principal(),
        },
        Some(Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap()),
    );

    five_ticks(pic);
    five_ticks(pic);

    pic.advance_time(Duration::from_secs(70));

    let _ = update_call::<(), Result<(), RequestScrapingError>>(
        pic,
        bsc_minter_id,
        "request_scraping_logs",
        (),
        None,
    );

    five_ticks(pic);
    five_ticks(pic);

    // 4th
    let canister_http_requests = pic.get_canister_http();
    println!("canister requests{:?}", canister_http_requests);

    // Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_BSC_HIGHER_BLOCK_NUMBER,
    );

    five_ticks(pic);
    five_ticks(pic);

    // 5th generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_GET_BSC_LOGS_ERC20,
    );

    // Ankr mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_GET_BSC_LOGS_ERC20,
    );

    // Drpc mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_GET_BSC_LOGS_ERC20,
    );

    // Alchemy mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_GET_BSC_LOGS_ERC20,
    );

    five_ticks(pic);

    // update the gas estimate, block number and native usdc price
    update_call::<ChainData, ()>(
        pic,
        bsc_minter_id,
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(23_402_978_u128),
            fee_history: MOCK_BSC_FEE_HISTORY_INNER.into(),
            native_token_usd_price: Some(1001.73),
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        pic,
        Principal::from_text("n44gr-qyaaa-aaaam-qbuha-cai").unwrap(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: bsc_minter_id,
                subaccount: None,
            },
            amount: Nat::from(3_000_000_000_000_000_000_u128),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

    five_ticks(pic);
    five_ticks(pic);

    // swap activation request
    let swap_contract_address =
        Address::from_str("0x98fff5F36C0cF12AE16d3D80F67B5E8ab5E1FfB1").unwrap();

    let dex_canister_id: Principal = Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap();

    update_call::<ActivateSwapReqest, Nat>(
        pic,
        bsc_minter_id,
        "activate_swap_feature",
        ActivateSwapReqest {
            twin_usdc_ledger_id: ic_usdc_bsc_principal(),
            swap_contract_address: swap_contract_address.to_string(),
            twin_usdc_decimals: 18,
            dex_canister_id,
            // 5 cents
            canister_signing_fee_twin_usdc_value: Nat::from(30_000_000_000_000_000_u128),
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    );

    five_ticks(pic);
    five_ticks(pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_BSC_LATEST,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_BSC_LATEST,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_BSC_LATEST,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_BSC_LATEST,
    );

    five_ticks(pic);
    five_ticks(pic);
    //
    // At this point there should be 2 http_requests
    // [0] public_node eth_sendRawTransaction
    // [1] ankr eth_sendRawTransaction
    let canister_http_requests = pic.get_canister_http();

    // public_node request
    // Trying to simulate real sendrawtransaction since there will only be one successful result and the rest of the nodes will return
    // one of the failed responses(NonceTooLow,NonceTooHigh,etc..,)
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_SEND_TRANSACTION_SUCCESS,
    );

    // ankr request
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Drpc request
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Alchemy request
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_SEND_TRANSACTION_ERROR,
    );
    // getting the finalized transaction count after sending transaction was successful.

    five_ticks(pic);
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_BSC_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_BSC_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_BSC_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_BSC_FINALIZED,
    );

    five_ticks(pic);
    five_ticks(pic);

    // At this point there should be two requests for eth_getTransactionReceipt
    // [0] public_node
    // [1] ankr
    let canister_http_requests = pic.get_canister_http();

    // public_node
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BSC_ERC20,
    );

    // ankr
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BSC_ERC20,
    );

    // public_node
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BSC_ERC20,
    );

    // ankr
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BSC_ERC20,
    );

    five_ticks(pic);
    five_ticks(pic);

    // charge gas tank
    update_call::<Nat, ()>(
        pic,
        bsc_minter_id,
        "charge_gas_tank",
        Nat::from(10_000_000_000_000_000_u128),
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    );

    five_ticks(pic);
    five_ticks(pic);

    let minter_info = query_call::<(), MinterInfo>(pic, bsc_minter_id, "get_minter_info", ());
    println!("{:?}", minter_info);
}

pub fn install_base_minter_and_setup(pic: &PocketIc) {
    // Create and install minter canister for bsc test net
    let base_minter_id = create_base_minter_canister(pic);
    pic.add_cycles(base_minter_id, 1_000_000_000_000);
    install_base_minter_canister(pic, base_minter_id);

    five_ticks(pic);
    five_ticks(pic);

    // At this time there should be 2 http requests:
    // [0] is for eth_getBlockByNumber
    // [1] is for eth_feeHistory
    let canister_http_requests = pic.get_canister_http();

    // 1st Generating mock response for eth_feehistory
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_BASE_FEE_HISTORY_RESPONSE,
    );

    // 2nd Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 1, MOCK_BASE_BLOCK_NUMBER);

    five_ticks(pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 0, MOCK_GET_LOGS_EMPTY);

    // Ankr mock submission
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 1, MOCK_GET_LOGS_EMPTY);

    // Drpc mock submission
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 2, MOCK_GET_LOGS_EMPTY);

    // Alchemy mock submissios
    generate_and_submit_mock_http_response(pic, &canister_http_requests, 3, MOCK_GET_LOGS_EMPTY);

    five_ticks(pic);

    // install icUSDC.base
    // Calling icrc2_approve and giving the permission to lsm for taking funds from users principal
    let _approve_result = update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        pic,
        icp_principal(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: lsm_principal(),
                subaccount: None,
            },
            amount: Nat::from(
                2_500_000_000_u128, // Users balance - approval fee => 99_950_000_000_000_000_u128 - 10_000_000_000_000_u128
            ),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        None,
    )
    .unwrap();

    update_call::<AddErc20Token, ()>(
        pic,
        base_minter_id,
        "add_erc20_token",
        AddErc20Token {
            chain_id: Nat::from(8453_u128),
            address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(),
            erc20_token_symbol: "icUSDC.base".to_string(),
            erc20_ledger_id: ic_usdc_base_principal(),
        },
        Some(Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap()),
    );

    five_ticks(pic);
    five_ticks(pic);

    pic.advance_time(Duration::from_secs(70));

    let _ = update_call::<(), Result<(), RequestScrapingError>>(
        pic,
        base_minter_id,
        "request_scraping_logs",
        (),
        None,
    );

    five_ticks(pic);
    five_ticks(pic);

    // 4th
    let canister_http_requests = pic.get_canister_http();
    println!("canister requests{:?}", canister_http_requests);

    // Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_BASE_HIGHER_BLOCK_NUMBER,
    );

    five_ticks(pic);
    five_ticks(pic);

    // 5th generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_GET_BASE_LOGS_ERC20,
    );

    // Ankr mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_GET_BASE_LOGS_ERC20,
    );

    // Drpc mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_GET_BASE_LOGS_ERC20,
    );

    // Alchemy mock submission
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_GET_BASE_LOGS_ERC20,
    );

    five_ticks(pic);
    five_ticks(pic);

    // update the gas estimate, block number and native usdc price
    update_call::<ChainData, ()>(
        pic,
        base_minter_id,
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(572_530_890_u128),
            fee_history: MOCK_BASE_FEE_HISTORY_INNER.into(),
            native_token_usd_price: Some(4475.43),
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        pic,
        Principal::from_text("3iven-myaaa-aaaai-q3u5q-cai").unwrap(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: base_minter_id,
                subaccount: None,
            },
            amount: Nat::from(3_000_000_000_000_000_000_u128),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

    five_ticks(pic);
    five_ticks(pic);

    // swap activation request
    let swap_contract_address =
        Address::from_str("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913").unwrap();

    let dex_canister_id: Principal = Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap();

    update_call::<ActivateSwapReqest, Nat>(
        pic,
        base_minter_id,
        "activate_swap_feature",
        ActivateSwapReqest {
            twin_usdc_ledger_id: ic_usdc_base_principal(),
            swap_contract_address: swap_contract_address.to_string(),
            twin_usdc_decimals: 6,
            dex_canister_id,
            // 5 cents
            canister_signing_fee_twin_usdc_value: Nat::from(30_000_u128),
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    );

    five_ticks(pic);
    five_ticks(pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_BASE_LATEST,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_BASE_LATEST,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_BASE_LATEST,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_BASE_LATEST,
    );

    five_ticks(pic);
    five_ticks(pic);
    //
    // At this point there should be 2 http_requests
    // [0] public_node eth_sendRawTransaction
    // [1] ankr eth_sendRawTransaction
    let canister_http_requests = pic.get_canister_http();

    // public_node request
    // Trying to simulate real sendrawtransaction since there will only be one successful result and the rest of the nodes will return
    // one of the failed responses(NonceTooLow,NonceTooHigh,etc..,)
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_SEND_TRANSACTION_SUCCESS,
    );

    // ankr request
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Drpc request
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Alchemy request
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_SEND_TRANSACTION_ERROR,
    );
    // getting the finalized transaction count after sending transaction was successful.

    five_ticks(pic);
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_BASE_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_BASE_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_BASE_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_BASE_FINALIZED,
    );

    five_ticks(pic);
    five_ticks(pic);

    // At this point there should be two requests for eth_getTransactionReceipt
    // [0] public_node
    // [1] ankr
    let canister_http_requests = pic.get_canister_http();

    // public_node
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BASE_ERC20,
    );

    // ankr
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BASE_ERC20,
    );

    // public_node
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BASE_ERC20,
    );

    // ankr
    generate_and_submit_mock_http_response(
        pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_RECEIPT_APPROVE_BASE_ERC20,
    );

    five_ticks(pic);
    five_ticks(pic);

    // charge gas tank
    update_call::<Nat, ()>(
        pic,
        base_minter_id,
        "charge_gas_tank",
        Nat::from(10_000_000_000_000_000_u128),
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    );

    five_ticks(pic);
    five_ticks(pic);

    let minter_info = query_call::<(), MinterInfo>(pic, base_minter_id, "get_minter_info", ());
    println!("{:?}", minter_info);
}

//create pools between ckUSDC and icUSDC.bsc and icUSDC.base nad provide lqiuidty
pub fn create_pools_and_provide_liquidty(pic: &PocketIc) {
    // approval from appic controller to dex canister to mint positions
    // ckUSDC approval
    update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        pic,
        ck_usdc_principal(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: dex_principal_id(),
                subaccount: None,
            },
            amount: Nat::from(3_000_000_000_u128),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

    // icUSDC.base approval
    update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        pic,
        ic_usdc_base_principal(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: dex_principal_id(),
                subaccount: None,
            },
            amount: Nat::from(3_000_000_000_u128),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

    // icUSDC.bsc approval
    update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        pic,
        ic_usdc_bsc_principal(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: dex_principal_id(),
                subaccount: None,
            },
            amount: Nat::from(3_000_000_000_000_000_000_000_u128),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

    let ck_usdc_usdc_bsc_pool_id =
        update_call::<CreatePoolArgs, Result<CandidPoolId, CreatePoolError>>(
            pic,
            dex_principal_id(),
            "create_pool",
            CreatePoolArgs {
                fee: Nat::from(1000_u128),
                sqrt_price_x96: Nat::from(79383368562352051400232_u128),
                token_a: ic_usdc_bsc_principal(),
                token_b: ck_usdc_principal(),
            },
            Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
        )
        .unwrap();

    let ck_usdc_usdc_base_pool_id =
        update_call::<CreatePoolArgs, Result<CandidPoolId, CreatePoolError>>(
            pic,
            dex_principal_id(),
            "create_pool",
            CreatePoolArgs {
                fee: Nat::from(100_u128),
                sqrt_price_x96: Nat::from(79348275437447525686522247306_u128),
                token_a: ic_usdc_base_principal(),
                token_b: ck_usdc_principal(),
            },
            Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
        )
        .unwrap();

    update_call::<MintPositionArgs, Result<Nat, MintPositionError>>(
        pic,
        dex_principal_id(),
        "mint_position",
        MintPositionArgs {
            amount1_max: Nat::from(371340035_u128),
            pool: ck_usdc_usdc_bsc_pool_id,
            from_subaccount: None,
            amount0_max: Nat::from(24075968775435293008_u128),
            tick_lower: Int::from(-276_360),
            tick_upper: Int::from(-276_280),
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

    update_call::<MintPositionArgs, Result<Nat, MintPositionError>>(
        pic,
        dex_principal_id(),
        "mint_position",
        MintPositionArgs {
            amount1_max: Nat::from(62998876_u128),
            pool: ck_usdc_usdc_base_pool_id,
            from_subaccount: None,
            amount0_max: Nat::from(18874016_u128),
            tick_lower: Int::from(-32),
            tick_upper: Int::from(49),
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

    //let all_pools = query_call::<(), Vec<(CandidPoolId, CandidPoolState)>>(
    //    pic,
    //    dex_principal_id(),
    //    "get_pools",
    //    (),
    //);

    let quote_result = query_call::<QuoteArgs, Result<Nat, QuoteError>>(
        pic,
        dex_principal_id(),
        "quote",
        QuoteArgs::QuoteExactInputParams(QuoteExactParams {
            path: vec![
                CandidPathKey {
                    fee: Nat::from(100_u8),
                    intermediary_token: ck_usdc_principal(),
                },
                CandidPathKey {
                    fee: Nat::from(1_000_u128),
                    intermediary_token: ic_usdc_bsc_principal(),
                },
            ],
            exact_token: ic_usdc_base_principal(),
            exact_amount: Nat::from(4300966_u128),
        }),
    )
    .unwrap();

    println!("all pools {:?}", quote_result);
}
