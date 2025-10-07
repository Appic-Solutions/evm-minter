#[cfg(test)]
mod tests;

pub mod providers;

use providers::{get_one_provider, get_providers, Provider};
use std::{collections::BTreeMap, convert::Infallible, fmt::Display};

use crate::{
    eth_types::Address,
    evm_config::EvmNetwork,
    logs::{PrintProxySink, INFO, TRACE_HTTP},
    numeric::{BlockNumber, GasAmount, LogIndex, TransactionCount, Wei, WeiPerGas},
    rpc_client::providers::get_custom_providers,
    rpc_declarations::{
        AccessList, Block, BlockSpec, BlockTag, CallParams, Data, FeeHistory, FeeHistoryParams,
        FixedSizeData, GetLogsParam, Hash, LogEntry, Quantity, SendRawTransactionResult, Topic,
        TransactionReceipt, TransactionStatus,
    },
    state::State,
};
use candid::Nat;
use evm_rpc_client::{
    evm_rpc_types::{
        self, AccessList as EvmAccessList, AccessListEntry as EvmAccessListEntry,
        Block as EvmBlock, BlockTag as EvmBlockTag, CallArgs, FeeHistory as EvmFeeHistory,
        FeeHistoryArgs as EvmFeeHistoryArgs, GetLogsArgs as EvmGetLogsArgs,
        GetTransactionCountArgs as EvmGetTransactionCountArgs, Hex, Hex20, Hex32, HexByte,
        HttpOutcallError, LogEntry as EvmLogEntry, MultiRpcResult as EvmMultiRpcResult, Nat256,
        RpcConfig as EvmRpcConfig, RpcError as EvmRpcError, RpcService as EvmRpcService,
        SendRawTransactionStatus as EvmSendRawTransactionStatus,
        TransactionReceipt as EvmTransactionReceipt, TransactionRequest,
    },
    RejectionCode,
};
use evm_rpc_client::{CallerService, EvmRpcClient, OverrideRpcConfig};
use ic_canister_log::log;
use num_traits::ToPrimitive;

// We expect most of the calls to contain zero events.
const ETH_GET_LOGS_INITIAL_RESPONSE_SIZE_ESTIMATE: u64 = 20_000;

// This constant is our approximation of the expected header size.
// The HTTP standard doesn't define any limit, and many implementations limit
// the headers size to 8 KiB. We chose a lower limit because headers observed on most providers
// fit in the constant defined below, and if there is spike, then the payload size adjustment
// should take care of that.
pub const HEADER_SIZE_LIMIT: u64 = 2 * 1024;

#[derive(Debug)]
pub struct RpcClient {
    evm_rpc_client: Option<EvmRpcClient<PrintProxySink>>,
    chain: EvmNetwork,
}
impl RpcClient {
    pub fn from_state_all_providers(state: &State) -> Self {
        let mut client = Self {
            evm_rpc_client: None,
            chain: state.evm_network,
        };
        const MIN_ATTACHED_CYCLES: u128 = 30_000_000_000;

        let providers = get_providers(client.chain);

        client.evm_rpc_client = Some(
            EvmRpcClient::builder(CallerService {}, TRACE_HTTP)
                .with_providers(providers)
                .with_evm_canister_id(state.evm_canister_id)
                .with_min_attached_cycles(MIN_ATTACHED_CYCLES)
                .with_override_rpc_config(OverrideRpcConfig {
                    eth_get_logs: Some(EvmRpcConfig {
                        response_size_estimate: Some(
                            ETH_GET_LOGS_INITIAL_RESPONSE_SIZE_ESTIMATE + HEADER_SIZE_LIMIT,
                        ),
                        response_consensus: Some(evm_rpc_types::ConsensusStrategy::Threshold {
                            total: None,
                            min: 2,
                        }),
                    }),
                    ..Default::default()
                })
                .build(),
        );

        client
    }

