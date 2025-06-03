use core::fmt;

use candid::Principal;
use ic_canister_log::log;
use minicbor::{Decode, Encode};
use new_contract::{ReceivedBurnEvent, ReceivedWrappedIcpTokenDeployedEvent};
use old_contract::{ReceivedErc20Event, ReceivedNativeEvent};
use thiserror::Error;

use crate::{
    checked_amount::CheckedAmountOf,
    logs::{DEBUG, INFO},
    numeric::{BlockNumber, LogIndex},
    rpc_declarations::{FixedSizeData, Hash},
};

#[cfg(test)]
mod test;

pub mod new_contract;
pub mod old_contract;
pub mod parser;
pub mod scraping;

/// A unique identifier of the event source: the source transaction hash and the log
/// entry index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct EventSource {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub log_index: LogIndex,
}

impl fmt::Display for EventSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}:{}", self.transaction_hash, self.log_index)
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EventSourceError {
    #[error("failed to decode principal from bytes {invalid_principal}")]
    InvalidPrincipal { invalid_principal: FixedSizeData },
    #[error("invalid ReceivedDepositEvent: {0}")]
    InvalidEvent(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceivedContractEvent {
    // old contract events
    NativeDeposit(ReceivedNativeEvent),
    Erc20Deposit(ReceivedErc20Event),
    // new contract events
    TokenBurn(ReceivedBurnEvent),
    WrappedDeployed(ReceivedWrappedIcpTokenDeployedEvent),
}

impl ReceivedContractEvent {
    /// Return event source, which is globally unique regardless of whether
    /// it is for ETH or ERC-20 deposit, burn or icp token deployed event. This is because the `transaction_hash` already
    /// unique determines the transaction, and `log_index` would match the place
    /// in which event appears for this transaction.
    pub fn source(&self) -> EventSource {
        match self {
            ReceivedContractEvent::NativeDeposit(evt) => evt.source(),
            ReceivedContractEvent::Erc20Deposit(evt) => evt.source(),
            ReceivedContractEvent::TokenBurn(evt) => evt.source(),
            ReceivedContractEvent::WrappedDeployed(evt) => evt.source(),
        }
    }
    pub fn block_number(&self) -> BlockNumber {
        match self {
            ReceivedContractEvent::NativeDeposit(evt) => evt.block_number,
            ReceivedContractEvent::Erc20Deposit(evt) => evt.block_number,
            ReceivedContractEvent::TokenBurn(evt) => evt.block_number,
            ReceivedContractEvent::WrappedDeployed(evt) => evt.block_number,
        }
    }
    pub fn log_index(&self) -> LogIndex {
        match self {
            ReceivedContractEvent::NativeDeposit(evt) => evt.log_index,
            ReceivedContractEvent::Erc20Deposit(evt) => evt.log_index,
            ReceivedContractEvent::TokenBurn(evt) => evt.log_index,
            ReceivedContractEvent::WrappedDeployed(evt) => evt.log_index,
        }
    }
    pub fn transaction_hash(&self) -> Hash {
        match self {
            ReceivedContractEvent::NativeDeposit(evt) => evt.transaction_hash,
            ReceivedContractEvent::Erc20Deposit(evt) => evt.transaction_hash,
            ReceivedContractEvent::TokenBurn(evt) => evt.transaction_hash,
            ReceivedContractEvent::WrappedDeployed(evt) => evt.transaction_hash,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceivedContractEventError {
    PendingLogEntry,
    InvalidEventSource {
        source: EventSource,
        error: EventSourceError,
    },
}

pub fn report_transaction_error(error: ReceivedContractEventError) {
    match error {
        ReceivedContractEventError::PendingLogEntry => {
            log!(
                DEBUG,
                "[report_transaction_error]: ignoring pending log entry",
            );
        }
        ReceivedContractEventError::InvalidEventSource { source, error } => {
            log!(
                INFO,
                "[report_transaction_error]: cannot process {source} due to {error}",
            );
        }
    }
}

enum InternalLedgerSubaccountTag {}
type InternalLedgerSubaccount = CheckedAmountOf<InternalLedgerSubaccountTag>;

/// Ledger subaccount.
///
/// Internally represented as a u256 to optimize cbor encoding for low values,
/// which can be represented as a u32 or a u64.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Decode, Encode)]
pub struct LedgerSubaccount(#[n(0)] InternalLedgerSubaccount);

impl LedgerSubaccount {
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        const DEFAULT_SUBACCOUNT: [u8; 32] = [0; 32];
        if bytes == DEFAULT_SUBACCOUNT {
            return None;
        }
        Some(Self(InternalLedgerSubaccount::from_be_bytes(bytes)))
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_be_bytes()
    }
}

/// Decode a candid::Principal from a slice of at most 32 bytes
/// encoded as follows
/// - the first byte is the number of bytes in the principal
/// - the next N bytes are the principal
/// - the remaining bytes are zero
///
/// Any other encoding will return an error.
/// Some specific valid [`Principal`]s are also not allowed
/// since the decoded principal will be used to receive twin tokens:
/// * the management canister principal
/// * the anonymous principal
///
/// This method MUST never panic (decode bytes from untrusted sources).
fn parse_principal_from_slice(slice: &[u8]) -> Result<Principal, String> {
    const ANONYMOUS_PRINCIPAL_BYTES: [u8; 1] = [4];

    if slice.is_empty() {
        return Err("slice too short".to_string());
    }
    if slice.len() > 32 {
        return Err(format!("Expected at most 32 bytes, got {}", slice.len()));
    }
    let num_bytes = slice[0] as usize;
    if num_bytes == 0 {
        return Err("management canister principal is not allowed".to_string());
    }
    if num_bytes > 29 {
        return Err(format!(
            "invalid number of bytes: expected a number in the range [1,29], got {num_bytes}",
        ));
    }
    if slice.len() < 1 + num_bytes {
        return Err("slice too short".to_string());
    }
    let (principal_bytes, trailing_zeroes) = slice[1..].split_at(num_bytes);
    if !trailing_zeroes
        .iter()
        .all(|trailing_zero| *trailing_zero == 0)
    {
        return Err("trailing non-zero bytes".to_string());
    }
    if principal_bytes == ANONYMOUS_PRINCIPAL_BYTES {
        return Err("anonymous principal is not allowed".to_string());
    }
    Principal::try_from_slice(principal_bytes).map_err(|err| err.to_string())
}
