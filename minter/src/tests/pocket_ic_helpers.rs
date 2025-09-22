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
use pocket_ic::{
    common::rest::{CanisterHttpReply, CanisterHttpResponse, MockCanisterHttpResponse},
    RejectResponse,
};

pub const MINTER_WASM_BYTES: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/evm_minter.wasm");
pub const LEDGER_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/ledger_canister_u256.wasm.gz");
pub const INDEX_WAM_BYTES: &[u8] = include_bytes!("../../../wasm/index_ng_canister_u256.wasm.gz");
pub const ARCHIVE_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/archive_canister_u256.wasm.gz");
pub const LSM_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/lsm.wasm");
pub const EVM_RPC_WASM_BYTES: &[u8] = include_bytes!("../../../wasm/evm_rpc.wasm");
pub const APPIC_HELPER_BYTES: &[u8] = include_bytes!("../../../wasm/appic_helper.wasm");
pub const DEX_CANISTER_BYTES: &[u8] = include_bytes!("../../../wasm/appic_dex.wasm");
pub const PROXY_CANISTER_BYTES: &[u8] = include_bytes!("../../../wasm/proxy_canister.wasm");

pub const TWENTY_TRILLIONS: u64 = 20_000_000_000_000;

pub const FIVE_TRILLIONS: u64 = 5_000_000_000_000;

pub const FOUR_TRILLIONS: u64 = 4_000_000_000_000;

pub const TWO_TRILLIONS: u64 = 2_000_000_000_000;

use candid::{CandidType, Nat, Principal};
use evm_rpc_client::evm_rpc_types::InstallArgs;
use pocket_ic::{PocketIc, PocketIcBuilder};

use super::{
    appic_helper_types::{InitArgs, LoggerArgs, MinterArgs},
    lsm_types::{InitArg as LsmInitArgs, LSMarg, LedgerManagerInfo},
};

use super::ledger_arguments::{
    ArchiveOptions, FeatureFlags as LedgerFeatureFlags, IndexArg, IndexInitArg,
    InitArgs as LedgerInitArgs, LedgerArgument, MetadataValue as LedgerMetadataValue,
};

use crate::{
    candid_types::{CandidBlockTag, Erc20Token, GasTankBalance, MinterInfo},
    evm_config::EvmNetwork,
    lifecycle::{InitArg, MinterArg, UpgradeArg},
    lsm_client::WasmHash,
    tests::{
        ledger_arguments::FeatureFlags,
        lsm_types::{
            AddErc20Arg, AddErc20Error, CyclesManagement, Erc20Contract, LedgerInitArg,
            LedgerSuiteVersion, ManagedCanisterStatus, ManagedCanisters,
        },
        swap::helpers::{base_minter_principal, bsc_minter_principal},
    },
};
//use ic_icrc1_index_ng::{IndexArg, InitArg as IndexInitArg};
use initialize_minter::create_and_install_minter_plus_dependency_canisters;

