pub mod runtime;
pub mod types;

use candid::Principal;

use runtime::Runtime;

use crate::dex_client::{
    runtime::MinterRuntime,
    types::{ReceivedSwapOrderEvent, SwapOrderCreationError},
};

pub struct DexClient {
    dex_canister_id: Principal,
    runtime: MinterRuntime,
}

impl DexClient {
    pub fn new(dex_canister_id: Principal) -> Self {
        Self {
            dex_canister_id,
            runtime: MinterRuntime,
        }
    }

    pub async fn minter_order(
        &self,
        args: &ReceivedSwapOrderEvent,
    ) -> Result<Result<(), SwapOrderCreationError>, (i32, String)> {
        let result: Result<(), SwapOrderCreationError> = self
            .runtime
            .call(self.dex_canister_id, "minter_order", (args,))
            .await
            .map(untuple)?;
        Ok(result)
    }
}

// extract the element from an unary tuple
fn untuple<T>(t: (T,)) -> T {
    t.0
}
