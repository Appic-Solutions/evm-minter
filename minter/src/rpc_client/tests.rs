mod providers {
    use evm_rpc_types::{RpcApi, RpcServices};
    use strum::IntoEnumIterator;

    use crate::{
        evm_config::EvmNetwork,
        rpc_client::providers::{get_providers, Provider},
        storage::set_rpc_api_key,
    };

    #[test]
    fn should_generate_url_with_api_key() {
        set_rpc_api_key(Provider::LlamaNodes, "Test_key_Llama".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());

        assert_eq!(
            Provider::LlamaNodes.get_url_with_api_key("https://polygon.llamarpc.com/"),
            "https://polygon.llamarpc.com/Test_key_Llama".to_string()
        );

        assert_eq!(
            Provider::PublicNode.get_url_with_api_key("https://polygon-bor-rpc.publicnode.com/"),
            "https://polygon-bor-rpc.publicnode.com/".to_string()
        );

        assert_eq!(
            Provider::Ankr.get_url_with_api_key("https://rpc.ankr.com/eth/"),
            "https://rpc.ankr.com/eth/Test_key_Ankr".to_string()
        );

        assert_eq!(
            Provider::DRPC.get_url_with_api_key("https://lb.drpc.org/ogrpc?network=ethereum&dkey="),
            "https://lb.drpc.org/ogrpc?network=ethereum&dkey=Test_key_DRPC".to_string()
        );

        assert_eq!(
            Provider::Alchemy.get_url_with_api_key("https://eth-mainnet.g.alchemy.com/v2/"),
            "https://eth-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string()
        )
    }

    #[test]
    fn should_retrieve_at_least_four_providers() {
        for network in EvmNetwork::iter() {
            match get_providers(network) {
                evm_rpc_types::RpcServices::Custom {
                    chain_id: _,
                    services,
                } => {
                    assert!(services.len() >= 2)
                }
                _ => (),
            }
        }
    }

    #[test]
    fn should_retrieve_ethereum_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::Ethereum.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/eth/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://ethereum-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=ethereum&dkey=Test_key_DRPC"
                        .to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://eth-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::Ethereum), expected);
    }

    #[test]
    fn should_retrieve_sepolia_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::Sepolia.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/eth_sepolia/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://ethereum-sepolia-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=sepolia&dkey=Test_key_DRPC".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://eth-sepolia.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::Sepolia), expected);
    }

    #[test]
    fn should_retrieve_arbitrum_one_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::ArbitrumOne.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/arbitrum/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://arbitrum-one-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=arbitrum&dkey=Test_key_DRPC"
                        .to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://arb-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::ArbitrumOne), expected);
    }

    #[test]
    fn should_retrieve_bsc_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::BSC.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/bsc/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://bsc-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=bsc&dkey=Test_key_DRPC".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://bnb-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::BSC), expected);
    }

    #[test]
    fn should_retrieve_bsc_testnet_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::BSCTestnet.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/bsc_testnet_chapel/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://bsc-testnet-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=bsc-testnet&dkey=Test_key_DRPC"
                        .to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://bnb-testnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::BSCTestnet), expected);
    }

    #[test]
    fn should_retrieve_polygon_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::Polygon.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/polygon/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://polygon-bor-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=polygon&dkey=Test_key_DRPC".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://polygon-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::Polygon), expected);
    }

    #[test]
    fn should_retrieve_optimism_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::Optimism.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/optimism/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://optimism-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=optimism&dkey=Test_key_DRPC"
                        .to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://opt-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::Optimism), expected);
    }

    #[test]
    fn should_retrieve_base_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::Base.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/base/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://base-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=base&dkey=Test_key_DRPC".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://base-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::Base), expected);
    }

    #[test]
    fn should_retrieve_avalanche_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::Avalanche.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/avalanche/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://avalanche-c-chain-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=avalanche&dkey=Test_key_DRPC"
                        .to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://avax-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::Avalanche), expected);
    }

    #[test]
    fn should_retrieve_fantom_providers() {
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());
        set_rpc_api_key(Provider::DRPC, "Test_key_DRPC".to_string());
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        let expected = RpcServices::Custom {
            chain_id: EvmNetwork::Fantom.chain_id(),
            services: vec![
                RpcApi {
                    url: "https://rpc.ankr.com/fantom/Test_key_Ankr".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://fantom-rpc.publicnode.com/".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://lb.drpc.org/ogrpc?network=fantom&dkey=Test_key_DRPC".to_string(),
                    headers: None,
                },
                RpcApi {
                    url: "https://fantom-mainnet.g.alchemy.com/v2/Test_key_Alchemy".to_string(),
                    headers: None,
                },
            ],
        };

        assert_eq!(get_providers(EvmNetwork::Fantom), expected);
    }
}

mod multi_rpc_results {
    use evm_rpc_types::{
        EthSepoliaService, HttpOutcallError, MultiRpcResult as EvmMultiRpcResult,
        RpcError as EvmRpcError, RpcService as EvmRpcService,
    };
    mod reduce_with_equality {
        use super::*;
        use crate::rpc_client::{MultiCallError, ReducedResult, SingleCallError};

        use evm_rpc_types::JsonRpcError;
        use ic_cdk::api::call::RejectionCode;