    pub fn from_state_custom_providers(state: &State, providers: Vec<Provider>) -> Self {
        let mut client = Self {
            evm_rpc_client: None,
            chain: state.evm_network,
        };
        const MIN_ATTACHED_CYCLES: u128 = 30_000_000_000;

        let providers = get_custom_providers(client.chain, providers);

        client.evm_rpc_client = Some(
            EvmRpcClient::builder(CallerService {}, TRACE_HTTP)
                .with_providers(providers)
                .with_evm_canister_id(state.evm_canister_id)
                .with_min_attached_cycles(MIN_ATTACHED_CYCLES)
                .with_override_rpc_config(OverrideRpcConfig {
                    eth_get_logs: Some(EvmRpcConfig {
                        response_size_estimate: Some(
                            ETH_GET_LOGS_INITIAL_RESPONSE_SIZE_ESTIMATE + HEADER_SIZE_LIMIT,
                        ),
                        response_consensus: None,
                    }),
                    ..Default::default()
                })
                .build(),
        );

        client
    }

    pub fn from_state_one_provider(state: &State, provider: Provider) -> Self {
        let mut client = Self {
            evm_rpc_client: None,
            chain: state.evm_network,
        };
        const MIN_ATTACHED_CYCLES: u128 = 30_000_000_000;

        let providers = get_one_provider(client.chain, provider);

        client.evm_rpc_client = Some(
            EvmRpcClient::builder(CallerService {}, TRACE_HTTP)
                .with_providers(providers)
                .with_evm_canister_id(state.evm_canister_id)
                .with_min_attached_cycles(MIN_ATTACHED_CYCLES)
                .with_override_rpc_config(OverrideRpcConfig {
                    eth_get_logs: Some(EvmRpcConfig {
                        response_size_estimate: Some(
                            ETH_GET_LOGS_INITIAL_RESPONSE_SIZE_ESTIMATE + HEADER_SIZE_LIMIT,
                        ),
                        response_consensus: None,
                    }),
                    ..Default::default()
                })
                .build(),
        );

        client
    }

    pub async fn eth_call(&self, params: CallParams) -> Result<String, MultiCallError<String>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            let result = evm_rpc_client
                .eth_call(into_evm_call_args(params))
                .await
                .reduce();
            result.result
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }

    pub async fn get_logs(
        &self,
        params: GetLogsParam,
    ) -> Result<Vec<LogEntry>, MultiCallError<Vec<LogEntry>>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            let result = evm_rpc_client
                .eth_get_logs(EvmGetLogsArgs {
                    from_block: Some(into_evm_block_tag(params.from_block)),
                    to_block: Some(into_evm_block_tag(params.to_block)),
                    addresses: params
                        .address
                        .into_iter()
                        .map(|a| Hex20::from(a.into_bytes()))
                        .collect(),
                    topics: Some(into_evm_topic(params.topics)),
                })
                .await
                .reduce();
            result.result
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }

    pub async fn get_block_by_number(
        &self,
        block: BlockSpec,
    ) -> Result<Block, MultiCallError<Block>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            let result = evm_rpc_client
                .eth_get_block_by_number(into_evm_block_tag(block))
                .await
                .reduce();
            result.result
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }

    pub async fn get_transaction_receipt(
        &self,
        tx_hash: Hash,
    ) -> Result<Option<TransactionReceipt>, MultiCallError<Option<TransactionReceipt>>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            return evm_rpc_client
                .eth_get_transaction_receipt(tx_hash.to_string())
                .await
                .reduce()
                .into();
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }

    pub async fn fee_history(
        &self,
        params: FeeHistoryParams,
    ) -> Result<FeeHistory, MultiCallError<FeeHistory>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            let result = evm_rpc_client
                .eth_fee_history(EvmFeeHistoryArgs {
                    block_count: Nat256::from_be_bytes(params.block_count.to_be_bytes()),
                    newest_block: into_evm_block_tag(params.highest_block),
                    reward_percentiles: Some(params.reward_percentiles),
                })
                .await
                .reduce();
            result.result
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }

    pub async fn get_finalized_transaction_count(
        &self,
        address: Address,
    ) -> Result<TransactionCount, MultiCallError<TransactionCount>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            let results = evm_rpc_client
                .eth_get_transaction_count(EvmGetTransactionCountArgs {
                    address: Hex20::from(address.into_bytes()),
                    block: EvmBlockTag::Finalized,
                })
                .await;
            results.reduce().reduce_with_equality().result
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }

    pub async fn get_latest_transaction_count(
        &self,
        address: Address,
    ) -> Result<TransactionCount, MultiCallError<TransactionCount>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            let results = evm_rpc_client
                .eth_get_transaction_count(EvmGetTransactionCountArgs {
                    address: Hex20::from(address.into_bytes()),
                    block: EvmBlockTag::Latest,
                })
                .await;
            results
                .reduce()
                .reduce_with_min_by_key(|transaction_count| *transaction_count)
                .result
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }

    pub async fn send_raw_transaction(
        &self,
        raw_signed_transaction_hex: String,
    ) -> Result<SendRawTransactionResult, MultiCallError<SendRawTransactionResult>> {
        if let Some(evm_rpc_client) = &self.evm_rpc_client {
            let result = evm_rpc_client
                .eth_send_raw_transaction(raw_signed_transaction_hex)
                .await
                .reduce();
            result.result
        } else {
            Err(MultiCallError::ConsistentEvmRpcCanisterError(String::from(
                "EVM RPC canister can not be None",
            )))
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Ord, PartialOrd)]
pub enum SingleCallError {
    HttpOutcallError(HttpOutcallError),
    JsonRpcError { code: i64, message: String },
    EvmRpcError(String),
}

