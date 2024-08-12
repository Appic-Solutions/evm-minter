#[cfg(test)]
mod tests;

pub mod types;

use crate::types::candid::{
    Block, BlockTag, FeeHistory, FeeHistoryArgs, GetLogsArgs, LogEntry, MultiRpcResult,
    ProviderError, RpcConfig, RpcError, RpcServices,
};
use async_trait::async_trait;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use ic_canister_log::{log, Sink};
use ic_cdk::api::call::RejectionCode;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

#[async_trait]
pub trait InterCanisterCall {
    async fn call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        cycles: u128,
    ) -> Result<Out, (RejectionCode, String)>
    where
        In: ArgumentEncoder + Send + 'static,
        Out: CandidType + DeserializeOwned + 'static;
}

#[derive(Clone, Debug, Copy, Eq, PartialEq)]
pub struct CallerService {}

#[async_trait]
impl InterCanisterCall for CallerService {
    async fn call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        cycles: u128,
    ) -> Result<Out, (RejectionCode, String)>
    where
        In: ArgumentEncoder + Send + 'static,
        Out: CandidType + DeserializeOwned + 'static,
    {
        ic_cdk::api::call::call_with_payment128(id, method, args, cycles)
            .await
            .map(|(res,)| res)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct OverrideRpcConfig {
    pub eth_get_block_by_number: Option<RpcConfig>,
    pub eth_get_logs: Option<RpcConfig>,
    pub eth_fee_history: Option<RpcConfig>,
}

// Clinet for making intercanister calls to evm_rpc_canister
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmRpcClient<L: Sink> {
    caller_service: CallerService,
    logger: L,
    providers: RpcServices,
    evm_canister_id: Principal,
    override_rpc_config: OverrideRpcConfig,
    min_attached_cycles: u128,
    max_num_retries: u32,
}
impl<L: Sink> EvmRpcClient<L> {
    pub fn builder(caller_service: CallerService, logger: L) -> EvmRpcClientBuilder<L> {
        EvmRpcClientBuilder::new(caller_service, logger)
    }

    pub async fn eth_get_block_by_number(&self, block: BlockTag) -> MultiRpcResult<Block> {
        self.call_internal(
            "eth_getBlockByNumber",
            self.override_rpc_config.eth_get_block_by_number.clone(),
            block,
        )
        .await
    }

    pub async fn eth_get_logs(&self, args: GetLogsArgs) -> MultiRpcResult<Vec<LogEntry>> {
        self.call_internal(
            "eth_getLogs",
            self.override_rpc_config.eth_get_logs.clone(),
            args,
        )
        .await
    }

    pub async fn eth_fee_history(
        &self,
        args: FeeHistoryArgs,
    ) -> MultiRpcResult<Option<FeeHistory>> {
        self.call_internal(
            "eth_feeHistory",
            self.override_rpc_config.eth_fee_history.clone(),
            args,
        )
        .await
    }

    async fn call_internal<In, Out>(
        &self,
        method: &str,
        config: Option<RpcConfig>,
        args: In,
    ) -> MultiRpcResult<Out>
    where
        In: CandidType + Send + Clone + Debug + 'static,
        Out: CandidType + DeserializeOwned + Debug + 'static,
    {
        let mut retries = 0;
        let mut attached_cycles = self.min_attached_cycles;

        loop {
            log!(
                self.logger,
                "[{}]: Calling providers {:?} for {} with arguments '{:?}' and {} cycles (retry {})",
                self.evm_canister_id,
                self.providers,
                method,
                args,
                attached_cycles,
                retries
            );
            let result: MultiRpcResult<Out> = self
                .caller_service
                .call(
                    self.evm_canister_id,
                    method,
                    (self.providers.clone(), config.clone(), args.clone()),
                    attached_cycles,
                )
                .await
                .unwrap_or_else(|(code, msg)| {
                    MultiRpcResult::Consistent(Err(RpcError::from_rejection(code, msg)))
                });
            log!(
                self.logger,
                "[{}]: Response to {} after {} retries: {:?}",
                self.evm_canister_id,
                method,
                retries,
                result
            );
            if let Some(expected) = max_expected_too_few_cycles_error(&result) {
                if retries < self.max_num_retries {
                    retries += 1;
                    attached_cycles = attached_cycles.saturating_mul(2).max(expected);
                    continue;
                } else {
                    log!(
                        self.logger,
                        "Too few cycles error after {} retries. Needed at least: {} cycles",
                        retries,
                        expected
                    );
                }
            }
            return result;
        }
    }
}

fn max_expected_too_few_cycles_error<Out>(result: &MultiRpcResult<Out>) -> Option<u128> {
    result
        .iter()
        .filter_map(|res| match res {
            Err(RpcError::ProviderError(ProviderError::TooFewCycles {
                expected,
                received: _,
            })) => Some(*expected),
            _ => None,
        })
        .max()
}
pub struct EvmRpcClientBuilder<L: Sink> {
    caller_service: CallerService,
    logger: L,
    providers: RpcServices,
    evm_canister_id: Principal,
    override_rpc_config: OverrideRpcConfig,
    min_attached_cycles: u128,
    max_num_retries: u32,
}

impl<L: Sink> EvmRpcClientBuilder<L> {
    pub fn new(caller_service: CallerService, logger: L) -> Self {
        const DEFAULT_PROVIDERS: RpcServices = RpcServices::EthMainnet(None);
        const EVM_RPC_CANISTER_ID_FIDUCIARY: Principal =
            Principal::from_slice(&[0_u8, 0, 0, 0, 2, 48, 0, 204, 1, 1]);
        const DEFAULT_MIN_ATTACHED_CYCLES: u128 = 3_000_000_000;
        const DEFAULT_MAX_NUM_RETRIES: u32 = 10;

        debug_assert_eq!(
            EVM_RPC_CANISTER_ID_FIDUCIARY,
            Principal::from_text("7hfb6-caaaa-aaaar-qadga-cai").unwrap()
        );

        Self {
            caller_service,
            logger,
            providers: DEFAULT_PROVIDERS,
            evm_canister_id: EVM_RPC_CANISTER_ID_FIDUCIARY,
            override_rpc_config: Default::default(),
            min_attached_cycles: DEFAULT_MIN_ATTACHED_CYCLES,
            max_num_retries: DEFAULT_MAX_NUM_RETRIES,
        }
    }

    // pub fn with_runtime<OtherRuntime: Runtime>(
    //     self,
    //     runtime: OtherRuntime,
    // ) -> EvmRpcClientBuilder<OtherRuntime, L> {
    //     EvmRpcClientBuilder {
    //         runtime,
    //         logger: self.logger,
    //         providers: self.providers,
    //         evm_canister_id: self.evm_canister_id,
    //         override_rpc_config: self.override_rpc_config,
    //         min_attached_cycles: self.min_attached_cycles,
    //         max_num_retries: self.max_num_retries,
    //     }
    // }

    pub fn with_providers(mut self, providers: RpcServices) -> Self {
        self.providers = providers;
        self
    }

    pub fn with_evm_canister_id(mut self, evm_canister_id: Principal) -> Self {
        self.evm_canister_id = evm_canister_id;
        self
    }

    pub fn with_override_rpc_config(mut self, override_rpc_config: OverrideRpcConfig) -> Self {
        self.override_rpc_config = override_rpc_config;
        self
    }

    pub fn with_min_attached_cycles(mut self, min_attached_cycles: u128) -> Self {
        self.min_attached_cycles = min_attached_cycles;
        self
    }

    pub fn with_max_num_retries(mut self, max_num_retries: u32) -> Self {
        self.max_num_retries = max_num_retries;
        self
    }

    pub fn build(self) -> EvmRpcClient<L> {
        EvmRpcClient {
            caller_service: self.caller_service,
            logger: self.logger,
            providers: self.providers,
            evm_canister_id: self.evm_canister_id,
            override_rpc_config: self.override_rpc_config,
            min_attached_cycles: self.min_attached_cycles,
            max_num_retries: self.max_num_retries,
        }
    }
}