        #[test]
        fn should_be_inconsistent_single_call_error() {
            let results = vec![
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                    Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                        code: RejectionCode::CanisterReject,
                        message: "reject".to_string(),
                    })),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                    Ok("world".to_string()),
                ),
            ];
            let reduced_results: ReducedResult<String> =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(results));

            let reduced_with_strategy = reduced_results.reduce_with_equality();

            assert_eq!(
                reduced_with_strategy.result,
                Err(MultiCallError::InconsistentResults(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Err(SingleCallError::HttpOutcallError(
                            HttpOutcallError::IcError {
                                code: RejectionCode::CanisterReject,
                                message: "reject".to_string(),
                            }
                        )),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                        Ok("world".to_string()),
                    ),
                ]))
            )
        }

        #[test]
        fn should_be_consistent_http_outcall_error() {
            let result: Result<String, EvmRpcError> =
                Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                    code: RejectionCode::CanisterReject,
                    message: "reject".to_string(),
                }));
            let reduced_result =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Consistent(result));
            let reduced_with_strategy = reduced_result.reduce_with_equality();

            assert_eq!(
                reduced_with_strategy.result,
                Err(MultiCallError::ConsistentHttpOutcallError(
                    HttpOutcallError::IcError {
                        code: RejectionCode::CanisterReject,
                        message: "reject".to_string(),
                    }
                ))
            );
        }

        #[test]
        fn should_be_consistent_rpc_error() {
            let result: Result<String, EvmRpcError> =
                Err(EvmRpcError::JsonRpcError(JsonRpcError {
                    code: -32700,
                    message: "insufficient funds for gas * price + value".to_string(),
                }));

            let reduced_result =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Consistent(result));

            let reduced_with_strategy = reduced_result.reduce_with_equality();

            assert_eq!(
                reduced_with_strategy.result,
                Err(MultiCallError::ConsistentJsonRpcError {
                    code: -32700,
                    message: "insufficient funds for gas * price + value".to_string(),
                })
            );
        }

        #[test]
        fn should_be_consistent_ok_result() {
            let result: Result<String, EvmRpcError> = Ok("0x01".to_string());

            let reduced_result =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Consistent(result));

            let reduced_with_strategy = reduced_result.reduce_with_equality();

            assert_eq!(reduced_with_strategy.result, Ok("0x01".to_string()));
        }

        #[test]
        fn should_be_consistent_if_only_one_http_error_and_two_consistent_ok() {
            let results = vec![
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                    Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                        code: RejectionCode::CanisterReject,
                        message: "reject".to_string(),
                    })),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                    Ok("world".to_string()),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                    Ok("world".to_string()),
                ),
            ];

            let reduced_results: ReducedResult<String> =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(results));

            let reduced_with_strategy = reduced_results.reduce_with_equality();

            assert_eq!(reduced_with_strategy.result, Ok("world".to_string()))
        }

        #[test]
        fn should_be_consistent_if_only_one_http_error_and_two_inconsistent_ok() {
            let results = vec![
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                    Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                        code: RejectionCode::CanisterReject,
                        message: "reject".to_string(),
                    })),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                    Ok("world".to_string()),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                    Ok("goodbye".to_string()),
                ),
            ];

            let reduced_results: ReducedResult<String> =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(results));

            let reduced_with_strategy = reduced_results.reduce_with_equality();

            assert_eq!(
                reduced_with_strategy.result,
                Err(MultiCallError::InconsistentResults(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Err(SingleCallError::HttpOutcallError(
                            HttpOutcallError::IcError {
                                code: RejectionCode::CanisterReject,
                                message: "reject".to_string(),
                            }
                        )),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                        Ok("world".to_string()),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok("goodbye".to_string()),
                    ),
                ]))
            )
        }
    }

    mod reduce_with_min_by_key {
        use super::*;
        use crate::{
            numeric::{BlockNumber, Wei},
            rpc_client::ReducedResult,
            rpc_declarations::Block,
        };

        #[test]
        fn should_get_minimum_block_number() {
            let results = vec![
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                    Ok(Block {
                        number: BlockNumber::new(0x411cda),
                        base_fee_per_gas: Wei::new(0x10),
                    }),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                    Ok(Block {
                        number: BlockNumber::new(0x411cd9),
                        base_fee_per_gas: Wei::new(0x10),
                    }),
                ),
            ];

            let reduced_result =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(results));

            let reduced_with_strategy = reduced_result.reduce_with_min_by_key(|block| block.number);

            assert_eq!(
                reduced_with_strategy.result,
                Ok(Block {
                    number: BlockNumber::new(0x411cd9),
                    base_fee_per_gas: Wei::new(0x10),
                })
            );
        }
    }

    mod reduce_with_stable_majority_by_key {
        use super::*;

        use crate::{
            numeric::{BlockNumber, WeiPerGas},
            rpc_client::{MultiCallError, ReducedResult},
            rpc_declarations::FeeHistory,
        };
        use ic_cdk::api::call::RejectionCode;

        #[test]
        fn should_get_unanimous_fee_history() {
            let results = vec![
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                    Ok(fee_history()),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                    Ok(fee_history()),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::BlockPi),
                    Ok(fee_history()),
                ),
            ];
            let reduced_result =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(results));
            let reduced_with_strategy = reduced_result
                .reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

            assert_eq!(reduced_with_strategy.result, Ok(fee_history()));
        }

        #[test]
        fn should_get_fee_history_with_2_out_of_3() {
            for index_non_majority in 0..3_usize {
                let index_majority = (index_non_majority + 1) % 3;
                let mut fees = [fee_history(), fee_history(), fee_history()];
                fees[index_non_majority].oldest_block = BlockNumber::new(0x10f73fd);
                assert_ne!(
                    fees[index_non_majority].oldest_block,
                    fees[index_majority].oldest_block
                );
                let majority_fee = fees[index_majority].clone();
                let [ankr_fee_history, alchemy_fee_history, public_node_fee_history] = fees;
                let reduced_results =
                    ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                        (
                            EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                            Ok(ankr_fee_history),
                        ),
                        (
                            EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                            Ok(alchemy_fee_history),
                        ),
                        (
                            EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                            Ok(public_node_fee_history),
                        ),
                    ]));

                let reduced_with_strategy = reduced_results
                    .reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

                assert_eq!(reduced_with_strategy.result, Ok(majority_fee));
            }
        }

        #[test]
        fn should_get_fee_history_with_2_out_of_3_when_third_is_error() {
            let reduced_results =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(fee_history()),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                        Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                            code: RejectionCode::CanisterReject,
                            message: "reject".to_string(),
                        })),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(fee_history()),
                    ),
                ]));

            let reduced_with_strategy = reduced_results
                .reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

            assert_eq!(reduced_with_strategy.result, Ok(fee_history()));
        }

        #[test]
        fn should_fail_when_no_strict_majority() {
            let ankr_fee_history = FeeHistory {
                oldest_block: BlockNumber::new(0x10f73fd),
                ..fee_history()
            };
            let alchemy_fee_history = FeeHistory {
                oldest_block: BlockNumber::new(0x10f73fc),
                ..fee_history()
            };
            let public_node_fee_history = FeeHistory {
                oldest_block: BlockNumber::new(0x10f73fe),
                ..fee_history()
            };

            let three_distinct_results =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(ankr_fee_history.clone()),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                        Ok(alchemy_fee_history.clone()),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(public_node_fee_history.clone()),
                    ),
                ]));

            let reduced = three_distinct_results
                .reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

            assert_eq!(
                reduced.result,
                Err(MultiCallError::InconsistentResults(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(public_node_fee_history.clone())
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(ankr_fee_history.clone())
                    ),
                ]))
            );

            let two_distinct_results =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(ankr_fee_history.clone()),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(public_node_fee_history.clone()),
                    ),
                ]));

            let reduced = two_distinct_results
                .reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

            assert_eq!(
                reduced.result,
                Err(MultiCallError::InconsistentResults(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(public_node_fee_history.clone())
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(ankr_fee_history.clone())
                    ),
                ]))
            );

            let two_distinct_results_and_error: ReducedResult<FeeHistory> =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(ankr_fee_history.clone()),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Alchemy),
                        Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                            code: RejectionCode::CanisterReject,
                            message: "reject".to_string(),
                        })),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(public_node_fee_history.clone()),
                    ),
                ]));

            let reduced = two_distinct_results_and_error
                .reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

            assert_eq!(
                reduced.result,
                Err(MultiCallError::InconsistentResults(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(public_node_fee_history.clone())
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(ankr_fee_history.clone())
                    ),
                ]))
            );
        }

        #[test]
        fn should_fail_when_fee_history_inconsistent_for_same_oldest_block() {
            let (fee, inconsistent_fee) = {
                let fee = fee_history();
                let mut inconsistent_fee = fee.clone();
                inconsistent_fee.base_fee_per_gas[0] = WeiPerGas::new(0x729d3f3b4);
                assert_ne!(fee, inconsistent_fee);
                (fee, inconsistent_fee)
            };

            let results = ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                    Ok(fee.clone()),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                    Ok(inconsistent_fee.clone()),
                ),
            ]));

            let reduced =
                results.reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

            assert_eq!(
                reduced.result,
                Err(MultiCallError::InconsistentResults(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(fee.clone())
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Ok(inconsistent_fee.clone())
                    ),
                ]))
            );
        }

        #[test]
        fn should_fail_when_no_sufficient_ok_responses() {
            let results = ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                    Ok(fee_history()),
                ),
                (
                    EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                    Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                        code: RejectionCode::CanisterReject,
                        message: "reject".to_string(),
                    })),
                ),
            ]));

            let reduced =
                results.reduce_with_strict_majority_by_key(|fee_history| fee_history.oldest_block);

            assert_eq!(
                reduced.result,
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::Ankr),
                        Ok(fee_history()),
                    ),
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                            code: RejectionCode::CanisterReject,
                            message: "reject".to_string(),
                        })),
                    ),
                ]))
                .result
            );
        }

        fn fee_history() -> FeeHistory {
            FeeHistory {
                oldest_block: BlockNumber::new(0x10f73fc),
                base_fee_per_gas: vec![
                    WeiPerGas::new(0x729d3f3b3),
                    WeiPerGas::new(0x766e503ea),
                    WeiPerGas::new(0x75b51b620),
                    WeiPerGas::new(0x74094f2b4),
                    WeiPerGas::new(0x716724f03),
                    WeiPerGas::new(0x73b467f76),
                ],
                reward: vec![
                    vec![WeiPerGas::new(0x5f5e100)],
                    vec![WeiPerGas::new(0x55d4a80)],
                    vec![WeiPerGas::new(0x5f5e100)],
                    vec![WeiPerGas::new(0x5f5e100)],
                    vec![WeiPerGas::new(0x5f5e100)],
                ],
            }
        }
    }

    mod has_http_outcall_error_matching {
        use super::*;
        use crate::rpc_client::{MultiCallError, ReducedResult};
        use evm_rpc_types::{HttpOutcallError, JsonRpcError};
        use ic_cdk::api::call::RejectionCode;
        use proptest::prelude::any;
        use proptest::proptest;

        proptest! {
            #[test]
            fn should_not_match_when_consistent_json_rpc_error(code in any::<i64>(), message in ".*") {
                let error: MultiCallError<String> = MultiCallError::ConsistentJsonRpcError {
                    code,
                    message,
                };
                let always_true = |_outcall_error: &HttpOutcallError| true;

                assert!(!error.has_http_outcall_error_matching(always_true));
            }
        }

        #[test]
        fn should_match_when_consistent_http_outcall_error() {
            let error: MultiCallError<String> =
                MultiCallError::ConsistentHttpOutcallError(HttpOutcallError::IcError {
                    code: RejectionCode::SysTransient,
                    message: "message".to_string(),
                });
            let always_true = |_outcall_error: &HttpOutcallError| true;
            let always_false = |_outcall_error: &HttpOutcallError| false;

            assert!(error.has_http_outcall_error_matching(always_true));
            assert!(!error.has_http_outcall_error_matching(always_false));
        }

        #[test]
        fn should_match_on_single_inconsistent_result_with_outcall_error() {
            let always_true = |_outcall_error: &HttpOutcallError| true;
            let error_with_no_outcall_error: ReducedResult<u128> =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Err(EvmRpcError::JsonRpcError(JsonRpcError {
                            code: -32700,
                            message: "error".to_string(),
                        })),
                    ),
                    (EvmRpcService::EthSepolia(EthSepoliaService::Alchemy), Ok(1)),
                ]));

            assert!(!error_with_no_outcall_error
                .result
                .err()
                .unwrap()
                .has_http_outcall_error_matching(always_true));

            let error_with_outcall_error =
                ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                    (
                        EvmRpcService::EthSepolia(EthSepoliaService::PublicNode),
                        Err(EvmRpcError::HttpOutcallError(HttpOutcallError::IcError {
                            code: RejectionCode::SysTransient,
                            message: "message".to_string(),
                        })),
                    ),
                    (EvmRpcService::EthSepolia(EthSepoliaService::Alchemy), Ok(1)),
                ]));
            assert!(error_with_outcall_error
                .result
                .err()
                .unwrap()
                .has_http_outcall_error_matching(always_true));
        }
    }
}

