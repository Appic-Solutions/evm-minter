// Pocket ic helpers:
// This mod was built by the purpose of simulating the minter_canisters opration on a subnet and testing
// both the deposit and the withdrawal flow to make sure there will be no point of failure in the mentioned flows
// and concurrent requests;

use std::time::Duration;

use icrc_ledger_types::{
    icrc1::account::Account,
    icrc2::approve::{ApproveArgs, ApproveError},
};
// For simulating http out calls, we use mock httpout call response.
use pocket_ic::common::rest::{CanisterHttpReply, CanisterHttpResponse, MockCanisterHttpResponse};

const MINTER_WASM_BYTES: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/evm_minter.wasm");
const LEDGER_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/ledger_canister_u256.wasm.gz");
const INDEX_WAM_BYTES: &[u8] = include_bytes!("../../../wasm/index_ng_canister_u256.wasm.gz");
const ARCHIVE_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/archive_canister_u256.wasm.gz");
const LSM_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/lsm.wasm");
const EVM_RPC_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/evm_rpc.wasm");

const TWENTY_TRILLIONS: u64 = 20_000_000_000_000;

const FIVE_TRILLIONS: u64 = 5_000_000_000_000;

const FOUR_TRILLIONS: u64 = 4_000_000_000_000;

const TWO_TRILLIONS: u64 = 2_000_000_000_000;

use candid::{CandidType, Nat, Principal};
use evm_rpc_types::InstallArgs;
use pocket_ic::{PocketIc, PocketIcBuilder, WasmResult};

use super::lsm_types::{InitArg as LsmInitArgs, LSMarg, LedgerManagerInfo};

use crate::{
    endpoints::{CandidBlockTag, Erc20Token, MinterInfo},
    evm_config::EvmNetwork,
    lifecycle::{InitArg, MinterArg, UpgradeArg},
    lsm_client::WasmHash,
    tests::lsm_types::{
        AddErc20Arg, AddErc20Error, CyclesManagement, Erc20Contract, LedgerInitArg,
        LedgerSuiteVersion, ManagedCanisterStatus, ManagedCanisters,
    },
};
use ic_icrc1_index_ng::{IndexArg, InitArg as IndexInitArg};
use ic_icrc1_ledger::{ArchiveOptions, InitArgs as LedgerInitArgs, LedgerArgument};
use intialize_minter::create_and_install_minter_plus_dependency_canisters;

