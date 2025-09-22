use std::fmt;

use crate::contract_logs::swap::swap_logs::ReceivedSwapEvent;
use crate::eth_types::Address;
use crate::numeric::{BlockNumber, Erc20Value, IcrcValue, LogIndex, Wei};
use crate::rpc_declarations::Hash;
use candid::Principal;
use minicbor::{Decode, Encode};

use hex_literal::hex;

use super::{EventSource, LedgerSubaccount, ReceivedContractEvent};

pub(crate) const RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC_OLD_CONTRACT: [u8; 32] =
    hex!("deaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275");

// "TokenBurn(address,uint256,bytes32,address,bytes32)": "0x37199deebd336af9013dbddaaf9a68e337707bb4ed64cb45ed12841af85e0377",
pub(crate) const RECEIVED_DEPOSITED_AND_BURNT_TOKENS_EVENT_TOPIC_NEW_CONTRACT: [u8; 32] =
    hex!("37199deebd336af9013dbddaaf9a68e337707bb4ed64cb45ed12841af85e0377");

// Deposited native tokens on the evm side(locked) so the wrapped token on the ICP side can be minted
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

// Deposited erc20 tokens on the evm side(locked) so the wrapped token on the ICP side can be minted
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

// burnt wrapped ICP tokens on the evm side so the ICP tokens can be release(unlocked) on the icp
// side
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedBurnEvent {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[n(3)]
    pub from_address: Address,
    #[n(4)]
    pub value: IcrcValue,
    #[cbor(n(5), with = "crate::cbor::principal")]
    pub principal: Principal,
    #[n(6)]
    pub wrapped_erc20_contract_address: Address,
    #[cbor(n(7), with = "crate::cbor::principal")]
    pub icrc_token_principal: Principal,
    #[n(8)]
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

impl From<ReceivedBurnEvent> for ReceivedContractEvent {
    fn from(event: ReceivedBurnEvent) -> Self {
        ReceivedContractEvent::WrappedIcrcBurn(event)
    }
}

impl From<ReceivedWrappedIcrcDeployedEvent> for ReceivedContractEvent {
    fn from(event: ReceivedWrappedIcrcDeployedEvent) -> Self {
        ReceivedContractEvent::WrappedIcrcDeployed(event)
    }
}

impl From<ReceivedSwapEvent> for ReceivedContractEvent {
    fn from(event: ReceivedSwapEvent) -> Self {
        ReceivedContractEvent::ReceivedSwapOrder(event)
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

impl fmt::Debug for ReceivedBurnEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceivedBurnEvent")
            .field("transaction_hash", &self.transaction_hash)
            .field("block_number", &self.block_number)
            .field("log_index", &self.log_index)
            .field("from_address", &self.from_address)
            .field("value", &self.value)
            .field("principal", &format_args!("{}", self.principal))
            .field(
                "wrapped_erc20_contract_address",
                &self.wrapped_erc20_contract_address,
            )
            .field("icrc_token_principal", &self.icrc_token_principal)
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

impl ReceivedBurnEvent {
    pub fn source(&self) -> EventSource {
        EventSource {
            transaction_hash: self.transaction_hash,
            log_index: self.log_index,
        }
    }
}

//  "WrappedTokenDeployed(bytes32,address)": "0xe63ddf723173735772522be59b64b9c95be6eb8f14b87948f670ad6f8949ab2e"
pub(crate) const RECEIVED_DEPLOYED_WRAPPED_ICRC_TOKEN_EVENT_TOPIC: [u8; 32] =
    hex!("e63ddf723173735772522be59b64b9c95be6eb8f14b87948f670ad6f8949ab2e");

// Fetched ReceivedWrappedIcrcDeployedEvent events to be saved into state
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedWrappedIcrcDeployedEvent {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[cbor(n(3), with = "crate::cbor::principal")]
    pub base_token: Principal,
    #[n(4)]
    pub deployed_wrapped_erc20: Address,
}

impl fmt::Debug for ReceivedWrappedIcrcDeployedEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceivedWrappedIcrcDeployedEvent")
            .field("transaction_hash", &self.transaction_hash)
            .field("block_number", &self.block_number)
            .field("log_index", &self.log_index)
            .field("base_token", &format_args!("{}", self.base_token))
            .field("wrapped_erc20", &self.deployed_wrapped_erc20)
            .finish()
    }
}

impl ReceivedWrappedIcrcDeployedEvent {
    pub fn source(&self) -> EventSource {
        EventSource {
            transaction_hash: self.transaction_hash,
            log_index: self.log_index,
        }
    }
}
