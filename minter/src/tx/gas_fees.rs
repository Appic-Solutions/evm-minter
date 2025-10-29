use crate::{
    guard::TimerGuard,
    logs::{DEBUG, INFO},
    numeric::{GasAmount, Wei, WeiPerGas},
    rpc_client::{MultiCallError, RpcClient},
    rpc_declarations::{BlockSpec, BlockTag, CallParams, FeeHistory, FeeHistoryParams, Quantity},
    state::{mutate_state, read_state, TaskType},
    withdraw::{
        ERC20_APPROVAL_TRANSACTION_GAS_LIMIT, ERC20_MINT_TRANSACTION_GAS_LIMIT,
        ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT,
    },
};
use evm_rpc_client::{eth_types::Address, Hex};
use ic_canister_log::log;
use std::str::FromStr;

/// Represents an estimate of gas fees.
///
/// Contains the base fee per gas and the maximum priority fee per gas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GasFeeEstimate {
    pub base_fee_per_gas: WeiPerGas,
    pub max_priority_fee_per_gas: WeiPerGas,
}

impl GasFeeEstimate {
    /// Computes the maximum fee per gas by doubling the base fee and adding the priority fee.
    ///
    /// # Returns
    /// An `Option` containing the estimated maximum fee per gas if it does not overflow, otherwise `None`.
    pub fn checked_estimate_max_fee_per_gas(&self) -> Option<WeiPerGas> {
        self.base_fee_per_gas
            .checked_mul(2_u8)
            .and_then(|base_fee_estimate| {
                base_fee_estimate.checked_add(self.max_priority_fee_per_gas)
            })
    }

    /// Estimates the maximum fee per gas. Falls back to `WeiPerGas::MAX` if the calculation fails.
    ///
    /// # Returns
    /// The estimated maximum fee per gas.
    pub fn estimate_max_fee_per_gas(&self) -> WeiPerGas {
        self.checked_estimate_max_fee_per_gas()
            .unwrap_or(WeiPerGas::MAX)
    }

    /// Converts the gas fee estimate to a transaction price with a specified gas limit.
    ///
    /// # Arguments
    /// * `gas_limit` - The gas limit for the transaction.
    ///
    /// # Returns
    /// A `TransactionPrice` containing the gas limit and fee estimates.
    pub fn to_price(self, gas_limit: GasAmount) -> TransactionPrice {
        TransactionPrice {
            gas_limit,
            max_fee_per_gas: self.estimate_max_fee_per_gas(),
            max_priority_fee_per_gas: self.max_priority_fee_per_gas,
        }
    }

    /// Computes the minimum of the maximum fee per gas by adding base and priority fees.
    /// Falls back to `WeiPerGas::MAX` if the calculation fails.
    ///
    /// # Returns
    /// The minimum maximum fee per gas.
    pub fn min_max_fee_per_gas(&self) -> WeiPerGas {
        self.base_fee_per_gas
            .checked_add(self.max_priority_fee_per_gas)
            .unwrap_or(WeiPerGas::MAX)
    }
}

/// Represents the price of a transaction.
///
/// Includes the gas limit, maximum fee per gas, and maximum priority fee per gas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionPrice {
    pub gas_limit: GasAmount,
    pub max_fee_per_gas: WeiPerGas,
    pub max_priority_fee_per_gas: WeiPerGas,
}

impl TransactionPrice {
    /// Computes the maximum transaction fee based on the gas limit and maximum fee per gas.
    ///
    /// # Returns
    /// The maximum transaction fee if calculation is successful, otherwise `Wei::MAX`.
    pub fn max_transaction_fee(&self) -> Wei {
        self.max_fee_per_gas
            .transaction_cost(self.gas_limit)
            .unwrap_or(Wei::MAX)
    }

    /// Estimates the new transaction price required to resubmit a transaction with updated gas fees.
    ///
    /// If the current transaction price is sufficient, it remains unchanged. Otherwise, it adjusts
    /// the maximum priority fee and possibly the maximum fee per gas to ensure the transaction can be resubmitted.
    ///
    /// # Arguments
    /// * `new_gas_fee` - The new gas fee estimate.
    ///
    /// # Returns
    /// A new `TransactionPrice` with updated values.
    pub fn resubmit_transaction_price(self, new_gas_fee: GasFeeEstimate) -> Self {
        let plus_10_percent = |amount: WeiPerGas| {
            amount
                .checked_add(
                    amount
                        .checked_div_ceil(10_u8)
                        .expect("BUG: must be Some() because divisor is non-zero"),
                )
                .unwrap_or(WeiPerGas::MAX)
        };

        if self.max_fee_per_gas >= new_gas_fee.min_max_fee_per_gas()
            && self.max_priority_fee_per_gas >= new_gas_fee.max_priority_fee_per_gas
        {
            self
        } else {
            // Increase max_priority_fee_per_gas by at least 10% if necessary, ensuring the transaction
            // remains resubmittable. Update max_fee_per_gas if it doesn't cover the new max_priority_fee_per_gas.
            let updated_max_priority_fee_per_gas = plus_10_percent(self.max_priority_fee_per_gas)
                .max(new_gas_fee.max_priority_fee_per_gas);
            let new_gas_fee = GasFeeEstimate {
                max_priority_fee_per_gas: updated_max_priority_fee_per_gas,
                ..new_gas_fee
            };
            let new_max_fee_per_gas = new_gas_fee.min_max_fee_per_gas().max(self.max_fee_per_gas);
            TransactionPrice {
                gas_limit: self.gas_limit,
                max_fee_per_gas: new_max_fee_per_gas,
                max_priority_fee_per_gas: updated_max_priority_fee_per_gas,
            }
        }
    }
}

