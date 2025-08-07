// This is an experimental feature to generate Rust binding from Candid.
// You may want to manually adjust some of the types.
#![allow(dead_code, unused_imports)]
use std::fmt::{self, Debug, Formatter};

use candid::{self, types::Serializer, types::Type, CandidType, Decode, Encode, Nat, Principal};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use std::fmt::Display;
use thiserror::Error;

use hex::FromHexError;

use ic_cdk::call::RejectCode;

#[derive(Clone, Debug, PartialEq, Eq, Default, CandidType, Deserialize)]
pub struct RpcConfig {
    #[serde(rename = "responseSizeEstimate")]
    pub response_size_estimate: Option<u64>,

    #[serde(rename = "responseConsensus")]
    pub response_consensus: Option<ConsensusStrategy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, CandidType, Deserialize)]
pub struct GetLogsRpcConfig {
    #[serde(rename = "responseSizeEstimate")]
    pub response_size_estimate: Option<u64>,

    #[serde(rename = "responseConsensus")]
    pub response_consensus: Option<ConsensusStrategy>,

    #[serde(rename = "maxBlockRange")]
    pub max_block_range: Option<u32>,
}

impl From<GetLogsRpcConfig> for RpcConfig {
    fn from(config: GetLogsRpcConfig) -> Self {
        Self {
            response_size_estimate: config.response_size_estimate,
            response_consensus: config.response_consensus,
        }
    }
}

impl GetLogsRpcConfig {
    pub fn max_block_range_or_default(&self) -> u32 {
        const DEFAULT_ETH_GET_LOGS_MAX_BLOCK_RANGE: u32 = 500;
        self.max_block_range
            .unwrap_or(DEFAULT_ETH_GET_LOGS_MAX_BLOCK_RANGE)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, CandidType, Deserialize)]
pub enum ConsensusStrategy {
    /// All providers must return the same non-error result.
    #[default]
    Equality,

    /// A subset of providers must return the same non-error result.
    Threshold {
        /// Total number of providers to be queried:
        /// * If `None`, will be set to the number of providers manually specified in `RpcServices`.
        /// * If `Some`, must correspond to the number of manually specified providers in `RpcServices`;
        ///   or if they are none indicating that default providers should be used, select the corresponding number of providers.
        total: Option<u8>,

        /// Minimum number of providers that must return the same (non-error) result.
        min: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RpcServices {
    Custom {
        #[serde(rename = "chainId")]
        chain_id: u64,
        services: Vec<RpcApi>,
    },
    EthMainnet(Option<Vec<EthMainnetService>>),
    EthSepolia(Option<Vec<EthSepoliaService>>),
    ArbitrumOne(Option<Vec<L2MainnetService>>),
    BaseMainnet(Option<Vec<L2MainnetService>>),
    OptimismMainnet(Option<Vec<L2MainnetService>>),
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, CandidType)]
pub struct HttpHeader {
    pub value: String,
    pub name: String,
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, CandidType)]
pub struct RpcApi {
    pub url: String,
    pub headers: Option<Vec<HttpHeader>>,
}

impl RpcApi {
    pub fn host_str(&self) -> Option<String> {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(|host| host.to_string()))
    }
}

impl Debug for RpcApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let host = self.host_str().unwrap_or("N/A".to_string());
        write!(f, "RpcApi {{ host: {}, url/headers: *** }}", host) //URL or header value could contain API keys
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, CandidType,
)]
pub enum EthMainnetService {
    Alchemy,
    Ankr,
    BlockPi,
    PublicNode,
    Cloudflare,
    Llama,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, CandidType,
)]
pub enum EthSepoliaService {
    Alchemy,
    Ankr,
    BlockPi,
    PublicNode,
    Sepolia,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, CandidType,
)]
pub enum L2MainnetService {
    Alchemy,
    Ankr,
    BlockPi,
    PublicNode,
    Llama,
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, CandidType)]
pub enum RpcService {
    Provider(u64),
    Custom(RpcApi),
    EthMainnet(EthMainnetService),
    EthSepolia(EthSepoliaService),
    ArbitrumOne(L2MainnetService),
    BaseMainnet(L2MainnetService),
    OptimismMainnet(L2MainnetService),
}

