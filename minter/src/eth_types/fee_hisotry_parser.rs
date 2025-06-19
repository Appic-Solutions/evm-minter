use evm_rpc_types::FeeHistory as EvmFeeHistory;

use crate::{numeric::BlockNumber, rpc_client::wei_per_gas_iter, rpc_declarations::FeeHistory};

pub fn parse_fee_history(fee_history: String) -> Option<FeeHistory> {
    let fee_history_parsed = serde_json::from_str::<EvmFeeHistory>(&fee_history).ok()?;

    Some(FeeHistory {
        oldest_block: BlockNumber::from(fee_history_parsed.oldest_block),
        base_fee_per_gas: wei_per_gas_iter(fee_history_parsed.base_fee_per_gas),
        reward: fee_history_parsed
            .reward
            .into_iter()
            .map(wei_per_gas_iter)
            .collect(),
    })
}