impl From<EvmRpcError> for SingleCallError {
    fn from(value: EvmRpcError) -> Self {
        match value {
            EvmRpcError::ProviderError(e) => SingleCallError::EvmRpcError(e.to_string()),
            EvmRpcError::HttpOutcallError(e) => SingleCallError::HttpOutcallError(e),
            EvmRpcError::JsonRpcError(e) => SingleCallError::JsonRpcError {
                code: e.code,
                message: e.message,
            },
            EvmRpcError::ValidationError(e) => SingleCallError::EvmRpcError(e.to_string()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MultiCallError<T> {
    ConsistentHttpOutcallError(HttpOutcallError),
    ConsistentJsonRpcError { code: i64, message: String },
    ConsistentEvmRpcCanisterError(String),
    InconsistentResults(Vec<(EvmRpcService, Result<T, SingleCallError>)>),
}

impl<T: Clone> MultiCallError<T> {
    pub fn has_http_outcall_error_matching<P: Fn(&HttpOutcallError) -> bool>(
        &self,
        predicate: P,
    ) -> bool {
        match self {
            MultiCallError::ConsistentHttpOutcallError(error) => predicate(error),
            MultiCallError::ConsistentJsonRpcError { .. } => false,
            MultiCallError::InconsistentResults(results) => {
                results
                    .iter()
                    .any(|(_rpcservice, rpc_result)| match rpc_result {
                        Ok(_) => false,
                        Err(rpc_error) => match rpc_error {
                            SingleCallError::HttpOutcallError(error) => predicate(error),
                            SingleCallError::JsonRpcError { .. }
                            | SingleCallError::EvmRpcError(_) => false,
                        },
                    })
            }
            MultiCallError::ConsistentEvmRpcCanisterError(_) => false,
        }
    }

    // If at there is at least one ok responses
    // Used for send_raw_transaction since nonce is only valid once
    fn at_least_one_ok(self) -> Result<(EvmRpcService, T), MultiCallError<T>> {
        match self {
            MultiCallError::InconsistentResults(inconsistent_results) => {
                let inconsistent_ok_results =
                    filter_inconsistent_ok_results(inconsistent_results.clone());
                match inconsistent_ok_results.len() {
                    0 => Err(MultiCallError::InconsistentResults(
                        inconsistent_results.clone(),
                    )),
                    _ => Ok(inconsistent_ok_results.into_iter().next().unwrap()),
                }
            }
            _ => Err(self),
        }
    }

    // If at there are at least two ok responses
    /// Expects at least 2 ok results to be ok or return the following error:
    /// * MultiCallError::ConsistentJsonRpcError: all errors are the same JSON-RPC error.
    /// * MultiCallError::ConsistentHttpOutcallError: all errors are the same HTTP outcall error.
    /// * MultiCallError::InconsistentResults if there are different errors or an ok result with some errors.
    pub fn at_least_two_ok(self) -> Result<Vec<(EvmRpcService, T)>, MultiCallError<T>> {
        match self {
            MultiCallError::InconsistentResults(inconsistent_results) => {
                let inconsistent_ok_results =
                    filter_inconsistent_ok_results(inconsistent_results.clone());

                match inconsistent_ok_results.len() {
                    0 => Err(MultiCallError::InconsistentResults(
                        inconsistent_results.clone(),
                    )),
                    1 => Err(MultiCallError::InconsistentResults(
                        inconsistent_results.clone(),
                    )),
                    _ => Ok(inconsistent_ok_results),
                }
            }
            _ => Err(self),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReducedResult<T> {
    result: Result<T, MultiCallError<T>>,
}

impl<T> From<Result<T, MultiCallError<T>>> for ReducedResult<T> {
    fn from(result: Result<T, MultiCallError<T>>) -> Self {
        Self { result }
    }
}

impl<T> From<ReducedResult<T>> for Result<T, MultiCallError<T>> {
    fn from(value: ReducedResult<T>) -> Self {
        value.result
    }
}

impl<T: std::fmt::Debug + std::cmp::PartialEq + Clone> ReducedResult<T> {
    /// Transform a `ReducedResult<T>` into a `ReducedResult<U>` by applying a mapping function `F`.
    /// The mapping function is also applied to the elements contained in the error `MultiCallError::InconsistentResults`.
    pub fn map_reduce<U, E: Display, F: Fn(T) -> Result<U, E>>(
        self,
        fallible_op: &F,
        // reduction: R,
    ) -> ReducedResult<U> {
        let result = match self.result {
            Ok(t) => fallible_op(t)
                .map_err(|e| MultiCallError::<U>::ConsistentEvmRpcCanisterError(e.to_string())),
            Err(MultiCallError::ConsistentHttpOutcallError(e)) => {
                Err(MultiCallError::<U>::ConsistentHttpOutcallError(e))
            }
            Err(MultiCallError::ConsistentJsonRpcError { code, message }) => {
                Err(MultiCallError::<U>::ConsistentJsonRpcError { code, message })
            }
            Err(MultiCallError::ConsistentEvmRpcCanisterError(e)) => {
                Err(MultiCallError::<U>::ConsistentEvmRpcCanisterError(e))
            }

            Err(MultiCallError::InconsistentResults(results)) => {
                let mapped_inconsistent_results = results
                    .into_iter()
                    .map(|(rpc_service, response)| {
                        let mapped_response = match response {
                            Ok(inconsistent_response) => fallible_op(inconsistent_response)
                                .map_err(|e| SingleCallError::EvmRpcError(e.to_string())),
                            Err(error) => Err(error),
                        };
                        (rpc_service, mapped_response)
                    })
                    .collect();
                Err(MultiCallError::<U>::InconsistentResults(
                    mapped_inconsistent_results,
                ))
            }
        };
        ReducedResult { result }
    }

    pub fn from_multi_result(value: EvmMultiRpcResult<T>) -> Self {
        let result = match value {
            EvmMultiRpcResult::Consistent(result) => match result {
                Ok(t) => Ok(t),
                Err(e) => match e {
                    EvmRpcError::ProviderError(e) => {
                        Err(MultiCallError::ConsistentEvmRpcCanisterError(e.to_string()))
                    }
                    EvmRpcError::HttpOutcallError(e) => {
                        Err(MultiCallError::ConsistentHttpOutcallError(e))
                    }
                    EvmRpcError::JsonRpcError(e) => Err(MultiCallError::ConsistentJsonRpcError {
                        code: e.code,
                        message: e.message,
                    }),
                    EvmRpcError::ValidationError(e) => {
                        Err(MultiCallError::ConsistentEvmRpcCanisterError(e.to_string()))
                    }
                },
            },
            EvmMultiRpcResult::Inconsistent(result) => {
                let converted_to_single_call_error = result
                    .into_iter()
                    .map(|(rpc_provider, rpc_result)| {
                        let mapped_rpc_result = match rpc_result {
                            Ok(ok_response) => Ok(ok_response),
                            Err(rpc_error) => Err(SingleCallError::from(rpc_error)),
                        };
                        (rpc_provider, mapped_rpc_result)
                    })
                    .collect();

                Err(MultiCallError::InconsistentResults(
                    converted_to_single_call_error,
                ))
            }
        };
        Self { result }
    }

    // Reduce the inconsistent result with the strategy that if there is even a single inconsistent response,
    // the new reduced result will be an inconsistent multi-error call type.
    pub fn reduce_with_equality(self) -> Self {
        match self.result {
            Ok(_) => self.clone(),
            Err(multi_error) => match multi_error.clone().at_least_two_ok() {
                Ok(inconsistent_result) => {
                    if let Some((_base_provider, base_result)) =
                        inconsistent_result.clone().into_iter().next()
                    {
                        let mut looped_inconsistent_results: Vec<T> = Vec::new();
                        for (_other_provider, other_result) in inconsistent_result.into_iter() {
                            if base_result != other_result {
                                looped_inconsistent_results.push(other_result)
                            }
                        }

                        match looped_inconsistent_results.len() {
                            0 => ReducedResult {
                                result: Ok(base_result),
                            },
                            _ => {
                                log!(
                                    INFO,
                                    "[reduce_with_equality]: inconsistent results {multi_error:?}"
                                );
                                ReducedResult {
                                    result: Err(multi_error),
                                }
                            }
                        }
                    } else {
                        log!(
                            INFO,
                            "[reduce_with_equality]: inconsistent results {multi_error:?}"
                        );
                        ReducedResult {
                            result: Err(multi_error),
                        }
                    }
                }
                Err(error) => ReducedResult { result: Err(error) },
            },
        }
    }

    // If Inconsistent, Aggregates results from multiple RPC node providers into a single result based on a strict majority rule.
    // The aggregation is performed by grouping results with the same key extracted using the provided extractor function.
    // If a strict majority (more than half) of results share the same value for a key, that value is returned.
    // If no strict majority exists, an error is returned with the inconsistent results.
    pub fn reduce_with_strict_majority_by_key<F: Fn(&T) -> K, K: Ord>(self, extractor: F) -> Self {
        match self.result {
            Ok(_) => self,
            Err(multi_error) => match multi_error.at_least_two_ok() {
                Ok(inconsistent_ok_results) => {
                    let mut votes_by_key: BTreeMap<K, BTreeMap<EvmRpcService, T>> = BTreeMap::new();
                    for (provider, result) in inconsistent_ok_results.into_iter() {
                        let key = extractor(&result);
                        match votes_by_key.remove(&key) {
                            Some(mut votes_for_same_key) => {
                                let (_other_provider, other_result) = votes_for_same_key
                                    .last_key_value()
                                    .expect("BUG: results_with_same_key is non-empty");
                                if &result != other_result {
                                    let error = MultiCallError::InconsistentResults(
                                        votes_for_same_key
                                            .into_iter()
                                            .chain(std::iter::once((provider, result)))
                                            .map(|(provider, result)| (provider, Ok(result)))
                                            .collect(),
                                    );
                                    log!(INFO,"[reduce_with_strict_majority_by_key]: inconsistent results {error:?}");
                                    return ReducedResult { result: Err(error) };
                                }
                                votes_for_same_key.insert(provider, result);
                                votes_by_key.insert(key, votes_for_same_key);
                            }
                            None => {
                                let _ =
                                    votes_by_key.insert(key, BTreeMap::from([(provider, result)]));
                            }
                        }
                    }
                    let mut tally: Vec<(K, BTreeMap<EvmRpcService, T>)> =
                        Vec::from_iter(votes_by_key);
                    tally.sort_unstable_by(
                        |(_left_key, left_ballot), (_right_key, right_ballot)| {
                            left_ballot.len().cmp(&right_ballot.len())
                        },
                    );
                    match tally.len() {
                        0 => panic!("BUG: tally should be non-empty"),
                        1 => ReducedResult {
                            result: Ok(tally
                                .pop()
                                .and_then(|(_key, mut ballot)| ballot.pop_last())
                                .expect("BUG: tally is non-empty")
                                .1),
                        },
                        _ => {
                            let mut first =
                                tally.pop().expect("BUG: tally has at least 2 elements");
                            let second = tally.pop().expect("BUG: tally has at least 2 elements");
                            if first.1.len() > second.1.len() {
                                ReducedResult {
                                    result: Ok(first
                                        .1
                                        .pop_last()
                                        .expect("BUG: tally should be non-empty")
                                        .1),
                                }
                            } else {
                                let error = MultiCallError::InconsistentResults(
                                    first
                                        .1
                                        .into_iter()
                                        .chain(second.1)
                                        .map(|(provider, result)| (provider, Ok(result)))
                                        .collect(),
                                );

                                log!( INFO,"[reduce_with_strict_majority_by_key]: no strict majority {error:?}");
                                ReducedResult { result: Err(error) }
                            }
                        }
                    }
                }
                Err(error) => ReducedResult { result: Err(error) },
            },
        }
    }

    // Used for send raw transaction, if inconsistent searches only for one Ok result since there will be only one ok result because multiple nonces should be unique
    pub fn reduce_with_only_one_key(self) -> Self {
        match self.result {
            Ok(_) => self,
            Err(error) => match error.at_least_one_ok() {
                Ok(desired_result) => ReducedResult {
                    result: Ok(desired_result.1),
                },
                Err(multicall_error) => ReducedResult {
                    result: Err(multicall_error),
                },
            },
        }
    }

    // If inconsistent returns the key with the minimum value
    pub fn reduce_with_min_by_key<F: FnMut(&T) -> K, K: Ord>(self, extractor: F) -> Self {
        match self.result {
            Ok(_) => self,
            Err(error) => match error.at_least_two_ok() {
                Ok(ok_results) => {
                    let mapped_to_consistent = ok_results
                        .into_iter()
                        .map(|(_rpc_service, result)| result)
                        .min_by_key(extractor)
                        .expect("BUG: ok_results is guaranteed to be non-empty");
                    ReducedResult {
                        result: Ok(mapped_to_consistent),
                    }
                }
                Err(multi_call_error) => ReducedResult {
                    result: Err(multi_call_error),
                },
            },
        }
    }
}
// Reduce trait implementation for converting EVM_RPC_CANISTER response into desired type.
// Convert inconsistent response into consistent if necessary with different strategies
trait Reduce {
    type Item;
    fn reduce(self) -> ReducedResult<Self::Item>;
}

impl Reduce for EvmMultiRpcResult<EvmBlock> {
    type Item = Block;

    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self)
            .reduce_with_equality()
            .map_reduce(&|block: EvmBlock| {
                Ok::<Block, String>(Block {
                    number: BlockNumber::from(block.number),
                    base_fee_per_gas: Wei::from(block.base_fee_per_gas.expect("BUG: must be present in blocks after the London Upgrade / EIP-1559, which pre-dates the minter")),

                })
            })
    }
}

impl Reduce for EvmMultiRpcResult<Block> {
    type Item = Block;

    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self).reduce_with_equality()
    }
}

impl Reduce for EvmMultiRpcResult<Vec<EvmLogEntry>> {
    type Item = Vec<LogEntry>;

    fn reduce(self) -> ReducedResult<Self::Item> {
        fn map_logs(logs: Vec<EvmLogEntry>) -> Result<Vec<LogEntry>, String> {
            logs.into_iter().map(map_single_log).collect()
        }

        fn map_single_log(log: EvmLogEntry) -> Result<LogEntry, String> {
            Ok(LogEntry {
                address: Address::new(log.address.into()),
                topics: log
                    .topics
                    .into_iter()
                    .map(|t| FixedSizeData(t.into()))
                    .collect(),
                data: Data(log.data.into()),
                block_number: log.block_number.map(BlockNumber::from),
                transaction_hash: log.transaction_hash.map(|h| Hash(h.into())),
                transaction_index: log
                    .transaction_index
                    .map(|i| Quantity::from_be_bytes(i.into_be_bytes())),
                block_hash: log.block_hash.map(|h| Hash(h.into())),
                log_index: log.log_index.map(LogIndex::from),
                removed: log.removed,
            })
        }

        ReducedResult::from_multi_result(self)
            .reduce_with_equality()
            .map_reduce(&map_logs)
    }
}

impl Reduce for EvmMultiRpcResult<Vec<LogEntry>> {
    type Item = Vec<LogEntry>;

    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self).reduce_with_equality()
    }
}

impl Reduce for EvmMultiRpcResult<String> {
    type Item = String;
    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self).reduce_with_equality()
    }
}

