#[cfg(test)]
mod tests;

// Ledger suite manager helper functions
// This module produces the wasm hashes that are used by native twin ledger and index canister
// With the produced wasm hash the necessary type for calling the add_new_native_ls function of lsm(Ledger suite manager) is then produced.
// As a next step an inter-canister call will happen at the init time to add the native Ledger suite the the LSM(ledger suite manager).
// This mechanism is designed to maintain cycles balance of twin native ledger suite checked through the manager canister.

use std::fmt::{Debug, Display, Formatter};

use crate::icrc_client::runtime::IcrcBoundedRuntime;
use crate::logs::INFO;
use crate::management::Reason;
use crate::state::{read_state, State};
use crate::{logs::DEBUG, management::CallError};
use candid::{self, CandidType, Nat, Principal};
use ic_canister_log::log;
use ic_cdk;
use icrc_ledger_client::ICRC1Client;
use icrc_ledger_types::icrc::generic_metadata_value::MetadataValue;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_bytes::ByteArray;
pub(crate) const LEDGER_BYTECODE: &[u8] =
    include_bytes!("../../wasm/index_ng_canister_u256.wasm.gz");
pub(crate) const INDEX_BYTECODE: &[u8] = include_bytes!("../../wasm/ledger_canister_u256.wasm.gz");

pub(crate) const _LEDGER_BYTECODE_RAW: &[u8] =
    include_bytes!("../../wasm/index_ng_canister_u256.raw.wasm");
pub(crate) const _INDEX_BYTECODE_RAW: &[u8] =
    include_bytes!("../../wasm/ledger_canister_u256.raw.wasm");

const ADD_NATIVE_LS_METHOD: &str = "add_native_ls";

// Define Hash types
const WASM_HASH_LENGTH: usize = 32;
pub type WasmHash = Hash<WASM_HASH_LENGTH>;

impl WasmHash {
    pub fn new(binary: Vec<u8>) -> Self {
        WasmHash::from(ic_crypto_sha2::Sha256::hash(binary.as_slice()))
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Deserialize, Serialize)]
#[serde(from = "serde_bytes::ByteArray<N>", into = "serde_bytes::ByteArray<N>")]
pub struct Hash<const N: usize>([u8; N]);

impl<const N: usize> Default for Hash<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> Display for Hash<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl<const N: usize> From<ByteArray<N>> for Hash<N> {
    fn from(value: ByteArray<N>) -> Self {
        Self(value.into_array())
    }
}

impl<const N: usize> From<Hash<N>> for ByteArray<N> {
    fn from(value: Hash<N>) -> Self {
        ByteArray::new(value.0)
    }
}

impl<const N: usize> AsRef<[u8]> for Hash<N> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<const N: usize> From<[u8; N]> for Hash<N> {
    fn from(value: [u8; N]) -> Self {
        Self(value)
    }
}

impl<const N: usize> From<Hash<N>> for [u8; N] {
    fn from(value: Hash<N>) -> Self {
        value.0
    }
}

#[derive(Clone, PartialEq, Debug, CandidType, Serialize, candid::Deserialize)]
pub struct InstalledNativeLedgerSuite {
    pub fee: candid::Nat,
    pub decimals: u8,
    pub logo: String,
    pub name: String,
    pub chain_id: candid::Nat,
    pub ledger_wasm_hash: String,
    pub ledger: Principal,
    pub index_wasm_hash: String,
    pub index: Principal,
    pub archives: Vec<Principal>,
    pub symbol: String,
}

#[derive(Clone, PartialEq, Debug, CandidType, Serialize, candid::Deserialize)]
pub enum InvalidNativeInstalledCanistersError {
    TokenAlreadyManaged,
    NotAllowed,
    WasmHashError,
    FailedToNotifyAppicHelper,
    AlreadyManagedPrincipals,
}

#[derive(Clone, PartialEq, Debug)]
pub struct LSMClient(Principal);