#[test]
fn should_create_and_install_and_upgrade_minter_casniter() {
    let pic = create_pic();

    let canister_id = create_minter_canister(&pic);

    assert_eq!(canister_id, minter_principal());

    pic.add_cycles(canister_id, 1_000_000_000_000);

    install_minter_canister(&pic, canister_id);

    five_ticks(&pic);

    let minter_info = query_call::<_, MinterInfo>(&pic, canister_id, "get_minter_info", ());

    assert_eq!(
        minter_info,
        MinterInfo {
            minter_address: Some("0x3b13DAFE68a5FDe26eACb4064559d97c1e4FB41a".to_string()),
            helper_smart_contract_address: Some(
                "0x733a1BEeF5A02990aAD285d7ED93fc1b622EeF1d".to_string()
            ),
            deposit_native_fee: Some(Nat::from(50_000_000_000_000_u64)),
            withdrawal_native_fee: Some(Nat::from(100_000_000_000_000_u64)),
            supported_erc20_tokens: Some(vec![]),
            minimum_withdrawal_amount: Some(Nat::from(200_000_000_000_000_u64)),
            block_height: Some(CandidBlockTag::Latest),
            last_observed_block_number: None,
            native_balance: Some(Nat::from(0_u128)),
            last_gas_fee_estimate: None,
            erc20_balances: Some(vec![]),
            last_scraped_block_number: Some(Nat::from(45944445_u64)),
            native_twin_token_ledger_id: Some("n44gr-qyaaa-aaaam-qbuha-cai".parse().unwrap()),
            swap_canister_id: None,
            ledger_suite_manager_id: Some("kmcdp-4yaaa-aaaag-ats3q-cai".parse().unwrap())
        }
    );

    let upgrade_args = MinterArg::UpgradeArg(UpgradeArg {
        native_minimum_withdrawal_amount: Some(Nat::from(400_000_000_000_000_u128)),
        native_ledger_transfer_fee: None,
        next_transaction_nonce: None,
        last_scraped_block_number: Some(Nat::from(100935911_u128)),
        evm_rpc_id: Some("7hfb6-caaaa-aaaar-qadga-cai".parse().unwrap()),
        helper_contract_address: None,
        block_height: None,
        min_max_priority_fee_per_gas: None,
        deposit_native_fee: Some(Nat::from(100_000_000_000_000_u64)),
        withdrawal_native_fee: Some(Nat::from(200_000_000_000_000_u64)),
    });
    let upgrade_bytes = candid::encode_one(upgrade_args).unwrap();

    upgrade_minter_casniter(&pic, canister_id, upgrade_bytes);

    five_ticks(&pic);

    let minter_info_after_upgrade =
        query_call::<_, MinterInfo>(&pic, canister_id, "get_minter_info", ());

    assert_eq!(
        minter_info_after_upgrade,
        MinterInfo {
            minter_address: Some("0x3b13DAFE68a5FDe26eACb4064559d97c1e4FB41a".to_string()),
            helper_smart_contract_address: Some(
                "0x733a1BEeF5A02990aAD285d7ED93fc1b622EeF1d".to_string()
            ),
            supported_erc20_tokens: Some(vec![]),
            minimum_withdrawal_amount: Some(Nat::from(400_000_000_000_000_u128)),
            deposit_native_fee: Some(Nat::from(100_000_000_000_000_u64)),
            withdrawal_native_fee: Some(Nat::from(200_000_000_000_000_u64)),
            block_height: Some(CandidBlockTag::Latest),
            last_observed_block_number: None,
            native_balance: Some(Nat::from(0_u128)),
            last_gas_fee_estimate: None,
            erc20_balances: Some(vec![]),
            last_scraped_block_number: Some(Nat::from(100935911_u128)),
            native_twin_token_ledger_id: Some("n44gr-qyaaa-aaaam-qbuha-cai".parse().unwrap()),
            swap_canister_id: None,
            ledger_suite_manager_id: Some("kmcdp-4yaaa-aaaag-ats3q-cai".parse().unwrap())
        }
    );
}

#[test]
fn should_create_and_install_all_minter_dependency_canisters() {
    let pic = create_pic();

    // Create and install lsm casniter
    let lsm_casniter_id = create_lsm_canister(&pic);
    pic.add_cycles(lsm_casniter_id, TWO_TRILLIONS.into());
    install_lsm_canister(&pic, lsm_casniter_id);
    five_ticks(&pic);
    let lsm_info = query_call::<(), LedgerManagerInfo>(&pic, lsm_casniter_id, "get_lsm_info", ());
    assert_eq!(
        lsm_info,
        LedgerManagerInfo {
            managed_canisters: vec![],
            cycles_management: CyclesManagement {
                cycles_for_ledger_creation: Nat::from(FIVE_TRILLIONS),
                cycles_for_archive_creation: Nat::from(TWO_TRILLIONS),
                cycles_for_index_creation: Nat::from(FIVE_TRILLIONS),
                cycles_top_up_increment: Nat::from(FOUR_TRILLIONS),
            },
            more_controller_ids: vec![sender_principal()],
            minter_ids: vec![(Nat::from(97_u64), minter_principal())],
            ledger_suite_version: Some(LedgerSuiteVersion {
                ledger_compressed_wasm_hash: WasmHash::new(LEDGER_WASM_BYTES.to_vec()).to_string(),
                index_compressed_wasm_hash: WasmHash::new(INDEX_WAM_BYTES.to_vec()).to_string(),
                archive_compressed_wasm_hash: WasmHash::new(ARCHIVE_WASM_BYTES.to_vec())
                    .to_string()
            }),
            ls_creation_icp_fee: Nat::from(2_500_000_000_u64),
            ls_creation_appic_fee: None
        }
    );

    // Create and install evm rpc canister
    let evm_rpc_canister_id = create_evm_rpc_canister(&pic);
    pic.add_cycles(evm_rpc_canister_id, TWO_TRILLIONS.into());
    install_evm_rpc_canister(&pic, evm_rpc_canister_id);
    five_ticks(&pic);

    // Create and install native ledger canister
    let native_ledger_canister_id = create_native_ledger_canister(&pic);
    pic.add_cycles(native_ledger_canister_id, TWO_TRILLIONS.into());
    install_native_ledger_canister(&pic, native_ledger_canister_id);
    five_ticks(&pic);

    // Create and install native index canister
    let native_index_canister_id = create_index_canister(&pic);
    pic.add_cycles(native_index_canister_id, TWO_TRILLIONS.into());
    install_index_canister(&pic, native_index_canister_id);
    five_ticks(&pic);
}

