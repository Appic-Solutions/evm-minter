use rust_decimal::prelude::*;
use rust_decimal::MathematicalOps;

use crate::numeric::Erc20Value;
use crate::numeric::Wei;

#[derive(Clone, Copy, Debug)]
pub struct MaxFeeUsd(pub Decimal);

impl MaxFeeUsd {
    pub fn new(max_fee_str: &str) -> Result<Self, String> {
        Ok(MaxFeeUsd(
            Decimal::from_str_exact(max_fee_str).map_err(|e| e.to_string())?,
        ))
    }

    pub fn to_twin_usdc_amount(&self, decimals: u8) -> Result<Erc20Value, String> {
        let max_fee = self.0;

        if max_fee.is_sign_negative() {
            return Err("Max fee cannot be negative".to_string());
        }

        let ten = Decimal::from(10);
        let multiplier = ten.powu(decimals as u64);

        let amount = max_fee * multiplier;

        let amount_u128 = amount
            .to_u128()
            .ok_or("Amount too large for u128".to_string())?;

        Ok(Erc20Value::from(amount_u128))
    }

    pub fn to_native_wei(&self, native_price_usd: f64) -> Result<Wei, String> {
        let max_fee = self.0;

        if max_fee.is_sign_negative() {
            return Err("Max fee cannot be negative".to_string());
        }

        let native_price =
            Decimal::from_f64(native_price_usd).ok_or("Invalid native price value".to_string())?;

        if native_price.is_zero() {
            return Err("Native price cannot be zero".to_string());
        }

        let ten = Decimal::from(10);
        let multiplier = ten.powu(18u64);

        let amount = (max_fee / native_price) * multiplier;

        let amount_u128 = amount
            .to_u128()
            .ok_or("Amount too large for u128".to_string())?;

        Ok(Wei::from(amount_u128))
    }

