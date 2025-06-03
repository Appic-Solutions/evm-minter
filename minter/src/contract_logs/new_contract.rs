use super::{EventSource, LedgerSubaccount};
use crate::{
    eth_types::Address,
    numeric::{BlockNumber, Erc20Value, LogIndex, Wei},
    rpc_declarations::Hash,
};
use candid::Principal;
use hex_literal::hex;
use minicbor::{Decode, Encode};

// in the new contract we decided to put both deposit(for evm tokens) and burn(for wrapped icp
// tokens on evm) actions in the same function called burn, therefor the produced event is called
// TokenBurn no matter if it was a deposit into the canister's address or a burnt wrapped icp
// token.
// the same event is applicable for native_deposit, erc20_deposit, and wrapped_icp_tokens_burn
// In case of deposit the minting process should happen, and in case of burn event the
// transfer(release of locked tokens) should be triggered.

//event TokenBurn(
//      address indexed fromAddress,
//      uint256 amount,
//      bytes32 indexed icpRecipient,
//      address indexed TokenAddress,
//      bytes32 subaccount
//  );

pub(crate) const RECEIVED_BURNT_TOKEN_EVENT: [u8; 32] =
    hex!("deaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275");

// Fetched burn event to be converted into a mint transaction or a release transaction
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
    pub amount: AmountType,
    #[n(5)]
    pub from_erc20: Address,
    #[cbor(n(6), with = "crate::cbor::principal")]
    pub principal: Principal,
    #[n(7)]
    pub subaccount: Option<LedgerSubaccount>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum AmountType {
    #[n(0)]
    Native(#[n(0)] Wei),
    #[n(1)]
    Erc20(#[n(0)] Erc20Value),
}
impl AmountType {
    pub fn get_wei_amount(&self) -> Wei {
        match self {
            AmountType::Native(wei) => *wei,
            AmountType::Erc20(_) => panic!("Expected to be wei"),
        }
    }
    pub fn get_erc20_value_amount(&self) -> Erc20Value {
        match self {
            AmountType::Native(_) => todo!(),
            AmountType::Erc20(erc20_value) => *erc20_value,
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

    pub fn from_address(&self) -> Address {
        self.from_address
    }

    pub fn block_number(&self) -> BlockNumber {
        self.block_number
    }

    pub fn amount(&self) -> AmountType {
        self.amount.clone()
    }

    pub fn recipient(&self) -> Principal {
        self.recipient
    }
}

pub(crate) const RECEIVED_WRAPPED_ICP_DEPLOYED_EVENT: [u8; 32] =
    hex!("deaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275");

//event WrappedTokenDeployed(
//    bytes32 indexed baseToken,
//    address indexed wrappedERC20
//);

// Fetched wrapped icp deployed erc20 token events to be saved into state
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedWrappedIcpTokenDeployedEvent {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[cbor(n(3), with = "crate::cbor::principal")]
    pub base_token: Principal,
    #[n(4)]
    pub wrapped_erc20: Address,
}

impl ReceivedWrappedIcpTokenDeployedEvent {
    pub fn source(&self) -> EventSource {
        EventSource {
            transaction_hash: self.transaction_hash,
            log_index: self.log_index,
        }
    }

    pub fn block_number(&self) -> BlockNumber {
        self.block_number
    }

    pub fn base_token(&self) -> Principal {
        self.base_token
    }

    pub fn wrapped_erc20(&self) -> Address {
        self.wrapped_erc20
    }
}
