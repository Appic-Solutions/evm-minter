#[cfg(test)]
mod tests;

use async_trait::async_trait;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use evm_rpc_types::CallArgs;
use ic_canister_log::{log, Sink};
use ic_cdk::api::call::RejectionCode;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

pub use evm_rpc_types::{
    Block, BlockTag, ConsensusStrategy, EthMainnetService, FeeHistory, FeeHistoryArgs, GetLogsArgs,
    GetTransactionCountArgs, Hex, Hex20, Hex256, Hex32, HexByte, HttpOutcallError, JsonRpcError,
    LogEntry, MultiRpcResult, Nat256, ProviderError, RpcApi, RpcConfig, RpcError, RpcResult,
    RpcService, RpcServices, SendRawTransactionStatus, TransactionReceipt, ValidationError,
};

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
    pub eth_get_transaction_receipt: Option<RpcConfig>,
    pub eth_get_transaction_count: Option<RpcConfig>,
    pub eth_send_raw_transaction: Option<RpcConfig>,
    pub eth_call: Option<RpcConfig>,
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

    pub async fn eth_call(&self, call_args: CallArgs) -> MultiRpcResult<String> {
        self.call_internal(
            "eth_call",
            self.override_rpc_config.eth_call.clone(),
            call_args,
        )
        .await
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

    pub async fn eth_get_transaction_receipt(
        &self,
        transaction_hash: String,
    ) -> MultiRpcResult<Option<TransactionReceipt>> {
        self.call_internal(
            "eth_getTransactionReceipt",
            self.override_rpc_config.eth_get_transaction_receipt.clone(),
            transaction_hash,
        )
        .await
    }

    pub async fn eth_get_transaction_count(
        &self,
        args: GetTransactionCountArgs,
    ) -> MultiRpcResult<Nat256> {
        self.call_internal(
            "eth_getTransactionCount",
            self.override_rpc_config.eth_get_transaction_count.clone(),
            args,
        )
        .await
    }

    pub async fn eth_send_raw_transaction(
        &self,
        raw_signed_tx_hex: String,
    ) -> MultiRpcResult<SendRawTransactionStatus> {
        self.call_internal(
            "eth_sendRawTransaction",
            self.override_rpc_config.eth_send_raw_transaction.clone(),
            raw_signed_tx_hex,
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
                .unwrap_or_else(|(code, message)| {
                    MultiRpcResult::Consistent(Err(RpcError::HttpOutcallError(
                        HttpOutcallError::IcError { code, message },
                    )))
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
    multi_rpc_result_iter(result)
        .filter_map(|res| match res {
            Err(RpcError::ProviderError(ProviderError::TooFewCycles {
                expected,
                received: _,
            })) => Some(*expected),
            _ => None,
        })
        .max()
}

fn multi_rpc_result_iter<Out>(
    result: &MultiRpcResult<Out>,
) -> Box<dyn Iterator<Item = &RpcResult<Out>> + '_> {
    match result {
        MultiRpcResult::Consistent(result) => Box::new(std::iter::once(result)),
        MultiRpcResult::Inconsistent(results) => {
            Box::new(results.iter().map(|(_service, result)| result))
        }
    }
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
        const DEFAULT_MIN_ATTACHED_CYCLES: u128 = 3_000_000_000;
        const DEFAULT_MAX_NUM_RETRIES: u32 = 10;

        Self {
            caller_service,
            logger,
            providers: DEFAULT_PROVIDERS,
            evm_canister_id: Principal::from_text("sosge-5iaaa-aaaag-alcla-cai").unwrap(),
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
