use std::time::Duration;

pub mod address;
mod cbor;
pub mod checked_amount;
pub mod deposit;
pub mod deposit_logs;
pub mod endpoints;
pub mod erc20;
pub mod eth_types;
pub mod evm_config;
pub mod guard;
pub mod ledger_client;
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
pub mod tx;
pub mod withdraw;

#[cfg(test)]
pub mod test_fixtures;

#[cfg(test)]
mod tests;

// Log scraping can also be requested manually
pub const SCRAPING_DEPOSIT_LOGS_INTERVAL: Duration = Duration::from_secs(20 * 60);
pub const PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL: Duration = Duration::from_secs(1 * 60);
pub const PROCESS_REIMBURSEMENT: Duration = Duration::from_secs(1 * 60);
pub const PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_RETRY_INTERVAL: Duration = Duration::from_secs(30);
pub const MINT_RETRY_DELAY: Duration = Duration::from_secs(30);

pub const FEES_SUBACCOUNT: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x0f,
    0xee,
];