impl Debug for RpcService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcService::Provider(provider_id) => write!(f, "Provider({})", provider_id),
            RpcService::Custom(_) => write!(f, "Custom(..)"), // Redact credentials
            RpcService::EthMainnet(service) => write!(f, "{:?}", service),
            RpcService::EthSepolia(service) => write!(f, "{:?}", service),
            RpcService::ArbitrumOne(service)
            | RpcService::BaseMainnet(service)
            | RpcService::OptimismMainnet(service) => write!(f, "{:?}", service),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, CandidType, Deserialize, Serialize)]
pub struct Provider {
    #[serde(rename = "providerId")]
    pub provider_id: u64,
    #[serde(rename = "chainId")]
    pub chain_id: u64,
    pub access: RpcAccess,
    pub alias: Option<RpcService>,
}

#[derive(Debug, Clone, PartialEq, Eq, CandidType, Deserialize, Serialize)]
pub enum RpcAccess {
    Authenticated {
        auth: RpcAuth,
        /// Public URL to use when the API key is not available.
        #[serde(rename = "publicUrl")]
        public_url: Option<String>,
    },
    Unauthenticated {
        #[serde(rename = "publicUrl")]
        public_url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, CandidType, Deserialize, Serialize)]
pub enum RpcAuth {
    /// API key will be used in an Authorization header as Bearer token, e.g.,
    /// `Authorization: Bearer API_KEY`
    BearerToken { url: String },
    UrlParameter {
        #[serde(rename = "urlPattern")]
        url_pattern: String,
    },
}

/// A `Nat` that is guaranteed to fit in 256 bits.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "candid::Nat", into = "candid::Nat")]
pub struct Nat256(Nat);

impl Display for Nat256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0 .0)
    }
}

impl Debug for Nat256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0 .0)
    }
}

impl Nat256 {
    pub const ZERO: Nat256 = Nat256(Nat(BigUint::ZERO));

    pub fn into_be_bytes(self) -> [u8; 32] {
        let value_bytes = self.0 .0.to_bytes_be();
        let mut value_u256 = [0u8; 32];
        assert!(
            value_bytes.len() <= 32,
            "BUG: Nat does not fit in a U256: {:?}",
            self.0
        );
        value_u256[32 - value_bytes.len()..].copy_from_slice(&value_bytes);
        value_u256
    }

    pub fn from_be_bytes(value: [u8; 32]) -> Self {
        Self::try_from(Nat::from(BigUint::from_bytes_be(&value)))
            .expect("BUG: Nat should fit in a U256")
    }
}

impl AsRef<Nat> for Nat256 {
    fn as_ref(&self) -> &Nat {
        &self.0
    }
}

impl CandidType for Nat256 {
    fn _ty() -> Type {
        Nat::_ty()
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_nat(self.as_ref())
    }
}

impl TryFrom<Nat> for Nat256 {
    type Error = String;

    fn try_from(value: Nat) -> Result<Self, Self::Error> {
        if value.0.to_bytes_le().len() > 32 {
            Err("Nat does not fit in a U256".to_string())
        } else {
            Ok(Nat256(value))
        }
    }
}

impl From<Nat256> for Nat {
    fn from(value: Nat256) -> Self {
        value.0
    }
}

macro_rules! impl_from_unchecked {
    ($f: ty, $($t: ty)*) => ($(
        impl From<$t> for $f {
            #[inline]
            fn from(v: $t) -> Self { Self::try_from(Nat::from(v)).unwrap() }
        }
    )*)
}
// all the types below are guaranteed to fit in 256 bits
impl_from_unchecked!( Nat256, usize u8 u16 u32 u64 u128 );

macro_rules! impl_hex_string {
    ($name: ident($data: ty)) => {
        #[doc = concat!("Ethereum hex-string (String representation is prefixed by 0x) wrapping a `", stringify!($data), "`. ")]
        #[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name($data);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_str("0x")?;
                f.write_str(&hex::encode(&self.0))
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self)
            }
        }


        impl From<$data> for $name {
            fn from(value: $data) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $data {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<[u8]> for $name {
            fn as_ref(&self) -> &[u8] {
                self.0.as_ref()
            }
        }

        impl CandidType for $name {
            fn _ty() -> Type {
                String::_ty()
            }

            fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_text(&self.to_string())
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if !s.starts_with("0x") {
                    return Err("Ethereum hex string doesn't start with 0x".to_string());
                }
                hex::FromHex::from_hex(&s[2..])
                    .map(Self)
                    .map_err(|e| format!("Invalid Ethereum hex string: {}", e))
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                value.parse()
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_string()
            }
        }
    };
}

