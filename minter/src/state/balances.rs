use std::collections::BTreeMap;

use candid::Principal;

use crate::{
    eth_types::Address,
    numeric::{Erc20Value, IcrcValue, LedgerBurnIndex, Wei},
    state::{
        audit::{process_event, EventType},
        mutate_state, read_state,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBalance {
    /// Amount of ETH controlled by the minter's address via tECDSA.
    /// Note that invalid deposits are not accounted for and so this value
    /// might be less than what is displayed by Etherscan
    /// or retrieved by the JSON-RPC call `eth_getBalance`.
    /// Also, some transactions may have gone directly to the minter's address
    /// without going via the helper smart contract.
    pub native_balance: Wei,
    /// Total amount of fees across all finalized transactions icNative -> Native. conversion of twin native token to token on the home chain.
    pub total_effective_tx_fees: Wei,
    /// Total amount of fees that were charged to the user during the withdrawal
    /// but not consumed by the finalized transaction icNative -> Native. conversion of twin native token to token on the home chain.
    pub total_unspent_tx_fees: Wei,

    // fee collected to cover signing cost, for withdraw and lock(mint on evm) operations.
    // after each operation withdrawal_native_fee should be added to total collected fee
    pub total_collected_operation_native_fee: Wei,
}

impl Default for NativeBalance {
    fn default() -> Self {
        Self {
            native_balance: Wei::ZERO,
            total_effective_tx_fees: Wei::ZERO,
            total_unspent_tx_fees: Wei::ZERO,
            total_collected_operation_native_fee: Wei::ZERO,
        }
    }
}

impl NativeBalance {
    pub fn eth_balance_add(&mut self, value: Wei) {
        self.native_balance = self.native_balance.checked_add(value).unwrap_or_else(|| {
            panic!(
                "BUG: overflow when adding {} to {}",
                value, self.native_balance
            )
        })
    }

    pub fn eth_balance_sub(&mut self, value: Wei) {
        self.native_balance = self.native_balance.checked_sub(value).unwrap_or_else(|| {
            panic!(
                "BUG: underflow when subtracting {} from {}",
                value, self.native_balance
            )
        })
    }

    pub fn total_effective_tx_fees_add(&mut self, value: Wei) {
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
    pub balance_by_erc20_contract: BTreeMap<Address, Erc20Value>,
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
                    panic!("BUG: overflow when adding {deposit} to {previous_value}")
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
                panic!("BUG: underflow when subtracting {withdrawal_amount} from {previous_value}")
            });
        self.balance_by_erc20_contract
            .insert(erc20_contract, new_value);
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IcrcBalances {
    pub balance_by_icrc_ledger: BTreeMap<Principal, IcrcValue>,
}

impl IcrcBalances {
    pub fn balance_of(&self, token_principal: &Principal) -> IcrcValue {
        *self
            .balance_by_icrc_ledger
            .get(token_principal)
            .unwrap_or(&IcrcValue::ZERO)
    }

    pub fn icrc_add(&mut self, token_principal: Principal, lock_amount: IcrcValue) {
        match self.balance_by_icrc_ledger.get(&token_principal) {
            Some(previous_value) => {
                let new_value = previous_value.checked_add(lock_amount).unwrap_or_else(|| {
                    panic!("BUG: overflow when adding {lock_amount} to {previous_value}")
                });
                self.balance_by_icrc_ledger
                    .insert(token_principal, new_value);
            }
            None => {
                self.balance_by_icrc_ledger
                    .insert(token_principal, lock_amount);
            }
        }
    }

    pub fn icrc_sub(&mut self, token_principal: Principal, release_amount: IcrcValue) {
        let previous_value = self
            .balance_by_icrc_ledger
            .get(&token_principal)
            .expect("BUG: Cannot subtract from a missing Icrc balance");
        let new_value = previous_value
            .checked_sub(release_amount)
            .unwrap_or_else(|| {
                panic!("BUG: underflow when subtracting {release_amount} from {previous_value}")
            });
        self.balance_by_icrc_ledger
            .insert(token_principal, new_value);
    }
}

//  tank for collecting un-used transaction fees to be used for next swap-bridge transactions sent
//  by the appic dex(User paying usdc to cover fees)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GasTank {
    pub usdc_balance: Erc20Value,
    pub native_balance: Wei,
}

impl GasTank {
    pub fn usdc_balance_add(&mut self, value: Erc20Value) {
        self.usdc_balance = self.usdc_balance.checked_add(value).unwrap_or_else(|| {
            panic!(
                "BUG: overflow when adding {} to {}",
                value, self.usdc_balance
            )
        })
    }

    pub fn usdc_balance_sub(&mut self, value: Erc20Value) {
        self.usdc_balance = self.usdc_balance.checked_sub(value).unwrap_or_else(|| {
            panic!(
                "BUG: underflow when subtracting {} from {}",
                value, self.usdc_balance
            )
        })
    }

    pub fn native_balance_add(&mut self, value: Wei) {
        self.native_balance = self.native_balance.checked_add(value).unwrap_or_else(|| {
            panic!(
                "BUG: overflow when adding {} to {}",
                value, self.native_balance
            )
        })
    }

    pub fn native_balance_sub(&mut self, value: Wei) {
        self.native_balance = self.native_balance.checked_sub(value).unwrap_or_else(|| {
            panic!(
                "BUG: underflow when subtracting {} from {}",
                value, self.native_balance
            )
        })
    }
}

impl Default for GasTank {
    fn default() -> Self {
        Self {
            native_balance: Wei::ZERO,
            usdc_balance: Erc20Value::ZERO,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseGasFromTankError {
    pub requested: Wei,
    pub available: Wei,
}
pub fn release_gas_from_tank_with_usdc(
    usdc_amount: Erc20Value,
    gas_amount: Wei,
    swap_tx_id: String,
) -> Result<LedgerBurnIndex, ReleaseGasFromTankError> {
    let (native_tank_balance, next_swap_ledger_burn_index) = read_state(|s| {
        (
            s.gas_tank.native_balance,
            s.next_swap_ledger_burn_index.expect(
                "Bug: next_swap_ledger_burn_index should be available if swapping is active",
            ),
        )
    });

    if native_tank_balance < gas_amount {
        return Err(ReleaseGasFromTankError {
            requested: gas_amount,
            available: native_tank_balance,
        });
    }

    mutate_state(|s| {
        process_event(
            s,
            EventType::ReleasedGasFromGasTankWithUsdc {
                usdc_amount,
                gas_amount,
                swap_tx_id,
            },
        )
    });

    Ok(next_swap_ledger_burn_index)
}