mod get_transaction_receipt {
    use crate::{
        numeric::{BlockNumber, GasAmount, WeiPerGas},
        rpc_declarations::{Hash, TransactionReceipt, TransactionStatus},
    };
    use assert_matches::assert_matches;
    use proptest::proptest;
    use std::str::FromStr;

    #[test]
    fn should_deserialize_transaction_receipt() {
        const RECEIPT: &str = r#"{
        "transactionHash": "0x0e59bd032b9b22aca5e2784e4cf114783512db00988c716cf17a1cc755a0a93d",
        "blockHash": "0x82005d2f17b251900968f01b0ed482cb49b7e1d797342bc504904d442b64dbe4",
        "blockNumber": "0x4132ec",
        "logs": [],
        "contractAddress": null,
        "effectiveGasPrice": "0xfefbee3e",
        "cumulativeGasUsed": "0x8b2e10",
        "from": "0x1789f79e95324a47c5fd6693071188e82e9a3558",
        "gasUsed": "0x5208",
        "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        "status": "0x01",
        "to": "0xdd2851cdd40ae6536831558dd46db62fac7a844d",
        "transactionIndex": "0x32",
        "type": "0x2"
    }"#;

        let receipt: TransactionReceipt = serde_json::from_str(RECEIPT).unwrap();
        assert_eq!(
            receipt,
            TransactionReceipt {
                block_hash: Hash::from_str(
                    "0x82005d2f17b251900968f01b0ed482cb49b7e1d797342bc504904d442b64dbe4"
                )
                .unwrap(),
                block_number: BlockNumber::new(0x4132ec),
                effective_gas_price: WeiPerGas::new(0xfefbee3e),
                gas_used: GasAmount::new(0x5208),
                status: TransactionStatus::Success,
                transaction_hash: Hash::from_str(
                    "0x0e59bd032b9b22aca5e2784e4cf114783512db00988c716cf17a1cc755a0a93d"
                )
                .unwrap(),
            }
        )
    }

    #[test]
    fn should_deserialize_transaction_status() {
        let status: TransactionStatus = serde_json::from_str("\"0x01\"").unwrap();
        assert_eq!(status, TransactionStatus::Success);

        // some providers do not return a full byte (2 hex digits) for the status
        let status: TransactionStatus = serde_json::from_str("\"0x1\"").unwrap();
        assert_eq!(status, TransactionStatus::Success);

        let status: TransactionStatus = serde_json::from_str("\"0x0\"").unwrap();
        assert_eq!(status, TransactionStatus::Failure);

        let status: TransactionStatus = serde_json::from_str("\"0x00\"").unwrap();
        assert_eq!(status, TransactionStatus::Failure);
    }

    #[test]
    fn should_deserialize_serialized_transaction_status() {
        let status: TransactionStatus =
            serde_json::from_str(&serde_json::to_string(&TransactionStatus::Success).unwrap())
                .unwrap();
        assert_eq!(status, TransactionStatus::Success);

        let status: TransactionStatus =
            serde_json::from_str(&serde_json::to_string(&TransactionStatus::Failure).unwrap())
                .unwrap();
        assert_eq!(status, TransactionStatus::Failure);
    }

    proptest! {
        #[test]
        fn should_fail_deserializing_wrong_transaction_status(wrong_status in 2_u32..u32::MAX) {
            let status = format!("\"0x{:x}\"", wrong_status);
            let error = serde_json::from_str::<TransactionStatus>(&status);
            assert_matches!(error, Err(e) if e.to_string().contains("invalid transaction status"));
        }
    }
}

