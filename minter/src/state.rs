#[cfg(test)]
pub mod tests;

pub mod audit;
pub mod balances;
pub mod event;
pub mod transactions;

use crate::{
    candid_types::{dex_orders::DexOrderArgs, SwapStatus},
    numeric::Erc20Value,
    state::{
        balances::GasTank,
        transactions::{data::TransactionCallData, ExecuteSwapRequest},
    },
    tx_id::SwapTxId,
};
use evm_rpc_client::address::ecdsa_public_key_to_address;
use evm_rpc_client::eth_types::Address;
use std::{
    cell::RefCell,
    collections::{btree_map, BTreeMap, BTreeSet, HashSet},
    fmt::{Display, Formatter},
};

use crate::{
    candid_types::DepositStatus,
    contract_logs::{EventSource, ReceivedContractEvent},
    erc20::{ERC20Token, ERC20TokenSymbol},
    evm_config::EvmNetwork,
    lifecycle::UpgradeArg,
    logs::DEBUG,
    map::DedupMultiKeyMap,
    numeric::{
        BlockNumber, IcrcValue, LedgerBurnIndex, LedgerMintIndex, LedgerReleaseIndex,
        TransactionNonce, Wei, WeiPerGas,
    },
    rpc_declarations::{BlockTag, Hash, TransactionReceipt, TransactionStatus},
    state::transactions::NativeWithdrawalRequest,
    tx::gas_fees::GasFeeEstimate,
};
use balances::{Erc20Balances, IcrcBalances, NativeBalance};
use candid::Principal;
use ic_canister_log::log;
use libsecp256k1::{PublicKey, PublicKeyFormat};
use serde_bytes::ByteBuf;
use strum_macros::EnumIter;
use transactions::{Erc20WithdrawalRequest, WithdrawalRequest, WithdrawalTransactions};

use ic_cdk::management_canister::EcdsaPublicKeyResult;

thread_local! {
    pub static STATE:RefCell<Option<State>>=RefCell::default();
}

pub const MAIN_DERIVATION_PATH: Vec<ByteBuf> = vec![];

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum InvalidEventReason {
    /// Deposit or release is invalid and was never minted or released.
    /// This is most likely due to a user error (e.g., user's IC principal cannot be decoded)
    /// or there is a critical issue in the logs returned from the JSON-RPC providers.
    InvalidEvent(String),

    /// Deposit is valid but it's unknown whether it was minted or not,
    /// most likely because there was an unexpected panic in the callback.
    /// The deposit is quarantined to avoid any double minting and
    /// will not be further processed without manual intervention.
    QuarantinedDeposit,
}

impl Display for InvalidEventReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidEventReason::InvalidEvent(reason) => {
                write!(f, "Invalid event: {reason}")
            }
            InvalidEventReason::QuarantinedDeposit => {
                write!(f, "Quarantined deposit")
            }
        }
    }
}
#[derive(Debug, Eq, PartialEq)]
pub enum InvalidStateError {
    InvalidTransactionNonce(String),
    InvalidEcdsaKeyName(String),
    InvalidLedgerId(String),
    InvalidHelperContractAddress(String),
    InvalidMinimumWithdrawalAmount(String),
    InvalidMinimumLedgerTransferFee(String),
    InvalidLastScrapedBlockNumber(String),
    InvalidMinimumMaximumPriorityFeePerGas(String),
    InvalidFeeInput(String),
}

// events for minted(wrapped) erc20 tokens
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MintedEvent {
    pub event: ReceivedContractEvent,
    pub mint_block_index: LedgerMintIndex,
    pub token_symbol: String,
    pub erc20_contract_address: Option<Address>,
}

// events for minted(wrapped) erc20 tokens to appic dex
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MintedToDex {
    pub event: ReceivedContractEvent,
    pub mint_block_index: LedgerMintIndex,
    pub minted_token: Principal,
    pub erc20_contract_address: Option<Address>,
    pub tx_id: SwapTxId,
}

// events for minted(wrapped) erc20 tokens to appic dex
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifiedToAppiDex {
    pub event: ReceivedContractEvent,
    pub tx_id: SwapTxId,
}