impl_hex_string!(HexByte(Byte));
impl_hex_string!(Hex20([u8; 20]));
impl_hex_string!(Hex32([u8; 32]));
impl_hex_string!(Hex256([u8; 256]));
impl_hex_string!(Hex(Vec<u8>));

/// A wrapper to be able to decode single character hex string
/// such as `0x0` or `0x1` into a byte. By default,
/// `FromHex::from_hex` will return `Err(FromHexError::OddLength)`
/// when trying to decode such strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Byte([u8; 1]);

impl Byte {
    pub fn into_byte(self) -> u8 {
        self.0[0]
    }
}

impl AsRef<[u8]> for Byte {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl hex::FromHex for Byte {
    type Error = FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let hex = hex.as_ref();
        match hex {
            &[a] => hex::FromHex::from_hex([b'0', a]).map(Self),
            h => hex::FromHex::from_hex(h).map(Self),
        }
    }
}

impl From<u8> for Byte {
    fn from(value: u8) -> Self {
        Self([value])
    }
}

impl From<u8> for HexByte {
    fn from(value: u8) -> Self {
        Self(Byte::from(value))
    }
}

impl From<HexByte> for u8 {
    fn from(value: HexByte) -> Self {
        value.0.into_byte()
    }
}

pub type RpcResult<T> = Result<T, RpcError>;

#[derive(Clone, Debug, Eq, PartialEq, CandidType, Deserialize)]
pub enum MultiRpcResult<T> {
    Consistent(RpcResult<T>),
    Inconsistent(Vec<(RpcService, RpcResult<T>)>),
}

impl<T> MultiRpcResult<T> {
    pub fn map<R>(self, mut f: impl FnMut(T) -> R) -> MultiRpcResult<R> {
        match self {
            MultiRpcResult::Consistent(result) => MultiRpcResult::Consistent(result.map(f)),
            MultiRpcResult::Inconsistent(results) => MultiRpcResult::Inconsistent(
                results
                    .into_iter()
                    .map(|(service, result)| {
                        (
                            service,
                            match result {
                                Ok(ok) => Ok(f(ok)),
                                Err(err) => Err(err),
                            },
                        )
                    })
                    .collect(),
            ),
        }
    }
}

impl<T: Debug> MultiRpcResult<T> {
    pub fn expect_consistent(self) -> RpcResult<T> {
        match self {
            MultiRpcResult::Consistent(result) => result,
            MultiRpcResult::Inconsistent(inconsistent_result) => {
                panic!("Expected consistent, but got: {:?}", inconsistent_result)
            }
        }
    }

    pub fn expect_inconsistent(self) -> Vec<(RpcService, RpcResult<T>)> {
        match self {
            MultiRpcResult::Consistent(consistent_result) => {
                panic!("Expected inconsistent:, but got: {:?}", consistent_result)
            }
            MultiRpcResult::Inconsistent(results) => results,
        }
    }
}