#[test]
fn should_create_and_install_and_upgrade_minter_canister() {
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
            deposit_native_fee: None,
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
            ledger_suite_manager_id: Some("kmcdp-4yaaa-aaaag-ats3q-cai".parse().unwrap()),
            total_collected_operation_fee: Some(Nat::from(0_u128)),
            icrc_balances: Some(vec![]),
            wrapped_icrc_tokens: Some(vec![]),
            helper_smart_contract_addresses: Some(vec![
                "0x733a1BEeF5A02990aAD285d7ED93fc1b622EeF1d".to_string()
            ]),
            is_swapping_active: false,
            dex_canister_id: None,
            swap_contract_address: None,
            twin_usdc_info: None,
            canister_signing_fee_twin_usdc_value: None,
            gas_tank: Some(GasTankBalance {
                native_balance: Nat::from(0_u8),
                usdc_balance: Nat::from(0_u8)
            }),
            last_native_token_usd_price_estimate: None,
            next_swap_ledger_burn_index: None
        }
    );

    let upgrade_args = MinterArg::UpgradeArg(UpgradeArg {
        native_minimum_withdrawal_amount: Some(Nat::from(400_000_000_000_000_u128)),
        native_ledger_transfer_fee: None,
        next_transaction_nonce: None,
        last_scraped_block_number: Some(Nat::from(100935911_u128)),
        evm_rpc_id: Some("7hfb6-caaaa-aaaar-qadga-cai".parse().unwrap()),
        helper_contract_address: Some("0xa2dD817c2fDc3a2996f1A5174CF8f1AaED466E82".to_string()),
        block_height: None,
        min_max_priority_fee_per_gas: None,
        deposit_native_fee: None,
        withdrawal_native_fee: Some(Nat::from(200_000_000_000_000_u64)),
    });
    let upgrade_bytes = candid::encode_one(upgrade_args).unwrap();

    upgrade_minter_canister(&pic, canister_id, upgrade_bytes);

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
            helper_smart_contract_addresses: Some(vec![
                "0x733a1BEeF5A02990aAD285d7ED93fc1b622EeF1d".to_string(),
                "0xa2dD817c2fDc3a2996f1A5174CF8f1AaED466E82".to_string()
            ]),
            supported_erc20_tokens: Some(vec![]),
            minimum_withdrawal_amount: Some(Nat::from(400_000_000_000_000_u128)),
            deposit_native_fee: None,
            withdrawal_native_fee: Some(Nat::from(200_000_000_000_000_u64)),
            block_height: Some(CandidBlockTag::Latest),
            last_observed_block_number: None,
            native_balance: Some(Nat::from(0_u128)),
            last_gas_fee_estimate: None,
            erc20_balances: Some(vec![]),
            last_scraped_block_number: Some(Nat::from(100935911_u128)),
            native_twin_token_ledger_id: Some("n44gr-qyaaa-aaaam-qbuha-cai".parse().unwrap()),
            swap_canister_id: None,
            ledger_suite_manager_id: Some("kmcdp-4yaaa-aaaag-ats3q-cai".parse().unwrap()),
            total_collected_operation_fee: Some(Nat::from(0_u128)),
            icrc_balances: Some(vec![]),
            wrapped_icrc_tokens: Some(vec![]),
            is_swapping_active: false,
            dex_canister_id: None,
            swap_contract_address: None,
            twin_usdc_info: None,
            canister_signing_fee_twin_usdc_value: None,
            gas_tank: Some(GasTankBalance {
                native_balance: Nat::from(0_u8),
                usdc_balance: Nat::from(0_u8)
            }),
            last_native_token_usd_price_estimate: None,
            next_swap_ledger_burn_index: None
        }
    );
}

