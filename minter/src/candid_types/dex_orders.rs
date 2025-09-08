use crate::{
    numeric::{Erc20Value, GasAmount, Wei},
    tx::gas_usd::MaxFeeUsd,
};

use num_traits::ToPrimitive;

use super::*;

// candid file designed for operations sent by appic dex
#[derive(CandidType, Deserialize, Clone, Debug, Encode, Decode, Eq, PartialEq)]
pub enum DexOrderArgs {
    #[n(0)]
    Swap(#[n(0)] DexSwapOrderArgs),
    #[n(1)]
    Bridge(#[n(0)] DexBridgeOrderArgs),
}

impl DexOrderArgs {
    pub fn tx_id(&self) -> String {
        match self {
            DexOrderArgs::Swap(order) => order.tx_id.to_lowercase(),
            DexOrderArgs::Bridge(order) => order.tx_id.to_lowercase(),
        }
    }

    pub fn gas_limit(&self) -> Result<GasAmount, String> {
        match self {
            DexOrderArgs::Swap(order) => GasAmount::try_from(order.gas_limit.clone())
                .map_err(|_| "ERROR: failed to convert Nat to u256".to_string()),
            DexOrderArgs::Bridge(order) => GasAmount::try_from(order.gas_limit.clone())
                .map_err(|_| "ERROR: failed to convert Nat to u256".to_string()),
        }
    }

    pub fn recipient(&self) -> Result<Address, String> {
        match self {
            DexOrderArgs::Swap(order) => Address::from_str(&order.recipient)
                .map_err(|_| "ERROR: Invalid recipient".to_string()),
            DexOrderArgs::Bridge(order) => Address::from_str(&order.recipient)
                .map_err(|_| "ERROR: Invalid recipient".to_string()),
        }
    }

    pub fn deadline(&self) -> Result<Erc20Value, String> {
        match self {
            DexOrderArgs::Swap(order) => Erc20Value::try_from(order.deadline.clone())
                .map_err(|_| "ERROR: failed to convert Nat to u256".to_string()),
            DexOrderArgs::Bridge(order) => Erc20Value::try_from(order.deadline.clone())
                .map_err(|_| "ERROR: failed to convert Nat to u256".to_string()),
        }
    }

    pub fn amount(&self) -> Nat {
        match self {
            DexOrderArgs::Swap(order) => order.amount_in.clone(),
            DexOrderArgs::Bridge(order) => order.amount.clone(),
        }
    }

    pub fn max_gas_fee_twin_usdc_amount(
        &self,
        twin_usdc_decimals: u8,
    ) -> Result<Erc20Value, DexOrderError> {
        match self {
            DexOrderArgs::Swap(order) => MaxFeeUsd::new(&order.max_gas_fee_usd)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount)?
                .to_twin_usdc_amount(twin_usdc_decimals)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount),
            DexOrderArgs::Bridge(order) => MaxFeeUsd::new(&order.max_gas_fee_usd)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount)?
                .to_twin_usdc_amount(twin_usdc_decimals)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount),
        }
    }

    pub fn max_gas_fee_amount(
        &self,
        native_token_usd_price_estimate: f64,
    ) -> Result<Wei, DexOrderError> {
        match self {
            DexOrderArgs::Swap(order) => MaxFeeUsd::new(&order.max_gas_fee_usd)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount)?
                .to_native_wei(native_token_usd_price_estimate)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount),
            DexOrderArgs::Bridge(order) => MaxFeeUsd::new(&order.max_gas_fee_usd)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount)?
                .to_native_wei(native_token_usd_price_estimate)
                .map_err(DexOrderError::InvalidMaxUsdFeeAmount),
        }
    }

    pub fn erc20_ledger_burn_index(&self) -> LedgerBurnIndex {
        match self {
            DexOrderArgs::Swap(order) => LedgerBurnIndex::new(
                order
                    .erc20_ledger_burn_index
                    .0
                    .to_u64()
                    .expect("nat does not fit into u64"),
            ),
            DexOrderArgs::Bridge(order) => LedgerBurnIndex::new(
                order
                    .erc20_ledger_burn_index
                    .0
                    .to_u64()
                    .expect("nat does not fit into u64"),
            ),
        }
    }
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct DexSwapOrderArgs {
    #[n(0)]
    pub tx_id: String,
    #[cbor(n(1), with = "crate::cbor::nat")]
    pub amount_in: Nat,
    #[cbor(n(2), with = "crate::cbor::nat")]
    pub min_amount_out: Nat,
    #[n(3)]
    pub commands: Vec<u8>,
    #[n(4)]
    pub commands_data: Vec<String>,
    #[n(5)]
    pub max_gas_fee_usd: String,
    #[cbor(n(6), with = "crate::cbor::nat")]
    pub gas_limit: Nat,
    #[cbor(n(7), with = "crate::cbor::nat")]
    pub deadline: Nat,
    #[n(8)]
    pub recipient: String,
    #[cbor(n(9), with = "crate::cbor::nat")]
    pub erc20_ledger_burn_index: Nat,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct DexBridgeOrderArgs {
    #[n(0)]
    pub tx_id: String,
    #[n(2)]
    pub recipient: String,
    #[cbor(n(3), with = "crate::cbor::nat")]
    pub amount: Nat,
    #[n(4)]
    pub max_gas_fee_usd: String,
    #[cbor(n(5), with = "crate::cbor::nat")]
    pub gas_limit: Nat,
    #[cbor(n(6), with = "crate::cbor::nat")]
    pub deadline: Nat,
    #[cbor(n(7), with = "crate::cbor::nat")]
    pub erc20_ledger_burn_index: Nat,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Encode, Decode)]
pub enum DexOrderError {
    #[n(0)]
    InvalidAmount,
    #[n(1)]
    InvalidMinAmountIn,
    #[n(2)]
    TemporarilyUnavailable(#[n(0)] String),
    #[n(3)]
    InvalidMaxUsdFeeAmount(#[n(0)] String),
    #[n(4)]
    MaxUsdFeeTooLow,
    #[n(5)]
    UsdcAmountInTooLow,
    #[n(6)]
    InvalidCommand(#[n(0)] String),
    #[n(7)]
    InvalidCommandData(#[n(0)] String),
    #[n(8)]
    InvalidRecipient(#[n(0)] String),
    #[n(9)]
    InvalidGasLimit(#[n(0)] String),
    #[n(10)]
    InvalidDeadline(#[n(0)] String),
    #[n(11)]
    NotEnoughGasInGasTank {
        #[cbor(n(0), with = "crate::cbor::nat")]
        requested: Nat,
        #[cbor(n(1), with = "crate::cbor::nat")]
        available: Nat,
    },
}
