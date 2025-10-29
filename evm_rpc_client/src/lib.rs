#[cfg(test)]
mod tests;

use async_trait::async_trait;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use evm_rpc_types::CallArgs;
use ic_canister_log::{log, Sink};
use ic_cdk::call::RejectCode;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::str::FromStr;

pub mod address;
pub mod eth_types;
pub mod evm_rpc_types;
pub mod logs;
pub mod native_http;
pub mod numeric;

pub use evm_rpc_types::{
    Block, BlockTag, ConsensusStrategy, EthMainnetService, FeeHistory, FeeHistoryArgs, GetLogsArgs,
    GetTransactionCountArgs, Hex, Hex20, Hex256, Hex32, HexByte, HttpOutcallError, JsonRpcError,
    LogEntry, MultiRpcResult, Nat256, ProviderError, RejectionCode, RpcApi, RpcConfig, RpcError,
    RpcResult, RpcService, RpcServices, SendRawTransactionStatus, TransactionReceipt,
    ValidationError,
};

use crate::native_http::candid_rpc::CandidRpcClient;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallerService {
    // send calls to evm rpc canister
    EvmRpcCanisterClient,
    // send calls using http out calls from the same cansiter
    RpcHttpOutCallClient,
}

#[derive(Clone, Debug, Copy, Eq, PartialEq)]
pub struct EvmRpcCanisterClinet {}

#[async_trait]
impl InterCanisterCall for EvmRpcCanisterClinet {
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
        let res = ic_cdk::call::Call::unbounded_wait(id, method)
            .with_cycles(cycles)
            .with_args(&args)
            .await
            .map_err(|e| match e {
                ic_cdk::call::CallFailed::InsufficientLiquidCycleBalance(
                    _insufficient_liquid_cycle_balance,
                ) => (
                    RejectionCode::CanisterError,
                    "Not enough cycles to make the call".to_string(),
                ),
                ic_cdk::call::CallFailed::CallPerformFailed(_call_perform_failed) => (
                    RejectionCode::Unknown,
                    "Failed to perfom the call, a retry should help".to_string(),
                ),
                ic_cdk::call::CallFailed::CallRejected(call_rejected) => (
                    call_rejected
                        .reject_code()
                        .unwrap_or(RejectCode::SysUnknown)
                        .into(),
                    call_rejected.reject_message().to_string(),
                ),
            })?
            .candid();

        match res {
            Ok(output) => Ok(output),
            Err(_err) => Err((RejectionCode::Unknown, "Decoding Failed".to_string())),
        }
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

    pub async fn eth_call(&self, call_args: CallArgs) -> MultiRpcResult<Hex> {
        match self.caller_service {
            CallerService::EvmRpcCanisterClient => {
                self.call_internal(
                    "eth_call",
                    self.override_rpc_config.eth_call.clone(),
                    call_args,
                )
                .await
            }
            CallerService::RpcHttpOutCallClient => {
                CandidRpcClient::new(
                    self.providers.clone(),
                    self.override_rpc_config.eth_call.clone(),
                )
                .expect("Failed to create candid client")
                .eth_call(call_args, self.min_attached_cycles)
                .await
            }
        }
    }

    pub async fn eth_get_block_by_number(&self, block: BlockTag) -> MultiRpcResult<Block> {
        match self.caller_service {
            CallerService::EvmRpcCanisterClient => {
                self.call_internal(
                    "eth_getBlockByNumber",
                    self.override_rpc_config.eth_get_block_by_number.clone(),
                    block,
                )
                .await
            }
            CallerService::RpcHttpOutCallClient => {
                CandidRpcClient::new(
                    self.providers.clone(),
                    self.override_rpc_config.eth_get_block_by_number.clone(),
                )
                .expect("Failed to create candid client")
                .eth_get_block_by_number(block, self.min_attached_cycles)
                .await
            }
        }
    }