    pub fn twin_usdc_from_native_wei(
        native_amount: Wei,
        native_price_usd: f64,
        decimals: u8,
    ) -> Result<Erc20Value, String> {
        if native_price_usd <= 0.0 {
            return Err("Native price must be positive".to_string());
        }
        let native_amount_dec =
            Decimal::from_f64(native_amount.as_f64()).ok_or("Invalid native amount".to_string())?;

        let ten = Decimal::from(10);
        let native_decimals = ten.powu(18u64);
        let native_in_units = native_amount_dec / native_decimals;
        let native_price =
            Decimal::from_f64(native_price_usd).ok_or("Invalid native price value".to_string())?;
        let usd_value = native_in_units * native_price;
        let multiplier = ten.powu(decimals as u64);
        let amount = usd_value * multiplier;
        let amount_u128 = amount
            .to_u128()
            .ok_or("Amount too large for u128".to_string())?;
        Ok(Erc20Value::from(amount_u128))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::numeric::Erc20Value;
    #[test]
    fn test_to_twin_usdc_amount_happy_path() {
        let max_fee = MaxFeeUsd::new("1.0").unwrap();
        let result = max_fee.to_twin_usdc_amount(6).unwrap();
        assert_eq!(result, Erc20Value::from(1_000_000u128)); // 1 * 10^6
    }
    #[test]
    fn test_to_twin_usdc_amount_another_happy_path() {
        let max_fee = MaxFeeUsd::new("3.5").unwrap();
        let result = max_fee.to_twin_usdc_amount(2).unwrap();
        assert_eq!(result, Erc20Value::from(350u128)); // 3.5 * 10^2 = 350
    }
    #[test]
    fn test_to_twin_usdc_amount_zero_decimals() {
        let max_fee = MaxFeeUsd::new("5.0").unwrap();
        let result = max_fee.to_twin_usdc_amount(0).unwrap();
        assert_eq!(result, Erc20Value::from(5u128)); // 5 * 10^0 = 5
    }
    #[test]
    fn test_to_twin_usdc_amount_negative() {
        let max_fee = MaxFeeUsd::new("-1.0").unwrap();
        let result = max_fee.to_twin_usdc_amount(6);
        assert_eq!(result, Err("Max fee cannot be negative".to_string()));
    }
    #[test]
    fn test_to_twin_usdc_amount_invalid_nan() {
        let max_fee = MaxFeeUsd::new(&f64::NAN.to_string());
        assert!(max_fee.is_err());
    }
    #[test]
    fn test_to_twin_usdc_amount_too_large() {
        let max_fee = MaxFeeUsd::new(&1e40.to_string()).unwrap();
        let result = max_fee.to_twin_usdc_amount(0);
        assert_eq!(result, Err("Amount too large for u128".to_string()));
    }
    #[test]
    fn test_to_twin_usdc_amount_fractional_truncation() {
        let max_fee = MaxFeeUsd::new("1.999").unwrap();
        let result = max_fee.to_twin_usdc_amount(0).unwrap();
        assert_eq!(result, Erc20Value::from(1u128)); // Truncated to 1
    }
    #[test]
    fn test_to_native_wei_happy_path() {
        let max_fee = MaxFeeUsd::new("1.0").unwrap();
        let result = max_fee.to_native_wei(0.1).unwrap();
        assert_eq!(result, Wei::from(10_000_000_000_000_000_000u128)); // 1 / 0.1 * 10^18 = 10 * 10^18 = 10^19
    }
    #[test]
    fn test_to_native_wei_another_happy_path() {
        let max_fee = MaxFeeUsd::new("3.0").unwrap();
        let result = max_fee.to_native_wei(1.0).unwrap();
        assert_eq!(result, Wei::from(3_000_000_000_000_000_000u128)); // 3 / 1 * 10^18 = 3e18
    }
    #[test]
    fn test_to_native_wei_zero_price() {
        let max_fee = MaxFeeUsd::new("1.0").unwrap();
        let result = max_fee.to_native_wei(0.0);
        assert_eq!(result, Err("Native price cannot be zero".to_string()));
    }
    #[test]
    fn test_to_native_wei_negative_price() {
        let max_fee = MaxFeeUsd::new("1.0").unwrap();
        let result = max_fee.to_native_wei(-1.0);
        // Division results in negative amount, which fails to_u128
        assert_eq!(result, Err("Amount too large for u128".to_string()));
    }
    #[test]
    fn test_to_native_wei_negative_max_fee() {
        let max_fee = MaxFeeUsd::new("-1.0").unwrap();
        let result = max_fee.to_native_wei(1.0);
        assert_eq!(result, Err("Max fee cannot be negative".to_string()));
    }
    #[test]
    fn test_to_native_wei_invalid_max_fee_nan() {
        let max_fee = MaxFeeUsd::new(&f64::NAN.to_string());
        assert!(max_fee.is_err());
    }
    #[test]
    fn test_to_native_wei_invalid_price_nan() {
        let max_fee = MaxFeeUsd::new("1.0").unwrap();
        let result = max_fee.to_native_wei(f64::NAN);
        assert_eq!(result, Err("Invalid native price value".to_string()));
    }
    #[test]
    fn test_to_native_wei_too_large() {
        let max_fee = MaxFeeUsd::new(&1e40.to_string()).unwrap();
        let result = max_fee.to_native_wei(1.0);
        assert_eq!(result, Err("Amount too large for u128".to_string()));
    }
    #[test]
    fn test_to_native_wei_fractional_truncation() {
        let max_fee = MaxFeeUsd::new("1.0").unwrap();
        let result = max_fee.to_native_wei(3.0).unwrap();
        assert_eq!(result, Wei::from(333_333_333_333_333_333u128)); // 1 / 3 * 10^18 ≈ 0.333... * 10^18, truncated
    }
    #[test]
    fn test_twin_usdc_from_native_wei_happy_path() {
        let native_amount = Wei::from(1_000_000_000_000_000_000u128); // 1 native
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, 1.0, 6).unwrap();
        assert_eq!(result, Erc20Value::from(1_000_000u128)); // 1 * 1 * 10^6
    }
    #[test]
    fn test_twin_usdc_from_native_wei_another_happy_path() {
        let native_amount = Wei::from(2_000_000_000_000_000_000u128); // 2 native
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, 3.5, 2).unwrap();
        assert_eq!(result, Erc20Value::from(700u128)); // 2 * 3.5 * 10^2 = 700
    }
    #[test]
    fn test_twin_usdc_from_native_wei_zero_decimals() {
        let native_amount = Wei::from(1_000_000_000_000_000_000u128);
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, 5.0, 0).unwrap();
        assert_eq!(result, Erc20Value::from(5u128)); // 1 * 5 * 10^0 = 5
    }
    #[test]
    fn test_twin_usdc_from_native_wei_negative_price() {
        let native_amount = Wei::from(1u128);
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, -1.0, 6);
        assert_eq!(result, Err("Native price must be positive".to_string()));
    }
    #[test]
    fn test_twin_usdc_from_native_wei_zero_price() {
        let native_amount = Wei::from(1u128);
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, 0.0, 6);
        assert_eq!(result, Err("Native price must be positive".to_string()));
    }
    #[test]
    fn test_twin_usdc_from_native_wei_invalid_price_nan() {
        let native_amount = Wei::from(1u128);
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, f64::NAN, 6);
        assert_eq!(result, Err("Invalid native price value".to_string()));
    }
    #[test]
    fn test_twin_usdc_from_native_wei_too_large() {
        let native_amount = Wei::from(u128::MAX);
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, 1.0, 0);
        assert_eq!(result, Err("Amount too large for u128".to_string()));
    }
    #[test]
    fn test_twin_usdc_from_native_wei_fractional_truncation() {
        let native_amount = Wei::from(1u128); // 1 wei
        let result = MaxFeeUsd::twin_usdc_from_native_wei(native_amount, 3.0, 0).unwrap();
        assert_eq!(result, Erc20Value::from(0u128)); // truncated to 0
    }
}