impl Reduce for EvmMultiRpcResult<Option<EvmTransactionReceipt>> {
    type Item = Option<TransactionReceipt>;

    fn reduce(self) -> ReducedResult<Self::Item> {
        fn map_transaction_receipt(
            receipt: Option<EvmTransactionReceipt>,
        ) -> Result<Option<TransactionReceipt>, String> {
            receipt
                .map(|evm_receipt| {
                    Ok(TransactionReceipt {
                        block_hash: Hash(evm_receipt.block_hash.into()),
                        block_number: BlockNumber::from(evm_receipt.block_number),
                        effective_gas_price: WeiPerGas::from(evm_receipt.effective_gas_price),
                        gas_used: GasAmount::from(evm_receipt.gas_used),
                        status: TransactionStatus::try_from(
                            evm_receipt
                                .status
                                .and_then(|s| s.as_ref().0.to_u8())
                                .ok_or("invalid transaction status")?,
                        )?,
                        transaction_hash: Hash(evm_receipt.transaction_hash.into()),
                    })
                })
                .transpose()
        }

        ReducedResult::from_multi_result(self)
            .reduce_with_equality()
            .map_reduce(&map_transaction_receipt)
    }
}

impl Reduce for EvmMultiRpcResult<Option<TransactionReceipt>> {
    type Item = Option<TransactionReceipt>;

    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self).reduce_with_equality()
    }
}