/// Asynchronously refreshes the gas fee estimate.
///
/// Uses a cached estimate if it is recent enough. Otherwise, fetches the latest fee history and recalculates the estimate.
///
/// # Returns
/// An `Option` containing the new `GasFeeEstimate` if successful, or `None` if the refresh fails.
pub async fn lazy_refresh_gas_fee_estimate() -> Option<GasFeeEstimate> {
    const MAX_AGE_NS: u64 = 10_000_000_000_u64; // 10 seconds

    async fn do_refresh() -> Option<GasFeeEstimate> {
        let _guard = match TimerGuard::new(TaskType::RefreshGasFeeEstimate) {
            Ok(guard) => guard,
            Err(e) => {
                log!(
                    DEBUG,
                    "[refresh_gas_fee_estimate]: Failed retrieving guard: {e:?}",
                );
                return None;
            }
        };

        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 3;

        let fee_history = loop {
            match get_fee_history().await {
                Ok(fee_history) => break fee_history, // Exit loop on success
                Err(e) => {
                    attempts += 1;
                    log!(
                        DEBUG,
                        "[refresh_gas_fee_estimate]: Failed retrieving fee history: {e:?} attempt {}/{} failed",
                        attempts,
                        MAX_ATTEMPTS
                    );

                    if attempts >= MAX_ATTEMPTS {
                        log!(
                        DEBUG,
                        "[refresh_gas_fee_estimate]: max retries reached. Skipping scrapping fee history fetching."
                    );
                        return None;
                    }
                }
            }
        };

        let gas_fee_estimate = match estimate_transaction_fee(&fee_history) {
            Ok(estimate) => {
                mutate_state(|s| {
                    s.last_transaction_price_estimate =
                        Some((ic_cdk::api::time(), estimate.clone()));
                });
                estimate
            }
            Err(e) => {
                log!(
                    INFO,
                    "[refresh_gas_fee_estimate]: Failed estimating gas fee: {e:?}",
                );
                return None;
            }
        };
        log!(
            INFO,
            "[refresh_gas_fee_estimate]: Estimated transaction fee: {:?}",
            gas_fee_estimate,
        );
        Some(gas_fee_estimate)
    }

    async fn get_fee_history() -> Result<FeeHistory, MultiCallError<FeeHistory>> {
        read_state(|s| {
            RpcClient::from_state_one_provider(
                s,
                crate::rpc_client::providers::Provider::PublicNode,
            )
        })
        .fee_history(FeeHistoryParams {
            block_count: Quantity::from(5_u8),
            highest_block: BlockSpec::Tag(BlockTag::Latest),
            reward_percentiles: vec![50],
        })
        .await
    }

    let now_ns = ic_cdk::api::time();
    match read_state(|s| s.last_transaction_price_estimate.clone()) {
        Some((last_estimate_timestamp_ns, estimate))
            if now_ns < last_estimate_timestamp_ns.saturating_add(MAX_AGE_NS) =>
        {
            Some(estimate)
        }
        _ => do_refresh().await,
    }
}

/// Possible errors when estimating transaction fees.
#[derive(Debug, PartialEq, Eq)]
pub enum TransactionFeeEstimationError {
    InvalidFeeHistory(String),
    Overflow(String),
}