mod evm_rpc_conversion {
    use crate::numeric::{BlockNumber, TransactionCount, Wei};
    use crate::rpc_client::{
        only_inconsistent_error_results_without_providers,
        only_inconsistent_ok_results_without_providers, TransactionReceipt,
    };
    use crate::rpc_client::{Block, LogEntry, MultiCallError, Reduce, ReducedResult};
    use crate::rpc_declarations::TransactionStatus;
    use crate::test_fixtures::arb::{
        arb_block, arb_evm_rpc_error, arb_hex, arb_hex20, arb_hex256, arb_hex32, arb_hex_byte,
        arb_log_entry, arb_nat_256, arb_transaction_receipt,
    };

    use evm_rpc_types::{
        Block as EvmBlock, EthMainnetService as EvmEthMainnetService, EthSepoliaService, Hex,
        Hex20, Hex32, LogEntry as EvmLogEntry, MultiRpcResult as EvmMultiRpcResult, Nat256,
        RpcError as EvmRpcError, RpcResult as EvmRpcResult, RpcService as EvmRpcService,
        RpcService, TransactionReceipt as EvmTransactionReceipt,
    };
    use proptest::collection::vec;
    use proptest::option;
    use proptest::{prelude::Strategy, prop_assert_eq, proptest};
    use std::collections::BTreeSet;
    use std::fmt::Debug;

    #[test]
    fn should_map_consistent_result() {
        let block = evm_rpc_block();
        let evm_result = EvmMultiRpcResult::Consistent(Ok(block.clone()));

        let reduced_block: Result<_, _> = evm_result.reduce().into();

        assert_eq!(
            reduced_block,
            Ok(Block {
                number: BlockNumber::try_from(block.number).unwrap(),
                base_fee_per_gas: Wei::try_from(block.base_fee_per_gas.unwrap()).unwrap(),
            })
        );
    }

    #[test]
    fn should_map_inconsistent_results() {
        let block = evm_rpc_block();
        let next_block = EvmBlock {
            number: BlockNumber::from(block.number.clone())
                .checked_increment()
                .unwrap()
                .into(),
            ..evm_rpc_block()
        };

        let evm_result = EvmMultiRpcResult::Inconsistent(vec![
            (
                EvmRpcService::EthMainnet(EvmEthMainnetService::Alchemy),
                Ok(block.clone()),
            ),
            (
                EvmRpcService::EthMainnet(EvmEthMainnetService::Ankr),
                Ok(next_block.clone()),
            ),
        ]);

        let reduced_block = evm_result.reduce();

        assert_eq!(
            reduced_block,
            ReducedResult::from_multi_result(EvmMultiRpcResult::Inconsistent(vec![
                (
                    EvmRpcService::EthMainnet(EvmEthMainnetService::Alchemy),
                    Ok(Block {
                        number: BlockNumber::try_from(block.number).unwrap(),
                        base_fee_per_gas: Wei::try_from(block.base_fee_per_gas.unwrap()).unwrap(),
                    }),
                ),
                (
                    EvmRpcService::EthMainnet(EvmEthMainnetService::Ankr),
                    Ok(Block {
                        number: BlockNumber::try_from(next_block.number).unwrap(),
                        base_fee_per_gas: Wei::try_from(next_block.base_fee_per_gas.unwrap())
                            .unwrap(),
                    }),
                ),
            ]))
        );
    }

    proptest! {
        #[test]
        fn should_have_consistent_block_between_minter_and_evm_rpc
        (
            blocks in minter_and_evm_rpc_blocks(),
            first_error in arb_evm_rpc_error(),
            second_error in arb_evm_rpc_error(),
            third_error in arb_evm_rpc_error(),
        ) {
            let (minter_block, evm_rpc_block) = blocks;
            test_consistency_between_minter_and_evm_rpc(minter_block, evm_rpc_block, first_error, second_error, third_error)?;
        }
    }