impl<T> From<RpcResult<T>> for MultiRpcResult<T> {
    fn from(result: RpcResult<T>) -> Self {
        MultiRpcResult::Consistent(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum RpcError {
    #[error("Provider error: {0}")]
    ProviderError(ProviderError),
    #[error("HTTP outcall error: {0}")]
    HttpOutcallError(HttpOutcallError),
    #[error("JSON-RPC error: {0}")]
    JsonRpcError(JsonRpcError),
    #[error("Validation error: {0}")]
    ValidationError(ValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum ProviderError {
    #[error("No permission to call this provider")]
    NoPermission,
    #[error("Not enough cycles, expected {expected}, received {received}")]
    TooFewCycles { expected: u128, received: u128 },
    #[error("Provider not found")]
    ProviderNotFound,
    #[error("Missing required provider")]
    MissingRequiredProvider,
    #[error("Invalid RPC config: {0}")]
    InvalidRpcConfig(String),
}

#[derive(CandidType, Deserialize, Clone, Copy, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RejectionCode {
    NoError = 0,

    SysFatal = 1,
    SysTransient = 2,
    DestinationInvalid = 3,
    CanisterReject = 4,
    CanisterError = 5,
    Unknown = 6,
}

impl From<i32> for RejectionCode {
    fn from(value: i32) -> Self {
        match value {
            0 => RejectionCode::NoError,
            1 => RejectionCode::SysFatal,
            2 => RejectionCode::SysTransient,
            3 => RejectionCode::DestinationInvalid,
            4 => RejectionCode::CanisterReject,
            5 => RejectionCode::CanisterError,
            _ => RejectionCode::Unknown,
        }
    }
}

impl From<RejectCode> for RejectionCode {
    fn from(value: RejectCode) -> Self {
        match value {
            RejectCode::SysFatal => Self::SysFatal,
            RejectCode::SysTransient => Self::SysTransient,
            RejectCode::DestinationInvalid => Self::DestinationInvalid,
            RejectCode::CanisterReject => Self::CanisterReject,
            RejectCode::CanisterError => Self::CanisterError,
            RejectCode::SysUnknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum HttpOutcallError {
    /// Error from the IC system API.
    #[error("IC error (code: {code:?}): {message}")]
    IcError {
        code: RejectionCode,
        message: String,
    },
    /// Response is not a valid JSON-RPC response,
    /// which means that the response was not successful (status other than 2xx)
    /// or that the response body could not be deserialized into a JSON-RPC response.
    #[error("Invalid HTTP JSON-RPC response: status {status}, body: {body}, parsing error: {parsing_error:?}")]
    InvalidHttpJsonRpcResponse {
        status: u16,
        body: String,
        #[serde(rename = "parsingError")]
        parsing_error: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, CandidType, Deserialize, Error)]
#[error("JSON-RPC error (code: {code}): {message}")]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum ValidationError {
    #[error("Custom: {0}")]
    Custom(String),
    #[error("Invalid hex: {0}")]
    InvalidHex(String),
}

impl From<ProviderError> for RpcError {
    fn from(err: ProviderError) -> Self {
        RpcError::ProviderError(err)
    }
}

impl From<HttpOutcallError> for RpcError {
    fn from(err: HttpOutcallError) -> Self {
        RpcError::HttpOutcallError(err)
    }
}

impl From<JsonRpcError> for RpcError {
    fn from(err: JsonRpcError) -> Self {
        RpcError::JsonRpcError(err)
    }
}

impl From<ValidationError> for RpcError {
    fn from(err: ValidationError) -> Self {
        RpcError::ValidationError(err)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, CandidType)]
pub struct FeeHistory {
    /// Lowest number block of the returned range.
    #[serde(rename = "oldestBlock")]
    pub oldest_block: Nat256,

    /// An array of block base fees per gas.
    /// This includes the next block after the newest of the returned range,
    /// because this value can be derived from the newest block.
    /// Zeroes are returned for pre-EIP-1559 blocks.
    #[serde(rename = "baseFeePerGas")]
    pub base_fee_per_gas: Vec<Nat256>,

    /// An array of block gas used ratios (gasUsed / gasLimit).
    #[serde(rename = "gasUsedRatio")]
    pub gas_used_ratio: Vec<f64>,

    /// A two-dimensional array of effective priority fees per gas at the requested block percentiles.
    #[serde(rename = "reward")]
    pub reward: Vec<Vec<Nat256>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, CandidType)]
pub struct LogEntry {
    /// The address from which this log originated.
    pub address: Hex20,

    /// Array of 0 to 4 32-byte DATA elements of indexed log arguments.
    /// In solidity: The first topic is the event signature hash (e.g. Deposit(address,bytes32,uint256)),
    /// unless you declared the event with the anonymous specifier.
    pub topics: Vec<Hex32>,

    /// Contains one or more 32-byte non-indexed log arguments.
    pub data: Hex,

    /// The block number in which this log appeared.
    /// None if the block is pending.
    #[serde(rename = "blockNumber")]
    pub block_number: Option<Nat256>,

    /// 32-byte hash of the transaction from which this log was created.
    /// None if the transaction is still pending.
    #[serde(rename = "transactionHash")]
    pub transaction_hash: Option<Hex32>,

    /// Integer of the transaction's position within the block the log was created from.
    /// None if the transaction is still pending.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Option<Nat256>,

    /// 32-byte hash of the block in which this log appeared.
    /// None if the block is pending.
    #[serde(rename = "blockHash")]
    pub block_hash: Option<Hex32>,

    /// Integer of the log index position in the block.
    /// None if the log is pending.
    #[serde(rename = "logIndex")]
    pub log_index: Option<Nat256>,

    /// "true" when the log was removed due to a chain reorganization.
    /// "false" if it is a valid log.
    #[serde(default)]
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, CandidType)]
pub struct TransactionReceipt {
    /// The hash of the block containing the transaction.
    #[serde(rename = "blockHash")]
    pub block_hash: Hex32,

    /// The number of the block containing the transaction.
    #[serde(rename = "blockNumber")]
    pub block_number: Nat256,

    /// The actual value per gas deducted from the sender's account.
    /// Before EIP-1559, this is equal to the transaction's gas price.
    /// After, it is equal to `baseFeePerGas + min(maxFeePerGas - baseFeePerGas, maxPriorityFeePerGas)`.
    #[serde(rename = "effectiveGasPrice")]
    pub effective_gas_price: Nat256,

    /// The amount of gas used by this specific transaction alone.
    #[serde(rename = "gasUsed")]
    pub gas_used: Nat256,

    /// Either 1 (success) or 0 (failure).
    /// Only specified for transactions included after the Byzantium upgrade.
    pub status: Option<Nat256>,

    /// The hash of the transaction
    #[serde(rename = "transactionHash")]
    pub transaction_hash: Hex32,

    /// The contract address created, if the transaction was a contract creation, otherwise `None`.
    #[serde(rename = "contractAddress")]
    pub contract_address: Option<Hex20>,

    /// The address of the sender
    pub from: Hex20,

    /// An array of log objects generated by this transaction.
    pub logs: Vec<LogEntry>,

    /// Bloom filter for light clients to quickly retrieve related logs.
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Hex256,

    /// Address of the receiver or `None` in a contract creation transaction.
    pub to: Option<Hex20>,

    /// Transaction's index position in the block
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Nat256,

    /// The type of the transaction:
    /// - "0x0" for legacy transactions (pre- EIP-2718)
    /// - "0x1" for access list transactions (EIP-2930)
    /// - "0x2" for EIP-1559 transactions
    #[serde(rename = "type")]
    pub tx_type: HexByte,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, CandidType)]
pub struct Block {
    /// Base fee per gas
    /// Only included for blocks after the London Upgrade / EIP-1559.
    #[serde(rename = "baseFeePerGas")]
    pub base_fee_per_gas: Option<Nat256>,

    /// Block number
    pub number: Nat256,

    /// Difficulty
    pub difficulty: Option<Nat256>,

    /// Extra data
    #[serde(rename = "extraData")]
    pub extra_data: Hex,

    /// Maximum gas allowed in this block
    #[serde(rename = "gasLimit")]
    pub gas_limit: Nat256,

    /// Gas used by all transactions in this block
    #[serde(rename = "gasUsed")]
    pub gas_used: Nat256,

    /// Block hash
    pub hash: Hex32,

    /// Bloom filter for the logs.
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Hex256,

    /// Miner
    pub miner: Hex20,

    /// Mix hash
    #[serde(rename = "mixHash")]
    pub mix_hash: Hex32,

    /// Nonce
    pub nonce: Nat256,

    /// Parent block hash
    #[serde(rename = "parentHash")]
    pub parent_hash: Hex32,

    /// Receipts root
    #[serde(rename = "receiptsRoot")]
    pub receipts_root: Hex32,

    /// Ommers hash
    #[serde(rename = "sha3Uncles")]
    pub sha3_uncles: Hex32,

    /// Block size
    pub size: Nat256,

    /// State root
    #[serde(rename = "stateRoot")]
    pub state_root: Hex32,

    /// Timestamp
    #[serde(rename = "timestamp")]
    pub timestamp: Nat256,

    /// Total difficulty is the sum of all difficulty values up to and including this block.
    ///
    /// Note: this field was removed from the official JSON-RPC specification in
    /// <https://github.com/ethereum/execution-apis/pull/570> and may no longer be served by providers.
    #[serde(rename = "totalDifficulty")]
    pub total_difficulty: Option<Nat256>,

    /// Transaction hashes
    #[serde(default)]
    pub transactions: Vec<Hex32>,

    /// Transactions root
    #[serde(rename = "transactionsRoot")]
    pub transactions_root: Option<Hex32>,

    /// Uncles
    #[serde(default)]
    pub uncles: Vec<Hex32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, CandidType)]
pub enum SendRawTransactionStatus {
    Ok(Option<Hex32>),
    InsufficientFunds,
    NonceTooLow,
    NonceTooHigh,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize, Default)]
pub enum BlockTag {
    #[default]
    Latest,
    Finalized,
    Safe,
    Earliest,
    Pending,
    Number(Nat256),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FeeHistoryArgs {
    /// Number of blocks in the requested range.
    /// Typically, providers request this to be between 1 and 1024.
    #[serde(rename = "blockCount")]
    pub block_count: Nat256,

    /// Highest block of the requested range.
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "newestBlock")]
    pub newest_block: BlockTag,

