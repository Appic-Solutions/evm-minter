use crate::evm_config::EvmNetwork;
use crate::storage::get_rpc_api_key;
use evm_rpc_client::evm_rpc_types::{RpcApi, RpcServices};
use minicbor::{Decode, Encode};

#[derive(Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Provider {
    #[n(0)]
    Ankr,
    #[n(1)]
    LlamaNodes,
    #[n(2)]
    PublicNode,
    #[n(3)]
    DRPC,
    #[n(4)]
    Alchemy,
}

impl Provider {
    pub fn get_url_with_api_key(&self, url: &str) -> String {
        match get_rpc_api_key(*self) {
            Some(api_key) => format!("{}{}", url, api_key),
            None => url.to_string(),
        }
    }
}

struct NetworkConfig {
    ankr_url: &'static str,
    llama_nodes_url: Option<&'static str>,
    public_node_url: &'static str,
    drpc_url: &'static str,
    alchemy_url: &'static str,
}

fn get_network_config(network: EvmNetwork) -> NetworkConfig {
    match network {
        EvmNetwork::Ethereum => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/eth/",
            llama_nodes_url: Some("https://eth.llamarpc.com/"),
            public_node_url: "https://ethereum-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=ethereum&dkey=",
            alchemy_url: "https://eth-mainnet.g.alchemy.com/v2/",
        },
        EvmNetwork::Sepolia => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/eth_sepolia/",
            llama_nodes_url: None,
            public_node_url: "https://ethereum-sepolia-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=sepolia&dkey=",
            alchemy_url: "https://eth-sepolia.g.alchemy.com/v2/",
        },
        EvmNetwork::ArbitrumOne => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/arbitrum/",
            llama_nodes_url: Some("https://arbitrum.llamarpc.com/"),
            public_node_url: "https://arbitrum-one-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=arbitrum&dkey=",
            alchemy_url: "https://arb-mainnet.g.alchemy.com/v2/",
        },
        EvmNetwork::BSC => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/bsc/",
            llama_nodes_url: Some("https://binance.llamarpc.com/"),
            public_node_url: "https://bsc-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=bsc&dkey=",
            alchemy_url: "https://bnb-mainnet.g.alchemy.com/v2/",
        },
        EvmNetwork::BSCTestnet => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/bsc_testnet_chapel/",
            llama_nodes_url: None,
            public_node_url: "https://bsc-testnet-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=bsc-testnet&dkey=",
            alchemy_url: "https://bnb-testnet.g.alchemy.com/v2/",
        },
        EvmNetwork::Polygon => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/polygon/",
            llama_nodes_url: Some("https://polygon.llamarpc.com/"),
            public_node_url: "https://polygon-bor-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=polygon&dkey=",
            alchemy_url: "https://polygon-mainnet.g.alchemy.com/v2/",
        },
        EvmNetwork::Optimism => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/optimism/",
            llama_nodes_url: Some("https://optimism.llamarpc.com/"),
            public_node_url: "https://optimism-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=optimism&dkey=",
            alchemy_url: "https://opt-mainnet.g.alchemy.com/v2/",
        },
        EvmNetwork::Base => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/base/",
            llama_nodes_url: Some("https://base.llamarpc.com/"),
            public_node_url: "https://base-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=base&dkey=",
            alchemy_url: "https://base-mainnet.g.alchemy.com/v2/",
        },
        EvmNetwork::Avalanche => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/avalanche/",
            llama_nodes_url: None,
            public_node_url: "https://avalanche-c-chain-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=avalanche&dkey=",
            alchemy_url: "https://avax-mainnet.g.alchemy.com/v2/",
        },
        EvmNetwork::Fantom => NetworkConfig {
            ankr_url: "https://rpc.ankr.com/fantom/",
            llama_nodes_url: None,
            public_node_url: "https://fantom-rpc.publicnode.com/",
            drpc_url: "https://lb.drpc.org/ogrpc?network=fantom&dkey=",
            alchemy_url: "https://fantom-mainnet.g.alchemy.com/v2/",
        },
    }
}

fn create_rpc_service(url: &str, provider: Provider) -> RpcApi {
    RpcApi {
        url: provider.get_url_with_api_key(url),
        headers: None,
    }
}

pub fn get_one_provider(network: EvmNetwork, provider: Provider) -> RpcServices {
    let config = get_network_config(network);
    let chain_id = network.chain_id();

    if provider == Provider::LlamaNodes && config.llama_nodes_url.is_none() {
        return RpcServices::Custom {
            chain_id,
            services: vec![],
        };
    }

    let url = match provider {
        Provider::Ankr => config.ankr_url,
        Provider::LlamaNodes => config.llama_nodes_url.unwrap(),
        Provider::PublicNode => config.public_node_url,
        Provider::DRPC => config.drpc_url,
        Provider::Alchemy => config.alchemy_url,
    };

    RpcServices::Custom {
        chain_id,
        services: vec![create_rpc_service(url, provider)],
    }
}

pub fn get_providers(network: EvmNetwork) -> RpcServices {
    let config = get_network_config(network);
    let chain_id = network.chain_id();
    let services = vec![
        create_rpc_service(config.ankr_url, Provider::Ankr),
        create_rpc_service(config.public_node_url, Provider::PublicNode),
        create_rpc_service(config.drpc_url, Provider::DRPC),
        create_rpc_service(config.alchemy_url, Provider::Alchemy),
    ];
    // Excluding LlamaNodes for large number of errors and latency
    //if let Some(llama_url) = config.llama_nodes_url {
    //    services.insert(0, create_rpc_service(llama_url, Provider::LlamaNodes));
    //}

    RpcServices::Custom { chain_id, services }
}