    proptest! {
        #[test]
        fn should_have_consistent_log_entries_between_minter_and_evm_rpc
        (
            minter_logs in vec(arb_log_entry(), 1..=100),
            first_error in arb_evm_rpc_error(),
            second_error in arb_evm_rpc_error(),
            third_error in arb_evm_rpc_error(),
        ) {
            let evm_rpc_logs: Vec<_> = minter_logs.clone().into_iter().map(evm_rpc_log_entry).collect();
            test_consistency_between_minter_and_evm_rpc(minter_logs, evm_rpc_logs, first_error, second_error, third_error)?;
        }
    }

    proptest! {
        #[test]
        fn should_have_consistent_transaction_receipts_between_minter_and_evm_rpc
        (
            transaction_receipts in minter_and_evm_rpc_transaction_receipts(),
            first_error in arb_evm_rpc_error(),
            second_error in arb_evm_rpc_error(),
            third_error in arb_evm_rpc_error(),
        ) {
            let (minter_tx_receipt, evm_rpc_tx_receipt) = transaction_receipts;
            test_consistency_between_minter_and_evm_rpc(minter_tx_receipt, evm_rpc_tx_receipt, first_error, second_error, third_error)?;
        }
    }

    proptest! {
        #[test]
        fn should_have_consistent_transaction_count_between_minter_and_evm_rpc
        (
            first_tx_count in arb_evm_rpc_transaction_count(),
            second_tx_count in arb_evm_rpc_transaction_count(),
            third_tx_count in arb_evm_rpc_transaction_count(),
        ) {
            let (ankr_evm_rpc_provider, public_node_evm_rpc_provider, alchemy_nodes_evm_rpc_provider) =
                evm_rpc_providers();
            let evm_results :ReducedResult<TransactionCount>= match (&first_tx_count, &second_tx_count, &third_tx_count) {
                (Ok(count_1), Ok(count_2), Ok(count_3)) if count_1 == count_2 && count_2 == count_3 => {
                    EvmMultiRpcResult::Consistent(Ok(count_1.clone()))
                }
                _ => EvmMultiRpcResult::Inconsistent(vec![
                    (ankr_evm_rpc_provider.clone(), first_tx_count.clone()),
                    (public_node_evm_rpc_provider.clone(), second_tx_count.clone()),
                    (alchemy_nodes_evm_rpc_provider.clone(), third_tx_count.clone()),
                ]),
            }.reduce();
            let minter_results : ReducedResult<TransactionCount>= EvmMultiRpcResult::Inconsistent(vec![
                (ankr_evm_rpc_provider, first_tx_count),
                (public_node_evm_rpc_provider, second_tx_count),
                (alchemy_nodes_evm_rpc_provider, third_tx_count),
            ]).reduce();


            prop_assert_eq_ignoring_provider
            (
                evm_results.clone().reduce_with_equality().result,
                minter_results.clone().reduce_with_equality().result,
            )?;
            prop_assert_eq_ignoring_provider
            (
                evm_results.clone().reduce_with_min_by_key(|transaction_count| *transaction_count).result,
                minter_results.clone().reduce_with_min_by_key(|transaction_count| *transaction_count).result,
            )?;
        }
    }

    fn test_consistency_between_minter_and_evm_rpc<M, E>(
        minter_ok: M,
        evm_rpc_ok: E,
        first_error: EvmRpcError,
        second_error: EvmRpcError,
        third_error: EvmRpcError,
    ) -> Result<(), proptest::prelude::TestCaseError>
    where
        M: Clone + Debug + PartialEq + serde::Serialize,
        E: Clone + Debug,
        EvmMultiRpcResult<E>: Reduce<Item = M>,
        EvmMultiRpcResult<M>: Reduce<Item = M>,
    {
        let (ankr_evm_rpc_provider, public_node_evm_rpc_provider, alchemy_nodes_evm_rpc_provider) =
            evm_rpc_providers();

        // 0 error
        let evm_result = EvmMultiRpcResult::Consistent(Ok(evm_rpc_ok.clone())).reduce();
        let minter_result: ReducedResult<M> =
            EvmMultiRpcResult::Consistent(Ok(minter_ok.clone())).reduce();
        prop_assert_eq!(evm_result.result, minter_result.result);

        // 1 error
        for first_error_index in 0..3_usize {
            let mut evm_results = vec![
                (ankr_evm_rpc_provider.clone(), Ok(evm_rpc_ok.clone())),
                (public_node_evm_rpc_provider.clone(), Ok(evm_rpc_ok.clone())),
                (
                    alchemy_nodes_evm_rpc_provider.clone(),
                    Ok(evm_rpc_ok.clone()),
                ),
            ];
            evm_results.get_mut(first_error_index).unwrap().1 = Err(first_error.clone());
            let evm_result = EvmMultiRpcResult::Inconsistent(evm_results).reduce();

            let mut minter_results = vec![
                (ankr_evm_rpc_provider.clone(), Ok(minter_ok.clone())),
                (public_node_evm_rpc_provider.clone(), Ok(minter_ok.clone())),
                (
                    alchemy_nodes_evm_rpc_provider.clone(),
                    Ok(minter_ok.clone()),
                ),
            ];
            minter_results.get_mut(first_error_index).unwrap().1 = Err(first_error.clone());
            let minter_result = EvmMultiRpcResult::Inconsistent(minter_results).reduce();

            prop_assert_eq!(evm_result.result, minter_result.result);
        }

        // 2 errors
        for ok_index in 0..3_usize {
            let mut evm_results = vec![
                (ankr_evm_rpc_provider.clone(), Err(first_error.clone())),
                (
                    public_node_evm_rpc_provider.clone(),
                    Err(second_error.clone()),
                ),
                (
                    alchemy_nodes_evm_rpc_provider.clone(),
                    Err(third_error.clone()),
                ),
            ];
            evm_results.get_mut(ok_index).unwrap().1 = Ok(evm_rpc_ok.clone());
            let evm_result = EvmMultiRpcResult::Inconsistent(evm_results).reduce();

            let mut minter_results = vec![
                (ankr_evm_rpc_provider.clone(), Err(first_error.clone())),
                (
                    public_node_evm_rpc_provider.clone(),
                    Err(second_error.clone()),
                ),
                (
                    alchemy_nodes_evm_rpc_provider.clone(),
                    Err(third_error.clone()),
                ),
            ];
            minter_results.get_mut(ok_index).unwrap().1 = Ok(minter_ok.clone());

            let minter_result = EvmMultiRpcResult::Inconsistent(minter_results).reduce();

            prop_assert_eq_ignoring_provider(
                evm_result.result.clone(),
                minter_result.result.clone(),
            )?;
        }

        // 3 errors
        let evm_result: ReducedResult<M> = EvmMultiRpcResult::Inconsistent::<E>(vec![
            (ankr_evm_rpc_provider.clone(), Err(first_error.clone())),
            (
                public_node_evm_rpc_provider.clone(),
                Err(second_error.clone()),
            ),
            (
                alchemy_nodes_evm_rpc_provider.clone(),
                Err(third_error.clone()),
            ),
        ])
        .reduce();
        let minter_result = EvmMultiRpcResult::Inconsistent::<M>(vec![
            (ankr_evm_rpc_provider.clone(), Err(first_error.clone())),
            (
                public_node_evm_rpc_provider.clone(),
                Err(second_error.clone()),
            ),
            (
                alchemy_nodes_evm_rpc_provider.clone(),
                Err(third_error.clone()),
            ),
        ])
        .reduce();

        prop_assert_eq_ignoring_provider(evm_result.result.clone(), minter_result.result.clone())?;

        Ok(())
    }