#[test]
fn should_install_lsm_casniter_and_create_ledger_suite() {
    let pic = create_pic();

    create_and_install_minter_plus_dependency_canisters(&pic);

    // Withdrawal Section
    // Calling icrc2_approve and giving the permission to lsm for taking funds from users principal
    let _approve_result = update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        &pic,
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

    five_ticks(&pic);

    let _create_erc20_ls_result = update_call::<AddErc20Arg, Result<(), AddErc20Error>>(
        &pic,
        lsm_principal(),
        "add_erc20_ls",
        AddErc20Arg {
            contract: Erc20Contract {
                chain_id: EvmNetwork::BSCTestnet.chain_id().into(),
                address: "0xdac17f958d2ee523a2206206994597c13d831ec7".to_string(),
            },
            ledger_init_arg: LedgerInitArg {
                transfer_fee: Nat::from(10_000_u128),
                decimals: 6,
                token_name: "USD Tether on icp".to_string(),
                token_symbol: "icUSDT".to_string(),
                token_logo: "".to_string(),
            },
        },
        None,
    );

    five_ticks(&pic);

    // Advance time for 1 hour.
    pic.advance_time(Duration::from_secs(1 * 60 * 60));

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    let lsm_info = query_call::<(), LedgerManagerInfo>(&pic, lsm_principal(), "get_lsm_info", ());

    let ic_usdt_ledger = match lsm_info
        .clone()
        .managed_canisters
        .into_iter()
        .find(|ls| {
            ls.erc20_contract
                == Erc20Contract {
                    chain_id: Nat::from(97_u64),
                    address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                }
        })
        .unwrap()
        .ledger
        .unwrap()
    {
        ManagedCanisterStatus::Created { canister_id } => {
            panic!("Ledger _should be installed at this point")
        }
        ManagedCanisterStatus::Installed {
            canister_id,
            installed_wasm_hash,
        } => (canister_id, installed_wasm_hash),
    };

    let ic_usdt_index = lsm_info
        .clone()
        .managed_canisters
        .into_iter()
        .find(|ls| {
            ls.erc20_contract
                == Erc20Contract {
                    chain_id: Nat::from(97_u64),
                    address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                }
        })
        .unwrap()
        .index;

    let ic_usdt_archives = lsm_info
        .clone()
        .managed_canisters
        .into_iter()
        .find(|ls| {
            ls.erc20_contract
                == Erc20Contract {
                    chain_id: Nat::from(97_u64),
                    address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                }
        })
        .unwrap()
        .archives;

    assert_eq!(
        lsm_info
            .managed_canisters
            .into_iter()
            .find(|ls| ls.erc20_contract
                == Erc20Contract {
                    address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                    chain_id: 97_u64.into()
                })
            .unwrap(),
        ManagedCanisters {
            erc20_contract: Erc20Contract {
                address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                chain_id: 97_u64.into()
            },
            twin_erc20_token_symbol: "icUSDT".to_string(),
            ledger: Some(ManagedCanisterStatus::Installed {
                canister_id: ic_usdt_ledger.0,
                installed_wasm_hash: ic_usdt_ledger.1
            }),
            index: ic_usdt_index,
            archives: ic_usdt_archives
        }
    );

    // icUSDT should be added to minter
    let minters_erc20_tokens =
        query_call::<(), MinterInfo>(&pic, minter_principal(), "get_minter_info", ())
            .supported_erc20_tokens
            .unwrap();
    assert_eq!(
        minters_erc20_tokens
            .into_iter()
            .find(|token| token.erc20_contract_address
                == "0xdAC17F958D2ee523a2206206994597C13D831ec7")
            .unwrap(),
        Erc20Token {
            erc20_token_symbol: "icUSDT".to_string(),
            erc20_contract_address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
            ledger_canister_id: ic_usdt_ledger.0
        }
    )
}

