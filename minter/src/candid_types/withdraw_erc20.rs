use super::*;

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