    fn prop_assert_eq_ignoring_provider<T: PartialEq + Debug + serde::Serialize + Clone>(
        left: Result<T, MultiCallError<T>>,
        right: Result<T, MultiCallError<T>>,
    ) -> Result<(), proptest::prelude::TestCaseError> {
        let left = left.as_ref();
        let right = right.as_ref();
        match left {
            Ok(_) => {
                prop_assert_eq!(left, right)
            }
            Err(e) => match e {
                MultiCallError::ConsistentHttpOutcallError(_)
                | MultiCallError::ConsistentJsonRpcError { .. }
                | MultiCallError::ConsistentEvmRpcCanisterError(_) => {
                    prop_assert_eq!(left, right)
                }
                MultiCallError::InconsistentResults(left_inconsistent_results) => {
                    let right_inconsistent_results = match right {
                        Err(MultiCallError::InconsistentResults(results)) => results,
                        _ => panic!("Expected inconsistent results"),
                    };
                    // Providers are used as keys for MultiCallResults::ok_results and MultiCallResults::errors,
                    // so since we want to ignore them, it makes sense to also ignore the order of the values,
                    // since different providers have different orderings.
                    prop_assert_eq!(
                        // It generally doesn't make sense for `T` to implement `Ord`,
                        // but in this context it can always be serialized to JSON,
                        // which we use for comparison purposes.
                        only_inconsistent_ok_results_without_providers(left_inconsistent_results)
                            .into_iter()
                            .map(|v| serde_json::to_string(&v).unwrap())
                            .collect::<BTreeSet<_>>(),
                        only_inconsistent_ok_results_without_providers(right_inconsistent_results)
                            .into_iter()
                            .map(|v| serde_json::to_string(&v).unwrap())
                            .collect::<BTreeSet<_>>()
                    );
                    prop_assert_eq!(
                        only_inconsistent_error_results_without_providers(
                            left_inconsistent_results
                        )
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                        only_inconsistent_error_results_without_providers(
                            right_inconsistent_results
                        )
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                    );
                }
            },
        }
        Ok(())
    }

    fn evm_rpc_providers() -> (RpcService, RpcService, RpcService) {
        let ankr_evm_rpc_provider = EvmRpcService::EthSepolia(EthSepoliaService::Ankr);
        let public_node_evm_rpc_provider = EvmRpcService::EthSepolia(EthSepoliaService::PublicNode);
        let alchemy_evm_rpc_provider = EvmRpcService::EthSepolia(EthSepoliaService::Ankr);
        (
            ankr_evm_rpc_provider,
            public_node_evm_rpc_provider,
            alchemy_evm_rpc_provider,
        )
    }
    pub fn minter_and_evm_rpc_blocks() -> impl Strategy<Value = (Block, EvmBlock)> {
        use proptest::prelude::Just;
        arb_block().prop_flat_map(|minter_block| {
            (Just(minter_block.clone()), arb_evm_rpc_block(minter_block))
        })
    }

    pub fn arb_evm_rpc_block(minter_block: Block) -> impl Strategy<Value = EvmBlock> {
        use proptest::{array, collection::vec};
        //prop_map is limited to tuples of at most 11 elements, so we group the Nat and String fields
        (
            array::uniform2(option::of(arb_nat_256())),
            array::uniform5(arb_nat_256()),
            arb_hex(),
            array::uniform6(arb_hex32()),
            arb_hex256(),
            arb_hex20(),
            proptest::option::of(arb_hex32()),
            array::uniform2(vec(arb_hex32(), 0..10)),
        )
            .prop_map(
                move |(
                    [difficulty, total_difficulty],
                    [gas_limit, gas_used, nonce, size, timestamp],
                    extra_data,
                    [hash, mix_hash, parent_hash, receipts_root, sha3_uncles, state_root],
                    logs_bloom,
                    miner,
                    transactions_root,
                    [transactions, uncles],
                )| EvmBlock {
                    base_fee_per_gas: Some(Nat256::from(minter_block.base_fee_per_gas)),
                    number: Nat256::from(minter_block.number),
                    difficulty,
                    extra_data,
                    gas_limit,
                    gas_used,
                    hash,
                    logs_bloom,
                    miner,
                    mix_hash,
                    nonce,
                    parent_hash,
                    receipts_root,
                    sha3_uncles,
                    size,
                    state_root,
                    timestamp,
                    total_difficulty,
                    transactions,
                    transactions_root,
                    uncles,
                },
            )
    }

    fn evm_rpc_log_entry(minter_log_entry: LogEntry) -> EvmLogEntry {
        EvmLogEntry {
            address: Hex20::from(minter_log_entry.address.into_bytes()),
            topics: minter_log_entry
                .topics
                .into_iter()
                .map(|topic| Hex32::from(topic.0))
                .collect(),
            data: Hex::from(minter_log_entry.data.0),
            block_number: minter_log_entry.block_number.map(Nat256::from),
            transaction_hash: minter_log_entry
                .transaction_hash
                .map(|hash| Hex32::from(hash.0)),
            transaction_index: minter_log_entry
                .transaction_index
                .map(|q| Nat256::from_be_bytes(q.to_be_bytes())),
            block_hash: minter_log_entry.block_hash.map(|hash| Hex32::from(hash.0)),
            log_index: minter_log_entry.log_index.map(Nat256::from),
            removed: minter_log_entry.removed,
        }
    }

