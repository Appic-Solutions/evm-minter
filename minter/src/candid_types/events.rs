use crate::candid_types::dex_orders::DexOrderArgs;
use crate::lifecycle::InitArg;
use crate::lifecycle::UpgradeArg;
use candid::{CandidType, Deserialize, Nat, Principal};
use serde_bytes::ByteBuf;

#[derive(CandidType, Deserialize, Debug, Clone)]
pub struct GetEventsArg {
    pub start: u64,
    pub length: u64,
}

#[derive(CandidType, Deserialize, Debug, Clone)]
pub struct GetEventsResult {
    pub events: Vec<Event>,
    pub total_event_count: u64,
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub timestamp: u64,
    pub payload: EventPayload,
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EventSource {
    pub transaction_hash: String,
    pub log_index: Nat,
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum ReimbursementIndex {
    Native {
        ledger_burn_index: Nat,
    },
    Erc20 {
        native_ledger_burn_index: Nat,
        ledger_id: Principal,
        erc20_ledger_burn_index: Nat,
    },
    IcrcWrap {
        native_ledger_burn_index: Nat,
        icrc_token: Principal,
        icrc_ledger_lock_index: Nat,
    },
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AccessListItem {
    pub address: String,
    pub storage_keys: Vec<ByteBuf>,
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UnsignedTransaction {
    pub chain_id: Nat,
    pub nonce: Nat,
    pub max_priority_fee_per_gas: Nat,
    pub max_fee_per_gas: Nat,
    pub gas_limit: Nat,
    pub destination: String,
    pub value: Nat,
    pub data: ByteBuf,
    pub access_list: Vec<AccessListItem>,
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Success,
    Failure,
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TransactionReceipt {
    pub block_hash: String,
    pub block_number: Nat,
    pub effective_gas_price: Nat,
    pub gas_used: Nat,
    pub status: TransactionStatus,
    pub transaction_hash: String,
}

#[derive(CandidType, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum EventPayload {
    Init(InitArg),
    Upgrade(UpgradeArg),
    AcceptedDeposit {
        transaction_hash: String,
        block_number: Nat,
        log_index: Nat,
        from_address: String,
        value: Nat,
        principal: Principal,
        subaccount: Option<[u8; 32]>,
    },
    AcceptedErc20Deposit {
        transaction_hash: String,
        block_number: Nat,
        log_index: Nat,
        from_address: String,
        value: Nat,
        principal: Principal,
        erc20_contract_address: String,
        subaccount: Option<[u8; 32]>,
    },
    InvalidDeposit {
        event_source: EventSource,
        reason: String,
    },
    MintedNative {
        event_source: EventSource,
        mint_block_index: Nat,
    },
    SyncedToBlock {
        block_number: Nat,
    },

    AcceptedNativeWithdrawalRequest {
        withdrawal_amount: Nat,
        destination: String,
        ledger_burn_index: Nat,
        from: Principal,
        from_subaccount: Option<[u8; 32]>,
        created_at: Option<u64>,
        l1_fee: Option<Nat>,
        withdrawal_fee: Option<Nat>,
    },
    CreatedTransaction {
        withdrawal_id: Nat,
        transaction: UnsignedTransaction,
    },
    SignedTransaction {
        withdrawal_id: Nat,
        raw_transaction: String,
    },
    ReplacedTransaction {
        withdrawal_id: Nat,
        transaction: UnsignedTransaction,
    },
    FinalizedTransaction {
        withdrawal_id: Nat,
        transaction_receipt: TransactionReceipt,
    },
    ReimbursedNativeWithdrawal {
        reimbursed_in_block: Nat,
        withdrawal_id: Nat,
        reimbursed_amount: Nat,
        transaction_hash: Option<String>,
    },
    ReimbursedErc20Withdrawal {
        withdrawal_id: Nat,
        burn_in_block: Nat,
        reimbursed_in_block: Nat,
        ledger_id: Principal,
        reimbursed_amount: Nat,
        transaction_hash: Option<String>,
    },
    SkippedBlock {
        block_number: Nat,
    },
    AddedErc20Token {
        chain_id: Nat,
        address: String,
        erc20_token_symbol: String,
        erc20_ledger_id: Principal,
    },
    AcceptedErc20WithdrawalRequest {
        max_transaction_fee: Nat,
        withdrawal_amount: Nat,
        erc20_contract_address: String,
        destination: String,
        native_ledger_burn_index: Nat,
        erc20_ledger_id: Principal,
        erc20_ledger_burn_index: Nat,
        from: Principal,
        from_subaccount: Option<[u8; 32]>,
        created_at: u64,
        l1_fee: Option<Nat>,
        withdrawal_fee: Option<Nat>,
        is_wrapped_mint: bool,
    },
    FailedErc20WithdrawalRequest {
        withdrawal_id: Nat,
        reimbursed_amount: Nat,
        to: Principal,
        to_subaccount: Option<[u8; 32]>,
    },
    MintedErc20 {
        event_source: EventSource,
        mint_block_index: Nat,
        erc20_token_symbol: String,
        erc20_contract_address: String,
    },
    QuarantinedDeposit {
        event_source: EventSource,
    },
    QuarantinedReimbursement {
        index: ReimbursementIndex,
    },

    AcceptedWrappedIcrcBurn {
        transaction_hash: String,
        block_number: Nat,
        log_index: Nat,
        from_address: String,
        value: Nat,
        principal: Principal,
        wrapped_erc20_contract_address: String,
        icrc_token_principal: Principal,
        subaccount: Option<[u8; 32]>,
    },
    InvalidEvent {
        event_source: EventSource,
        reason: String,
    },
    DeployedWrappedIcrcToken {
        transaction_hash: String,
        block_number: Nat,
        log_index: Nat,
        base_token: Principal,
        deployed_wrapped_erc20: String,
    },
    // The release event was quarantined due to transfer errors, will retry later
    QuarantinedRelease {
        event_source: EventSource,
    },

    ReleasedIcrcToken {
        event_source: EventSource,
        release_block_index: Nat,
        transfer_fee: Nat,
    },
    FailedIcrcLockRequest {
        withdrawal_id: Nat,
        reimbursed_amount: Nat,
        to: Principal,
        to_subaccount: Option<[u8; 32]>,
    },
    ReimbursedIcrcWrap {
        native_ledger_burn_index: Nat,
        lock_in_block: Nat,
        reimbursed_in_block: Nat,
        reimbursed_icrc_token: Principal,
        reimbursed_amount: Nat,
        transaction_hash: Option<String>,
        transfer_fee: Option<Nat>,
    },
    AcceptedSwapActivationRequest,
    SwapContractActivated {
        swap_contract_address: String,
        usdc_contract_address: String,
        twin_usdc_ledger_id: Principal,
        twin_usdc_decimals: Nat,
        canister_signing_fee_twin_usdc_value: Nat,
    },
    ReceivedSwapOrder {
        transaction_hash: String,
        block_number: Nat,
        log_index: Nat,
        from_address: String,
        recipient: String,
        token_in: String,
        token_out: String,
        amount_in: Nat,
        amount_out: Nat,
        bridged_to_minter: bool,
        encoded_swap_data: String,
    },
    MintedToAppicDex {
        event_source: EventSource,
        mint_block_index: Nat,
        minted_token: Principal,
        erc20_contract_address: String,
        tx_id: String,
    },
    NotifiedSwapEventOrderToAppicDex {
        event_source: EventSource,
        tx_id: String,
    },

    ReleasedGasFromGasTankWithUsdc {
        usdc_amount: Nat,
        gas_amount: Nat,
        swap_tx_id: String,
    },
    AcceptedSwapRequest {
        max_transaction_fee: Nat,
        erc20_token_in: String,
        erc20_amount_in: Nat,
        min_amount_out: Nat,
        recipient: String,
        deadline: Nat,
        swap_contract: String,
        gas_limit: Nat,
        native_ledger_burn_index: Nat,
        erc20_ledger_id: Principal,
        erc20_ledger_burn_index: Nat,
        from: Principal,
        from_subaccount: Option<[u8; 32]>,
        created_at: u64,
        l1_fee: Option<Nat>,
        withdrawal_fee: Option<Nat>,
        swap_tx_id: String,
        is_refund: bool,
    },
    QuarantinedDexOrder(DexOrderArgs),
    QuarantinedSwapRequest {
        max_transaction_fee: Nat,
        erc20_token_in: String,
        erc20_amount_in: Nat,
        min_amount_out: Nat,
        recipient: String,
        deadline: Nat,
        swap_contract: String,
        gas_limit: Nat,
        native_ledger_burn_index: Nat,
        erc20_ledger_id: Principal,
        erc20_ledger_burn_index: Nat,
        from: Principal,
        from_subaccount: Option<[u8; 32]>,
        created_at: u64,
        l1_fee: Option<Nat>,
        withdrawal_fee: Option<Nat>,
        swap_tx_id: String,
        is_refund: bool,
    },
    GasTankUpdate {
        usdc_withdrawn: Nat,
        native_deposited: Nat,
    },
}