    /// A monotonically increasing list of percentile values between 0 and 100.
    /// For each block in the requested range, the transactions will be sorted in ascending order
    /// by effective tip per gas and the corresponding effective tip for the percentile
    /// will be determined, accounting for gas consumed.
    #[serde(rename = "rewardPercentiles")]
    pub reward_percentiles: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GetLogsArgs {
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "fromBlock")]
    pub from_block: Option<BlockTag>,

    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "toBlock")]
    pub to_block: Option<BlockTag>,

    /// Contract address or a list of addresses from which logs should originate.
    pub addresses: Vec<Hex20>,

    /// Array of 32-byte DATA topics.
    /// Topics are order-dependent.
    /// Each topic can also be an array of DATA with "or" options.
    pub topics: Option<Vec<Vec<Hex32>>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GetTransactionCountArgs {
    pub address: Hex20,
    pub block: BlockTag,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CallArgs {
    pub transaction: TransactionRequest,
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    /// Default to "latest" if unspecified, see <https://github.com/ethereum/execution-apis/issues/461>.
    pub block: Option<BlockTag>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransactionRequest {
    /// The type of the transaction:
    /// - "0x0" for legacy transactions (pre- EIP-2718)
    /// - "0x1" for access list transactions (EIP-2930)
    /// - "0x2" for EIP-1559 transactions
    #[serde(rename = "type")]
    pub tx_type: Option<HexByte>,

    /// Transaction nonce
    pub nonce: Option<Nat256>,

    /// Address of the receiver or `None` in a contract creation transaction.
    pub to: Option<Hex20>,

    /// The address of the sender.
    pub from: Option<Hex20>,

    /// Gas limit for the transaction.
    pub gas: Option<Nat256>,

    /// Amount of ETH sent with this transaction.
    pub value: Option<Nat256>,

    /// Transaction input data
    pub input: Option<Hex>,

    /// The legacy gas price willing to be paid by the sender in wei.
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<Nat256>,

    /// Maximum fee per gas the sender is willing to pay to miners in wei.
    #[serde(rename = "maxPriorityFeePerGas")]
    pub max_priority_fee_per_gas: Option<Nat256>,

    /// The maximum total fee per gas the sender is willing to pay (includes the network / base fee and miner / priority fee) in wei.
    #[serde(rename = "maxFeePerGas")]
    pub max_fee_per_gas: Option<Nat256>,

    /// The maximum total fee per gas the sender is willing to pay for blob gas in wei.
    #[serde(rename = "maxFeePerBlobGas")]
    pub max_fee_per_blob_gas: Option<Nat256>,

    /// EIP-2930 access list
    #[serde(rename = "accessList")]
    pub access_list: Option<AccessList>,

    /// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
    #[serde(rename = "blobVersionedHashes")]
    pub blob_versioned_hashes: Option<Vec<Hex32>>,

    /// Raw blob data.
    pub blobs: Option<Vec<Hex>>,

    /// Chain ID that this transaction is valid on.
    #[serde(rename = "chainId")]
    pub chain_id: Option<Nat256>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[serde(transparent)]
pub struct AccessList(pub Vec<AccessListEntry>);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccessListEntry {
    pub address: Hex20,
    #[serde(rename = "storageKeys")]
    pub storage_keys: Vec<Hex32>,
}

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
pub struct InstallArgs {
    pub demo: Option<bool>,
    #[serde(rename = "manageApiKeys")]
    pub manage_api_keys: Option<Vec<Principal>>,
    #[serde(rename = "logFilter")]
    pub log_filter: Option<LogFilter>,
    #[serde(rename = "overrideProvider")]
    pub override_provider: Option<OverrideProvider>,
    #[serde(rename = "nodesInSubnet")]
    pub nodes_in_subnet: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub enum LogFilter {
    ShowAll,
    HideAll,
    ShowPattern(RegexString),
    HidePattern(RegexString),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub struct OverrideProvider {
    #[serde(rename = "overrideUrl")]
    pub override_url: Option<RegexSubstitution>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub struct RegexString(pub String);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub struct RegexSubstitution {
    pub pattern: RegexString,
    pub replacement: String,
}
