#[cfg(test)]
mod tests;

pub mod audit;
pub mod event;
pub mod transactions;
use std::{
    cell::RefCell,
    collections::{btree_map, BTreeMap, BTreeSet, HashSet},
    fmt::{Display, Formatter},
};

use candid::Principal;
use ic_canister_log::log;
use ic_crypto_secp256k1::PublicKey;
use serde_bytes::ByteBuf;
use transactions::{
    Erc20WithdrawalRequest, TransactionCallData, WithdrawalRequest, WithdrawalTransactions,
};

use crate::{
    address::ecdsa_public_key_to_address,
    deposit_logs::{EventSource, ReceivedDepositEvent},
    erc20::{ERC20Token, ERC20TokenSymbol},
    eth_types::Address,
    evm_config::EvmNetwork,
    lifecycle::UpgradeArg,
    logs::DEBUG,
    map::DedupMultiKeyMap,
    numeric::{
        BlockNumber, Erc20Value, LedgerBurnIndex, LedgerMintIndex, TransactionNonce, Wei, WeiPerGas,
    },
    rpc_declarations::{BlockTag, TransactionReceipt, TransactionStatus},
    tx::GasFeeEstimate,
};
use strum_macros::EnumIter;

use ic_cdk::api::management_canister::ecdsa::EcdsaPublicKeyResponse;

thread_local! {
    pub static STATE:RefCell<Option<State>>=RefCell::default();
}

pub const MAIN_DERIVATION_PATH: Vec<ByteBuf> = vec![];

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum InvalidEventReason {
    /// Deposit is invalid and was never minted.
    /// This is most likely due to a user error (e.g., user's IC principal cannot be decoded)
    /// or there is a critical issue in the logs returned from the JSON-RPC providers.
    InvalidDeposit(String),

    /// Deposit is valid but it's unknown whether it was minted or not,
    /// most likely because there was an unexpected panic in the callback.
    /// The deposit is quarantined to avoid any double minting and
    /// will not be further processed without manual intervention.
    QuarantinedDeposit,
}

