use std::fmt;

use crate::eth_types::Address;
use crate::numeric::{BlockNumber, Erc20Value, LogIndex, Wei};
use crate::rpc_declarations::Hash;
use candid::Principal;
use minicbor::{Decode, Encode};

use hex_literal::hex;

use super::{EventSource, EventSourceError, LedgerSubaccount, ReceivedContractEvent};

// old deposit contract

pub(crate) const RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC: [u8; 32] =
    hex!("deaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275");

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedNativeEvent {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[n(3)]
    pub from_address: Address,
    #[n(4)]
    pub value: Wei,
    #[cbor(n(5), with = "crate::cbor::principal")]
    pub principal: Principal,
    #[n(6)]
    pub subaccount: Option<LedgerSubaccount>,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedErc20Event {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[n(3)]
    pub from_address: Address,
    #[n(4)]
    pub value: Erc20Value,
    #[cbor(n(5), with = "crate::cbor::principal")]
    pub principal: Principal,
    #[n(6)]
    pub erc20_contract_address: Address,
    #[n(7)]
    pub subaccount: Option<LedgerSubaccount>,
}

impl From<ReceivedNativeEvent> for ReceivedContractEvent {
    fn from(event: ReceivedNativeEvent) -> Self {
        ReceivedContractEvent::NativeDeposit(event)
    }
}

impl From<ReceivedErc20Event> for ReceivedContractEvent {
    fn from(event: ReceivedErc20Event) -> Self {
        ReceivedContractEvent::Erc20Deposit(event)
    }
}

impl fmt::Debug for ReceivedNativeEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceivedNativeEvent")
            .field("transaction_hash", &self.transaction_hash)
            .field("block_number", &self.block_number)
            .field("log_index", &self.log_index)
            .field("from_address", &self.from_address)
            .field("value", &self.value)
            .field("principal", &format_args!("{}", self.principal))
            .field("subaccount", &self.subaccount)
            .finish()
    }
}

impl fmt::Debug for ReceivedErc20Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceivedErc20Event")
            .field("transaction_hash", &self.transaction_hash)
            .field("block_number", &self.block_number)
            .field("log_index", &self.log_index)
            .field("from_address", &self.from_address)
            .field("value", &self.value)
            .field("principal", &format_args!("{}", self.principal))
            .field("contract_address", &self.erc20_contract_address)
            .field("subaccount", &self.subaccount)
            .finish()
    }
}

impl ReceivedNativeEvent {
    pub fn source(&self) -> EventSource {
        EventSource {
            transaction_hash: self.transaction_hash,
            log_index: self.log_index,
        }
    }
}

impl ReceivedErc20Event {
    pub fn source(&self) -> EventSource {
        EventSource {
            transaction_hash: self.transaction_hash,
            log_index: self.log_index,
        }
    }
}
