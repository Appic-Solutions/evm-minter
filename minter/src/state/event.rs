use candid::Principal;
use minicbor::{Decode, Encode};

use crate::{
    deposit_logs::{EventSource, ReceivedDepositEvent, ReceivedErc20Event, ReceivedNativeEvent},
    erc20::ERC20Token,
    eth_types::Address,
    lifecycle::{InitArg, UpgradeArg},
    numeric::{BlockNumber, LedgerBurnIndex, LedgerMintIndex},
    rpc_declarations::TransactionReceipt,
    tx::{Eip1559TransactionRequest, SignedEip1559TransactionRequest},
};

use super::transactions::{
    Erc20WithdrawalRequest, NativeWithdrawalRequest, Reimbursed, ReimbursementIndex,
    ReimbursementRequest,
};

/// The event describing the  minter state transition.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub enum EventType {
    //  The minter initialization event.
    //  Must be the first event in the log.
    #[n(0)]
    Init(#[n(0)] InitArg),
    //  The minter upgraded with the specified arguments.
    #[n(1)]
    Upgrade(#[n(0)] UpgradeArg),
    /// The minter discovered a deposit in the helper contract logs.
    #[n(2)]
    AcceptedDeposit(#[n(0)] ReceivedNativeEvent),
    /// The minter discovered an invalid deposit in the helper contract logs.
    #[n(4)]
    InvalidDeposit {
        /// The unique identifier of the deposit on the Ethereum network.
        #[n(0)]
        event_source: EventSource,
        /// The reason why minter considers the deposit invalid.
        #[n(1)]
        reason: String,
    },
    //  The minter minted NAtive in response to a deposit.
    #[n(5)]
    MintedNative {
        /// The unique identifier of the deposit on the Ethereum network.
        #[n(0)]
        event_source: EventSource,
        /// The transaction index on the Native ledger.
        #[cbor(n(1), with = "crate::cbor::id")]
        mint_block_index: LedgerMintIndex,
    },
    /// The minter processed the helper smart contract logs up to the specified height.
    #[n(6)]
    SyncedToBlock {
        /// The last processed block number for ETH helper contract (inclusive).
        #[n(0)]
        block_number: BlockNumber,
    },
    /// The minter accepted a new ETH withdrawal request.
    #[n(7)]
    AcceptedNativeWithdrawalRequest(#[n(0)] NativeWithdrawalRequest),
    // /// The minter created a new transaction to handle a withdrawal request.
    #[n(8)]
    CreatedTransaction {
        #[cbor(n(0), with = "crate::cbor::id")]
        withdrawal_id: LedgerBurnIndex,
        #[n(1)]
        transaction: Eip1559TransactionRequest,
    },
    /// The minter signed a transaction.
    #[n(9)]
    SignedTransaction {
        /// The withdrawal identifier.
        #[cbor(n(0), with = "crate::cbor::id")]
        withdrawal_id: LedgerBurnIndex,
        /// The signed transaction.
        #[n(1)]
        transaction: SignedEip1559TransactionRequest,
    },
    /// The minter created a new transaction to handle an existing withdrawal request.
    #[n(10)]
    ReplacedTransaction {
        /// The withdrawal identifier.
        #[cbor(n(0), with = "crate::cbor::id")]
        withdrawal_id: LedgerBurnIndex,
        /// The replacement transaction.
        #[n(1)]
        transaction: Eip1559TransactionRequest,
    },
    /// The minter observed the transaction being included in a finalized Ethereum block.
    #[n(11)]
    FinalizedTransaction {
        /// The withdrawal identifier.
        #[cbor(n(0), with = "crate::cbor::id")]
        withdrawal_id: LedgerBurnIndex,
        /// The receipt for the finalized transaction.
        #[n(1)]
        transaction_receipt: TransactionReceipt,
    },
    /// The minter successfully reimbursed a failed withdrawal
    /// or the transaction fee associated with a ckERC20 withdrawal.
    #[n(12)]
    ReimbursedNativeWithdrawal(#[n(0)] Reimbursed),
    /// Add a new ERC20 token.
    #[n(14)]
    AddedErc20Token(#[n(0)] ERC20Token),
    /// The minter discovered a erc20 deposit in the helper contract logs.
    #[n(15)]
    AcceptedErc20Deposit(#[n(0)] ReceivedErc20Event),
    // /// The minter accepted a new ERC-20 withdrawal request.
    #[n(16)]
    AcceptedErc20WithdrawalRequest(#[n(0)] Erc20WithdrawalRequest),
    #[n(17)]
    MintedErc20 {
        /// The unique identifier of the deposit on the Ethereum network.
        #[n(0)]
        event_source: EventSource,
        /// The transaction index on the native ledger.
        #[cbor(n(1), with = "crate::cbor::id")]
        mint_block_index: LedgerMintIndex,
        #[n(2)]
        erc20_token_symbol: String,
        #[n(3)]
        erc20_contract_address: Address,
    },

    #[n(19)]
    ReimbursedErc20Withdrawal {
        #[cbor(n(0), with = "crate::cbor::id")]
        native_ledger_burn_index: LedgerBurnIndex,
        #[cbor(n(1), with = "crate::cbor::principal")]
        erc20_ledger_id: Principal,
        #[n(2)]
        reimbursed: Reimbursed,
    },
    /// The minter could not burn the given amount of ckERC20 tokens.
    #[n(20)]
    FailedErc20WithdrawalRequest(#[n(0)] ReimbursementRequest),
    /// The minter unexpectedly panic while processing a deposit.
    // /// The deposit is quarantined to prevent any double minting and
    // /// will not be processed without further manual intervention.
    #[n(21)]
    QuarantinedDeposit {
        /// The unique identifier of the deposit on the Ethereum network.
        #[n(0)]
        event_source: EventSource,
    },
    /// The minter unexpectedly panic while processing a reimbursement.
    /// The reimbursement is quarantined to prevent any double minting and
    /// will not be processed without further manual intervention.
    #[n(22)]
    QuarantinedReimbursement {
        /// The unique identifier of the reimbursement.
        #[n(0)]
        index: ReimbursementIndex,
    },
    // /// Skipped block for a specific helper contract.
    #[n(23)]
    SkippedBlock {
        #[n(0)]
        block_number: BlockNumber,
    },
}

impl ReceivedDepositEvent {
    pub fn into_deposit(self) -> EventType {
        match self {
            ReceivedDepositEvent::Native(event) => EventType::AcceptedDeposit(event),
            ReceivedDepositEvent::Erc20(event) => EventType::AcceptedErc20Deposit(event),
        }
    }
}

#[derive(Encode, Decode, Debug, PartialEq, Eq)]
pub struct Event {
    /// The canister time at which the minter generated this event.
    #[n(0)]
    pub timestamp: u64,
    /// The event type.
    #[n(1)]
    pub payload: EventType,
}
