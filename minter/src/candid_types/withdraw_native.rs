use super::*;

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
