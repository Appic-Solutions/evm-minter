use candid::{CandidType, Deserialize, Principal};
use icrc_ledger_types::icrc1::account::Account;

#[derive(CandidType, Deserialize)]
pub struct InitArgs {
    pub decimals: Option<u8>,
    pub token_symbol: String,
    pub transfer_fee: candid::Nat,
    pub metadata: Vec<(String, MetadataValue)>,
    pub minting_account: Account,
    pub initial_balances: Vec<(Account, candid::Nat)>,
    pub fee_collector_account: Option<Account>,
    pub archive_options: ArchiveOptions,
    pub max_memo_length: Option<u16>,
    pub index_principal: Option<Principal>,
    pub token_name: String,
    pub feature_flags: Option<FeatureFlags>,
}

#[derive(CandidType, Deserialize)]
pub enum LedgerArgument {
    Upgrade(Option<UpgradeArgs>),
    Init(InitArgs),
}

#[derive(CandidType, Deserialize)]
pub struct FeatureFlags {
    pub icrc2: bool,
}

#[derive(CandidType, Deserialize)]
pub struct ChangeArchiveOptions {
    pub num_blocks_to_archive: Option<u64>,
    pub max_transactions_per_response: Option<u64>,
    pub trigger_threshold: Option<u64>,
    pub more_controller_ids: Option<Vec<Principal>>,
    pub max_message_size_bytes: Option<u64>,
    pub cycles_for_archive_creation: Option<u64>,
    pub node_max_memory_size_bytes: Option<u64>,
    pub controller_id: Option<Principal>,
}

#[derive(CandidType, Deserialize)]
pub enum MetadataValue {
    Int(candid::Int),
    Nat(candid::Nat),
    Blob(serde_bytes::ByteBuf),
    Text(String),
}

#[derive(CandidType, Deserialize)]
pub struct UpgradeArgs {
    pub change_archive_options: Option<ChangeArchiveOptions>,
    pub token_symbol: Option<String>,
    pub transfer_fee: Option<candid::Nat>,
    pub metadata: Option<Vec<(String, MetadataValue)>>,
    pub change_fee_collector: Option<ChangeFeeCollector>,
    pub max_memo_length: Option<u16>,
    pub index_principal: Option<Principal>,
    pub token_name: Option<String>,
    pub feature_flags: Option<FeatureFlags>,
}

#[derive(CandidType, Deserialize)]
pub struct ArchiveOptions {
    pub num_blocks_to_archive: u64,
    pub max_transactions_per_response: Option<u64>,
    pub trigger_threshold: u64,
    pub more_controller_ids: Option<Vec<Principal>>,
    pub max_message_size_bytes: Option<u64>,
    pub cycles_for_archive_creation: Option<u64>,
    pub node_max_memory_size_bytes: Option<u64>,
    pub controller_id: Principal,
}

#[derive(CandidType, Deserialize)]
pub enum ChangeFeeCollector {
    SetTo(Account),
    Unset,
}

#[derive(CandidType, Deserialize)]
pub struct UpgradeArg {
    pub ledger_id: Option<Principal>,
    pub retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
}

#[derive(CandidType, Deserialize)]
pub struct IndexInitArg {
    pub ledger_id: Principal,
    pub retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
}

#[derive(CandidType, Deserialize)]
pub enum IndexArg {
    Upgrade(UpgradeArg),
    Init(IndexInitArg),
}
