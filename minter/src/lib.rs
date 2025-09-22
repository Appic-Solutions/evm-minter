use std::time::Duration;

pub mod address;
pub mod candid_types;
pub mod cbor;
pub mod checked_amount;
pub mod contract_logs;
pub mod deposit;
pub mod dex_client;
pub mod erc20;
pub mod eth_types;
pub mod evm_config;
pub mod guard;
pub mod icrc_client;
pub mod lifecycle;
pub mod logs;
pub mod lsm_client;
pub mod management;
pub mod map;
pub mod memo;
pub mod numeric;
pub mod rpc_client;
pub mod rpc_declarations;
pub mod state;
pub mod storage;
pub mod swap;
pub mod tx;
pub mod tx_id;
pub mod withdraw;

#[cfg(test)]
pub mod test_fixtures;

#[cfg(test)]
mod tests;

// Log scraping can also be requested manually
pub const SCRAPING_CONTRACT_LOGS_INTERVAL: Duration = Duration::from_secs(20 * 60);
pub const PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL: Duration = Duration::from_secs(60);
pub const PROCESS_REIMBURSEMENT: Duration = Duration::from_secs(60);
pub const PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_RETRY_INTERVAL: Duration = Duration::from_secs(5);
pub const MINT_RETRY_DELAY: Duration = Duration::from_secs(30);

pub const APPIC_CONTROLLER_PRINCIPAL: &str =
    "tb3vi-54bcb-4oudm-fmp2s-nntjp-rmhd3-ukvnq-lawfq-vk5vy-mnlc7-pae";

pub const RPC_HELPER_PRINCIPAL: &str =
    "o74ab-rm2co-uhvn6-6ec2d-3kkvk-bwlcw-356yj-lbma2-m4qew-l4ett-wae";

pub const FEES_SUBACCOUNT: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x0f,
    0xee,
];