    fn evm_rpc_block() -> EvmBlock {
        EvmBlock {
            base_fee_per_gas: Some(8_876_901_983_u64.into()),
            number: 20_061_336_u32.into(),
            difficulty: Some(0_u8.into()),
            extra_data: "0xd883010d0e846765746888676f312e32312e36856c696e7578".parse().unwrap(),
            gas_limit: 30_000_000_u32.into(),
            gas_used: 2_858_256_u32.into(),
            hash: "0x3a68e81a96d436f421b7cae6a66f78f6aef075340edaec5c7c1db0919c0f909b".parse().unwrap(),
            logs_bloom: "0x006000060010410010180000940006000000200040006108008801008022000900a005820000001100000300000d058962202900084080a0000031080022800000480c08100000006800000a20002028841080209044003041000940802448100002002a820085000000008400200d40204c10110810040403000210020004000a20208028104110a48429100033080e000040050501004800850042405230204230800000a0202282019080040040090a858000014014800440000208000008081804124002800030002040080610c000050002502000100005000a08002000001020500100804612440042300c0080040812000a1208420108200000000045".parse().unwrap(),
            miner: "0xd2732e3e4c264ab330af53f661f6da91cbbb594a".parse().unwrap(),
            mix_hash: "0x472d18a0b90d7007028dded03d7ef9923c2a7fc60f7e276bc6928fa9aeb6cbe8".parse().unwrap(),
            nonce: 0_u8.into(),
            parent_hash: "0xc0debe594704702ec9c2e5a56595ccbc285305108286a6a19aa33f8b3755da65".parse().unwrap(),
            receipts_root: "0x54179d043f2fe97f122a01366cd6ad18868501253282575fb00cada3fecf8fe1".parse().unwrap(),
            sha3_uncles: "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347".parse().unwrap(),
            size: 17_484_u32.into(),
            state_root: "0x1e25cbd8eb25aadda3da160fd9b3fd46dfae61d7df1097d7990ca420e5c7c608".parse().unwrap(),
            timestamp: 1_718_021_363_u32.into(),
            total_difficulty: Some(58_750_003_716_598_352_816_469_u128.into()),
            transactions: vec![
                "0x5f17526ee5ab415ed44aa3788f0e8154230faa50f8b6d547a95858a8a90f259e",
                "0x1d0d559a2e113a4a4b738c97536c48e4a047a491614ddefe77c6e0f25b9e3a42",
                "0xd8c4f005fd4c7832205f6eda9bfde5bc5f9e0a1002b325d02348889f39e21850",
                "0xee14faac7f1d05a71ce69b11116a2ed8bf7a020a7b81a6a7a82096fdea7823a5",
                "0x63725de23700e115a48cb969a9e26eea56a65a971d63a21cc9cc660aa0cf4204",
                "0x77cbe1a9c3aef1ee9f345de7c189e9631e5458b194ba91ab2d9dc6d625e7eb68",
                "0x0e3403dcc6dea9dec03203ed9b1b89c66fd606abe3ac8bb33ed440283e5444cb",
                "0x91935e9885348f1ec4d673532c4f6709a28298f804b8054dea406407b00566af",
                "0x728b9eab683e4a59e75ebe03f1f9cdf081c04bc57f505cd8fc157a282e299c08",
                "0xb00dfcae52ef97f4f80965603f7c3a4c7f8c58e3e12caf6b636a522d0fbfef86",
                "0x604737ccc8f69cd4c1cd4c1e8f62655272d0a6db98923e907a5b0404d1822df4",
                "0x079ffeb1040d2490e248eb85047422bf3519c5fb5e3632ec3f245217c540a4b1",
                "0xd0c5a03b82d2b7cb62be59cb691cf5f6b0940b433360545e23e55044741f51dd",
                "0xe5707c1a13739613acec865053b88a03d7417004dec6086b544d92f4d9235880",
                "0x8f8541fa86b636d26b620c15741095e2920c27545b4b42efde1e15a702f99a00",
                "0x763b7f0bde974974d96e2ae1c9bee1bea8841acebf7c188477b198c93022f797",
                "0x9e518c8ced080b6d25836b506a5424ff98ca1933637e7586dd9464c48930880a",
                "0x08467c33ab74e9a379d63cbb1a03097c7cde7f85a166e60804c855cfd8bdcb96",
                "0x38928c665e5c62509066deaffcc94d497928b26bfef33d570a92b09af3a6cbbd",
                "0x2c616b1f2aa52a5f481d8aa5ebe0991f1f03d5c676f1b92cd03496ce753a5ae2",
                "0x3a4cf1999fe714e2be26f12a05270d85bb2033ee787727b94e5a7a3494e45f59",
                "0x8b3fc42aa0de7d0a181829229bc6ec8a5dd6c5d096945c0a2d149dd48a38e94a",
                "0xf1a3521cb1c73ae3bf5af18e25fdff023adabeea83503f73ca8721ce6ea27bfa",
                "0xff3265ddf367f97b50f95e4295bd101914fced55677315dee6c7618e31c721b6",
                "0xe6cc4470987f866cbddfe8e47a069a803fbda1b71055c49e96e78bdbe0cf1462",
                "0xccb8d52db4861b571240d71a58ba6cf8ea8e06567b82d68d517d383753cd8c65",
                "0x7c620a3c26299632c513f3940aae5dc971d1bedc93f669482e760cf4a86e25ee",
                "0xc2b265b37be476a291c87f10913960fe7ac790796248fb07e39fa40502e9fc03",
                "0x78083d9907ab4136e7df0cc266e4a6cddc4cf9e62948d5ab6bf81821ed26f45e",
                "0xf3776413512018e401b49b4661ecfd3f6daabe4aa52b3ae158ef8f10be424ca1",
                "0x53bc3267ef9f8f5a2d7be33f111391cbee7f13390de9bd531f5f216eef13582d",
                "0x6fc125dda0b34acd12f72fc5980fa5250ed1cfa985e60f5535123e7bfe82baca",
                "0xf9ace1b33ed117617cdae76a79a8fa758a8f3817c3aaf245a94953f565001d8a",
                "0xb186f79d1d6218ce61715f579ae2bde0582dede16d0ef9cf1cd85735f05445ea",
                "0x75e69b143d0fb26e4113c2dd0c2f702b2e917b5c23d81aaf587243525ef56e5a",
                "0xe6595bcb2ae614d890d38d153057826c3ad08063332276fa1b16e5f13b06e7a2",
                "0xd473fc760fb6cd8c81b7fe26e1bb6114d95957be22a713e1aac2cc63c2a3f0a3",
                "0x132d23074d8442c60337018bba749e0479f49b3d99a449934031289de6bd4587",
                "0xcead5cec4d5a30b28af721d8efbf77f05261daf76f60bc36298dbdc2793af703",
                "0x8b5b553313660e25a9a357c050576122e6d697314b1044f19f076df0d33f9823",
                "0xd73e844cd930c7463023fcc2eab8e40de90a5476f1c69d9466f506ec0a1c6953",
                "0x70bf1aed5af719155b036b0d761b86610e22855f60279982d1ca83c2c1493861",
                "0x5c2f23360e5247942d0b5150745cb4d8692de92e0fcb3cdfedff0341ff1f3a8e",
                "0x1c2eaceb326006f77142e3ffacc660d17b5b1ccf0ef2d22026149b9973d03752",
                "0x27f087175f96f9169e5e5320dffc920bab0181958df8385a143ac1ce9b7703a5",
                "0x672608a35f4fa4bb65955138521a887a892b0cd35d09f0359d00fdfa5cf427fd",
                "0x3b8942ca076f4e4e3e6222b577da88d888c79768d337bef14de6d75ba2540d11",
                "0x7e1614b107c5a7adc7478198b2d99d3dee48e443f1f475524479aee0a4c9e402",
                "0x5f9c5284a47ed5a6f6e672d48fea29966b3d91d63487ab47bc8f5514f231e687",
                "0x3715bb37c438c4e95fab950f573d184770faf8019018d2b47d6220003f0b35d0",
                "0x33137040d80df84243b63833eea5b34a505a2ca8fb1a34318b74cecf5f4aa7c8",
                "0x470940a47746125aae7513cb22bdac628865ee3df34e99bd0ecd48ff23b47f41",
                "0x875c9fda2e0ccffde973385ee72d106f1fea12fda8d250f55a85007e13422e40",
                "0xd3a08793b023ff2eb5c3e1d9b90254172a796095972d8dc2714cc094f6fc6c19",
                "0x135366e9141a1b871e73941f00c2e321b4ab51c99d58b95f1b201f30c3f7d0d2",
                "0xc93ec0af7511a39dfe389fb37d21144914c99ddc8d259e47146e8b45d288e8f8",
                "0x6ba2a677ff759be8e76f42e1b5d009b5a39f186fa417f00679105059b4cc725c",
                "0x8657b391f8575ab4f7323a5e24e3ca53df61cb433cf88cbef40000c05badedc7",
                "0x6e14d76d37b4dab55b5e49276b207b0e4f117ef8103728f8dadc487996e30c34",
                "0xac4489a73246f8f81503e11254003158893785ae4a603eedddec8b23945d3630",
                "0x50b5e07019621c041d061df0dc447674d719391862238c25181fd45f4bea441c",
                "0x424431243694085158cdcf5ed1666b88421fb3c7fde538acf36f8ea8316d827b",
                "0xf1d5e8256194f29e7da773ea8ef9e60ba7c5ceb7fb9ab9966d2c7b53d4c347ff",
                "0x25f85c5fcda53d733bf0dafe552984b0e17e5202fe9225a9a1bf94b50575e5d8",
                "0xe2499f7bbc8acdc3f273ac29f7757071844b739d2a84ab19440a9b1a3cbe901d",
                "0x25525be1316671638e2b6146f3e3259be8dee11cf8a24cb64b0feb2ad7f1ebf9",
                "0x0518268fb4b06a1285997efb841615a74d113571332ac7c935d2a303ca1d6f23",
                "0x1510c9bf4678ec3e67d05c908ba6d2762c4a815476638cc1d281d65a7dab6745"
            ].into_iter().map(|s| s.parse().unwrap()).collect(),
            transactions_root: Some("0xdee0b25a965ff236e4d2e89f56de233759d71ad3e3e150ceb4cf5bb1f0ecf5c0".parse().unwrap()),
            uncles: vec![],
        }
    }