pub fn query_call<I, O>(pic: &PocketIc, canister_id: Principal, method: &str, payload: I) -> O
where
    O: CandidType + for<'a> serde::Deserialize<'a>,
    I: CandidType,
{
    let wasm_result = pic
        .query_call(
            canister_id,
            sender_principal(),
            method,
            encode_call_args(payload).unwrap(),
        )
        .unwrap();

    decode_wasm_result::<O>(wasm_result).unwrap()
}

pub fn update_call<I, O>(
    pic: &PocketIc,
    canister_id: Principal,
    method: &str,
    payload: I,
    sender: Option<Principal>,
) -> O
where
    O: CandidType + for<'a> serde::Deserialize<'a>,
    I: CandidType,
{
    let sender_princiapl = match sender {
        Some(p_id) => p_id,
        None => sender_principal(),
    };
    let wasm_result = pic
        .update_call(
            canister_id,
            sender_princiapl,
            method,
            encode_call_args(payload).unwrap(),
        )
        .unwrap();

    decode_wasm_result::<O>(wasm_result).unwrap()
}

pub fn encode_call_args<I>(args: I) -> Result<Vec<u8>, ()>
where
    I: CandidType,
{
    Ok(candid::encode_one(args).unwrap())
}

pub fn decode_wasm_result<O>(wasm_result: WasmResult) -> Result<O, ()>
where
    O: CandidType + for<'a> serde::Deserialize<'a>,
{
    match wasm_result {
        pocket_ic::WasmResult::Reply(vec) => Ok(candid::decode_one(&vec).unwrap()),
        pocket_ic::WasmResult::Reject(_) => Err(()),
    }
}

pub fn create_pic() -> PocketIc {
    PocketIcBuilder::new()
        .with_nns_subnet()
        .with_ii_subnet()
        .with_application_subnet()
        .build()
}

fn create_minter_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(Some(sender_principal()), None, minter_principal())
        .expect("Should create the casniter")
}

fn install_minter_canister(pic: &PocketIc, canister_id: Principal) {
    let init_args = MinterArg::InitArg(InitArg {
        evm_network: crate::evm_config::EvmNetwork::BSCTestnet,
        ecdsa_key_name: "key_1".to_string(),
        helper_contract_address: Some("0x733a1beef5a02990aad285d7ed93fc1b622eef1d".to_string()),
        native_ledger_id: "n44gr-qyaaa-aaaam-qbuha-cai".parse().unwrap(),
        native_index_id: "eysav-tyaaa-aaaap-akqfq-cai".parse().unwrap(),
        native_symbol: "icTestBNB".to_string(),
        block_height: CandidBlockTag::Latest,
        native_minimum_withdrawal_amount: Nat::from(200_000_000_000_000_u128),
        native_ledger_transfer_fee: Nat::from(10_000_000_000_000_u128),
        next_transaction_nonce: Nat::from(0_u128),
        last_scraped_block_number: Nat::from(45944445_u64),
        min_max_priority_fee_per_gas: Nat::from(3_000_000_000_u128),
        ledger_suite_manager_id: "kmcdp-4yaaa-aaaag-ats3q-cai".parse().unwrap(),
        deposit_native_fee: Nat::from(50_000_000_000_000_u64),
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

fn create_lsm_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap(),
    )
    .expect("Should create the casniter")
}

fn install_lsm_canister(pic: &PocketIc, canister_id: Principal) {
    let lsm_init_bytes = LSMarg::InitArg(LsmInitArgs {
        more_controller_ids: vec![sender_principal()],
        minter_ids: vec![(Nat::from(97_u64), minter_principal())],
        cycles_management: None,
        twin_ls_creation_fee_icp_token: Nat::from(2_500_000_000_u64),
        twin_ls_creation_fee_appic_token: None,
    });
    pic.install_canister(
        canister_id,
        LSM_WASM_BYTES.to_vec(),
        encode_call_args(lsm_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

fn create_icp_ledger_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap(),
    )
    .expect("Should create the casniter")
}

fn install_icp_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    use ic_icrc1_ledger::FeatureFlags as LedgerFeatureFlags;
    use icrc_ledger_types::icrc1::account::Account as LedgerAccount;

    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: LedgerFeatureFlags = LedgerFeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let minter_id = minter_principal();

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: LedgerAccount::from(minter_id),
        fee_collector_account: Some(LedgerAccount {
            owner: minter_id,
            subaccount: Some(LEDGER_FEE_SUBACCOUNT),
        }),
        initial_balances: vec![(
            LedgerAccount::from(sender_principal()),
            Nat::from(5_500_020_000_u128),
        )],
        transfer_fee: Nat::from(10_000_u128),
        decimals: Some(18_u8),
        token_name: "icTestBNB".to_string(),
        token_symbol: "icTestBNB".to_string(),
        metadata: vec![],
        archive_options: ArchiveOptions {
            trigger_threshold: 2_000,
            num_blocks_to_archive: 1_000,
            node_max_memory_size_bytes: Some(THREE_GIGA_BYTES),
            max_message_size_bytes: None,
            controller_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai")
                .unwrap()
                .into(),
            more_controller_ids: Some(vec![sender_principal().into()]),
            cycles_for_archive_creation: Some(2_000_000_000_000_u64),
            max_transactions_per_response: None,
        },
        max_memo_length: Some(MAX_MEMO_LENGTH),
        feature_flags: Some(ICRC2_FEATURE),
        maximum_number_of_accounts: None,
        accounts_overflow_trim_quantity: None,
    });

    pic.install_canister(
        canister_id,
        LEDGER_WASM_BYTES.to_vec(),
        encode_call_args(ledger_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

fn create_evm_rpc_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("sosge-5iaaa-aaaag-alcla-cai").unwrap(),
    )
    .expect("Should create the casniter")
}

