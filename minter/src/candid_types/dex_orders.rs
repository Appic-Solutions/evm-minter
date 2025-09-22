use crate::{
    numeric::{Erc20Value, GasAmount, Wei},
    tx::gas_usd::MaxFeeUsd,
};

use num_traits::ToPrimitive;

use super::*;

// candid file designed for operations sent by appic dex
#[derive(CandidType, Deserialize, Clone, Debug, Encode, Decode, Eq, PartialEq)]
pub struct DexOrderArgs {
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
    pub max_gas_fee_usd: Option<String>,
    #[n(6)]
    pub signing_fee: Option<String>,
    #[cbor(n(7), with = "crate::cbor::nat")]
    pub gas_limit: Nat,
    #[cbor(n(8), with = "crate::cbor::nat")]
    pub deadline: Nat,
    #[n(9)]
    pub recipient: String,
    #[cbor(n(10), with = "crate::cbor::nat")]
    pub erc20_ledger_burn_index: Nat,
    #[n(11)]
    pub is_refund: bool,
}

impl DexOrderArgs {
    pub fn tx_id(&self) -> String {
        self.tx_id.to_lowercase()
    }

    pub fn gas_limit(&self) -> Result<GasAmount, String> {
        GasAmount::try_from(self.gas_limit.clone())
            .map_err(|_| "ERROR: failed to convert Nat to u256".to_string())
    }

    pub fn recipient(&self) -> Result<Address, String> {
        Address::from_str(&self.recipient).map_err(|_| "ERROR: Invalid recipient".to_string())
    }

    pub fn deadline(&self) -> Result<Erc20Value, String> {
        Erc20Value::try_from(self.deadline.clone())
            .map_err(|_| "ERROR: failed to convert Nat to u256".to_string())
    }

    pub fn amount(&self) -> Nat {
        self.amount_in.clone()
    }

    pub fn max_gas_fee_twin_usdc_amount(
        &self,
        twin_usdc_decimals: u8,
        canister_singing_fee: Erc20Value,
    ) -> Option<Erc20Value> {
        let max_fee_usd = MaxFeeUsd::new(&self.max_gas_fee_usd.clone()?)
            .ok()?
            .to_twin_usdc_amount(twin_usdc_decimals)
            .ok()?;

        // dedicated_signing_fee_twin_usdc_amount - actuall canister signing fee
        let unused_signing_fee = self
            .dedicated_signing_fee_twin_usdc_amount(twin_usdc_decimals)
            .unwrap_or(Erc20Value::ZERO)
            .checked_sub(canister_singing_fee)
            .unwrap_or(Erc20Value::ZERO);

        Some(
            max_fee_usd
                .checked_add(unused_signing_fee)
                .unwrap_or(max_fee_usd),
        )
    }

    pub fn max_gas_fee_amount(
        &self,
        canister_singing_fee: Erc20Value,
        twin_usdc_decimals: u8,
        native_token_usd_price_estimate: f64,
    ) -> Option<Wei> {
        let max_gas_fee_twin_usdc_amount =
            self.max_gas_fee_twin_usdc_amount(twin_usdc_decimals, canister_singing_fee)?;

        MaxFeeUsd::native_wei_from_twin_usdc(
            max_gas_fee_twin_usdc_amount,
            native_token_usd_price_estimate,
            twin_usdc_decimals,
        )
        .ok()
    }

    pub fn erc20_ledger_burn_index(&self) -> LedgerBurnIndex {
        LedgerBurnIndex::new(
            self.erc20_ledger_burn_index
                .0
                .to_u64()
                .expect("nat does not fit into u64"),
        )
    }

    pub fn dedicated_signing_fee_twin_usdc_amount(
        &self,
        twin_usdc_decimals: u8,
    ) -> Option<Erc20Value> {
        MaxFeeUsd::new(&self.signing_fee.clone()?)
            .ok()?
            .to_twin_usdc_amount(twin_usdc_decimals)
            .ok()
    }
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
