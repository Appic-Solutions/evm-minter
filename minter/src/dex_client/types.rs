use candid::{CandidType, Nat};
use serde::Deserialize;

// a fetched swap event from the swap contract logs
#[derive(CandidType, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct ReceivedSwapOrderEvent {
    pub from_address: String,
    // recipient can be either an EVM address or an ICP principal id or an BTC address
    pub recipient: String,
    // token in on the initial evm swap
    pub token_in: String,
    pub token_out: String,

    // amount in on the initial swap
    pub amount_in: Nat,
    pub amount_out: Nat,
    // the whole encoded swap transaction flow
    pub encoded_swap_data: String,
    pub tx_id: String,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub enum SwapOrderCreationError {
    InvalidMinter,
    InvalidAmountOut,
    InvalidFromAddress,
    InvalidOriginChain,
    InvalidToChain,
    InvalidOriginAndDestinationChain,
    FailedRlpDecoding,
    InvalidIcpSwapStep,
    InvalidRecipient(String),
    InvalidRlpData(RlpDecodeError),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, CandidType, Deserialize)]
pub enum RlpDecodeError {
    InvalidRlpData,
    InvalidStructure,
    InvalidDataType,
    MissingField,
    InvalidChainId(String),
    InvalidAmount,
    InvalidTokenAddress(String),
    DataTooLarge,
    VersionMismatch,
}
