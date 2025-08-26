use core::fmt;
use hex_literal::hex;
use minicbor::{Decode, Encode};

use crate::{
    contract_logs::EventSource,
    eth_types::Address,
    numeric::{BlockNumber, Erc20Value, LogIndex},
    rpc_declarations::{Data, FixedSizeData, Hash},
};

pub(crate) const RECEIVED_SWAP_EVENT_TOPIC: [u8; 32] =
    hex!("c33dada04354dd803ea44b93af35ba61d4bfa477f5f06c86b6a00cfc0c261bea");

/// A swap was executed(same chain, or cross chain), and if corsschain the event should be sent to
/// the dex canister for further execution
/// event SwapExecuted(
///    address user,
///    bytes32 indexed recipient,
///    address indexed tokenIn,
///    address indexed tokenOut,
///    uint256 amountIn,
///    uint256 amountOut,
///    bool bridgeToMinter,
///    bytes encodedData
///);
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedSwapEvent {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[n(3)]
    pub from_address: Address,
    #[n(4)]
    // recipient can be either an EVM address or an ICP principal id or an BTC address
    pub recipient: FixedSizeData,
    // token in on the initial evm swap
    #[n(5)]
    pub token_in: Address,
    #[n(6)]
    pub token_out: Address,
    #[n(7)]
    // amount in on the initial swap
    pub amount_in: Erc20Value,
    #[n(8)]
    pub amount_out: Erc20Value,
    #[n(9)]
    // specifies if funds were bridged to minter to initiate a corsschain swap or not
    pub bridged_to_minter: bool,
    #[n(10)]
    // the whole encoded swap transaction flow
    pub encoded_swap_data: Data,
}

impl fmt::Debug for ReceivedSwapEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceivedErc20Event")
            .field("transaction_hash", &self.transaction_hash)
            .field("block_number", &self.block_number)
            .field("log_index", &self.log_index)
            .field("from_address", &self.from_address)
            .field("recipient", &self.recipient)
            .field("token_in", &self.token_in)
            .field("token_out", &self.token_out)
            .field("amount_in", &self.amount_in)
            .field("amount_out", &self.amount_out)
            .field("bridged_to_minter", &self.bridged_to_minter)
            .field("encoded_swap_data", &self.encoded_swap_data)
            .finish()
    }
}

impl ReceivedSwapEvent {
    pub fn source(&self) -> EventSource {
        EventSource {
            transaction_hash: self.transaction_hash,
            log_index: self.log_index,
        }
    }
}