impl LSMClient {
    pub fn new(lsm_id: Principal) -> Self {
        Self(lsm_id)
    }
    pub fn new_native_ls(
        &self,
        symbol: String,
        ledger_id: Principal,
        index_id: Principal,
        chain_id: u64,
        fee: Nat,
        decimals: u8,
        logo: String,
        name: String,
    ) -> InstalledNativeLedgerSuite {
        return InstalledNativeLedgerSuite {
            symbol,
            ledger: ledger_id,
            ledger_wasm_hash: WasmHash::new(LEDGER_BYTECODE.to_vec()).to_string(),
            index: index_id,
            index_wasm_hash: WasmHash::new(INDEX_BYTECODE.to_vec()).to_string(),
            archives: vec![],
            chain_id: Nat::from(chain_id),
            fee,
            decimals,
            logo,
            name,
        };
    }
    // Produces the InstalledNativeLedgerSuite through init args
    pub async fn call_lsm_to_add_twin_native(
        self,
        state: State,
    ) -> Result<(), InvalidNativeInstalledCanistersError> {
        let chain_id = state.evm_network.chain_id();

        let icrc_client = ICRC1Client {
            runtime: IcrcBoundedRuntime,
            ledger_canister_id: state.native_ledger_id,
        };

        let logo = match icrc_client.metadata().await {
            Ok(metadata_list) => metadata_list
                .into_iter()
                .find_map(|(title, value)| {
                    if title == "icrc1:logo" {
                        let logo_string = match value {
                            MetadataValue::Nat(_nat) => "".to_string(),
                            MetadataValue::Int(_int) => "".to_string(),
                            MetadataValue::Text(text) => text,
                            MetadataValue::Blob(_byte_buf) => "".to_string(),
                        };
                        return Some(logo_string);
                    } else {
                        return None;
                    }
                })
                .unwrap_or("".to_string()),
            Err(_) => "".to_string(),
        };

        let native_ls_args = self.new_native_ls(
            state.native_symbol.to_string(),
            state.native_ledger_id,
            state.native_index_id,
            chain_id,
            state.native_ledger_transfer_fee.into(),
            18_u8, // Native tokens always have 18 decimals
            logo,
            state.native_symbol.to_string(),
        );

        let result: Result<(), InvalidNativeInstalledCanistersError> = self
            .call_canister(self.0, ADD_NATIVE_LS_METHOD, native_ls_args)
            .await
            .expect("This call should be successful for a successful initialization");

        result
    }

    async fn call_canister<I, O>(
        &self,
        canister_id: Principal,
        method: &str,
        arg: I,
    ) -> Result<O, CallError>
    where
        I: CandidType + Debug + Send + 'static,
        O: CandidType + DeserializeOwned + Debug + 'static,
    {
        log!(
            DEBUG,
            "Calling canister '{}' with method '{}' and payload '{:?}'",
            canister_id,
            method,
            arg
        );
        let res = ic_cdk::call::Call::unbounded_wait(canister_id, method)
            .with_arg(&arg)
            .await
            .map_err(|err| CallError {
                reason: Reason::from_call_failed(err),
                method: method.to_string(),
            })?
            .candid();

        log!(
            DEBUG,
            "Result of calling canister '{}' with method '{}' and payload '{:?}': {:?}",
            canister_id,
            method,
            arg,
            res
        );

        match res {
            Ok(output) => Ok(output),
            Err(_err) => Err(CallError {
                method: method.to_string(),
                reason: Reason::DecodingFailed,
            }),
        }
    }
}

pub async fn lazy_add_native_ls_to_lsm_canister() {
    // Call ledger_suite_manager to add the native twin token

    let state = read_state(|s| s.clone());

    let lsm_client = LSMClient::new(state.ledger_suite_manager_id.unwrap());

    let add_native_ls_result = lsm_client.call_lsm_to_add_twin_native(state.clone()).await;
    match add_native_ls_result {
        Ok(()) => {
            log!(INFO, "Added native ls to lsm canister");
        }

        Err(e) => {
            log!(DEBUG, "Failed to to add native ls to lsm canister.{:?}", e);
        }
    }
}