#[test]
fn should_create_and_install_all_minter_dependency_canisters() {
    let pic = create_pic();

    // Create and install lsm canister
    let lsm_canister_id = create_lsm_canister(&pic);
    pic.add_cycles(lsm_canister_id, TWO_TRILLIONS.into());
    install_lsm_canister(&pic, lsm_canister_id);
    five_ticks(&pic);
    let lsm_info = query_call::<(), LedgerManagerInfo>(&pic, lsm_canister_id, "get_lsm_info", ());
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
            minter_ids: vec![
                (Nat::from(56_u64), minter_principal()),
                (Nat::from(8453_u64), base_minter_principal())
            ],
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
fn should_install_lsm_canister_and_create_ledger_suite() {
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
                chain_id: EvmNetwork::BSC.chain_id().into(),
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
                    chain_id: Nat::from(56_u64),
                    address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                }
        })
        .unwrap()
        .ledger
        .unwrap()
    {
        ManagedCanisterStatus::Created { canister_id: _ } => {
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
                    chain_id: Nat::from(56_u64),
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
                    chain_id: Nat::from(56_u64),
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
                    chain_id: 56_u64.into()
                })
            .unwrap(),
        ManagedCanisters {
            erc20_contract: Erc20Contract {
                address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                chain_id: 56_u64.into()
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
    let wasm_result = pic.query_call(
        canister_id,
        sender_principal(),
        method,
        encode_call_args(payload).unwrap(),
    );

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
    let sender_principal = match sender {
        Some(p_id) => p_id,
        None => sender_principal(),
    };
    let wasm_result = pic.update_call(
        canister_id,
        sender_principal,
        method,
        encode_call_args(payload).unwrap(),
    );

    decode_wasm_result::<O>(wasm_result).unwrap()
}

pub fn encode_call_args<I>(args: I) -> Result<Vec<u8>, ()>
where
    I: CandidType,
{
    Ok(candid::encode_one(args).unwrap())
}

pub fn decode_wasm_result<O>(result: Result<Vec<u8>, RejectResponse>) -> Result<O, ()>
where
    O: CandidType + for<'a> serde::Deserialize<'a>,
{
    match result {
        Ok(bytes) => Ok(candid::decode_one(&bytes).unwrap()),
        Err(e) => panic!("{e:?}"),
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
        .expect("Should create the canister")
}

fn install_minter_canister(pic: &PocketIc, canister_id: Principal) {
    let init_args = MinterArg::InitArg(InitArg {
        evm_network: crate::evm_config::EvmNetwork::BSC,
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

pub fn create_lsm_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap(),
    )
    .expect("Should create the canister")
}

pub fn install_lsm_canister(pic: &PocketIc, canister_id: Principal) {
    let lsm_init_bytes = LSMarg::Init(LsmInitArgs {
        more_controller_ids: vec![sender_principal()],
        minter_ids: vec![
            (Nat::from(56_u64), bsc_minter_principal()),
            (Nat::from(8453_u64), base_minter_principal()),
        ],
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

pub fn create_appic_helper_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("zjydy-zyaaa-aaaaj-qnfka-cai").unwrap(),
    )
    .expect("Should create the canister")
}

pub fn install_appic_helper_canister(pic: &PocketIc, canister_id: Principal) {
    let appic_helper_init = LoggerArgs::Init(InitArgs {
        minters: vec![MinterArgs {
            chain_id: Nat::from(56_u64),
            minter_id: minter_principal(),
            operator: super::appic_helper_types::Operator::AppicMinter,
            last_observed_event: Nat::from(0_u8),
            last_scraped_event: Nat::from(0_u8),
            evm_to_icp_fee: Nat::from(50_000_000_000_000_u64),
            icp_to_evm_fee: Nat::from(100_000_000_000_000_u64),
        }],
    });
    pic.install_canister(
        canister_id,
        APPIC_HELPER_BYTES.to_vec(),
        encode_call_args(appic_helper_init).unwrap(),
        Some(sender_principal()),
    );
}

pub fn create_icp_ledger_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap(),
    )
    .expect("Should create the canister")
}

pub fn install_icp_ledger_canister(pic: &PocketIc, canister_id: Principal) {
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
        minting_account: LedgerAccount::from(
            Principal::from_text("rwlgt-iiaaa-aaaaa-aaaaa-cai").unwrap(),
        ),
        fee_collector_account: Some(LedgerAccount {
            owner: minter_id,
            subaccount: Some(LEDGER_FEE_SUBACCOUNT),
        }),
        initial_balances: vec![
            (
                LedgerAccount::from(sender_principal()),
                Nat::from(15_500_020_000_u128),
            ),
            (
                LedgerAccount::from(
                    Principal::from_text(
                        "b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe",
                    )
                    .unwrap(),
                ),
                Nat::from(5_000_000_000_u128),
            ),
        ],
        transfer_fee: Nat::from(10_000_u128),
        decimals: Some(8_u8),
        token_name: "icTestBNB".to_string(),
        token_symbol: "icTestBNB".to_string(),
        metadata: vec![],
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

pub fn create_evm_rpc_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("sosge-5iaaa-aaaag-alcla-cai").unwrap(),
    )
    .expect("Should create the canister")
}

pub fn install_evm_rpc_canister(pic: &PocketIc, canister_id: Principal) {
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
    .expect("Should create the canister")
}

fn install_native_ledger_canister(pic: &PocketIc, canister_id: Principal) {
    const LEDGER_FEE_SUBACCOUNT: [u8; 32] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x0f, 0xee,
    ];
    const MAX_MEMO_LENGTH: u16 = 80;
    const ICRC2_FEATURE: FeatureFlags = FeatureFlags { icrc2: true };

    const THREE_GIGA_BYTES: u64 = 3_221_225_472;

    let minter_id = minter_principal();

    let ledger_init_bytes = LedgerArgument::Init(LedgerInitArgs {
        minting_account: Account::from(minter_id),
        fee_collector_account: Some(Account {
            owner: minter_id,
            subaccount: Some(LEDGER_FEE_SUBACCOUNT),
        }),
        initial_balances: vec![],
        transfer_fee: Nat::from(10_000_000_000_000_u128),
        decimals: Some(18_u8),
        token_name: "icTestBNB".to_string(),
        token_symbol: "icTestBNB".to_string(),
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

fn create_index_canister(pic: &PocketIc) -> Principal {
    pic.create_canister_with_id(
        Some(sender_principal()),
        None,
        Principal::from_text("eysav-tyaaa-aaaap-akqfq-cai").unwrap(),
    )
    .expect("Should create the canister")
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

pub fn upgrade_minter_canister(pic: &PocketIc, canister_id: Principal, upgrade_bytes: Vec<u8>) {
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
// Initializes a test environment containing evm_rpc_canister, lsm canister, native ledger canister and native index canister.
// Through this test simulation, real scenarios like concurrency, http failures, no consensus agreement, etc can be tested.

// First  the dependency canisters are installed then the minter canister is installed.
pub mod initialize_minter {
    use super::*;

    pub fn create_and_install_minter_plus_dependency_canisters(pic: &PocketIc) {
        // Create and install icp ledger
        let icp_canister_id = create_icp_ledger_canister(pic);
        pic.add_cycles(icp_canister_id, TWO_TRILLIONS.into());
        install_icp_ledger_canister(pic, icp_canister_id);
        five_ticks(pic);

        // Create and install appic helper
        let appic_helper_id = create_appic_helper_canister(pic);
        pic.add_cycles(appic_helper_id, TWENTY_TRILLIONS.into());
        install_appic_helper_canister(pic, appic_helper_id);
        five_ticks(pic);
        five_ticks(pic);

        // Create and install lsm canister
        let lsm_canister_id = create_lsm_canister(pic);
        pic.add_cycles(lsm_canister_id, TWENTY_TRILLIONS.into());
        install_lsm_canister(pic, lsm_canister_id);
        five_ticks(pic);
        five_ticks(pic);

        // Create and install evm rpc canister
        let evm_rpc_canister_id = create_evm_rpc_canister(pic);
        pic.add_cycles(evm_rpc_canister_id, TWO_TRILLIONS.into());
        install_evm_rpc_canister(pic, evm_rpc_canister_id);
        five_ticks(pic);

        // Create and install native ledger canister
        let native_ledger_canister_id = create_native_ledger_canister(pic);
        pic.add_cycles(native_ledger_canister_id, TWO_TRILLIONS.into());
        install_native_ledger_canister(pic, native_ledger_canister_id);
        five_ticks(pic);

        // Create and install native index canister
        let native_index_canister_id = create_index_canister(pic);
        pic.add_cycles(native_index_canister_id, TWO_TRILLIONS.into());
        install_index_canister(pic, native_index_canister_id);
        five_ticks(pic);

        // Create and install minter canister for bsc test net
        let minter_id = create_minter_canister(pic);
        pic.add_cycles(minter_id, 1_000_000_000_000);
        install_minter_canister(pic, minter_id);
        five_ticks(pic);
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
