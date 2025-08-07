pub use evm_rpc_types::{MultiRpcResult, ProviderError, RpcApi, RpcError, RpcService};

use crate::evm_rpc_types;

mod max_expected_too_few_cycles_error {
    use super::*;
    use crate::max_expected_too_few_cycles_error;

    #[test]
    fn should_get_max_expected_too_few_cycles_for_custom_rpc_service() {
        let result: MultiRpcResult<()> = MultiRpcResult::Inconsistent(vec![
            (
                RpcService::Custom(RpcApi {
                    url: "https://eth.llamarpc.com".to_string(),
                    headers: None,
                }),
                Err(RpcError::ProviderError(ProviderError::TooFewCycles {
                    expected: 701_433_600,
                    received: 350_729_600,
                })),
            ),
            (
                RpcService::Custom(RpcApi {
                    url: "https://ethereum-rpc.publicnode.com".to_string(),
                    headers: None,
                }),
                Err(RpcError::ProviderError(ProviderError::TooFewCycles {
                    expected: 863_894_400,
                    received: 350_729_600,
                })),
            ),
            (
                RpcService::Custom(RpcApi {
                    url: "https://rpc.ankr.com/eth".to_string(),
                    headers: None,
                }),
                Err(RpcError::ProviderError(ProviderError::TooFewCycles {
                    expected: 893_894_400,
                    received: 350_729_600,
                })),
            ),
        ]);

        let max_too_few_cycles = max_expected_too_few_cycles_error(&result);

        assert_eq!(max_too_few_cycles, Some(893_894_400));
    }
}