impl Display for InvalidEventReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidEventReason::InvalidDeposit(reason) => {
                write!(f, "Invalid deposit: {}", reason)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MintedEvent {
    pub deposit_event: ReceivedDepositEvent,
    pub mint_block_index: LedgerMintIndex,
    pub token_symbol: String,
    pub erc20_contract_address: Option<Address>,
}

impl MintedEvent {
    pub fn source(&self) -> EventSource {
        self.deposit_event.source()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct State {
    pub evm_network: EvmNetwork,
    pub ecdsa_key_name: String,
    pub native_ledger_id: Principal,
    pub native_index_id: Principal,
    pub native_symbol: ERC20TokenSymbol,
    pub helper_contract_address: Option<Address>,

    // Principal id of EVM_RPC_CANISTER
    pub evm_canister_id: Principal,
    pub ecdsa_public_key: Option<EcdsaPublicKeyResponse>,

    pub native_ledger_transfer_fee: Wei,
    pub native_minimum_withdrawal_amount: Wei,

    pub block_height: BlockTag,
    pub first_scraped_block_number: BlockNumber,
    pub last_scraped_block_number: BlockNumber,
    pub last_observed_block_number: Option<BlockNumber>,
    pub last_observed_block_time: Option<u64>,
    pub events_to_mint: BTreeMap<EventSource, ReceivedDepositEvent>,
    pub minted_events: BTreeMap<EventSource, MintedEvent>,
    pub invalid_events: BTreeMap<EventSource, InvalidEventReason>,

    pub withdrawal_transactions: WithdrawalTransactions,
    pub skipped_blocks: BTreeSet<BlockNumber>,

    // Current balance of Native held by the minter.
    // Computed based on audit events.
    pub native_balance: NativeBalance,

    // Current balance of ERC-20 tokens held by the minter.
    // Computed based on audit events.
    pub erc20_balances: Erc20Balances,

    // /// Per-principal lock for pending withdrawals
    pub pending_withdrawal_principals: BTreeSet<Principal>,

    /// Locks preventing concurrent execution timer tasks
    pub active_tasks: HashSet<TaskType>,
    // Number of HTTP out-calls since the last upgrade.
    // Used to correlate request and response in logs.
    // pub http_request_counter: u64,
    pub last_transaction_price_estimate: Option<(u64, GasFeeEstimate)>,

    // Fees taken per deposit and withdrawal in native token format
    // Option types, since the operation can be free as well
    // If the deposit type is Erc20, Fees will be free cause fees will be charged in native token wei format
    // How ever for withdrawal users need native token anyways so we can charge them with fees in twin native token
    // Withdrawal fees should cover cycles cost for signing messages
    pub deposit_native_fee: Option<Wei>,
    pub withdrawal_native_fee: Option<Wei>,

    // Canister ID of the ledger suite manager that
    // can add new ERC-20 token to the minter
    pub ledger_suite_manager_id: Option<Principal>,
    /// ERC-20 tokens that the minter can mint:
    /// - primary key: ledger ID for the ERC20 token
    /// - secondary key: ERC-20 contract address on Ethereum
    /// - value: ERC20 token symbol
    pub erc20_tokens: DedupMultiKeyMap<Principal, Address, ERC20TokenSymbol>,

    pub min_max_priority_fee_per_gas: WeiPerGas,

    // Appic swapper canister_id
    pub swap_canister_id: Option<Principal>,
}

impl State {
    pub fn minter_address(&self) -> Option<Address> {
        let pubkey = PublicKey::deserialize_sec1(&self.ecdsa_public_key.as_ref()?.public_key)
            .unwrap_or_else(|e| {
                ic_cdk::trap(&format!("failed to decode minter's public key: {:?}", e))
            });
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
            .helper_contract_address
            .iter()
            .any(|address| address == &Address::ZERO)
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
        1000_u16
    }

    pub fn events_to_mint(&self) -> Vec<ReceivedDepositEvent> {
        self.events_to_mint.values().cloned().collect()
    }

    pub fn has_events_to_mint(&self) -> bool {
        !self.events_to_mint.is_empty()
    }

    /// Quarantine the deposit event to prevent double minting.
    /// WARNING!: It's crucial that this method does not panic,
    /// since it's called inside the clean-up callback, when an unexpected panic did occur before.
    fn record_quarantined_deposit(&mut self, source: EventSource) -> bool {
        self.events_to_mint.remove(&source);
        match self.invalid_events.entry(source) {
            btree_map::Entry::Occupied(_) => false,
            btree_map::Entry::Vacant(entry) => {
                entry.insert(InvalidEventReason::QuarantinedDeposit);
                true
            }
        }
    }

    fn record_event_to_mint(&mut self, event: &ReceivedDepositEvent) {
        let event_source = event.source();
        assert!(
            !self.events_to_mint.contains_key(&event_source),
            "there must be no two different events with the same source"
        );
        assert!(!self.minted_events.contains_key(&event_source));
        assert!(!self.invalid_events.contains_key(&event_source));
        if let ReceivedDepositEvent::Erc20(event) = event {
            assert!(
                self.erc20_tokens
                    .contains_alt(&event.erc20_contract_address),
                "BUG: unsupported ERC-20 contract address in event {event:?}"
            )
        }

        self.events_to_mint.insert(event_source, event.clone());

        self.update_balance_upon_deposit(event)
    }

    pub fn record_skipped_block(&mut self, block_number: BlockNumber) {
        assert!(
            self.skipped_blocks.insert(block_number),
            "BUG: block {} was already skipped ",
            block_number,
        );
    }

    fn record_invalid_deposit(&mut self, source: EventSource, error: String) -> bool {
        assert!(
            !self.events_to_mint.contains_key(&source),
            "attempted to mark an accepted event as invalid"
        );
        assert!(
            !self.minted_events.contains_key(&source),
            "attempted to mark a minted event {source:?} as invalid"
        );

        match self.invalid_events.entry(source) {
            btree_map::Entry::Occupied(_) => false,
            btree_map::Entry::Vacant(entry) => {
                entry.insert(InvalidEventReason::InvalidDeposit(error));
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
        let deposit_event = match self.events_to_mint.remove(&source) {
            Some(event) => event,
            None => panic!("attempted to mint Twin tokens for an unknown event {source:?}"),
        };
        assert_eq!(
            self.minted_events.insert(
                source,
                MintedEvent {
                    deposit_event,
                    mint_block_index,
                    token_symbol: token_symbol.to_string(),
                    erc20_contract_address,
                },
            ),
            None,
            "attempted to mint native twice for the same event {source:?}"
        );
    }

    pub fn record_erc20_withdrawal_request(&mut self, request: Erc20WithdrawalRequest) {
        assert!(
            self.erc20_tokens
                .contains_alt(&request.erc20_contract_address),
            "BUG: unsupported ERC-20 token {}",
            request.erc20_contract_address
        );
        self.withdrawal_transactions
            .record_withdrawal_request(request);
    }

    pub fn record_finalized_transaction(
        &mut self,
        withdrawal_id: &LedgerBurnIndex,
        receipt: &TransactionReceipt,
    ) {
        self.withdrawal_transactions
            .record_finalized_transaction(*withdrawal_id, receipt.clone());
        self.update_balance_upon_withdrawal(withdrawal_id, receipt);
    }

    fn update_balance_upon_deposit(&mut self, event: &ReceivedDepositEvent) {
        match event {
            ReceivedDepositEvent::Native(event) => self.native_balance.eth_balance_add(event.value),
            ReceivedDepositEvent::Erc20(event) => self
                .erc20_balances
                .erc20_add(event.erc20_contract_address, event.value),
        };
    }

    fn update_balance_upon_withdrawal(
        &mut self,
        withdrawal_id: &LedgerBurnIndex,
        receipt: &TransactionReceipt,
    ) {
        let tx_fee = receipt.effective_transaction_fee();
        let tx = self
            .withdrawal_transactions
            .get_finalized_transaction(withdrawal_id)
            .expect("BUG: missing finalized transaction");
        let withdrawal_request = self
            .withdrawal_transactions
            .get_processed_withdrawal_request(withdrawal_id)
            .expect("BUG: missing withdrawal request");
        let charged_tx_fee = match withdrawal_request {
            WithdrawalRequest::Native(req) => req
                .withdrawal_amount
                .checked_sub(tx.transaction().amount)
                .expect("BUG: withdrawal amount MUST always be at least the transaction amount"),
            WithdrawalRequest::Erc20(req) => req.max_transaction_fee,
        };
        let unspent_tx_fee = charged_tx_fee.checked_sub(tx_fee).expect(
            "BUG: charged transaction fee MUST always be at least the effective transaction fee",
        );
        let debited_amount = match receipt.status {
            TransactionStatus::Success => tx
                .transaction()
                .amount
                .checked_add(tx_fee)
                .expect("BUG: debited amount always fits into U256"),
            TransactionStatus::Failure => tx_fee,
        };
        self.native_balance.eth_balance_sub(debited_amount);
        self.native_balance.total_effective_tx_fees_add(tx_fee);
        self.native_balance
            .total_unspent_tx_fees_add(unspent_tx_fee);

        if receipt.status == TransactionStatus::Success && !tx.transaction_data().is_empty() {
            let TransactionCallData::Erc20Transfer { to: _, value } = TransactionCallData::decode(
                tx.transaction_data(),
            )
            .expect("BUG: failed to decode transaction data from transaction issued by minter");
            self.erc20_balances.erc20_sub(*tx.destination(), value);
        }
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
        ensure_eq!(self.helper_contract_address, other.helper_contract_address);
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
            deposit_native_fee,
            withdrawal_native_fee,
        } = upgrade_args;
        if let Some(nonce) = next_transaction_nonce {
            let nonce = TransactionNonce::try_from(nonce)
                .map_err(|e| InvalidStateError::InvalidTransactionNonce(format!("ERROR: {}", e)))?;
            self.withdrawal_transactions
                .update_next_transaction_nonce(nonce);
        }
        if let Some(amount) = native_minimum_withdrawal_amount {
            let minimum_withdrawal_amount = Wei::try_from(amount).map_err(|e| {
                InvalidStateError::InvalidMinimumWithdrawalAmount(format!("ERROR: {}", e))
            })?;
            self.native_minimum_withdrawal_amount = minimum_withdrawal_amount;
        }
        if let Some(minimum_amount) = native_ledger_transfer_fee {
            let native_ledger_transfer_fee = Wei::try_from(minimum_amount).map_err(|e| {
                InvalidStateError::InvalidMinimumLedgerTransferFee(format!("ERROR: {}", e))
            })?;
            self.native_ledger_transfer_fee = native_ledger_transfer_fee;
        }

        if let Some(min_max_priority_per_gas) = min_max_priority_fee_per_gas {
            let min_max_priority_fee_per_gas = WeiPerGas::try_from(min_max_priority_per_gas)
                .map_err(|e| {
                    InvalidStateError::InvalidMinimumMaximumPriorityFeePerGas(format!(
                        "ERROR: {}",
                        e
                    ))
                })?;
            self.min_max_priority_fee_per_gas = min_max_priority_fee_per_gas;
        }

        if let Some(address) = helper_contract_address {
            let helper_contract_address = Address::from_str(&address).map_err(|e| {
                InvalidStateError::InvalidHelperContractAddress(format!("ERROR: {}", e))
            })?;
            self.helper_contract_address = Some(helper_contract_address);
        }

        if let Some(block_number) = last_scraped_block_number {
            self.last_scraped_block_number = BlockNumber::try_from(block_number).map_err(|e| {
                InvalidStateError::InvalidLastScrapedBlockNumber(format!("ERROR: {}", e))
            })?;
        }
        if let Some(block_height) = block_height {
            self.block_height = block_height.into();
        }

        if let Some(evm_id) = evm_rpc_id {
            self.evm_canister_id = evm_id;
        }

        if let Some(deposit_native_fee) = deposit_native_fee {
            // Conversion to Wei tag
            let deposit_native_fee_converted = Wei::try_from(deposit_native_fee)
                .map_err(|e| InvalidStateError::InvalidFeeInput(format!("ERROR: {}", e)))?;

            // If fee is set to zero it should be remapped to None
            let deposit_native_fee = if deposit_native_fee_converted == Wei::ZERO {
                None
            } else {
                Some(deposit_native_fee_converted)
            };

            self.deposit_native_fee = deposit_native_fee;
        }

        if let Some(withdrawal_native_fee) = withdrawal_native_fee {
            // Conversion to Wei tag
            let withdrawal_native_fee_converted = Wei::try_from(withdrawal_native_fee)
                .map_err(|e| InvalidStateError::InvalidFeeInput(format!("ERROR: {}", e)))?;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBalance {
    /// Amount of ETH controlled by the minter's address via tECDSA.
    /// Note that invalid deposits are not accounted for and so this value
    /// might be less than what is displayed by Etherscan
    /// or retrieved by the JSON-RPC call `eth_getBalance`.
    /// Also, some transactions may have gone directly to the minter's address
    /// without going via the helper smart contract.
    native_balance: Wei,
    /// Total amount of fees across all finalized transactions icNative -> Native. conversion of twin native token to token on the home chain.
    total_effective_tx_fees: Wei,
    /// Total amount of fees that were charged to the user during the withdrawal
    /// but not consumed by the finalized transaction icNative -> Native. conversion of twin native token to token on the home chain.
    total_unspent_tx_fees: Wei,
}

impl Default for NativeBalance {
    fn default() -> Self {
        Self {
            native_balance: Wei::ZERO,
            total_effective_tx_fees: Wei::ZERO,
            total_unspent_tx_fees: Wei::ZERO,
        }
    }
}

impl NativeBalance {
    fn eth_balance_add(&mut self, value: Wei) {
        self.native_balance = self.native_balance.checked_add(value).unwrap_or_else(|| {
            panic!(
                "BUG: overflow when adding {} to {}",
                value, self.native_balance
            )
        })
    }

    fn eth_balance_sub(&mut self, value: Wei) {
        self.native_balance = self.native_balance.checked_sub(value).unwrap_or_else(|| {
            panic!(
                "BUG: underflow when subtracting {} from {}",
                value, self.native_balance
            )
        })
    }

    fn total_effective_tx_fees_add(&mut self, value: Wei) {
        self.total_effective_tx_fees = self
            .total_effective_tx_fees
            .checked_add(value)
            .unwrap_or_else(|| {
                panic!(
                    "BUG: overflow when adding {} to {}",
                    value, self.total_effective_tx_fees
                )
            })
    }

    fn total_unspent_tx_fees_add(&mut self, value: Wei) {
        self.total_unspent_tx_fees = self
            .total_unspent_tx_fees
            .checked_add(value)
            .unwrap_or_else(|| {
                panic!(
                    "BUG: overflow when adding {} to {}",
                    value, self.total_unspent_tx_fees
                )
            })
    }

    pub fn native_balance(&self) -> Wei {
        self.native_balance
    }

    pub fn total_effective_tx_fees(&self) -> Wei {
        self.total_effective_tx_fees
    }

    pub fn total_unspent_tx_fees(&self) -> Wei {
        self.total_unspent_tx_fees
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Erc20Balances {
    balance_by_erc20_contract: BTreeMap<Address, Erc20Value>,
}

impl Erc20Balances {
    pub fn balance_of(&self, erc20_contract: &Address) -> Erc20Value {
        *self
            .balance_by_erc20_contract
            .get(erc20_contract)
            .unwrap_or(&Erc20Value::ZERO)
    }

    pub fn erc20_add(&mut self, erc20_contract: Address, deposit: Erc20Value) {
        match self.balance_by_erc20_contract.get(&erc20_contract) {
            Some(previous_value) => {
                let new_value = previous_value.checked_add(deposit).unwrap_or_else(|| {
                    panic!(
                        "BUG: overflow when adding {} to {}",
                        deposit, previous_value
                    )
                });
                self.balance_by_erc20_contract
                    .insert(erc20_contract, new_value);
            }
            None => {
                self.balance_by_erc20_contract
                    .insert(erc20_contract, deposit);
            }
        }
    }

    pub fn erc20_sub(&mut self, erc20_contract: Address, withdrawal_amount: Erc20Value) {
        let previous_value = self
            .balance_by_erc20_contract
            .get(&erc20_contract)
            .expect("BUG: Cannot subtract from a missing ERC-20 balance");
        let new_value = previous_value
            .checked_sub(withdrawal_amount)
            .unwrap_or_else(|| {
                panic!(
                    "BUG: underflow when subtracting {} from {}",
                    withdrawal_amount, previous_value
                )
            });
        self.balance_by_erc20_contract
            .insert(erc20_contract, new_value);
    }
}

#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq, EnumIter)]
pub enum TaskType {
    Mint,
    RetrieveEth,
    ScrapLogs,
    RefreshGasFeeEstimate,
    Reimbursement,
    MintErc20,
}

pub async fn lazy_call_ecdsa_public_key() -> PublicKey {
    use ic_cdk::api::management_canister::ecdsa::{
        ecdsa_public_key, EcdsaCurve, EcdsaKeyId, EcdsaPublicKeyArgument,
    };

    fn to_public_key(response: &EcdsaPublicKeyResponse) -> PublicKey {
        PublicKey::deserialize_sec1(&response.public_key).unwrap_or_else(|e| {
            ic_cdk::trap(&format!("failed to decode minter's public key: {:?}", e))
        })
    }

    if let Some(ecdsa_pk_response) = read_state(|s| s.ecdsa_public_key.clone()) {
        return to_public_key(&ecdsa_pk_response);
    }
    let key_name = read_state(|s| s.ecdsa_key_name.clone());
    log!(DEBUG, "Fetching the ECDSA public key {key_name}");
    let (response,) = ecdsa_public_key(EcdsaPublicKeyArgument {
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
    .unwrap_or_else(|(error_code, message)| {
        ic_cdk::trap(&format!(
            "failed to get minter's public key: {} (error code = {:?})",
            message, error_code,
        ))
    });
    mutate_state(|s| s.ecdsa_public_key = Some(response.clone()));
    to_public_key(&response)
}

pub async fn minter_address() -> Address {
    ecdsa_public_key_to_address(&lazy_call_ecdsa_public_key().await)
}