impl Reduce for EvmMultiRpcResult<Option<EvmFeeHistory>> {
    type Item = FeeHistory;

    fn reduce(self) -> ReducedResult<Self::Item> {
        fn map_fee_history(fee_history: Option<EvmFeeHistory>) -> Result<FeeHistory, String> {
            let fee_history = fee_history.ok_or("No fee history available")?;
            Ok(FeeHistory {
                oldest_block: BlockNumber::from(fee_history.oldest_block),
                base_fee_per_gas: wei_per_gas_iter(fee_history.base_fee_per_gas),
                reward: fee_history
                    .reward
                    .into_iter()
                    .map(wei_per_gas_iter)
                    .collect(),
            })
        }

        ReducedResult::from_multi_result(self)
            .map_reduce(&map_fee_history)
            .reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block)
    }
}

pub fn wei_per_gas_iter(values: Vec<Nat256>) -> Vec<WeiPerGas> {
    values.into_iter().map(WeiPerGas::from).collect()
}

impl Reduce for EvmMultiRpcResult<EvmSendRawTransactionStatus> {
    type Item = SendRawTransactionResult;

    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self)
            .map_reduce(&|tx_status| {
                Ok::<SendRawTransactionResult, Infallible>(SendRawTransactionResult::from(
                    tx_status,
                ))
            })
            .reduce_with_only_one_key()
    }
}

