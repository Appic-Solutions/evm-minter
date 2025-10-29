use crate::evm_rpc_types;
use crate::evm_rpc_types::{Hex, Hex32, MultiRpcResult, Nat256, RpcResult, ValidationError};
use crate::native_http::constants::ETH_GET_LOGS_MAX_BLOCKS;

use crate::native_http::{EthRpcClient, MultiCallError};
use candid::Nat;
use ethers_core::{types::Transaction, utils::rlp};

pub mod cketh_conversion;

fn process_result<T>(result: Result<T, MultiCallError<T>>) -> MultiRpcResult<T> {
    match result {
        Ok(value) => MultiRpcResult::Consistent(Ok(value)),
        Err(err) => match err {
            MultiCallError::ConsistentError(err) => MultiRpcResult::Consistent(Err(err)),
            MultiCallError::InconsistentResults(multi_call_results) => {
                let results = multi_call_results.into_vec();
                MultiRpcResult::Inconsistent(results)
            }
        },
    }
}

/// Adapt the `EthRpcClient` to the `Candid` interface used by the EVM-RPC canister.
pub struct CandidRpcClient {
    client: EthRpcClient,
}

impl CandidRpcClient {
    pub fn new(
        source: crate::evm_rpc_types::RpcServices,
        config: Option<crate::evm_rpc_types::RpcConfig>,
    ) -> RpcResult<Self> {
        Ok(Self {
            client: EthRpcClient::new(source, config)?,
        })
    }

    pub async fn eth_get_logs(
        &self,
        args: crate::evm_rpc_types::GetLogsArgs,
        cycles_available: u128,
    ) -> MultiRpcResult<Vec<crate::evm_rpc_types::LogEntry>> {
        use crate::native_http::candid_rpc::cketh_conversion::{
            from_log_entries, into_get_logs_param,
        };

        if let (
            Some(crate::evm_rpc_types::BlockTag::Number(from)),
            Some(crate::evm_rpc_types::BlockTag::Number(to)),
        ) = (&args.from_block, &args.to_block)
        {
            let from = Nat::from(from.clone());
            let to = Nat::from(to.clone());
            let block_count = if to > from { to - from } else { from - to };
            if block_count > ETH_GET_LOGS_MAX_BLOCKS {
                return MultiRpcResult::Consistent(Err(ValidationError::Custom(format!(
                    "Requested {} blocks; limited to {} when specifying a start and end block",
                    block_count, ETH_GET_LOGS_MAX_BLOCKS
                ))
                .into()));
            }
        }
        process_result(
            self.client
                .eth_get_logs(into_get_logs_param(args), cycles_available)
                .await,
        )
        .map(from_log_entries)
    }

    pub async fn eth_get_block_by_number(
        &self,
        block: evm_rpc_types::BlockTag,
        cycles_available: u128,
    ) -> MultiRpcResult<evm_rpc_types::Block> {
        use crate::native_http::candid_rpc::cketh_conversion::{from_block, into_block_spec};
        process_result(
            self.client
                .eth_get_block_by_number(into_block_spec(block), cycles_available)
                .await,
        )
        .map(from_block)
    }

    pub async fn eth_get_transaction_receipt(
        &self,
        hash: Hex32,
        cycles_available: u128,
    ) -> MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>> {
        use crate::native_http::candid_rpc::cketh_conversion::{
            from_transaction_receipt, into_hash,
        };
        process_result(
            self.client
                .eth_get_transaction_receipt(into_hash(hash), cycles_available)
                .await,
        )
        .map(|option| option.map(from_transaction_receipt))
    }

    pub async fn eth_get_transaction_count(
        &self,
        args: evm_rpc_types::GetTransactionCountArgs,
        cycles_available: u128,
    ) -> MultiRpcResult<Nat256> {
        use crate::native_http::candid_rpc::cketh_conversion::into_get_transaction_count_params;
        process_result(
            self.client
                .eth_get_transaction_count(
                    into_get_transaction_count_params(args),
                    cycles_available,
                )
                .await,
        )
        .map(Nat256::from)
    }

    pub async fn eth_fee_history(
        &self,
        args: evm_rpc_types::FeeHistoryArgs,
        cycles_available: u128,
    ) -> MultiRpcResult<evm_rpc_types::FeeHistory> {
        use crate::native_http::candid_rpc::cketh_conversion::{
            from_fee_history, into_fee_history_params,
        };
        process_result(
            self.client
                .eth_fee_history(into_fee_history_params(args), cycles_available)
                .await,
        )
        .map(from_fee_history)
    }

    pub async fn eth_send_raw_transaction(
        &self,
        raw_signed_transaction_hex: Hex,
        cycles_available: u128,
    ) -> MultiRpcResult<evm_rpc_types::SendRawTransactionStatus> {
        use crate::native_http::candid_rpc::cketh_conversion::from_send_raw_transaction_result;
        let transaction_hash = get_transaction_hash(&raw_signed_transaction_hex);
        process_result(
            self.client
                .eth_send_raw_transaction(raw_signed_transaction_hex.to_string(), cycles_available)
                .await,
        )
        .map(|result| from_send_raw_transaction_result(transaction_hash.clone(), result))
    }

    pub async fn eth_call(
        &self,
        args: evm_rpc_types::CallArgs,
        cycles_available: u128,
    ) -> MultiRpcResult<evm_rpc_types::Hex> {
        use crate::native_http::candid_rpc::cketh_conversion::{from_data, into_eth_call_params};
        process_result(
            self.client
                .eth_call(into_eth_call_params(args), cycles_available)
                .await,
        )
        .map(from_data)
    }
}

fn get_transaction_hash(raw_signed_transaction_hex: &Hex) -> Option<Hex32> {
    let transaction: Transaction = rlp::decode(raw_signed_transaction_hex.as_ref()).ok()?;
    Some(Hex32::from(transaction.hash.0))
}
