use async_trait::async_trait;
use candid::{
    utils::{ArgumentDecoder, ArgumentEncoder},
    Principal,
};
use ic_cdk::call::CallErrorExt;
pub use icrc_ledger_client::{ICRC1Client, Runtime};

pub struct MinterRuntime;

#[async_trait]
impl Runtime for MinterRuntime {
    async fn call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, (i32, String)>
    where
        In: ArgumentEncoder + Send,
        Out: for<'a> ArgumentDecoder<'a>,
    {
        ic_cdk::call::Call::unbounded_wait(id, method)
            .with_args(&args)
            .await
            .map_err(|err| match err {
                ic_cdk::call::CallFailed::InsufficientLiquidCycleBalance(
                    _insufficient_liquid_cycle_balance,
                ) => (
                    0,
                    "not enough cycles to complete this operation".to_string(),
                ),
                ic_cdk::call::CallFailed::CallPerformFailed(call_perform_failed) => (
                    600,
                    format!(
                        "call performance failed, is clean reject: {}, is retryable:{}",
                        call_perform_failed.is_clean_reject(),
                        call_perform_failed.is_clean_reject()
                    ),
                ),
                ic_cdk::call::CallFailed::CallRejected(call_rejected) => (
                    call_rejected.raw_reject_code() as i32,
                    call_rejected.reject_message().to_string(),
                ),
            })?
            .candid_tuple::<Out>()
            .map_err(|err| (0, format!("Candid decoding failed {err}")))
    }
}
