use super::*;

#[derive(CandidType, Deserialize)]
pub struct WrapIcrcArg {
    pub amount: Nat,
    pub icrc_ledger_id: Principal,
    pub recipient: String,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub struct RetrieveWrapIcrcRequest {
    pub native_block_index: Nat,
    pub icrc_block_index: Nat,
}

impl From<Erc20WithdrawalRequest> for RetrieveWrapIcrcRequest {
    fn from(value: Erc20WithdrawalRequest) -> Self {
        Self {
            native_block_index: candid::Nat::from(value.native_ledger_burn_index.get()),
            icrc_block_index: candid::Nat::from(value.erc20_ledger_burn_index.get()),
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Debug, Ord, PartialOrd, PartialEq, Eq)]
pub struct WrappedIcrcToken {
    pub base_token: Principal,
    pub deployed_wrapped_erc20: String,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub enum WrapIcrcError {
    TokenNotSupported {
        supported_tokens: Vec<WrappedIcrcToken>,
    },

    NativeLedgerError {
        error: LedgerError,
    },

    NativeFeeTransferError {
        error: FeeError,
    },
    IcrcLedgerError {
        native_block_index: Nat,
        error: LedgerError,
    },
    AmountTooLow,
    TemporarilyUnavailable(String),
    InvalidDestination(String),
    TransferFeeUnknow(String),
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
