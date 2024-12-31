use candid::{Nat, Principal};

use crate::lsm_client::{WasmHash, INDEX_BYTECODE, LEDGER_BYTECODE};

use super::{InstalledNativeLedgerSuite, LSMClient, _INDEX_BYTECODE_RAW, _LEDGER_BYTECODE_RAW};

#[test]
fn should_generate_installed_native_ledger_suite_args() {
    let lsm_client = LSMClient::new(Principal::from_text("kmcdp-4yaaa-aaaag-ats3q-cai").unwrap());

    let generated_args = lsm_client.new_native_ls(
        "icBNB".to_string(),
        Principal::from_text("n44gr-qyaaa-aaaam-qbuha-cai").unwrap(),
        Principal::from_text("eysav-tyaaa-aaaap-akqfq-cai").unwrap(),
        97_u64,
        Nat::from(1000000_u64),
        18,
        "".to_string(),
        "icBNB".to_string(),
    );

    let expected_installed_args = InstalledNativeLedgerSuite {
        symbol: "icBNB".to_string(),
        ledger: Principal::from_text("n44gr-qyaaa-aaaam-qbuha-cai").unwrap(),
        ledger_wasm_hash: WasmHash::new(LEDGER_BYTECODE.to_vec()).to_string(),
        index: Principal::from_text("eysav-tyaaa-aaaap-akqfq-cai").unwrap(),
        index_wasm_hash: WasmHash::new(INDEX_BYTECODE.to_vec()).to_string(),
        archives: vec![],
        chain_id: Nat::from(97_u64),
        fee: Nat::from(1000000_u64),
        decimals: 18,
        logo: "".to_string(),
        name: "icBNB".to_string(),
    };

    assert_eq!(generated_args, expected_installed_args);
}

#[test]
fn should_generate_wasm_hash() {
    let ledger_wasm_hash = WasmHash::new(LEDGER_BYTECODE.to_vec()).to_string();

    let index_wasm_hash = WasmHash::new(INDEX_BYTECODE.to_vec()).to_string();

    assert_eq!(
        ledger_wasm_hash,
        "95a33968b765f72111e7e069ec971ff1799b138db541a1b97e050bcb3f4035e5".to_string()
    );

    assert_eq!(
        index_wasm_hash,
        "21268ee81181bb12ac6ea45881305259878a0c26aa61eb924f2c121579658116".to_string()
    );
}

#[test]
fn gzipped_wasm_hash_should_not_be_equal_to_raw_wasm() {
    let ledger_wasm_hash = WasmHash::new(LEDGER_BYTECODE.to_vec()).to_string();

    let index_wasm_hash = WasmHash::new(INDEX_BYTECODE.to_vec()).to_string();

    let ledger_wasm_hash_raw = WasmHash::new(_LEDGER_BYTECODE_RAW.to_vec()).to_string();

    let index_wasm_hash_raw = WasmHash::new(_INDEX_BYTECODE_RAW.to_vec()).to_string();

    assert_ne!(ledger_wasm_hash, ledger_wasm_hash_raw);

    assert_ne!(index_wasm_hash, index_wasm_hash_raw);
}
