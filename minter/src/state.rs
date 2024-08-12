use std::cell::RefCell;

use candid::Principal;

use crate::{
    eth_types::Address, lifecycles::EvmNetwork, numeric::BlockNumber, rpc_declrations::BlockTag,
};
use ic_cdk::api::management_canister::ecdsa::EcdsaPublicKeyResponse;

thread_local! {
    pub static STATE:RefCell<Option<State>>=RefCell::default();
}

#[derive(Debug, PartialEq, Clone)]
pub struct State {
    pub evm_network_id: EvmNetwork,
    pub ecdsa_key_name: String,
    pub native_twin_ledger_id: Principal,
    pub helper_contract_address: Option<Address>,
    pub evm_canister_id: Principal,
    pub ecdsa_public_key: Option<EcdsaPublicKeyResponse>,
    // pub cketh_minimum_withdrawal_amount: Wei,
    pub block_height: BlockTag,
    pub first_scraped_block_number: BlockNumber,
    pub last_scraped_block_number: BlockNumber,
    pub last_observed_block_number: Option<BlockNumber>,
    // pub events_to_mint: BTreeMap<EventSource, ReceivedEvent>,
    // pub minted_events: BTreeMap<EventSource, MintedEvent>,
    // pub invalid_events: BTreeMap<EventSource, InvalidEventReason>,
    // pub eth_transactions: EthTransactions,
    // pub skipped_blocks: BTreeMap<Address, BTreeSet<BlockNumber>>,

    // /// Current balance of ETH held by the minter.
    // /// Computed based on audit events.
    // pub eth_balance: EthBalance,

    // /// Current balance of ERC-20 tokens held by the minter.
    // /// Computed based on audit events.
    // pub erc20_balances: Erc20Balances,

    // /// Per-principal lock for pending withdrawals
    // pub pending_withdrawal_principals: BTreeSet<Principal>,

    // /// Locks preventing concurrent execution timer tasks
    // pub active_tasks: HashSet<TaskType>,

    // /// Number of HTTP outcalls since the last upgrade.
    // /// Used to correlate request and response in logs.
    // pub http_request_counter: u64,

    // pub last_transaction_price_estimate: Option<(u64, GasFeeEstimate)>,

    // /// Canister ID of the ledger suite orchestrator that
    // /// can add new ERC-20 token to the minter
    // pub ledger_suite_orchestrator_id: Option<Principal>,

    // /// Canister ID of the EVM RPC canister that
    // /// handles communication with Ethereum
    // pub evm_rpc_id: Option<Principal>,

    // /// ERC-20 tokens that the minter can mint:
    // /// - primary key: ledger ID for the ckERC20 token
    // /// - secondary key: ERC-20 contract address on Ethereum
    // /// - value: ckERC20 token symbol
    // pub ckerc20_tokens: DedupMultiKeyMap<Principal, Address, CkTokenSymbol>
}

impl State {
    // Returns the blockcheight
    pub const fn block_height(&self) -> BlockTag {
        self.block_height
    }
}
pub fn read_state<R>(f: impl FnOnce(&State) -> R) -> R {
    STATE.with(|s| f(s.borrow().as_ref().expect("BUG: state is not initialized")))
}

/// Mutates (part of) the current state using `f`.
///
/// Panics if there is no state.
pub fn mutate_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut State) -> R,
{
    STATE.with(|s| {
        f(s.borrow_mut()
            .as_mut()
            .expect("BUG: state is not initialized"))
    })
}