fn install_evm_rpc_canister(pic: &PocketIc, canister_id: Principal) {
    let install_args = InstallArgs::default();
    pic.install_canister(
        canister_id,
        EVM_RPC_WASM_BYTES.to_vec(),
        encode_call_args(install_args).unwrap(),
        Some(sender_principal()),
    );
}

fn create_native_ledger_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("n44gr-qyaaa-aaaam-qbuha-cai").unwrap(),
    )
    .expect("Should create the casniter")
}

fn install_native_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    use ic_icrc1_ledger::FeatureFlags as LedgerFeatureFlags;
    use icrc_ledger_types::icrc::generic_metadata_value::MetadataValue as LedgerMetadataValue;
    use icrc_ledger_types::icrc1::account::Account as LedgerAccount;

    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: LedgerFeatureFlags = LedgerFeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let minter_id = minter_principal();

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: LedgerAccount::from(minter_id),
        fee_collector_account: Some(LedgerAccount {
            owner: minter_id,
            subaccount: Some(LEDGER_FEE_SUBACCOUNT),
        }),
        initial_balances: vec![],
        transfer_fee: Nat::from(10_000_000_000_000_u128),
        decimals: Some(18_u8),
        token_name:  "icTestBNB".to_string(),
        token_symbol:  "icTestBNB".to_string(),
        metadata: vec![(
            "icrc1:logo".to_string(),
            LedgerMetadataValue::from("data:image/png;base64,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"),
        )],
        archive_options: ArchiveOptions {
            trigger_threshold: 2_000,
            num_blocks_to_archive: 1_000,
            node_max_memory_size_bytes: Some(THREE_GIGA_BYTES),
            max_message_size_bytes: None,
            controller_id: Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap().into(),
            more_controller_ids: Some(vec![sender_principal().into()]),
            cycles_for_archive_creation: Some(
                2_000_000_000_000_u64
            ),
            max_transactions_per_response: None,
        },
        max_memo_length: Some(MAX_MEMO_LENGTH),
        feature_flags: Some(ICRC2_FEATURE),
        maximum_number_of_accounts: None,
        accounts_overflow_trim_quantity: None,
    });

    pic.install_canister(
        canister_id,
        LEDGER_WASM_BYTES.to_vec(),
        encode_call_args(ledger_init_bytes).unwrap(),
        Some(sender_principal()),
    );
}

fn create_index_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("eysav-tyaaa-aaaap-akqfq-cai").unwrap(),
    )
    .expect("Should create the casniter")
}