/// Estimates
/// the transaction fee based on fee history.
/// Determines the base fee per gas for the next block and computes the maximum priority fee based on historic values.
/// Returns an estimate of the gas fee.
///
/// # Arguments
/// * `fee_history` - The fee history to use for estimation.
///
/// # Returns
/// A `Result` containing the `GasFeeEstimate` if successful, or an error if estimation fails.
pub fn estimate_transaction_fee(
    fee_history: &FeeHistory,
) -> Result<GasFeeEstimate, TransactionFeeEstimationError> {
    let min_max_priority_fee_per_gas: WeiPerGas =
        read_state(|state| state.min_max_priority_fee_per_gas); // Different on each network

    let base_fee_per_gas_next_block = *fee_history.base_fee_per_gas.last().ok_or(
        TransactionFeeEstimationError::InvalidFeeHistory(
            "base_fee_per_gas should not be empty to be able to evaluate transaction price"
                .to_string(),
        ),
    )?;

    let max_priority_fee_per_gas = {
        let mut rewards: Vec<&WeiPerGas> = fee_history.reward.iter().flatten().collect();
        let historic_max_priority_fee_per_gas =
            **median(&mut rewards).ok_or(TransactionFeeEstimationError::InvalidFeeHistory(
                "should be non-empty with rewards of the last 5 blocks".to_string(),
            ))?;
        historic_max_priority_fee_per_gas.max(min_max_priority_fee_per_gas)
    };

    let gas_fee_estimate = GasFeeEstimate {
        base_fee_per_gas: base_fee_per_gas_next_block,
        max_priority_fee_per_gas,
    };

    if gas_fee_estimate
        .checked_estimate_max_fee_per_gas()
        .is_none()
    {
        return Err(TransactionFeeEstimationError::Overflow(
            "max_fee_per_gas overflowed".to_string(),
        ));
    }

    Ok(gas_fee_estimate)
}

pub async fn estimate_erc20_transaction_fee() -> Option<Wei> {
    lazy_refresh_gas_fee_estimate()
        .await
        .map(|gas_fee_estimate| {
            gas_fee_estimate
                .to_price(ERC20_WITHDRAWAL_TRANSACTION_GAS_LIMIT)
                .max_transaction_fee()
        })
}

pub async fn estimate_icrc_wrap_transaction_fee() -> Option<Wei> {
    lazy_refresh_gas_fee_estimate()
        .await
        .map(|gas_fee_estimate| {
            gas_fee_estimate
                .to_price(ERC20_MINT_TRANSACTION_GAS_LIMIT)
                .max_transaction_fee()
        })
}

pub async fn estimate_usdc_approval_fee() -> Option<Wei> {
    lazy_refresh_gas_fee_estimate()
        .await
        .map(|gas_fee_estimate| {
            gas_fee_estimate
                .to_price(ERC20_APPROVAL_TRANSACTION_GAS_LIMIT)
                .max_transaction_fee()
        })
}

pub async fn estimate_dex_order_fee(gas_estimate: GasAmount) -> Option<Wei> {
    lazy_refresh_gas_fee_estimate()
        .await
        .map(|gas_fee_estimate| {
            gas_fee_estimate
                .to_price(gas_estimate)
                .max_transaction_fee()
        })
}

/// Computes the median of a slice of values.
///
/// # Arguments
/// * `values` - The slice of values to compute the median of.
///
/// # Returns
/// An `Option` containing the median value, or `None` if the slice is empty.
fn median<T: Ord>(values: &mut [T]) -> Option<&T> {
    if values.is_empty() {
        return None;
    }
    let (_, item, _) = values.select_nth_unstable(values.len() / 2);
    Some(item)
}

// L1 Fee Estimates,
// Some l2s like Op and Base include an extra l1 fee,
// That can be calculated via calls to fee oracles, by passing the sample transaction
// The fee calculation is based on tx bytes length.

// Native Transfer (EIP-1559):
// Typical Size: 111–114 bytes.

// ERC-20 Transfer (EIP-1559):
// Typical Size: 172–180 bytes.

const ORACLE_ADDRESS: &str = "0xb528D11cC114E026F138fE568744c6D45ce6Da7A";

const GET_L1_GAS_FUNCTION_SELECTOR: [u8; 4] = hex_literal::hex!("49948e0e");

const SAMPLE_ERC20_TRANSFER_TX:&str="02f86c0185059682f008503b9aca00825208946b175474e89094c44da98b954eedeac495271d0f80b844a9059cbb0000000000000000000000005a3c9f349cbf0d6b4c7a5671e3d06ce72c826b6b000000000000000000000000000000000000000000000000000000000016345785d8a0000c080a09d14d0d3aabc124ff0c6875b1d6db7a632a53cd0b62e94942be6b850a05af21a074bc3c2cf34378480b44843544f3d0430c8131f64cbaec99a256468e00dbf5d6";

const SAMPLE_CALLDATA_FOR_GET_L1_FEE:&str="49948e0e000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000b602f86c0185059682f008503b9aca00825208946b175474e89094c44da98b954eedeac495271d0f80b844a9059cbb0000000000000000000000005a3c9f349cbf0d6b4c7a5671e3d06ce72c826b6b000000000000000000000000000000000000000000000000000000000016345785d8a0000c080a09d14d0d3aabc124ff0c6875b1d6db7a632a53cd0b62e94942be6b850a05af21a074bc3c2cf34378480b44843544f3d0430c8131f64cbaec99a256468e00dbf5d600000000000000000000";

