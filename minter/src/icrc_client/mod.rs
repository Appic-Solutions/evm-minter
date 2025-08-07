use crate::{
    erc20::ERC20Token,
    icrc_client::runtime::IcrcBoundedRuntime,
    logs::DEBUG,
    memo::BurnMemo,
    numeric::{LedgerBurnIndex, LedgerLockIndex},
    state::State,
    FEES_SUBACCOUNT,
};
use candid::{Nat, Principal};
use ic_canister_log::log;
// use ic_canister_log::log;
use crate::erc20::ERC20TokenSymbol;
use icrc_ledger_client::ICRC1Client;
use icrc_ledger_types::{
    icrc1::{account::Account, transfer::Memo},
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use num_traits::ToPrimitive;

pub mod runtime;

pub struct LedgerClient {
    token_symbol: ERC20TokenSymbol,
    client: ICRC1Client<IcrcBoundedRuntime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ERC20Ledger {
    pub token_symbol: ERC20TokenSymbol,
    pub id: Principal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LedgerBurnError {
    TemporarilyUnavailable {
        message: String,
        ledger: ERC20Ledger,
    },
    AmountTooLow {
        minimum_burn_amount: Nat,
        failed_burn_amount: Nat,
        ledger: ERC20Ledger,
    },
    InsufficientFunds {
        balance: Nat,
        failed_burn_amount: Nat,
        ledger: ERC20Ledger,
    },
    InsufficientAllowance {
        allowance: Nat,
        failed_burn_amount: Nat,
        ledger: ERC20Ledger,
    },
}

impl LedgerClient {
    pub fn native_ledger_from_state(state: &State) -> Self {
        Self {
            token_symbol: state.native_symbol.clone(),
            client: ICRC1Client {
                runtime: IcrcBoundedRuntime,
                ledger_canister_id: state.native_ledger_id,
            },
        }
    }

    pub fn erc20_ledger(token: &ERC20Token) -> Self {
        Self {
            token_symbol: token.erc20_token_symbol.clone(),
            client: ICRC1Client {
                runtime: IcrcBoundedRuntime,
                ledger_canister_id: token.erc20_ledger_id,
            },
        }
    }

    pub async fn burn_from<A: Into<Nat>>(
        &self,
        from: Account,
        amount: A,
        memo: BurnMemo,
        fee: Option<A>,
    ) -> Result<LedgerBurnIndex, LedgerBurnError> {
        let amount = amount.into();
        match self
            .client
            .transfer_from(TransferFromArgs {
                spender_subaccount: None,
                from,
                to: ic_cdk::api::canister_self().into(),
                amount: amount.clone(),
                fee: fee.map(|fee| fee.into()),
                memo: Some(Memo::from(memo)),
                created_at_time: None, // We don't set this field to disable transaction deduplication
                                       // which is unnecessary in canister-to-canister calls.
            })
            .await
        {
            Ok(Ok(block_index)) => Ok(LedgerBurnIndex::new(
                block_index.0.to_u64().expect("nat does not fit into u64"),
            )),
            Ok(Err(transfer_from_error)) => {
                log!(
                    DEBUG,
                    "[burn]: failed to transfer_from from the {:?} ledger with error: {transfer_from_error:?}",
                    self.native_ledger()
                );
                let burn_error = match transfer_from_error {
                    TransferFromError::BadFee { expected_fee } => {
                        panic!("BUG: bad fee, expected fee: {expected_fee}")
                    }
                    TransferFromError::BadBurn { min_burn_amount } => {
                        LedgerBurnError::AmountTooLow {
                            minimum_burn_amount: min_burn_amount,
                            failed_burn_amount: amount.clone(),
                            ledger: self.native_ledger(),
                        }
                    }
                    TransferFromError::InsufficientFunds { balance } => {
                        LedgerBurnError::InsufficientFunds {
                            balance,
                            failed_burn_amount: amount.clone(),
                            ledger: self.native_ledger(),
                        }
                    }
                    TransferFromError::InsufficientAllowance { allowance } => {
                        LedgerBurnError::InsufficientAllowance {
                            allowance,
                            failed_burn_amount: amount,
                            ledger: self.native_ledger(),
                        }
                    }
                    TransferFromError::TooOld => panic!("BUG: transfer too old"),
                    TransferFromError::CreatedInFuture { ledger_time } => {
                        panic!("BUG: created in future, ledger time: {ledger_time}")
                    }
                    TransferFromError::Duplicate { duplicate_of } => {
                        panic!("BUG: duplicate transfer of: {duplicate_of}")
                    }
                    TransferFromError::TemporarilyUnavailable => {
                        LedgerBurnError::TemporarilyUnavailable {
                            message: format!(
                                "{} ledger temporarily unavailable, try again",
                                self.token_symbol
                            ),
                            ledger: self.native_ledger(),
                        }
                    }
                    TransferFromError::GenericError {
                        error_code,
                        message,
                    } => LedgerBurnError::TemporarilyUnavailable {
                        message: format!(
                        "{} ledger unreachable, error code: {error_code}, with message: {message}",
                        self.token_symbol
                    ),
                        ledger: self.native_ledger(),
                    },
                };
                Err(burn_error)
            }
            Err((error_code, message)) => {
                let err_msg = format!(
                    "failed to call {} ledger with error_code: {error_code} and message: {message}",
                    self.token_symbol
                );
                log!(DEBUG, "[burn]: {err_msg}",);
                Err(LedgerBurnError::TemporarilyUnavailable {
                    message: err_msg,
                    ledger: self.native_ledger(),
                })
            }
        }
    }

    pub async fn lock<A: Into<Nat>>(
        &self,
        from: Account,
        amount: A,
    ) -> Result<LedgerLockIndex, LedgerBurnError> {
        let amount = amount.into();
        match self
            .client
            .transfer_from(TransferFromArgs {
                spender_subaccount: None,
                from,
                to: Account {
                    owner: ic_cdk::api::canister_self(),
                    subaccount: Some(FEES_SUBACCOUNT),
                },
                amount: amount.clone(),
                fee: None,
                memo: None,
                created_at_time: None,
            })
            .await
        {
            Ok(Ok(block_index)) => Ok(LedgerLockIndex::new(
                block_index.0.to_u64().expect("nat does not fit into u64"),
            )),
            Ok(Err(transfer_from_error)) => {
                log!(
                    DEBUG,
                    "[burn]: failed to transfer_from from the {:?} ledger with error: {transfer_from_error:?}",
                    self.native_ledger()
                );
                let transfer_err = match transfer_from_error {
                    TransferFromError::BadFee { expected_fee } => {
                        panic!("BUG: bad fee, expected fee: {expected_fee}")
                    }
                    TransferFromError::BadBurn { min_burn_amount: _ } => {
                        panic!("BUG: Burn should not happen in lock")
                    }
                    TransferFromError::InsufficientFunds { balance } => {
                        LedgerBurnError::InsufficientFunds {
                            balance,
                            failed_burn_amount: amount.clone(),
                            ledger: self.native_ledger(),
                        }
                    }
                    TransferFromError::InsufficientAllowance { allowance } => {
                        LedgerBurnError::InsufficientAllowance {
                            allowance,
                            failed_burn_amount: amount,
                            ledger: self.native_ledger(),
                        }
                    }
                    TransferFromError::TooOld => panic!("BUG: transfer too old"),
                    TransferFromError::CreatedInFuture { ledger_time } => {
                        panic!("BUG: created in future, ledger time: {ledger_time}")
                    }
                    TransferFromError::Duplicate { duplicate_of } => {
                        panic!("BUG: duplicate transfer of: {duplicate_of}")
                    }
                    TransferFromError::TemporarilyUnavailable => {
                        LedgerBurnError::TemporarilyUnavailable {
                            message: format!(
                                "{} ledger temporarily unavailable, try again",
                                self.token_symbol
                            ),
                            ledger: self.native_ledger(),
                        }
                    }
                    TransferFromError::GenericError {
                        error_code,
                        message,
                    } => LedgerBurnError::TemporarilyUnavailable {
                        message: format!(
                        "{} ledger unreachable, error code: {error_code}, with message: {message}",
                        self.token_symbol
                    ),
                        ledger: self.native_ledger(),
                    },
                };
                Err(transfer_err)
            }
            Err((error_code, message)) => {
                let err_msg = format!(
                    "failed to call {} ledger with error_code: {error_code} and message: {message}",
                    self.token_symbol
                );
                log!(DEBUG, "[burn]: {err_msg}",);
                Err(LedgerBurnError::TemporarilyUnavailable {
                    message: err_msg,
                    ledger: self.native_ledger(),
                })
            }
        }
    }

    fn native_ledger(&self) -> ERC20Ledger {
        ERC20Ledger {
            token_symbol: self.token_symbol.clone(),
            id: self.client.ledger_canister_id,
        }
    }

    pub fn icrc_ledger(icrc_ledger_id: Principal) -> Self {
        Self {
            token_symbol: ERC20TokenSymbol("".to_string()),
            client: ICRC1Client {
                runtime: IcrcBoundedRuntime,
                ledger_canister_id: icrc_ledger_id,
            },
        }
    }

    pub async fn transfer_fee(&self) -> Result<Nat, String> {
        self.client.fee().await.map_err(|err| err.1)
    }
}
