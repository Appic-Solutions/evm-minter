use crate::eth_types::Address;
use crate::ledger_client::{FeeTransferError, LedgerBurnError};
// // use crate::rpc_client::responses::TransactionReceipt;
// // use crate::ledger_client::LedgerBurnError;
use crate::numeric::LedgerBurnIndex;
use crate::rpc_declarations::TransactionReceipt;
use crate::state::transactions::NativeWithdrawalRequest;
use crate::state::transactions::{self, Erc20WithdrawalRequest};
use crate::tx::{SignedEip1559TransactionRequest, TransactionPrice};
use candid::{CandidType, Deserialize, Nat, Principal};
use icrc_ledger_types::icrc1::account::Account;
use minicbor::{Decode, Encode};
use serde::Serialize;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
// For wallet connection
#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Icrc28TrustedOriginsResponse {
    pub trusted_origins: Vec<String>,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Eip1559TransactionPriceArg {
    pub erc20_ledger_id: Principal,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Eip1559TransactionPrice {
    pub gas_limit: Nat,
    pub max_fee_per_gas: Nat,
    pub max_priority_fee_per_gas: Nat,
    pub max_transaction_fee: Nat,
    pub timestamp: Option<u64>,
}

impl From<TransactionPrice> for Eip1559TransactionPrice {
    fn from(value: TransactionPrice) -> Self {
        Self {
            gas_limit: value.gas_limit.into(),
            max_fee_per_gas: value.max_fee_per_gas.into(),
            max_priority_fee_per_gas: value.max_priority_fee_per_gas.into(),
            max_transaction_fee: value.max_transaction_fee().into(),
            timestamp: None,
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Erc20Token {
    pub erc20_token_symbol: String,
    pub erc20_contract_address: String,
    pub ledger_canister_id: Principal,
}

impl From<crate::erc20::ERC20Token> for Erc20Token {
    fn from(value: crate::erc20::ERC20Token) -> Self {
        Self {
            erc20_token_symbol: value.erc20_token_symbol.to_string(),
            erc20_contract_address: value.erc20_contract_address.to_string(),
            ledger_canister_id: value.erc20_ledger_id,
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Erc20Balance {
    pub erc20_contract_address: String,
    pub balance: Nat,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MinterInfo {
    pub minter_address: Option<String>,
    pub helper_smart_contract_address: Option<String>,
    pub supported_erc20_tokens: Option<Vec<Erc20Token>>,
    pub minimum_withdrawal_amount: Option<Nat>,
    pub deposit_native_fee: Option<Nat>,
    pub withdrawal_native_fee: Option<Nat>,
    pub block_height: Option<CandidBlockTag>,
    pub last_observed_block_number: Option<Nat>,
    pub native_balance: Option<Nat>,
    pub last_gas_fee_estimate: Option<GasFeeEstimate>,
    pub erc20_balances: Option<Vec<Erc20Balance>>,
    pub last_scraped_block_number: Option<Nat>,
    pub native_twin_token_ledger_id: Option<Principal>,
    pub swap_canister_id: Option<Principal>,
    pub ledger_suite_manager_id: Option<Principal>,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct GasFeeEstimate {
    pub max_fee_per_gas: Nat,
    pub max_priority_fee_per_gas: Nat,
    pub timestamp: u64,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Transaction {
    pub transaction_hash: String,
}

impl From<&SignedEip1559TransactionRequest> for Transaction {
    fn from(value: &SignedEip1559TransactionRequest) -> Self {
        Self {
            transaction_hash: value.hash().to_string(),
        }
    }
}

impl From<&TransactionReceipt> for Transaction {
    fn from(receipt: &TransactionReceipt) -> Self {
        Self {
            transaction_hash: receipt.transaction_hash.to_string(),
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub struct RetrieveNativeRequest {
    pub block_index: Nat,
}

#[derive(
    CandidType, Debug, Default, Serialize, Deserialize, Clone, Encode, Decode, PartialEq, Eq,
)]
#[cbor(index_only)]
pub enum CandidBlockTag {
    /// The latest mined block.
    #[default]
    #[cbor(n(0))]
    Latest,
    /// The latest safe head block.
    /// See
    /// <https://www.alchemy.com/overviews/ethereum-commitment-levels#what-are-ethereum-commitment-levels>
    #[cbor(n(1))]
    Safe,
    /// The latest finalized block.
    /// See
    /// <https://www.alchemy.com/overviews/ethereum-commitment-levels#what-are-ethereum-commitment-levels>
    #[cbor(n(2))]
    Finalized,
}

impl From<NativeWithdrawalRequest> for RetrieveNativeRequest {
    fn from(value: NativeWithdrawalRequest) -> Self {
        Self {
            block_index: Nat::from(value.ledger_burn_index.get()),
        }
    }
}

#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum RetrieveWithdrawalStatus {
    NotFound,
    Pending,
    TxCreated,
    TxSent(Transaction),
    TxFinalized(TxFinalizedStatus),
}

#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum TxFinalizedStatus {
    Success {
        transaction_hash: String,
        effective_transaction_fee: Option<Nat>,
    },
    PendingReimbursement(Transaction),
    Reimbursed {
        transaction_hash: String,
        reimbursed_amount: Nat,
        reimbursed_in_block: Nat,
    },
}

impl Display for RetrieveWithdrawalStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RetrieveWithdrawalStatus::NotFound => write!(f, "Not Found"),
            RetrieveWithdrawalStatus::Pending => write!(f, "Pending"),
            RetrieveWithdrawalStatus::TxCreated => write!(f, "Created"),
            RetrieveWithdrawalStatus::TxSent(tx) => write!(f, "Sent({})", tx.transaction_hash),
            RetrieveWithdrawalStatus::TxFinalized(tx_status) => match tx_status {
                TxFinalizedStatus::Success {
                    transaction_hash, ..
                } => write!(f, "Confirmed({})", transaction_hash),
                TxFinalizedStatus::PendingReimbursement(tx) => {
                    write!(f, "PendingReimbursement({})", tx.transaction_hash)
                }
                TxFinalizedStatus::Reimbursed {
                    reimbursed_in_block,
                    transaction_hash,
                    reimbursed_amount,
                } => write!(
                    f,
                    "Failure({}, reimbursed: {} Wei in block: {})",
                    transaction_hash, reimbursed_amount, reimbursed_in_block
                ),
            },
        }
    }
}

#[derive(CandidType, Deserialize)]
pub struct WithdrawalArg {
    pub amount: Nat,
    pub recipient: String,
}

#[derive(CandidType, Deserialize, Debug, PartialEq)]
pub enum WithdrawalError {
    AmountTooLow { min_withdrawal_amount: Nat },
    InsufficientFunds { balance: Nat },
    InsufficientAllowance { allowance: Nat },
    TemporarilyUnavailable(String),
    InvalidDestination(String),
}

impl From<LedgerBurnError> for WithdrawalError {
    fn from(error: LedgerBurnError) -> Self {
        match error {
            LedgerBurnError::TemporarilyUnavailable { message, .. } => {
                Self::TemporarilyUnavailable(message)
            }
            LedgerBurnError::InsufficientFunds { balance, .. } => {
                Self::InsufficientFunds { balance }
            }
            LedgerBurnError::InsufficientAllowance { allowance, .. } => {
                Self::InsufficientAllowance { allowance }
            }
            LedgerBurnError::AmountTooLow {
                minimum_burn_amount,
                failed_burn_amount,
                ledger,
            } => {
                panic!("BUG: withdrawal amount {failed_burn_amount} on the Native ledger {ledger:?} should always be higher than the ledger transaction fee {minimum_burn_amount}")
            }
        }
    }
}

impl From<FeeTransferError> for WithdrawalError {
    fn from(error: FeeTransferError) -> Self {
        match error {
            FeeTransferError::TemporarilyUnavailable { message, .. } => {
                Self::TemporarilyUnavailable(message)
            }
            FeeTransferError::InsufficientFunds { balance, .. } => {
                Self::InsufficientFunds { balance }
            }
            FeeTransferError::InsufficientAllowance { allowance, .. } => {
                Self::InsufficientAllowance { allowance }
            }
            FeeTransferError::AmountTooLow {
                minimum_transfer_amount,
                failed_transfer_amount,
                ledger,
            } => {
                panic!("BUG: withdrawal amount {failed_transfer_amount} on the Native ledger {ledger:?} should always be higher than the ledger transaction fee {minimum_transfer_amount}")
            }
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Eq, PartialEq, Debug)]
pub enum WithdrawalSearchParameter {
    ByWithdrawalId(u64),
    ByRecipient(String),
    BySenderAccount(Account),
}

impl TryFrom<WithdrawalSearchParameter> for transactions::WithdrawalSearchParameter {
    type Error = String;

    fn try_from(parameter: WithdrawalSearchParameter) -> Result<Self, String> {
        use WithdrawalSearchParameter::*;
        match parameter {
            ByWithdrawalId(index) => Ok(Self::ByWithdrawalId(LedgerBurnIndex::new(index))),
            ByRecipient(address) => Ok(Self::ByRecipient(Address::from_str(&address)?)),
            BySenderAccount(account) => Ok(Self::BySenderAccount(account)),
        }
    }
}

#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct WithdrawalDetail {
    pub withdrawal_id: u64,
    pub recipient_address: String,
    pub from: Principal,
    pub from_subaccount: Option<[u8; 32]>,
    pub token_symbol: String,
    pub withdrawal_amount: Nat,
    pub max_transaction_fee: Option<Nat>,
    pub status: WithdrawalStatus,
}

#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum WithdrawalStatus {
    Pending,
    TxCreated,
    TxSent(Transaction),
    TxFinalized(TxFinalizedStatus),
}

#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum DepositStatus {
    InvalidDeposit,
    Minted,
    Accepted,
    Quarantined,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub struct AddErc20Token {
    pub chain_id: Nat,
    pub address: String,
    pub erc20_token_symbol: String,
    pub erc20_ledger_id: Principal,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub enum RequestScrapingError {
    CalledTooManyTimes,
    InvalidBlockNumber,
    BlockAlreadyObserved,
}

pub mod events {
    use crate::lifecycle::InitArg;
    use crate::lifecycle::UpgradeArg;
    use candid::{CandidType, Deserialize, Nat, Principal};
    use serde_bytes::ByteBuf;

    #[derive(CandidType, Deserialize, Debug, Clone)]
    pub struct GetEventsArg {
        pub start: u64,
        pub length: u64,
    }

    #[derive(CandidType, Deserialize, Debug, Clone)]
    pub struct GetEventsResult {
        pub events: Vec<Event>,
        pub total_event_count: u64,
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct Event {
        pub timestamp: u64,
        pub payload: EventPayload,
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct EventSource {
        pub transaction_hash: String,
        pub log_index: Nat,
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub enum ReimbursementIndex {
        Native {
            ledger_burn_index: Nat,
        },
        Erc20 {
            native_ledger_burn_index: Nat,
            ledger_id: Principal,
            erc20_ledger_burn_index: Nat,
        },
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct AccessListItem {
        pub address: String,
        pub storage_keys: Vec<ByteBuf>,
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct UnsignedTransaction {
        pub chain_id: Nat,
        pub nonce: Nat,
        pub max_priority_fee_per_gas: Nat,
        pub max_fee_per_gas: Nat,
        pub gas_limit: Nat,
        pub destination: String,
        pub value: Nat,
        pub data: ByteBuf,
        pub access_list: Vec<AccessListItem>,
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub enum TransactionStatus {
        Success,
        Failure,
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub struct TransactionReceipt {
        pub block_hash: String,
        pub block_number: Nat,
        pub effective_gas_price: Nat,
        pub gas_used: Nat,
        pub status: TransactionStatus,
        pub transaction_hash: String,
    }

    #[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
    pub enum EventPayload {
        Init(InitArg),
        Upgrade(UpgradeArg),
        AcceptedDeposit {
            transaction_hash: String,
            block_number: Nat,
            log_index: Nat,
            from_address: String,
            value: Nat,
            principal: Principal,
            subaccount: Option<[u8; 32]>,
        },
        AcceptedErc20Deposit {
            transaction_hash: String,
            block_number: Nat,
            log_index: Nat,
            from_address: String,
            value: Nat,
            principal: Principal,
            erc20_contract_address: String,
            subaccount: Option<[u8; 32]>,
        },
        InvalidDeposit {
            event_source: EventSource,
            reason: String,
        },
        MintedNative {
            event_source: EventSource,
            mint_block_index: Nat,
        },
        SyncedToBlock {
            block_number: Nat,
        },

        AcceptedNativeWithdrawalRequest {
            withdrawal_amount: Nat,
            destination: String,
            ledger_burn_index: Nat,
            from: Principal,
            from_subaccount: Option<[u8; 32]>,
            created_at: Option<u64>,
        },
        CreatedTransaction {
            withdrawal_id: Nat,
            transaction: UnsignedTransaction,
        },
        SignedTransaction {
            withdrawal_id: Nat,
            raw_transaction: String,
        },
        ReplacedTransaction {
            withdrawal_id: Nat,
            transaction: UnsignedTransaction,
        },
        FinalizedTransaction {
            withdrawal_id: Nat,
            transaction_receipt: TransactionReceipt,
        },
        ReimbursedNativeWithdrawal {
            reimbursed_in_block: Nat,
            withdrawal_id: Nat,
            reimbursed_amount: Nat,
            transaction_hash: Option<String>,
        },
        ReimbursedErc20Withdrawal {
            withdrawal_id: Nat,
            burn_in_block: Nat,
            reimbursed_in_block: Nat,
            ledger_id: Principal,
            reimbursed_amount: Nat,
            transaction_hash: Option<String>,
        },
        SkippedBlock {
            block_number: Nat,
        },
        AddedErc20Token {
            chain_id: Nat,
            address: String,
            erc20_token_symbol: String,
            erc20_ledger_id: Principal,
        },
        AcceptedErc20WithdrawalRequest {
            max_transaction_fee: Nat,
            withdrawal_amount: Nat,
            erc20_contract_address: String,
            destination: String,
            native_ledger_burn_index: Nat,
            erc20_ledger_id: Principal,
            erc20_ledger_burn_index: Nat,
            from: Principal,
            from_subaccount: Option<[u8; 32]>,
            created_at: u64,
        },
        FailedErc20WithdrawalRequest {
            withdrawal_id: Nat,
            reimbursed_amount: Nat,
            to: Principal,
            to_subaccount: Option<[u8; 32]>,
        },
        MintedErc20 {
            event_source: EventSource,
            mint_block_index: Nat,
            erc20_token_symbol: String,
            erc20_contract_address: String,
        },
        QuarantinedDeposit {
            event_source: EventSource,
        },
        QuarantinedReimbursement {
            index: ReimbursementIndex,
        },
    }
}

#[derive(CandidType, Deserialize)]
pub struct WithdrawErc20Arg {
    pub amount: Nat,
    pub erc20_ledger_id: Principal,
    pub recipient: String,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub struct RetrieveErc20Request {
    pub native_block_index: Nat,
    pub erc20_block_index: Nat,
}

impl From<Erc20WithdrawalRequest> for RetrieveErc20Request {
    fn from(value: Erc20WithdrawalRequest) -> Self {
        Self {
            native_block_index: candid::Nat::from(value.native_ledger_burn_index.get()),
            erc20_block_index: candid::Nat::from(value.erc20_ledger_burn_index.get()),
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub enum WithdrawErc20Error {
    TokenNotSupported {
        supported_tokens: Vec<Erc20Token>,
    },

    NativeLedgerError {
        error: LedgerError,
    },

    NativeFeeTransferError {
        error: FeeError,
    },
    Erc20LedgerError {
        native_block_index: Nat,
        error: LedgerError,
    },
    TemporarilyUnavailable(String),
    InvalidDestination(String),
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub enum LedgerError {
    InsufficientFunds {
        balance: Nat,
        failed_burn_amount: Nat,
        token_symbol: String,
        ledger_id: Principal,
    },
    AmountTooLow {
        minimum_burn_amount: Nat,
        failed_burn_amount: Nat,
        token_symbol: String,
        ledger_id: Principal,
    },
    InsufficientAllowance {
        allowance: Nat,
        failed_burn_amount: Nat,
        token_symbol: String,
        ledger_id: Principal,
    },
    TemporarilyUnavailable(String),
}

impl From<LedgerBurnError> for LedgerError {
    fn from(error: LedgerBurnError) -> Self {
        match error {
            LedgerBurnError::TemporarilyUnavailable { message, .. } => {
                LedgerError::TemporarilyUnavailable(message)
            }
            LedgerBurnError::InsufficientFunds {
                balance,
                failed_burn_amount,
                ledger,
            } => LedgerError::InsufficientFunds {
                balance,
                failed_burn_amount,
                token_symbol: ledger.token_symbol.to_string(),
                ledger_id: ledger.id,
            },
            LedgerBurnError::InsufficientAllowance {
                allowance,
                failed_burn_amount,
                ledger,
            } => LedgerError::InsufficientAllowance {
                allowance,
                failed_burn_amount,
                token_symbol: ledger.token_symbol.to_string(),
                ledger_id: ledger.id,
            },
            LedgerBurnError::AmountTooLow {
                minimum_burn_amount,
                failed_burn_amount,
                ledger,
            } => LedgerError::AmountTooLow {
                minimum_burn_amount,
                failed_burn_amount,
                token_symbol: ledger.token_symbol.to_string(),
                ledger_id: ledger.id,
            },
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub enum FeeError {
    InsufficientFunds {
        balance: Nat,
        failed_transfer_amount: Nat,
        token_symbol: String,
        ledger_id: Principal,
    },
    AmountTooLow {
        minimum_transfer_amount: Nat,
        failed_transfer_amount: Nat,
        token_symbol: String,
        ledger_id: Principal,
    },
    InsufficientAllowance {
        allowance: Nat,
        failed_transfer_amount: Nat,
        token_symbol: String,
        ledger_id: Principal,
    },
    TemporarilyUnavailable(String),
}

impl From<FeeTransferError> for FeeError {
    fn from(error: FeeTransferError) -> Self {
        match error {
            FeeTransferError::TemporarilyUnavailable { message, .. } => {
                FeeError::TemporarilyUnavailable(message)
            }
            FeeTransferError::InsufficientFunds {
                balance,
                failed_transfer_amount,
                ledger,
            } => FeeError::InsufficientFunds {
                balance,
                failed_transfer_amount,
                token_symbol: ledger.token_symbol.to_string(),
                ledger_id: ledger.id,
            },
            FeeTransferError::InsufficientAllowance {
                allowance,
                failed_transfer_amount,
                ledger,
            } => FeeError::InsufficientAllowance {
                allowance,
                failed_transfer_amount,
                token_symbol: ledger.token_symbol.to_string(),
                ledger_id: ledger.id,
            },
            FeeTransferError::AmountTooLow {
                minimum_transfer_amount,
                failed_transfer_amount,
                ledger,
            } => FeeError::AmountTooLow {
                minimum_transfer_amount,
                failed_transfer_amount,
                token_symbol: ledger.token_symbol.to_string(),
                ledger_id: ledger.id,
            },
        }
    }
}