// Transaction Count reduction
impl Reduce for EvmMultiRpcResult<Nat256> {
    type Item = TransactionCount;
    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self)
            .map_reduce(&|transaction_count: Nat256| TransactionCount::try_from(transaction_count))
    }
}

impl Reduce for EvmMultiRpcResult<Nat> {
    type Item = TransactionCount;
    fn reduce(self) -> ReducedResult<Self::Item> {
        ReducedResult::from_multi_result(self)
            .map_reduce(&|transaction_count: Nat| TransactionCount::try_from(transaction_count))
    }
}

fn into_evm_blob_version_hashes(hashes: Vec<Hash>) -> Vec<Hex32> {
    hashes.into_iter().map(|hash| Hex32::from(hash.0)).collect()
}

fn into_evm_blobs(blobs: Vec<Vec<u8>>) -> Vec<Hex> {
    blobs.into_iter().map(Hex::from).collect()
}
fn into_evm_access_list(access_list: AccessList) -> EvmAccessList {
    let entries: Vec<EvmAccessListEntry> = access_list
        .0
        .into_iter()
        .map(|entry| EvmAccessListEntry {
            address: Hex20::from(entry.address.into_bytes()),
            storage_keys: entry
                .storage_keys
                .into_iter()
                .map(|key| Hex32::from(key.0))
                .collect(),
        })
        .collect();

    EvmAccessList(entries)
}