// Components of the Transaction
// Transaction Type: 0x02 (EIP-1559).
// Chain ID: 0x2119 (Base).
// Nonce: 0x00 (0).
// Max Priority Fee Per Gas: 0x3b9aca00 (1 Gwei).
// Max Fee Per Gas: 0x12a05f200 (50 Gwei).
// Gas Limit: 0xfde8 (65,000, typical for ERC-20 transfer).
// To: ERC-20 contract address (e.g., 0xabcdef1234567890abcdef1234567890abcdef12).
// Value: 0x00 (no ETH sent).
// Data: ABI-encoded transfer(address,uint256) call.
// Access List: Empty (0x80).
// Signature: 65 bytes (v, r, s).

// Sample erc20 tx is slightly longer than native transfer, so we choose erc20 transfer for both of
// them to add some buffer for native transfer as well

/// Asynchronously refreshes the l1 fee estimate.
///
/// fetches the latest l1_fee from gas_fee oracle.
/// Add a 10% buffer to the fetched estimate to make a safe zone.
///
/// # Returns
/// An `Option` containing the new  l1 fee estimate in `Wei` if successful, or `None` if the fetch fails.
// we se default l1 fee instead of fetching it on-chain becuase its high enough to cover the any
// sort of transaction cost.
pub const DEFAULT_L1_BASE_GAS_FEE: Wei = Wei::new(1000000000000_u128);
pub async fn lazy_fetch_l1_fee_estimate() -> Option<Wei> {
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 3;

    let l1_fee = loop {
        match get_l1_fee().await {
            Ok(fee_estimate) => break fee_estimate.to_string(), // Exit loop on success
            Err(e) => {
                attempts += 1;
                log!(
                    DEBUG,
                    "[fetch_l1_fee_estimate]: Failed retrieving l1 fee: {e:?} attempt {}/{} failed",
                    attempts,
                    MAX_ATTEMPTS
                );

                if attempts >= MAX_ATTEMPTS {
                    log!(
                        DEBUG,
                        "[fetch_l1_fee_estimate]: max retries reached. Skipping l1 fee estimation."
                    );
                    return None;
                }
            }
        }
    };

    async fn get_l1_fee() -> Result<Hex, MultiCallError<Hex>> {
        let chain_id = read_state(|s| s.evm_network()).chain_id();

        read_state(RpcClient::from_state_all_providers)
            .eth_call(CallParams {
                transaction: crate::rpc_declarations::TransactionRequestParams {
                    tx_type: None,
                    nonce: None,
                    to: Some(
                        Address::from_str(ORACLE_ADDRESS)
                            .expect("sould not fail converting oracle address"),
                    ),
                    from: None,
                    gas: None,
                    value: None,
                    input: Some(
                        hex::decode(SAMPLE_CALLDATA_FOR_GET_L1_FEE)
                            .expect("bug: Failed to decode dummy transactioncall data"),
                    ),
                    gas_price: None,
                    max_priority_fee_per_gas: None,
                    max_fee_per_gas: None,
                    max_fee_per_blob_gas: None,
                    access_list: None,
                    blob_versioned_hashes: None,
                    blobs: None,
                    chain_id: Some(chain_id),
                },
                block: Some(BlockSpec::Tag(BlockTag::Latest)),
            })
            .await
    }

    Some(parse_l1_fee_resposne(l1_fee))
}

fn parse_l1_fee_resposne(l1_fee_string: String) -> Wei {
    Wei::from_str_hex(&l1_fee_string).expect("expected a correct unint 256 hex string")
}

#[test]
fn check_inpts() {
    use ethnum::U256;
    use evm_rpc_client::evm_rpc_types::Hex;

    let generated_tx_call_data = {
        let tx_bytes =
            hex::decode(SAMPLE_ERC20_TRANSFER_TX).expect("Failed to decode tx_data into bytes");
        let mut data = Vec::with_capacity(4 + 32 + 32 + tx_bytes.len());

        // 1. Function selector
        data.extend(GET_L1_GAS_FUNCTION_SELECTOR);

        // 2. Offset to data (always 0x20 since data starts right after this 32-byte word)
        data.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x20,
        ]);

        // 3. Length of the transaction data
        let len = tx_bytes.len();
        data.extend_from_slice(&U256::from(len as u64).to_be_bytes()[..32]);

        // 4. The actual transaction data
        data.extend_from_slice(&tx_bytes);

        data.extend_from_slice(&[0u8; 10]); // 5 zero bytes
        data
    };

    assert_eq!(
        Hex::from(generated_tx_call_data),
        Hex::from(hex::decode(SAMPLE_CALLDATA_FOR_GET_L1_FEE).expect("Failed to convert to hex"))
    );
}
