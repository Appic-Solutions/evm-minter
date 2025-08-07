use candid::{CandidType, Principal};
use ic_cdk::call::CallFailed;
use ic_management_canister_types::{
    EcdsaCurve, EcdsaKeyId, SignWithEcdsaArgs, SignWithEcdsaResult,
};
use serde::de::DeserializeOwned;
use std::fmt::{self};

/// Represents an error from a management canister call, such as
/// `sign_with_ecdsa`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    pub method: String,
    pub reason: Reason,
}

impl CallError {
    /// Returns the name of the method that resulted in this error.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the failure reason.
    pub fn reason(&self) -> &Reason {
        &self.reason
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "management call '{}' failed: {}",
            self.method, self.reason
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The reason for the management call failure.
pub enum Reason {
    /// The canister does not have enough cycles to submit the request.
    OutOfCycles,
    /// The call failed with an error.
    CanisterError(String),
    /// The management canister rejected the signature request (not enough
    /// cycles, the ECDSA subnet is overloaded, etc.).
    Rejected(String),
    /// The call failed with a transient error. Retrying may help.
    TransientInternalError(String),
    /// The call failed with a non-transient error. Retrying will not help.
    InternalError(String),

    /// Decoding Failed most probably a bug
    DecodingFailed,
}

impl fmt::Display for Reason {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfCycles => write!(fmt, "the canister is out of cycles"),
            Self::CanisterError(msg) => write!(fmt, "canister error: {}", msg),
            Self::Rejected(msg) => {
                write!(fmt, "the management canister rejected the call: {}", msg)
            }
            Reason::TransientInternalError(msg) => write!(fmt, "transient internal error: {}", msg),
            Reason::InternalError(msg) => write!(fmt, "internal error: {}", msg),
            Reason::DecodingFailed => write!(fmt, "Decoding failed most probably a bug"),
        }
    }
}

impl Reason {
    pub fn from_call_failed(err: CallFailed) -> Self {
        match err {
            CallFailed::InsufficientLiquidCycleBalance(_insufficient_liquid_cycle_balance) => {
                Self::OutOfCycles
            }
            CallFailed::CallPerformFailed(_call_perform_failed) => {
                Self::TransientInternalError("Failed to perform call".to_string())
            }
            CallFailed::CallRejected(call_rejected) => {
                let reject_message = call_rejected.reject_message().to_string();
                match call_rejected
                    .reject_code()
                    .unwrap_or(ic_cdk::call::RejectCode::SysUnknown)
                {
                    ic_cdk::call::RejectCode::SysFatal
                    | ic_cdk::call::RejectCode::DestinationInvalid
                    | ic_cdk::call::RejectCode::SysUnknown => Self::InternalError(reject_message),
                    ic_cdk::call::RejectCode::SysTransient => {
                        Self::TransientInternalError(reject_message)
                    }
                    ic_cdk::call::RejectCode::CanisterReject => Self::Rejected(reject_message),
                    ic_cdk::call::RejectCode::CanisterError => Self::CanisterError(reject_message),
                }
            }
        }
    }
}

async fn call<I, O>(method: &str, payment: u128, input: &I) -> Result<O, CallError>
where
    I: CandidType,
    O: CandidType + DeserializeOwned,
{
    let balance = ic_cdk::api::canister_cycle_balance();
    if balance < payment as u128 {
        return Err(CallError {
            method: method.to_string(),
            reason: Reason::OutOfCycles,
        });
    }

    let res = ic_cdk::call::Call::unbounded_wait(Principal::management_canister(), method)
        .with_cycles(payment)
        .with_arg(input)
        .await
        .map_err(|e| CallError {
            reason: Reason::from_call_failed(e),
            method: method.to_string(),
        })?
        .candid();

    match res {
        Ok(output) => Ok(output),
        Err(_err) => Err(CallError {
            method: method.to_string(),
            reason: Reason::DecodingFailed,
        }),
    }
}

/// Signs a message hash using the tECDSA API.
pub async fn sign_with_ecdsa(
    key_name: String,
    derivation_path: Vec<Vec<u8>>,
    message_hash: [u8; 32],
) -> Result<[u8; 64], CallError> {
    const CYCLES_PER_SIGNATURE: u128 = 27_000_000_000;

    let reply: SignWithEcdsaResult = call(
        "sign_with_ecdsa",
        CYCLES_PER_SIGNATURE,
        &SignWithEcdsaArgs {
            message_hash: message_hash.to_vec(),
            derivation_path,
            key_id: EcdsaKeyId {
                curve: EcdsaCurve::Secp256k1,
                name: key_name.clone(),
            },
        },
    )
    .await?;

    let signature_length = reply.signature.len();
    Ok(<[u8; 64]>::try_from(reply.signature).unwrap_or_else(|_| {
        panic!(
            "BUG: invalid signature from management canister. Expected 64 bytes but got {} bytes",
            signature_length
        )
    }))
}
