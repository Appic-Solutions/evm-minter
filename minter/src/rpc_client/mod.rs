use crate::{
    lifecycles::EvmNetwork,
    logs::{PrintProxySink, DEBUG, INFO, TRACE_HTTP},
    state::State,
};
use evm_rpc_client::{
    types::candid::{EthSepoliaService, RpcConfig as EvmRpcConfig, RpcServices},
    CallerService, EvmRpcClient, OverrideRpcConfig,
};
// We expect most of the calls to contain zero events.
const ETH_GET_LOGS_INITIAL_RESPONSE_SIZE_ESTIMATE: u64 = 100;

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
    pub fn from_state(state: &State) -> Self {
        let mut client = Self {
            evm_rpc_client: None,
            chain: state.evm_network_id,
        };
        const MIN_ATTACHED_CYCLES: u128 = 300_000_000_000;

        // TODO: Add afunction to chose custom providers based on the chainid
        let providers = RpcServices::EthSepolia(Some(vec![EthSepoliaService::Alchemy]));

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
                    }),
                    ..Default::default()
                })
                .build(),
        );

        client
    }
}