fn into_evm_block_tag(block: BlockSpec) -> EvmBlockTag {
    match block {
        BlockSpec::Number(n) => EvmBlockTag::Number(n.into()),
        BlockSpec::Tag(BlockTag::Latest) => EvmBlockTag::Latest,
        BlockSpec::Tag(BlockTag::Safe) => EvmBlockTag::Safe,
        BlockSpec::Tag(BlockTag::Finalized) => EvmBlockTag::Finalized,
    }
}

fn into_evm_topic(topics: Vec<Topic>) -> Vec<Vec<Hex32>> {
    let into_hex_32 = |data: FixedSizeData| Hex32::from(data.0);
    let mut result = Vec::with_capacity(topics.len());
    for topic in topics {
        result.push(match topic {
            Topic::Single(single_topic) => vec![into_hex_32(single_topic)],
            Topic::Multiple(multiple_topic) => {
                multiple_topic.into_iter().map(into_hex_32).collect()
            }
        });
    }
    result
}

pub fn is_response_too_large(error: &HttpOutcallError) -> bool {
    match error {
        HttpOutcallError::IcError { code, message } => {
            code == &RejectionCode::SysFatal && message.contains("size limit")
        }
        _ => false,
    }
}

pub fn into_evm_call_args(call_params: CallParams) -> CallArgs {
    CallArgs {
        transaction: TransactionRequest {
            tx_type: call_params.transaction.tx_type.map(HexByte::from),
            nonce: call_params
                .transaction
                .nonce
                .map(|nonce| Nat256::from_be_bytes(nonce.to_be_bytes())),
            to: call_params
                .transaction
                .to
                .map(|to| Hex20::from(to.into_bytes())),
            from: call_params
                .transaction
                .from
                .map(|from| Hex20::from(from.into_bytes())),
            gas: call_params
                .transaction
                .gas
                .map(|gas| Nat256::from_be_bytes(gas.to_be_bytes())),
            value: call_params
                .transaction
                .value
                .map(|value| Nat256::from_be_bytes(value.to_be_bytes())),
            input: call_params.transaction.input.map(Hex::from),
            gas_price: call_params
                .transaction
                .gas_price
                .map(|gas_price| Nat256::from_be_bytes(gas_price.to_be_bytes())),
            max_priority_fee_per_gas: call_params.transaction.max_priority_fee_per_gas.map(
                |max_priority_fee_per_gas| {
                    Nat256::from_be_bytes(max_priority_fee_per_gas.to_be_bytes())
                },
            ),

            max_fee_per_gas: call_params
                .transaction
                .max_fee_per_gas
                .map(|max_fee_per_gas| Nat256::from_be_bytes(max_fee_per_gas.to_be_bytes())),

            max_fee_per_blob_gas: call_params.transaction.max_fee_per_blob_gas.map(
                |max_fee_per_blob_gas| Nat256::from_be_bytes(max_fee_per_blob_gas.to_be_bytes()),
            ),
            access_list: call_params
                .transaction
                .access_list
                .map(into_evm_access_list),
            blob_versioned_hashes: call_params
                .transaction
                .blob_versioned_hashes
                .map(into_evm_blob_version_hashes),
            blobs: call_params.transaction.blobs.map(into_evm_blobs),
            chain_id: call_params.transaction.chain_id.map(Nat256::from),
        },
        block: call_params.block.map(into_evm_block_tag),
    }
}