// events for unlocked(unwrapped) icp tokens
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleasedEvent {
    pub event: ReceivedContractEvent,
    pub transfer_block_index: LedgerReleaseIndex,
    pub transfer_fee: IcrcValue,
    pub icrc_ledger: Principal,
    pub erc20_contract_address: Address,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinUSDCInfo {
    pub address: Address,
    pub ledger_id: Principal,
    pub decimals: u8,
}

impl MintedEvent {
    pub fn source(&self) -> EventSource {
        self.event.source()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct State {
    pub evm_network: EvmNetwork,
    pub ecdsa_key_name: String,
    pub native_ledger_id: Principal,
    pub native_index_id: Principal,
    pub native_symbol: ERC20TokenSymbol,
    pub helper_contract_addresses: Option<Vec<Address>>,

    // Principal id of EVM_RPC_CANISTER
    pub evm_canister_id: Principal,
    pub ecdsa_public_key: Option<EcdsaPublicKeyResult>,

    pub native_ledger_transfer_fee: Wei,
    pub native_minimum_withdrawal_amount: Wei,

    pub block_height: BlockTag,
    pub first_scraped_block_number: BlockNumber,
    pub last_scraped_block_number: BlockNumber,
    pub last_observed_block_number: Option<BlockNumber>,
    pub last_observed_block_time: Option<u64>,

    // latest requested block to scrape for getting deposit or swap events
    pub lastest_requested_block_to_scrape: Option<BlockNumber>,

    // after icp-evm bridge update we have both events to mint and events to release locked
    // icp tokens in case the wrapped ones on the evm side are already burnt
    pub events_to_mint: BTreeMap<EventSource, ReceivedContractEvent>,
    pub events_to_release: BTreeMap<EventSource, ReceivedContractEvent>,

    pub minted_events: BTreeMap<EventSource, MintedEvent>,
    pub released_events: BTreeMap<EventSource, ReleasedEvent>,
    pub invalid_events: BTreeMap<EventSource, InvalidEventReason>,

    // received release event was correct, but there was a problem with releasing,
    // e.g. canister out of cycles or unknown transfer fee.
    pub quarantined_releases: BTreeMap<EventSource, ReceivedContractEvent>,

    pub withdrawal_transactions: WithdrawalTransactions,
    pub skipped_blocks: BTreeSet<BlockNumber>,

    // Current balance of Native held by the minter.
    // Computed based on audit events.
    pub native_balance: NativeBalance,

    // Current balance of ERC-20 tokens held by the minter.
    // Computed based on audit events.
    pub erc20_balances: Erc20Balances,

    pub icrc_balances: IcrcBalances,

    // /// Per-principal lock for pending withdrawals
    pub pending_withdrawal_principals: BTreeSet<Principal>,

    /// Locks preventing concurrent execution timer tasks
    pub active_tasks: HashSet<TaskType>,

    // Transaction price estimate
    pub last_transaction_price_estimate: Option<(u64, GasFeeEstimate)>,
    pub last_native_token_usd_price_estimate: Option<(u64, f64)>,

    /// fees charged for withdraw and mint_wrapped icp tokens operations in order to cover signing cost,
    /// can be none in case there is no need to charge any withdrawal fees.
    pub withdrawal_native_fee: Option<Wei>,

    // Canister ID of the ledger suite manager that
    // can add new ERC-20 token to the minter
    pub ledger_suite_manager_id: Option<Principal>,

    /// ERC-20 tokens that the minter can mint:
    /// - primary key: ledger ID for the ERC20 token
    /// - secondary key: ERC-20 contract address on EVM
    /// - value: ERC20 token symbol
    pub erc20_tokens: DedupMultiKeyMap<Principal, Address, ERC20TokenSymbol>,

    /// Icrc tokens that the minter can lock, and mint on the evm side
    /// - primary key: ledger ID for the ICRC token
    /// - secondary key: ERC-20 contract address on EVM
    /// - value: IcrcValue token transfer fee
    pub wrapped_icrc_tokens: DedupMultiKeyMap<Principal, Address, Option<IcrcValue>>,

    pub min_max_priority_fee_per_gas: WeiPerGas,

    // Appic swapper canister_id
    pub dex_canister_id: Option<Principal>,

    // received swap events to be minted to appic dex to start a crosschain swap
    pub swap_events_to_mint_to_appic_dex: BTreeMap<EventSource, ReceivedContractEvent>,

    // minted swap events waiting to be notified to appic dex
    pub swap_events_to_be_notified: BTreeMap<EventSource, MintedToDex>,

    // notified events to appic dex
    pub notified_swap_events: BTreeMap<EventSource, NotifiedToAppiDex>,

    // TWIN USDC address and ledger_id
    pub twin_usdc_info: Option<TwinUSDCInfo>,
    // swap contract address
    pub swap_contract_address: Option<Address>,

    // canister_fee in twin usdc amount for covering signing cost
    pub canister_signing_fee_twin_usdc_amount: Option<Erc20Value>,

    // is the maximum approval given to the swap contract
    pub is_swapping_active: bool,

    // gas tank
    pub gas_tank: GasTank,

    // next swap burn index
    pub next_swap_ledger_burn_index: Option<LedgerBurnIndex>,

    // Quarantined swap requests
    // Swap requests that failed to process
    // key = swap_tx_id
    pub quarantined_dex_orders: BTreeMap<String, DexOrderArgs>,
}

impl State {
    pub fn minter_address(&self) -> Option<Address> {
        let pubkey = PublicKey::parse_slice(
            &self.ecdsa_public_key.as_ref()?.public_key,
            Some(PublicKeyFormat::Compressed),
        )
        .unwrap_or_else(|e| ic_cdk::trap(format!("failed to decode minter's public key: {e:?}")));
        Some(ecdsa_public_key_to_address(&pubkey))
    }

    pub fn validate_config(&self) -> Result<(), InvalidStateError> {
        if self.ecdsa_key_name.trim().is_empty() {
            return Err(InvalidStateError::InvalidEcdsaKeyName(
                "ecdsa_key_name cannot be blank".to_string(),
            ));
        }
        if self.native_ledger_id == Principal::anonymous() {
            return Err(InvalidStateError::InvalidLedgerId(
                "ledger_id cannot be the anonymous principal".to_string(),
            ));
        }
        if self
            .helper_contract_addresses
            .iter()
            .any(|addresses| addresses.contains(&Address::ZERO))
        {
            return Err(InvalidStateError::InvalidHelperContractAddress(
                "helper_contract_address cannot be the zero address".to_string(),
            ));
        }
        if self.native_minimum_withdrawal_amount == Wei::ZERO {
            return Err(InvalidStateError::InvalidMinimumWithdrawalAmount(
                "minimum_withdrawal_amount must be positive".to_string(),
            ));
        }

        if self.native_minimum_withdrawal_amount < self.native_ledger_transfer_fee {
            return Err(InvalidStateError::InvalidMinimumWithdrawalAmount(
                "minimum_withdrawal_amount must cover ledger transaction fee, \
                otherwise ledger can return a BadBurn error that should be returned to the user"
                    .to_string(),
            ));
        }
        Ok(())
    }

    // Returns the block height
    pub const fn block_height(&self) -> BlockTag {
        self.block_height
    }

    pub const fn evm_network(&self) -> EvmNetwork {
        self.evm_network
    }

    pub fn max_block_spread_for_logs_scraping(&self) -> u16 {
        // Limit set by the EVM-RPC canister itself, see
        // https://github.com/internet-computer-protocol/evm-rpc-canister/blob/3cce151d4c1338d83e6741afa354ccf11dff41e8/src/candid_rpc.rs#L192
        500_u16
    }

    pub fn events_to_mint(&self) -> Vec<ReceivedContractEvent> {
        self.events_to_mint.values().cloned().collect()
    }

    pub fn swap_events_to_mint_to_appic_dex(&self) -> Vec<ReceivedContractEvent> {
        self.swap_events_to_mint_to_appic_dex
            .values()
            .cloned()
            .collect()
    }

    pub fn swap_events_to_be_notified(&self) -> Vec<MintedToDex> {
        self.swap_events_to_be_notified.values().cloned().collect()
    }

    pub fn has_events_to_mint(&self) -> bool {
        !self.events_to_mint.is_empty()
    }

    pub fn events_to_release(&self) -> Vec<ReceivedContractEvent> {
        self.events_to_release.values().cloned().collect()
    }

    pub fn has_events_to_release(&self) -> bool {
        !self.events_to_release.is_empty()
    }

    pub fn has_events_to_mint_and_notify(&self) -> bool {
        !self.swap_events_to_mint_to_appic_dex.is_empty()
    }

    /// Quarantine the deposit event to prevent double minting.
    /// WARNING!: It's crucial that this method does not panic,
    /// since it's called inside the clean-up callback, when an unexpected panic did occur before.
    fn record_quarantined_deposit(&mut self, source: EventSource) -> bool {
        self.events_to_mint.remove(&source);
        self.swap_events_to_mint_to_appic_dex.remove(&source);
        match self.invalid_events.entry(source) {
            btree_map::Entry::Occupied(_) => false,
            btree_map::Entry::Vacant(entry) => {
                entry.insert(InvalidEventReason::QuarantinedDeposit);
                true
            }
        }
    }

    fn record_quarantined_release(&mut self, source: EventSource, event: ReceivedContractEvent) {
        self.events_to_release.remove(&source);
        self.quarantined_releases.insert(source, event);
    }

    fn record_contract_events(&mut self, event: &ReceivedContractEvent) {
        let event_source = event.source();
        assert!(
            !self.events_to_mint.contains_key(&event_source),
            "there must be no two different events with the same source"
        );
        assert!(
            !self.events_to_release.contains_key(&event_source),
            "there must be no two different events with the same source"
        );

        assert!(!self.minted_events.contains_key(&event_source));
        assert!(!self.released_events.contains_key(&event_source));
        assert!(!self.invalid_events.contains_key(&event_source));

        match event {
            ReceivedContractEvent::NativeDeposit(_received_native_event) => {
                self.events_to_mint.insert(event_source, event.clone());
                self.update_balance_upon_deposit(event)
            }
            ReceivedContractEvent::Erc20Deposit(received_erc20_event) => {
                assert!(
                    self.erc20_tokens
                        .contains_alt(&received_erc20_event.erc20_contract_address),
                    "BUG: unsupported ERC-20 contract address in event {event:?}"
                );

                self.events_to_mint.insert(event_source, event.clone());

                self.update_balance_upon_deposit(event)
            }
            ReceivedContractEvent::WrappedIcrcBurn(received_burn_event) => {
                assert!(
                    self.wrapped_icrc_tokens
                        .contains_alt(&received_burn_event.wrapped_erc20_contract_address),
                    "BUG: unsupported wrapped ICRC contract address in event{event:?}"
                );

                self.events_to_release.insert(event_source, event.clone());
            }
            ReceivedContractEvent::WrappedIcrcDeployed(wrapped_icrc_deployed) => {
                assert!(
                    !self
                        .wrapped_icrc_tokens
                        .contains_alt(&wrapped_icrc_deployed.deployed_wrapped_erc20),
                    "BUG: deployed ERC20 wrapped ICRC token has been already recorded"
                );

                self.wrapped_icrc_tokens
                    .try_insert(
                        wrapped_icrc_deployed.base_token,
                        wrapped_icrc_deployed.deployed_wrapped_erc20,
                        None,
                    )
                    .expect("Bug: duplicate wrapped icp token should've been detected before");
            }
            ReceivedContractEvent::ReceivedSwapOrder(received_swap_event) => {
                assert!(self.is_swapping_active, "BUG: There should be no swap event fetched if swap feature is not yet activated");
                assert!(self.dex_canister_id.is_some(), "BUG: Swap events can not be minted to appic dex if appic dex canister id is not represented");
                assert!(received_swap_event.bridged_to_minter, "BUG: Events with bridged_to_minter=false should have already been filtered out");
                assert!(
                    !received_swap_event.encoded_swap_data.0.is_empty(),
                    "BUG: Swap events with empty encoded data should've alread been filtered out"
                );

                self.swap_events_to_mint_to_appic_dex
                    .insert(event_source, event.clone());

                self.update_balance_upon_swap(event);
            }
        };
    }

    pub fn record_skipped_block(&mut self, block_number: BlockNumber) {
        assert!(
            self.skipped_blocks.insert(block_number),
            "BUG: block {block_number} was already skipped ",
        );
    }

    fn record_invalid_event(&mut self, source: EventSource, error: String) -> bool {
        assert!(
            !self.events_to_mint.contains_key(&source),
            "attempted to mark an accepted event as invalid"
        );
        assert!(
            !self.minted_events.contains_key(&source),
            "attempted to mark a minted event {source:?} as invalid"
        );
        assert!(
            !self.released_events.contains_key(&source),
            "attempted to mark a released event {source:?} as invalid"
        );

        match self.invalid_events.entry(source) {
            btree_map::Entry::Occupied(_) => false,
            btree_map::Entry::Vacant(entry) => {
                entry.insert(InvalidEventReason::InvalidEvent(error));
                true
            }
        }
    }

    fn record_successful_mint(
        &mut self,
        source: EventSource,
        token_symbol: &str,
        mint_block_index: LedgerMintIndex,
        erc20_contract_address: Option<Address>,
    ) {
        assert!(
            !self.invalid_events.contains_key(&source),
            "attempted to mint an event previously marked as invalid {source:?}"
        );
        let event = match self.events_to_mint.remove(&source) {
            Some(event) => event,
            None => panic!("attempted to mint Twin tokens for an unknown event {source:?}"),
        };
        assert_eq!(
            self.minted_events.insert(
                source,
                MintedEvent {
                    event,
                    mint_block_index,
                    token_symbol: token_symbol.to_string(),
                    erc20_contract_address,
                },
            ),
            None,
            "attempted to mint native twice for the same event {source:?}"
        );
    }

    fn record_successful_release(
        &mut self,
        source: EventSource,
        transfer_fee: IcrcValue,
        transfer_block_index: LedgerReleaseIndex,
        erc20_contract_address: Address,
        icrc_ledger: Principal,
    ) {
        assert!(
            !self.invalid_events.contains_key(&source),
            "attempted to release an event previously marked as invalid {source:?}"
        );
        let event = match self.events_to_release.remove(&source) {
            Some(event) => event,
            None => panic!("attempted to release icrc tokens for an unknown event {source:?}"),
        };

        assert_eq!(
            self.released_events.insert(
                source,
                ReleasedEvent {
                    event: event.clone(),
                    erc20_contract_address,
                    transfer_block_index,
                    transfer_fee,
                    icrc_ledger
                },
            ),
            None,
            "attempted to mint native twice for the same event {source:?}"
        );

        self.update_balance_upon_release(&event);
    }

    pub fn record_successful_mint_to_dex(
        &mut self,
        source: EventSource,
        mint_block_index: LedgerMintIndex,
        minted_token: Principal,
        erc20_contract_address: Address,
        tx_id: SwapTxId,
    ) {
        assert!(
            !self.invalid_events.contains_key(&source),
            "attempted to mint an event previously marked as invalid {source:?}"
        );

        let event = match self.swap_events_to_mint_to_appic_dex.remove(&source) {
            Some(event) => event,
            None => panic!("attempted to mint Twin tokens for an unknown event {source:?}"),
        };

        assert_eq!(
            self.swap_events_to_be_notified.insert(
                source,
                MintedToDex {
                    event,
                    mint_block_index,
                    minted_token,
                    erc20_contract_address: Some(erc20_contract_address),
                    tx_id,
                },
            ),
            None,
            "attempted to mint native twice for the same event {source:?}"
        );
    }

    pub fn record_notified_swap_event_to_appic_dex(
        &mut self,
        source: EventSource,
        tx_id: SwapTxId,
    ) {
        assert!(
            !self.invalid_events.contains_key(&source),
            "attempted to notify appic dex with an event previously marked as invalid {source:?}"
        );

        assert!(
            !self.swap_events_to_mint_to_appic_dex.contains_key(&source),
            "attempted to notify an event not minted yet{source:?}"
        );

        let event = match self.swap_events_to_be_notified.remove(&source) {
            Some(event) => event,
            None => panic!("attempted to mint Twin tokens for an unknown event {source:?}"),
        };

        assert_eq!(
            self.notified_swap_events.insert(
                source,
                NotifiedToAppiDex {
                    event: event.event,
                    tx_id
                },
            ),
            None,
            "attempted to record notified swap evetn twice for same event {source:?}"
        );
    }

    pub fn get_deposit_status(&self, tx_hash: Hash) -> Option<DepositStatus> {
        if self
            .minted_events
            .keys()
            .any(|event_source| event_source.transaction_hash == tx_hash)
        {
            return Some(DepositStatus::Minted);
        }

        if self
            .events_to_mint()
            .iter()
            .any(|deposit_event| deposit_event.transaction_hash() == tx_hash)
        {
            return Some(DepositStatus::Accepted);
        }

        if self
            .invalid_events
            .keys()
            .any(|event_source| event_source.transaction_hash == tx_hash)
        {
            return Some(DepositStatus::InvalidDeposit);
        }

        if self
            .released_events
            .keys()
            .any(|event_source| event_source.transaction_hash == tx_hash)
        {
            return Some(DepositStatus::Released);
        }

        if self
            .events_to_release()
            .iter()
            .any(|event_source| event_source.transaction_hash() == tx_hash)
        {
            return Some(DepositStatus::Accepted);
        }

        None
    }

    pub fn get_swap_status(&self, tx_hash: Hash) -> Option<SwapStatus> {
        if self
            .swap_events_to_mint_to_appic_dex
            .keys()
            .any(|event_source| event_source.transaction_hash == tx_hash)
        {
            return Some(SwapStatus::AcceptedSwap);
        }

        if let Some((_source, minted_order)) = self
            .swap_events_to_be_notified
            .iter()
            .find(|(source, _)| source.transaction_hash == tx_hash)
        {
            return Some(SwapStatus::MintedToAppicDex(minted_order.tx_id.0.clone()));
        }

        if let Some((_source, notified_event)) = self
            .notified_swap_events
            .iter()
            .find(|(source, _)| source.transaction_hash == tx_hash)
        {
            return Some(SwapStatus::NotifiedAppicDex(notified_event.tx_id.0.clone()));
        }

        None
    }

    pub fn record_native_withdrawal_request(&mut self, request: NativeWithdrawalRequest) {
        self.withdrawal_transactions
            .record_withdrawal_request(request);
    }

    pub fn record_erc20_withdrawal_request(&mut self, request: Erc20WithdrawalRequest) {
        if request.is_wrapped_mint.unwrap_or_default() {
            assert!(self
                .wrapped_icrc_tokens
                .contains_alt(&request.erc20_contract_address));
            // balance update since icrc tokens were locked
            self.update_balance_upon_icrc_lock(
                request.erc20_ledger_id,
                request.withdrawal_amount.change_units(),
            );
        } else {
            assert!(
                self.erc20_tokens
                    .contains_alt(&request.erc20_contract_address),
                "BUG: unsupported ERC-20 token {}",
                request.erc20_contract_address
            );
        }

        self.withdrawal_transactions
            .record_withdrawal_request(request);
    }

    pub fn record_swap_request(&mut self, request: ExecuteSwapRequest) {
        self.withdrawal_transactions
            .remove_failed_swap_request_by_swap_tx_id(&request.swap_tx_id);

        self.quarantined_dex_orders.remove(&request.swap_tx_id);

        self.withdrawal_transactions
            .record_withdrawal_request(request);
    }

    pub fn record_finalized_transaction(
        &mut self,
        withdrawal_id: &LedgerBurnIndex,
        receipt: &TransactionReceipt,
    ) {
        let withdrawal_request = self
            .withdrawal_transactions
            .get_processed_withdrawal_request(withdrawal_id)
            .expect("BUG: missing withdrawal request")
            .clone();

        match withdrawal_request {
            WithdrawalRequest::Native(_) | WithdrawalRequest::Erc20(_) => {}
            WithdrawalRequest::Erc20Approve(_) => {
                self.is_swapping_active = true;
            }
            WithdrawalRequest::Swap(_) => {}
        }

        self.withdrawal_transactions
            .record_finalized_transaction(*withdrawal_id, receipt.clone());
        self.update_balance_upon_withdrawal(withdrawal_id, receipt, withdrawal_request);
    }

    fn update_balance_upon_deposit(&mut self, event: &ReceivedContractEvent) {
        match event {
            ReceivedContractEvent::NativeDeposit(event) => {
                self.native_balance.eth_balance_add(event.value)
            }
            ReceivedContractEvent::Erc20Deposit(event) => self
                .erc20_balances
                .erc20_add(event.erc20_contract_address, event.value),

            _ => panic!("Bug: Invalid event, it should have already been filtered out"),
        };
    }

    // update balance upopn releaseing locked icrc tokens
    fn update_balance_upon_release(&mut self, event: &ReceivedContractEvent) {
        match event {
            ReceivedContractEvent::WrappedIcrcBurn(received_burn_event) => {
                self.icrc_balances.icrc_sub(
                    received_burn_event.icrc_token_principal,
                    received_burn_event.value,
                );
            }
            _ => panic!("Bug: Invalid event, it should have already been filtered out"),
        };
    }

    fn update_balance_upon_icrc_lock(&mut self, locked_icrc_token: Principal, amount: IcrcValue) {
        self.icrc_balances.icrc_add(locked_icrc_token, amount);
    }

    fn update_balance_upon_swap(&mut self, event: &ReceivedContractEvent) {
        match event {
            ReceivedContractEvent::ReceivedSwapOrder(event) => self
                .erc20_balances
                .erc20_add(event.token_out, event.amount_out),
            _ => panic!("Bug: Invalid event, it should have already been filtered out"),
        }
    }

    fn update_balance_upon_withdrawal(
        &mut self,
        withdrawal_id: &LedgerBurnIndex,
        receipt: &TransactionReceipt,
        withdrawal_request: WithdrawalRequest,
    ) {
        let tx = self
            .withdrawal_transactions
            .get_finalized_transaction(withdrawal_id)
            .expect("BUG: missing finalized transaction");
        //let withdrawal_request = self
        //    .withdrawal_transactions
        //    .get_processed_withdrawal_request(withdrawal_id)
        //    .expect("BUG: missing withdrawal request");

        let l1_fee = withdrawal_request.l1_fee().unwrap_or(Wei::ZERO);

        let withdrawal_fee = withdrawal_request.withdrawal_fee().unwrap_or(Wei::ZERO);

        let tx_fee = receipt.effective_transaction_fee();

        // charged_tx_fee is only the fee paid to cover transaction fee excluding any other fee
        let (charged_tx_fee, is_wrapped_mint) = match withdrawal_request {
            WithdrawalRequest::Native(req) => {
                let total_charged_fees = req
                    .withdrawal_amount
                    .checked_sub(*tx.transaction_amount())
                    .expect(
                        "Bug: withdrawal_amount should always be higher than transaction amount",
                    );

                let charged_tx_fee=total_charged_fees.checked_sub(l1_fee)
                                    .expect("total_charged_fees should be higher than l1_fee")
                                    .checked_sub(withdrawal_fee).expect("Bug: total_charged_fees should be higer than l1_fee and withdrawal_fee combined");

                (charged_tx_fee, false)
            }
            WithdrawalRequest::Erc20(req) => (
                req.max_transaction_fee,
                req.is_wrapped_mint.unwrap_or_default(),
            ),
            WithdrawalRequest::Erc20Approve(req) => (req.max_transaction_fee, false),
            WithdrawalRequest::Swap(req) => (req.max_transaction_fee, false),
        };

        let unspent_tx_fee = charged_tx_fee.checked_sub(tx_fee).expect(
            "BUG: charged transaction fee MUST always be at least the effective transaction fee",
        );

        // we dont add the withdrawal_fee to debited amount, since its already added to total_collected_operation_native_fee
        let debited_amount = match receipt.status {
            TransactionStatus::Success => tx
                .transaction()
                .amount
                .checked_add(tx_fee)
                .expect("BUG: debited amount always fits into U256")
                .checked_add(l1_fee)
                .expect("BUG: debited amount always fits into U256"),

            TransactionStatus::Failure => tx_fee
                .checked_add(l1_fee)
                .expect("BUG: debited amount always fits into U256"),
        };

        println!(
            "debited amount: {debited_amount}, transaction amuont: {:?}, tx_fee: {tx_fee}",
            tx.transaction_amount()
        );

        self.native_balance.eth_balance_sub(debited_amount);
        self.native_balance.total_effective_tx_fees_add(tx_fee);

        // we add the unspent transaction fee to the gas tank to be sued for later
        self.gas_tank.native_balance_add(unspent_tx_fee);

        // whether if transactions fails or not the minter paid for the signing cost and we add it
        // to the gas tank to be used later
        self.gas_tank.native_balance_add(withdrawal_fee);

        // update erc20 balances only if request is erc20 and tx is not a wrapped_mint for icrc
        // tokens
        if receipt.status == TransactionStatus::Success
            && !tx.transaction_data().is_empty()
            && !is_wrapped_mint
        {
            let tx_data = TransactionCallData::decode(tx.transaction_data())
                .expect("BUG: failed to decode transaction data from transaction issued by minter");
            match tx_data {
                TransactionCallData::Erc20Transfer { to: _, value } => {
                    self.erc20_balances.erc20_sub(*tx.destination(), value);
                }
                TransactionCallData::Erc20Approve {
                    spender: _,
                    value: _,
                } => {}
                TransactionCallData::ExecuteSwap {
                    commands: _,
                    data: _,
                    token_in,
                    amount_in,
                    min_amount_out: _,
                    deadline: _,
                    encoded_data: _,
                    recipient: _,
                    bridge_to_minter: _,
                } => {
                    self.erc20_balances.erc20_sub(token_in, amount_in);
                }
            }
        }
    }

    pub fn update_gas_tank_balance(&mut self, usdc_withdrawn: Erc20Value, native_deposited: Wei) {
        self.gas_tank.usdc_balance_sub(usdc_withdrawn);
        self.gas_tank.native_balance_add(native_deposited);
    }

    pub fn find_erc20_token_by_ledger_id(&self, erc20_ledger_id: &Principal) -> Option<ERC20Token> {
        self.erc20_tokens
            .get_entry(erc20_ledger_id)
            .map(|(erc20_address, symbol)| ERC20Token {
                erc20_contract_address: *erc20_address,
                erc20_ledger_id: *erc20_ledger_id,
                chain_id: self.evm_network,
                erc20_token_symbol: symbol.clone(),
            })
    }

    pub fn find_icp_token_ledger_id_by_wrapped_erc20_address(
        &self,
        wrapped_erc20_address: &Address,
    ) -> Option<Principal> {
        self.wrapped_icrc_tokens
            .get_entry_alt(wrapped_erc20_address)
            .map(|(ledger_id, _symbol)| *ledger_id)
    }

    pub fn find_wrapped_erc20_token_by_icrc_ledger_id(
        &self,
        ledger_id: &Principal,
    ) -> Option<Address> {
        self.wrapped_icrc_tokens
            .get_entry(ledger_id)
            .map(|(address, _transfer_fee)| *address)
    }

    pub fn supported_wrapped_icrc_tokens(&self) -> impl Iterator<Item = (Principal, Address)> + '_ {
        self.wrapped_icrc_tokens
            .iter()
            .map(|(ledger_id, address, _transfer_fee)| (*ledger_id, *address))
    }

    pub fn supported_erc20_tokens(&self) -> impl Iterator<Item = ERC20Token> + '_ {
        self.erc20_tokens
            .iter()
            .map(|(ledger_id, erc20_address, symbol)| ERC20Token {
                erc20_contract_address: *erc20_address,
                erc20_ledger_id: *ledger_id,
                chain_id: self.evm_network,
                erc20_token_symbol: symbol.clone(),
            })
    }

    pub fn record_add_erc20_token(&mut self, erc20_token: ERC20Token) {
        assert_eq!(
            self.evm_network, erc20_token.chain_id,
            "ERROR: Expected {}, but got {}",
            self.evm_network, erc20_token.chain_id
        );
        let erc20_with_same_symbol = self
            .supported_erc20_tokens()
            .filter(|erc20| erc20.erc20_token_symbol == erc20_token.erc20_token_symbol)
            .collect::<Vec<_>>();
        assert_eq!(
            erc20_with_same_symbol,
            vec![],
            "ERROR: ERC20 token symbol {} is already used by {:?}",
            erc20_token.erc20_token_symbol,
            erc20_with_same_symbol
        );
        assert_eq!(
            self.erc20_tokens.try_insert(
                erc20_token.erc20_ledger_id,
                erc20_token.erc20_contract_address,
                erc20_token.erc20_token_symbol,
            ),
            Ok(()),
            "ERROR: some ERC20 tokens use the same ERC20 ledger ID or ERC-20 address"
        );
    }

    pub fn activate_swap_feature(
        &mut self,
        twin_usdc_ids: (Address, Principal),
        swap_contract_address: Address,
        twin_usdc_decimals: u8,
        dex_canister_id: Principal,
        canister_signing_fee_twin_usdc_amount: Erc20Value,
    ) {
        self.twin_usdc_info = Some(TwinUSDCInfo {
            address: twin_usdc_ids.0,
            ledger_id: twin_usdc_ids.1,
            decimals: twin_usdc_decimals,
        });
        self.swap_contract_address = Some(swap_contract_address);
        self.dex_canister_id = Some(dex_canister_id);
        self.canister_signing_fee_twin_usdc_amount = Some(canister_signing_fee_twin_usdc_amount);
        // For an operation we need a ledger bunr index but since the swap operations use the
        // native tokens that are already burned and located in the gas tank, we have an internal
        // counter that is not synced with the actual native ledger burn index and it starts from
        // u64::MAX / 2 to make it safe in order to prevent conflicts with actual ledger operations
        //
        // Adter every operation(adding usdc to gas tank and getting native token) the swap ledger
        // bunr index will be incremented.
        self.next_swap_ledger_burn_index =
            Some(LedgerBurnIndex::new(10_000_000_000_000_000_000_u64));
    }

    pub fn release_gas_from_tank_with_usdc(&mut self, usdc_amount: Erc20Value, gas_amount: Wei) {
        self.gas_tank.native_balance_sub(gas_amount);
        self.gas_tank.usdc_balance_add(usdc_amount);

        // increment the next swap ledger burn index after releasing gas
        self.next_swap_ledger_burn_index = Some(LedgerBurnIndex::new(
            self.next_swap_ledger_burn_index.unwrap().get() + 1,
        ));
    }

    pub fn record_quarantined_dex_order(&mut self, swap_request: DexOrderArgs) {
        self.quarantined_dex_orders
            .insert(swap_request.tx_id(), swap_request);
    }

    /// Checks whether two states are equivalent.
    pub fn is_equivalent_to(&self, other: &Self) -> Result<(), String> {
        // We define the equivalence using the upgrade procedure.
        // Replaying the event log won't produce exactly the same state we had before the upgrade,
        // but a state that equivalent for all practical purposes.
        //
        // For example, we don't compare:
        // 1. Computed fields and caches, such as `ecdsa_public_key`.
        // 2. Transient fields, such as `active_tasks`.
        use ic_utils_ensure::ensure_eq;

        ensure_eq!(self.evm_network, other.evm_network);
        ensure_eq!(self.native_ledger_id, other.native_ledger_id);
        ensure_eq!(self.ecdsa_key_name, other.ecdsa_key_name);
        ensure_eq!(
            self.helper_contract_addresses,
            other.helper_contract_addresses
        );
        ensure_eq!(
            self.native_minimum_withdrawal_amount,
            other.native_minimum_withdrawal_amount
        );
        ensure_eq!(
            self.first_scraped_block_number,
            other.first_scraped_block_number
        );
        ensure_eq!(
            self.last_scraped_block_number,
            other.last_scraped_block_number
        );
        ensure_eq!(self.block_height, other.block_height);
        ensure_eq!(self.events_to_mint, other.events_to_mint);
        ensure_eq!(self.minted_events, other.minted_events);
        ensure_eq!(self.invalid_events, other.invalid_events);

        ensure_eq!(self.erc20_tokens, other.erc20_tokens);

        self.withdrawal_transactions
            .is_equivalent_to(&other.withdrawal_transactions)
    }

    fn upgrade(&mut self, upgrade_args: UpgradeArg) -> Result<(), InvalidStateError> {
        use std::str::FromStr;

        let UpgradeArg {
            next_transaction_nonce,
            native_minimum_withdrawal_amount,
            helper_contract_address,
            block_height,
            last_scraped_block_number,
            evm_rpc_id,
            native_ledger_transfer_fee,
            min_max_priority_fee_per_gas,
            // deposit native fee is deprecated
            deposit_native_fee: _,
            withdrawal_native_fee,
        } = upgrade_args;
        if let Some(nonce) = next_transaction_nonce {
            let nonce = TransactionNonce::try_from(nonce)
                .map_err(|e| InvalidStateError::InvalidTransactionNonce(format!("ERROR: {e}")))?;
            self.withdrawal_transactions
                .update_next_transaction_nonce(nonce);
        }
        if let Some(amount) = native_minimum_withdrawal_amount {
            let minimum_withdrawal_amount = Wei::try_from(amount).map_err(|e| {
                InvalidStateError::InvalidMinimumWithdrawalAmount(format!("ERROR: {e}"))
            })?;
            self.native_minimum_withdrawal_amount = minimum_withdrawal_amount;
        }
        if let Some(minimum_amount) = native_ledger_transfer_fee {
            let native_ledger_transfer_fee = Wei::try_from(minimum_amount).map_err(|e| {
                InvalidStateError::InvalidMinimumLedgerTransferFee(format!("ERROR: {e}"))
            })?;
            self.native_ledger_transfer_fee = native_ledger_transfer_fee;
        }

        if let Some(min_max_priority_per_gas) = min_max_priority_fee_per_gas {
            let min_max_priority_fee_per_gas = WeiPerGas::try_from(min_max_priority_per_gas)
                .map_err(|e| {
                    InvalidStateError::InvalidMinimumMaximumPriorityFeePerGas(format!("ERROR: {e}"))
                })?;
            self.min_max_priority_fee_per_gas = min_max_priority_fee_per_gas;
        }

        if let Some(addr) = helper_contract_address {
            let contract_address = Address::from_str(&addr).map_err(|e| -> InvalidStateError {
                InvalidStateError::InvalidHelperContractAddress(format!("Invalid address: {e}"))
            })?;
            self.helper_contract_addresses
                .get_or_insert_with(std::vec::Vec::new)
                .push(contract_address);
        }

        if let Some(block_number) = last_scraped_block_number {
            self.last_scraped_block_number = BlockNumber::try_from(block_number).map_err(|e| {
                InvalidStateError::InvalidLastScrapedBlockNumber(format!("ERROR: {e}"))
            })?;
        }
        if let Some(block_height) = block_height {
            self.block_height = block_height.into();
        }

        if let Some(evm_id) = evm_rpc_id {
            self.evm_canister_id = evm_id;
        }

        if let Some(withdrawal_native_fee) = withdrawal_native_fee {
            // Conversion to Wei tag
            let withdrawal_native_fee_converted = Wei::try_from(withdrawal_native_fee)
                .map_err(|e| InvalidStateError::InvalidFeeInput(format!("ERROR: {e}")))?;

            // If fee is set to zero it should be remapped to None
            let withdrawal_native_fee = if withdrawal_native_fee_converted == Wei::ZERO {
                None
            } else {
                Some(withdrawal_native_fee_converted)
            };

            self.withdrawal_native_fee = withdrawal_native_fee;
        }

        self.validate_config()
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

#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq, EnumIter)]
pub enum TaskType {
    Mint,
    MintToDexAndSwap,
    RetrieveEth,
    ScrapLogs,
    RefreshGasFeeEstimate,
    Reimbursement,
    MintErc20,
}

pub async fn lazy_call_ecdsa_public_key() -> PublicKey {
    use ic_cdk::management_canister::{
        ecdsa_public_key, EcdsaCurve, EcdsaKeyId, EcdsaPublicKeyArgs,
    };

    fn to_public_key(response: &EcdsaPublicKeyResult) -> PublicKey {
        PublicKey::parse_slice(&response.public_key, Some(PublicKeyFormat::Compressed))
            .unwrap_or_else(|e| {
                ic_cdk::trap(format!("failed to decode minter's public key: {e:?}"))
            })
    }

    if let Some(ecdsa_pk_response) = read_state(|s| s.ecdsa_public_key.clone()) {
        return to_public_key(&ecdsa_pk_response);
    }
    let key_name = read_state(|s| s.ecdsa_key_name.clone());
    log!(DEBUG, "Fetching the ECDSA public key {key_name}");
    let response = ecdsa_public_key(&EcdsaPublicKeyArgs {
        canister_id: None,
        derivation_path: MAIN_DERIVATION_PATH
            .into_iter()
            .map(|x| x.to_vec())
            .collect(),
        key_id: EcdsaKeyId {
            curve: EcdsaCurve::Secp256k1,
            name: key_name,
        },
    })
    .await
    .unwrap_or_else(|err| ic_cdk::trap(format!("failed to get minter's public key:{err} ")));
    mutate_state(|s| s.ecdsa_public_key = Some(response.clone()));
    to_public_key(&response)
}

pub async fn minter_address() -> Address {
    ecdsa_public_key_to_address(&lazy_call_ecdsa_public_key().await)
}