    fn minter_and_evm_rpc_transaction_receipts(
    ) -> impl Strategy<Value = (Option<TransactionReceipt>, Option<EvmTransactionReceipt>)> {
        use proptest::{option, prelude::Just};
        option::of(arb_transaction_receipt()).prop_flat_map(|minter_tx_receipt| {
            (
                Just(minter_tx_receipt.clone()),
                arb_evm_rpc_transaction_receipt(minter_tx_receipt),
            )
        })
    }

    fn arb_evm_rpc_transaction_receipt(
        minter_tx_receipt: Option<TransactionReceipt>,
    ) -> impl Strategy<Value = Option<EvmTransactionReceipt>> {
        use proptest::{collection::vec, option, prelude::Just};

        match minter_tx_receipt {
            None => Just(None).boxed(),
            Some(r) => (
                option::of(arb_hex20()),
                arb_hex20(),
                vec(arb_log_entry(), 1..=100),
                arb_hex256(),
                option::of(arb_hex20()),
                arb_nat_256(),
                arb_hex_byte(),
            )
                .prop_map(
                    move |(
                        contract_address,
                        from,
                        minter_logs,
                        logs_bloom,
                        to,
                        transaction_index,
                        tx_type,
                    )| {
                        Some(EvmTransactionReceipt {
                            block_hash: Hex32::from(r.block_hash.0),
                            block_number: r.block_number.into(),
                            effective_gas_price: r.effective_gas_price.into(),
                            gas_used: r.gas_used.into(),
                            status: Some(match r.status {
                                TransactionStatus::Success => Nat256::from(1_u8),
                                TransactionStatus::Failure => Nat256::ZERO,
                            }),
                            transaction_hash: Hex32::from(r.transaction_hash.0),
                            contract_address,
                            from,
                            logs: minter_logs.into_iter().map(evm_rpc_log_entry).collect(),
                            logs_bloom,
                            to,
                            transaction_index,
                            tx_type,
                        })
                    },
                )
                .boxed(),
        }
    }

    fn arb_evm_rpc_transaction_count() -> impl Strategy<Value = EvmRpcResult<Nat256>> {
        proptest::result::maybe_ok(arb_nat_256(), arb_evm_rpc_error())
    }
}