pub fn filter_inconsistent_ok_results<T>(
    inconsistent_results: Vec<(EvmRpcService, Result<T, SingleCallError>)>,
) -> Vec<(EvmRpcService, T)> {
    inconsistent_results
        .into_iter()
        .filter_map(|(rpc_service, result)| result.ok().map(|value| (rpc_service, value)))
        .collect()
}
pub fn filter_inconsistent_error_results<T>(
    inconsistent_results: Vec<(EvmRpcService, Result<T, SingleCallError>)>,
) -> Vec<(EvmRpcService, SingleCallError)> {
    inconsistent_results
        .into_iter()
        .filter_map(|(rpc_service, result)| result.err().map(|error| (rpc_service, error)))
        .collect()
}

pub fn only_inconsistent_ok_results_without_providers<T: Clone>(
    inconsistent_results: &[(EvmRpcService, Result<T, SingleCallError>)],
) -> Vec<T> {
    inconsistent_results
        .iter()
        .filter_map(|(_rpc_service, result)| result.clone().ok())
        .collect()
}

pub fn only_inconsistent_error_results_without_providers<T: Clone>(
    inconsistent_results: &[(EvmRpcService, Result<T, SingleCallError>)],
) -> Vec<SingleCallError> {
    inconsistent_results
        .iter()
        .filter_map(|(_rpc_service, result)| result.clone().err())
        .collect()
}
