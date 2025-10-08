use crate::candid_types::withdraw_native::SwapDetails;
use crate::candid_types::wrapped_icrc::WrappedIcrcToken;
use crate::eth_types::Address;
use crate::numeric::LedgerBurnIndex;
use crate::rpc_declarations::TransactionReceipt;
use crate::state::transactions::NativeWithdrawalRequest;
use crate::state::transactions::{self, Erc20WithdrawalRequest};
use crate::tx::gas_fees::TransactionPrice;
use crate::tx::SignedEip1559TransactionRequest;
use candid::{CandidType, Deserialize, Nat, Principal};
use icrc_ledger_types::icrc1::account::Account;
use minicbor::{Decode, Encode};
use serde::Serialize;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub mod chain_data;
pub mod dex_orders;
pub mod events;
pub mod swap_status;
pub mod withdraw_erc20;
pub mod withdraw_native;
pub mod wrapped_icrc;

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
pub struct GasTankBalance {
    pub native_balance: Nat,
    pub usdc_balance: Nat,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Erc20Balance {
    pub erc20_contract_address: String,
    pub balance: Nat,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct IcrcBalance {
    pub icrc_token: Principal,
    pub balance: Nat,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MinterInfo {
    pub minter_address: Option<String>,
    pub helper_smart_contract_address: Option<String>,
    pub helper_smart_contract_addresses: Option<Vec<String>>,
    pub supported_erc20_tokens: Option<Vec<Erc20Token>>,
    pub minimum_withdrawal_amount: Option<Nat>,
    pub deposit_native_fee: Option<Nat>,
    pub withdrawal_native_fee: Option<Nat>,
    pub block_height: Option<CandidBlockTag>,
    pub last_observed_block_number: Option<Nat>,
    pub native_balance: Option<Nat>,
    pub total_collected_operation_fee: Option<Nat>,
    pub last_gas_fee_estimate: Option<GasFeeEstimate>,
    pub last_native_token_usd_price_estimate: Option<NativeTokenUsdPriceEstimate>,
    pub erc20_balances: Option<Vec<Erc20Balance>>,
    pub icrc_balances: Option<Vec<IcrcBalance>>,
    pub gas_tank: Option<GasTankBalance>,
    pub last_scraped_block_number: Option<Nat>,
    pub native_twin_token_ledger_id: Option<Principal>,
    pub swap_canister_id: Option<Principal>,
    pub ledger_suite_manager_id: Option<Principal>,
    pub wrapped_icrc_tokens: Option<Vec<WrappedIcrcToken>>,
    pub is_swapping_active: bool,
    pub dex_canister_id: Option<Principal>,
    pub swap_contract_address: Option<String>,
    pub twin_usdc_info: Option<CandidTwinUsdcInfo>,
    pub canister_signing_fee_twin_usdc_value: Option<Nat>,
    pub next_swap_ledger_burn_index: Option<Nat>,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct CandidTwinUsdcInfo {
    pub address: String,
    pub ledger_id: Principal,
    pub decimals: u8,
}

#[derive(CandidType, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct GasFeeEstimate {
    pub max_fee_per_gas: Nat,
    pub max_priority_fee_per_gas: Nat,
    pub timestamp: u64,
}

#[derive(CandidType, Deserialize, Eq, Clone, Debug, PartialEq)]
pub struct NativeTokenUsdPriceEstimate {
    pub price: String,
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
                } => write!(f, "Confirmed({transaction_hash})"),
                TxFinalizedStatus::PendingReimbursement(tx) => {
                    write!(f, "PendingReimbursement({})", tx.transaction_hash)
                }
                TxFinalizedStatus::Reimbursed {
                    reimbursed_in_block,
                    transaction_hash,
                    reimbursed_amount,
                } => write!(
                    f,
                    "Failure({transaction_hash}, reimbursed: {reimbursed_amount} Wei in block: {reimbursed_in_block})"
                ),
            },
        }
    }
}

#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum DepositStatus {
    InvalidDeposit,
    Minted,
    Accepted,
    Quarantined,
    Released,
}

pub type CandidSwapTxId = String;

#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum SwapStatus {
    // from minter status
    AcceptedSwap,
    MintedToAppicDex(CandidSwapTxId),
    NotifiedAppicDex(CandidSwapTxId),

    // to minter status
    PendingSwap(SwapDetails),
    SwapTxCreated(SwapDetails),
    SwapTxSent(Transaction),
    SwapTxFinalized(TxFinalizedStatus),
    // in case the swap tx is failed and the refund is being processed
    PendingFailedSwap(SwapDetails),
    PendingRefundSwap(SwapDetails),
    RefundSwapTxCreated(SwapDetails),
    RefundSwapTxSent(Transaction),
    RefundSwapTxFinalized(TxFinalizedStatus),

    QuarantinedSwap,
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

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub struct ActivateSwapReqest {
    pub twin_usdc_ledger_id: Principal,
    pub swap_contract_address: String,
    pub twin_usdc_decimals: u8,
    pub dex_canister_id: Principal,
    pub canister_signing_fee_twin_usdc_value: Nat,
}