    pub async fn eth_get_logs(&self, args: GetLogsArgs) -> MultiRpcResult<Vec<LogEntry>> {
        match self.caller_service {
            CallerService::EvmRpcCanisterClient => {
                self.call_internal(
                    "eth_getLogs",
                    self.override_rpc_config.eth_get_logs.clone(),
                    args,
                )
                .await
            }
            CallerService::RpcHttpOutCallClient => {
                CandidRpcClient::new(
                    self.providers.clone(),
                    self.override_rpc_config.eth_get_logs.clone(),
                )
                .expect("Failed to create candid client")
                .eth_get_logs(args, self.min_attached_cycles)
                .await
            }
        }
    }

    pub async fn eth_fee_history(&self, args: FeeHistoryArgs) -> MultiRpcResult<FeeHistory> {
        match self.caller_service {
            CallerService::EvmRpcCanisterClient => {
                self.call_internal(
                    "eth_feeHistory",
                    self.override_rpc_config.eth_fee_history.clone(),
                    args,
                )
                .await
            }
            CallerService::RpcHttpOutCallClient => {
                CandidRpcClient::new(
                    self.providers.clone(),
                    self.override_rpc_config.eth_fee_history.clone(),
                )
                .expect("Failed to create candid client")
                .eth_fee_history(args, self.min_attached_cycles)
                .await
            }
        }
    }

    pub async fn eth_get_transaction_receipt(
        &self,
        transaction_hash: String,
    ) -> MultiRpcResult<Option<TransactionReceipt>> {
        match self.caller_service {
            CallerService::EvmRpcCanisterClient => {
                self.call_internal(
                    "eth_getTransactionReceipt",
                    self.override_rpc_config.eth_get_transaction_receipt.clone(),
                    transaction_hash,
                )
                .await
            }
            CallerService::RpcHttpOutCallClient => {
                CandidRpcClient::new(
                    self.providers.clone(),
                    self.override_rpc_config.eth_get_transaction_receipt.clone(),
                )
                .expect("Failed to create candid client")
                .eth_get_transaction_receipt(
                    Hex32::from_str(&transaction_hash).unwrap(),
                    self.min_attached_cycles,
                )
                .await
            }
        }
    }

    pub async fn eth_get_transaction_count(
        &self,
        args: GetTransactionCountArgs,
    ) -> MultiRpcResult<Nat256> {
        match self.caller_service {
            CallerService::EvmRpcCanisterClient => {
                self.call_internal(
                    "eth_getTransactionCount",
                    self.override_rpc_config.eth_get_transaction_count.clone(),
                    args,
                )
                .await
            }
            CallerService::RpcHttpOutCallClient => {
                CandidRpcClient::new(
                    self.providers.clone(),
                    self.override_rpc_config.eth_get_transaction_count.clone(),
                )
                .expect("Failed to create candid client")
                .eth_get_transaction_count(args, self.min_attached_cycles)
                .await
            }
        }
    }

    pub async fn eth_send_raw_transaction(
        &self,
        raw_signed_tx_hex: String,
    ) -> MultiRpcResult<SendRawTransactionStatus> {
        match self.caller_service {
            CallerService::EvmRpcCanisterClient => {
                self.call_internal(
                    "eth_sendRawTransaction",
                    self.override_rpc_config.eth_send_raw_transaction.clone(),
                    raw_signed_tx_hex,
                )
                .await
            }
            CallerService::RpcHttpOutCallClient => {
                CandidRpcClient::new(
                    self.providers.clone(),
                    self.override_rpc_config.eth_send_raw_transaction.clone(),
                )
                .expect("Failed to create candid client")
                .eth_send_raw_transaction(
                    Hex::from_str(&raw_signed_tx_hex).unwrap(),
                    self.min_attached_cycles,
                )
                .await
            }
        }
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
            let client = EvmRpcCanisterClinet {};

            let result: MultiRpcResult<Out> = client
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