fn install_index_canister(pic: &PocketIc, canister_id: Principal) {
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

pub fn upgrade_minter_casniter(pic: &PocketIc, canister_id: Principal, upgrade_bytes: Vec<u8>) {
    pic.upgrade_canister(
        canister_id,
        MINTER_WASM_BYTES.to_vec(),
        upgrade_bytes,
        Some(sender_principal()),
    )
    .unwrap()
}

pub fn five_ticks(pic: &PocketIc) {
    pic.tick();
    pic.tick();
    pic.tick();
    pic.tick();
    pic.tick();
}

pub fn sender_principal() -> Principal {
    Principal::from_text("matbl-u2myk-jsllo-b5aw6-bxboq-7oon2-h6wmo-awsxf-pcebc-4wpgx-4qe").unwrap()
}

pub fn minter_principal() -> Principal {
    Principal::from_text("2ztvj-yaaaa-aaaap-ahiza-cai").unwrap()
}

pub fn lsm_principal() -> Principal {
    Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap()
}

pub fn icp_principal() -> Principal {
    Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap()
}

pub fn native_ledger_principal() -> Principal {
    Principal::from_text("n44gr-qyaaa-aaaam-qbuha-cai").unwrap()
}
// Initalizes a test environmet containing evm_rpc_canister, lsm canister, native ledger caister and native index canister.
// Through this test simulation, real senarios like concurrncy, http failures, no consensus agreement, etc can be tested.

// First  the dependency canisters are installed then the minter casniter is intalled.
pub mod intialize_minter {
    use super::*;

    pub fn create_and_install_minter_plus_dependency_canisters(pic: &PocketIc) {
        // Create and install icp ledger
        let icp_cansiter_id = create_icp_ledger_canister(&pic);
        pic.add_cycles(icp_cansiter_id, TWO_TRILLIONS.into());
        install_icp_ledger_canister(&pic, icp_cansiter_id);
        five_ticks(&pic);

        // Create and install lsm casniter
        let lsm_casniter_id = create_lsm_canister(&pic);
        pic.add_cycles(lsm_casniter_id, TWENTY_TRILLIONS.into());
        install_lsm_canister(&pic, lsm_casniter_id);
        five_ticks(&pic);

        // Withdrawal Section
        // Calling icrc2_approve and giving the permission to lsm for taking funds from users principal
        let _approve_result = update_call::<ApproveArgs, Result<Nat, ApproveError>>(
            &pic,
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

        five_ticks(&pic);

        // Add icUSDC to lsm
        let _result = update_call::<AddErc20Arg, Result<(), AddErc20Error>>(
            &pic,
            lsm_principal(),
            "add_erc20_ls",
            AddErc20Arg {
                contract: Erc20Contract {
                    chain_id: EvmNetwork::BSCTestnet.chain_id().into(),
                    address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                },
                ledger_init_arg: LedgerInitArg {
                    transfer_fee: Nat::from(10_000_u128),
                    decimals: 6,
                    token_name: "USD Coin on icp".to_string(),
                    token_symbol: "icUSDC".to_string(),
                    token_logo: "".to_string(),
                },
            },
            None,
        )
        .unwrap();

        five_ticks(&pic);

        // Advance time for 1 hour.
        pic.advance_time(Duration::from_secs(1 * 60 * 60));

        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);
        five_ticks(&pic);

        // Create and install evm rpc canister
        let evm_rpc_canister_id = create_evm_rpc_canister(&pic);
        pic.add_cycles(evm_rpc_canister_id, TWO_TRILLIONS.into());
        install_evm_rpc_canister(&pic, evm_rpc_canister_id);
        five_ticks(&pic);

        // Create and install native ledger canister
        let native_ledger_canister_id = create_native_ledger_canister(&pic);
        pic.add_cycles(native_ledger_canister_id, TWO_TRILLIONS.into());
        install_native_ledger_canister(&pic, native_ledger_canister_id);
        five_ticks(&pic);

        // Create and install native index canister
        let native_index_canister_id = create_index_canister(&pic);
        pic.add_cycles(native_index_canister_id, TWO_TRILLIONS.into());
        install_index_canister(&pic, native_index_canister_id);
        five_ticks(&pic);

        // Create and install minter canister for bsc test net
        let minter_id = create_minter_canister(&pic);
        pic.add_cycles(minter_id, 1_000_000_000_000);
        install_minter_canister(&pic, minter_id);
        five_ticks(&pic);
    }
}

pub fn generate_successful_mock_response(
    subnet_id: Principal,
    request_id: u64,
    body: Vec<u8>,
) -> MockCanisterHttpResponse {
    MockCanisterHttpResponse {
        subnet_id,
        request_id,
        response: CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
            status: 200,
            headers: vec![],
            body: body.to_vec(),
        }),
        additional_responses: vec![],
    }
}
