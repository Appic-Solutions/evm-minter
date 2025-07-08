# EVM Minter Canister
This repository contains the code for the Minter Canister, which facilitates bidirectional token bridging between the Internet Computer (ICP) and EVM-compatible chains. The bridge supports minting and burning of tokens, allowing tokens to be converted from EVM to ICP and vice versa. Twin tokens are linked to their corresponding tokens on the respective chain with a 1:1 ratio. For each twin token minted on one chain, the corresponding token is locked or burned on the other chain.

---

# Token Conversion Flow

## EVM to ICP Conversion
![EVM to ICP Token Conversion Flow](https://github.com/user-attachments/assets/af6a6d3e-9c12-4a99-bb69-7b50925cf5f5)

The process to convert tokens (native or ERC20) from an EVM chain to ICP begins when a user calls the `burn` function in the helper smart contract. This function processes both **native tokens** and **ERC20 tokens**, eliminating the need for multiple contracts. The tokens are burned or locked in an account created using **ECDSA** for the **minter canister**, and an event is emitted to record the action. Here’s the structure of the event:

```solidity
    // Event to log token burns or locks
    event TokenBurn(
        address indexed fromAddress,
        uint256 amount,
        bytes32 indexed icpRecipient,
        address indexed TokenAddress,
        bytes32 subaccount
    );
```

The contract's **event logs** are burned using multiple **RPC** providers to ensure reliability. The `eth_getLogs` function is called periodically, with timing adjustment based on each EVM chain’s block speed. These logs are converted into specific events based on the token type and direction:

- **Native Tokens Deposited (Locked)**: Triggers minted of wrapped tokens on ICP.
```rust
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedNativeEvent {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[n(3)]
    pub from_address: Address,
    #[n(4)]
    pub value: Wei,
    #[cbor(n(5), with = with="crate::cbor::principal")]
    pub principal: Principal,
    #[n(6)]
    pub subaccount: Option<LedgerSubaccount>,
}
```

- **ERC20 Tokens Deposited (Locked)**: Triggers minted of wrapped tokens on ICP.
```rust
#[derive(Clone, PartialEq, Eq, Clone, 
#[derive(Clone)]
pub struct ReceivedErc20Event {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[n(3)]
    pub from_address: Address,
    #[n(4)]
    pub value: Erc20Value,
    #[cbor(n(5), with= = "crate::cbor::principal")]
    pub principal: Principal,
    #[n(6)]
    pub erc20_contract_address: Address,
    #[n(7)]
    pub subaccount: Option<LedgerSubaccount>,
}
```

- **Wrapped ICP Tokens Burned**: Triggers release (unlocking) of ICP tokens on the ICP side.
```rust
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ReceivedBurnEvent {
    #[n(0)]
    pub transaction_hash: Hash,
    #[n(1)]
    pub block_number: BlockNumber,
    #[cbor(n(2))]
    pub log_index: LogIndex,
    #[n(3)]
    pub from_address: Address,
    #[n(4)]
    pub value: IcrcValue,
    #[cbor(n(5), with = "crate::cbor::principal")]
    pub principal: Principal,
    #[n(6)]
    pub wrapped_erc20_contract_address: Address,
    #[cbor(n(7), with = "crate::cbor::principal")]
    pub icrc_token_principal: Principal,
    #[n(8)]
    pub subaccount: Option<LedgerSubaccount>,
}
```

Invalid logs (e.g., those with invalid principals or addresses) are saved as **invalid events**. A **timer** triggers the **mint function** to mint new **twin tokens** on ICP based on these events. The minted tokens are transferred to the user, and the minting actions are logged.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MintedEvent {
    pub burn_event: Option<ReceivedBurnEvent>,
    pub native_event: Option<ReceivedNativeEvent>,
    pub erc20_event: Option<ReceivedErc20>,
    pub mint_block_index: LedgerMintIndex,
    pub token_symbol: String,
    pub erc20_contract_address: Option<Address>,
}
```

## ICP to EVM Conversion
![ICP to EVM Token Conversion Flow](https://github.com/user-attachments/assets/38900b1e-cb67-48b4-9ca5-2677f0f16605)

The process to add an ICRC token as a wrapped ERC20 token on the EVM side involves calling the `wrap_icrc` function. This function burns the ICRC token on the ICP side, deploys a corresponding wrapped ERC20 token on the EVM side, and emits the following event, which is captured by the minter:

```solidity
    // Event emitted when a wrapped token is deployed on EVM
    event WrappedTokenDeployed(
        bytes32 indexed baseToken,
        address indexed wrappedERC20
    );
```

Here, `baseToken` is the principal ID of the ICRC token in `bytes32` format. The process to convert tokens from ICP to EVM varies by token type:

- **Native Tokens**: The user approves the burning of tokens to the minter principal and calls the `withdraw_native` function.
- **ERC20 Tokens**: The user approves the burning of tokens to the minter principal and calls the `withdraw_erc20` function.
- **Wrapped ICP Tokens**: The user calls the `wrap_icrc` function to wrap ICP tokens, followed by burning them to the minter principal.

After approval, the **minter canister burns the tokens** using the **ICRC ledger client**, and a **withdrawal request** is created. Here’s the structure for withdrawal requests:

```rust
#[derive(Clone, Eq, PartialEq, Encode, Decode)]
pub struct NativeWithdrawalRequest {
    pub withdrawal_amount: Wei,
    pub destination: Address,
    pub ledger_burn_index: LedgerBurnIndex,
    pub from: Principal,
    pub from_subaccount: Option<Subaccount>,
    pub created_at: Option<u64>,
}

/// ERC-20 withdrawal request
#[derive(Clone, Eq, PartialEq, Encode, Decode)]
pub struct Erc20WithdrawalRequest {
    pub max_transaction_fee: Wei,
    pub withdrawal_amount: Erc20Value,
    pub destination: Address,
    pub native_ledger_burn_index: LedgerBurnIndex,
    pub erc20_contract_address: Address,
    pub erc20_ledger_id: Principal,
    pub erc20_ledger_burn_index: LedgerBurnIndex,
    pub from: Principal,
    pub from_subaccount: Option<Subaccount>,
    pub created_at: u64,
}
```

These requests are saved in the **canister’s state**. A **timer** processes them in four steps:

1. `create_transactions_batch()`
2. `sign_transactions_batch()`
3. `send_transactions_batch()`
4. `finalize_transactions_batch()`

If a transaction fails due to low gas, it is resubmitted with a 10% gas increase. For other failures, the **twin tokens** are **refunded** to the user on the ICP network.

---

# Key Modules

- **EVM_RPC_CLIENT module**: Manages calls to the `evm_rpc_canister`. If a "TooFewCycles" error occurs, the call is retried until successful.
  
- **RpcClient module**: Converts `evm_rpc_canister` responses into formats usable by the minter canister, ensuring consistency across varied responses.

- **State module**: Tracks general information about the minter canister and logs all events, including burns, mints, withdrawals, and wrapped token deployments.

```rust
#[derive(Debug, PartialEq, Clone)]
pub struct State {
    pub evm_network: EvmNetwork,
    pub ecdsa_key_name: String,
    pub native_ledger_id: Principal,
    pub native_symbol: ERC20TokenSymbol,
    pub helper_contract_address: Option<Address>,
    pub evm_canister_id: Principal,
    pub ecdsa_public_key: Option<EcdsaPublicKeyResponse>,
    pub native_ledger_transfer_fee: Wei,
    pub native_minimum_withdrawal_amount: Wei,
    pub block_height: BlockTag,
    pub first_scraped_block_number: BlockNumber,
    pub last_scraped_block_number: BlockNumber,
    pub last_observed_block_number: Option<BlockNumber>,
    pub events_to_mint: BTreeMap<EventSource, ReceivedBurnEvent>,
    pub minted_events: BTreeMap<EventSource, MintedEvent>,
    pub invalid_events: BTreeMap<EventSource, InvalidEventReason>,
    pub withdrawal_transactions: WithdrawalTransactions,
    pub native_balance: NativeBalance,
    pub erc20_balances: Erc20Balances,
    pub pending_withdrawal_principals: BTreeSet<Principal>,
    pub active_tasks: HashSet<TaskType>,
    pub last_transaction_price_estimate: Option<(u64, GasFeeEstimate)>,
    pub erc20_tokens: DedupMultiKeyMap<Principal, Address, ERC20TokenSymbol>,
    pub min_max_priority_fee_per_gas: WeiPerGas,
}
```

- **LedgerClient**: Manages calls to **ICRC ledgers** for minting and burning twin tokens (`icrc1_transfer`, `icrc2_transfer_from`).

---

This is an updated version of the EVM Minter Canister, supporting bidirectional token bridging and ICRC token wrapping as ERC20 tokens on EVM. Future improvements may include paying withdrawal fees using native tokens instead of twin tokens on ICP.
