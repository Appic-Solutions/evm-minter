use crate::candid_types::dex_orders::DexOrderError;
use crate::evm_config::EvmNetwork;
use crate::logs::DEBUG;
use crate::rpc_declarations::Data;
use crate::state::balances::{release_gas_from_tank_with_usdc, ReleaseGasFromTankError};
use crate::state::transactions::data::Command;
use crate::state::transactions::ExecuteSwapRequest;
use crate::state::TwinUSDCInfo;
use crate::swap::command_data::decode_commands_data;
use crate::tx::gas_fees::{estimate_dex_order_fee, DEFAULT_L1_BASE_GAS_FEE};
use crate::tx::gas_usd::MaxFeeUsd;
use crate::withdraw::{REFUND_FAILED_SWAP_GAS_LIMIT, UNLIMITED_DEADLINE};
use crate::{
    candid_types::dex_orders::DexOrderArgs,
    numeric::{Erc20Value, Wei},
};
use candid::Principal;
use evm_rpc_client::eth_types::Address;
use ic_canister_log::log;

pub mod command_data;

pub async fn build_dex_swap_request(
    args: &DexOrderArgs,
    twin_usdc_info: &TwinUSDCInfo,
    gas_usd_price: f64,
    actual_signing_fee: Erc20Value,
    swap_contract: Address,
    evm_network: EvmNetwork,
    from: Principal,
) -> Result<ExecuteSwapRequest, DexOrderError> {
    let gas_limit = args.gas_limit().map_err(DexOrderError::InvalidGasLimit)?;

    let erc20_tx_fee =
        estimate_dex_order_fee(gas_limit)
            .await
            .ok_or(DexOrderError::TemporarilyUnavailable(
                "Failed to retrieve current gas fee".to_string(),
            ))?;

    // if the max transaction fee is specified in the request use that, else use the estimated tx
    // fee, in the refund transactions the max fee is not represented thats why we should calculate
    // it.
    let max_transaction_fee =
        match args.max_gas_fee_amount(actual_signing_fee, twin_usdc_info.decimals, gas_usd_price) {
            Some(max_transaction_fee) => max_transaction_fee,
            None => erc20_tx_fee,
        };

    let max_gas_fee_twin_usdc = match args
        .max_gas_fee_twin_usdc_amount(twin_usdc_info.decimals, actual_signing_fee)
    {
        Some(max_gas_fee_twin_usdc) => {
            log!(
                DEBUG,
                "[dex_order]: Using specified max gas fee in twin USDC: {:?}",
                max_gas_fee_twin_usdc
            );
            max_gas_fee_twin_usdc
        }
        None => {
            let converted_fee = MaxFeeUsd::twin_usdc_from_native_wei(
                max_transaction_fee,
                gas_usd_price,
                twin_usdc_info.decimals,
            )
            .map_err(DexOrderError::TemporarilyUnavailable)?;
            log!(DEBUG, "[dex_order]: Calculated max gas fee in twin USDC: {:?} from max_transaction_fee: {:?}", 
            converted_fee, max_transaction_fee);
            converted_fee
        }
    };

    let l1_fee = match evm_network {
        EvmNetwork::Base => Some(DEFAULT_L1_BASE_GAS_FEE),
        _ => None,
    };

    let total_required_fee = erc20_tx_fee
        .checked_add(l1_fee.unwrap_or(Wei::ZERO))
        .unwrap_or(Wei::MAX);

    if max_transaction_fee < total_required_fee {
        log!(
            DEBUG,
            "[dex_order]: Max transaction fee too low: {:?}, required: {:?}",
            max_transaction_fee,
            total_required_fee
        );
        return Err(DexOrderError::MaxUsdFeeTooLow);
    }

    let recipient = args.recipient().map_err(DexOrderError::InvalidRecipient)?;
    let deadline = args.deadline().map_err(DexOrderError::InvalidDeadline)?;
    let now = ic_cdk::api::time();
    let erc20_ledger_burn_index = args.erc20_ledger_burn_index();

    let (erc20_amount_in, min_amount_out, all_twin_usdc_fees, commands, commands_data) =
        prepare_order_details(args, max_gas_fee_twin_usdc, actual_signing_fee)?;

    let native_ledger_burn_index =
        release_gas_from_tank_with_usdc(all_twin_usdc_fees, max_transaction_fee, args.tx_id())
            .map_err(
                |ReleaseGasFromTankError {
                     requested,
                     available,
                 }| DexOrderError::NotEnoughGasInGasTank {
                    requested: requested.into(),
                    available: available.into(),
                },
            )?;

    Ok(ExecuteSwapRequest {
        max_transaction_fee,
        erc20_token_in: twin_usdc_info.address,
        erc20_amount_in,
        min_amount_out,
        recipient,
        deadline,
        commands,
        commands_data,
        swap_contract,
        gas_estimate: gas_limit,
        native_ledger_burn_index,
        erc20_ledger_id: twin_usdc_info.ledger_id,
        erc20_ledger_burn_index,
        from,
        from_subaccount: None,
        created_at: now,
        l1_fee,
        withdrawal_fee: None,
        swap_tx_id: args.tx_id(),
        is_refund: false,
    })
}

fn prepare_order_details(
    args: &DexOrderArgs,
    max_gas_fee_twin_usdc: Erc20Value,
    signing_fee: Erc20Value,
) -> Result<(Erc20Value, Erc20Value, Erc20Value, Vec<Command>, Vec<Data>), DexOrderError> {
    let amount_in =
        Erc20Value::try_from(args.amount_in.clone()).map_err(|_| DexOrderError::InvalidAmount)?;
    let min_amount_out = Erc20Value::try_from(args.min_amount_out.clone())
        .map_err(|_| DexOrderError::InvalidAmount)?;

    let all_twin_usdc_fees = max_gas_fee_twin_usdc
        .checked_add(signing_fee)
        .unwrap_or(Erc20Value::MAX);

    let amount_in_minus_fees = amount_in
        .checked_sub(all_twin_usdc_fees)
        .ok_or(DexOrderError::UsdcAmountInTooLow)?;

    let commands_data =
        decode_commands_data(&args.commands_data).map_err(DexOrderError::InvalidCommandData)?;
    let commands = args
        .commands
        .iter()
        .map(|&command| Command::from_u8(command).map_err(DexOrderError::InvalidCommand))
        .collect::<Result<Vec<Command>, DexOrderError>>()?;
    Ok((
        amount_in_minus_fees,
        min_amount_out,
        all_twin_usdc_fees,
        commands,
        commands_data,
    ))
}

pub async fn build_dex_swap_refund_request(
    args: &DexOrderArgs,
    twin_usdc_info: &TwinUSDCInfo,
    gas_usd_price: f64,
    signing_fee: Erc20Value,
    evm_network: EvmNetwork,
    from: Principal,
    swap_contract: Address,
) -> Result<ExecuteSwapRequest, DexOrderError> {
    let amount = args.amount_in.clone();
    let original_amount =
        Erc20Value::try_from(amount).expect("BUG: amount should be valid at this point");
    let recipient = args
        .recipient()
        .expect("BUG: recipient should be valid at this point");

    let erc20_tx_fee = estimate_dex_order_fee(REFUND_FAILED_SWAP_GAS_LIMIT)
        .await
        .ok_or(DexOrderError::TemporarilyUnavailable(
            "Failed to retrieve current gas fee".to_string(),
        ))?;

    let l1_fee = match evm_network {
        EvmNetwork::Base => Some(DEFAULT_L1_BASE_GAS_FEE),
        _ => None,
    };
    let fee_to_be_deducted = erc20_tx_fee
        .checked_add(l1_fee.unwrap_or(Wei::ZERO))
        .expect("Bug: Tx_fee plus l1_fee should fit in u256");

    let max_gas_fee_twin_usdc = MaxFeeUsd::twin_usdc_from_native_wei(
        fee_to_be_deducted,
        gas_usd_price,
        twin_usdc_info.decimals,
    )
    .map_err(|_| DexOrderError::UsdcAmountInTooLow)?;

    let all_twin_usdc_fees = max_gas_fee_twin_usdc
        .checked_add(signing_fee)
        .unwrap_or(Erc20Value::MAX);

    let amount_in = original_amount
        .checked_sub(all_twin_usdc_fees)
        .ok_or(DexOrderError::UsdcAmountInTooLow)?;

    let native_ledger_burn_index =
        release_gas_from_tank_with_usdc(all_twin_usdc_fees, fee_to_be_deducted, args.tx_id())
            .map_err(
                |ReleaseGasFromTankError {
                     requested,
                     available,
                 }| DexOrderError::NotEnoughGasInGasTank {
                    requested: requested.into(),
                    available: available.into(),
                },
            )?;

    let now = ic_cdk::api::time();

    Ok(ExecuteSwapRequest {
        max_transaction_fee: erc20_tx_fee,
        erc20_token_in: twin_usdc_info.address,
        erc20_amount_in: amount_in,
        min_amount_out: amount_in,
        recipient,
        deadline: UNLIMITED_DEADLINE,
        commands: vec![],
        commands_data: vec![],
        swap_contract,
        gas_estimate: REFUND_FAILED_SWAP_GAS_LIMIT,
        native_ledger_burn_index,
        erc20_ledger_id: twin_usdc_info.ledger_id,
        erc20_ledger_burn_index: args.erc20_ledger_burn_index(),
        from,
        from_subaccount: None,
        created_at: now,
        l1_fee,
        withdrawal_fee: None,
        swap_tx_id: args.tx_id(),
        is_refund: true,
    })
}

pub fn is_quarantine_error(err: &DexOrderError) -> bool {
    matches!(
        err,
        DexOrderError::NotEnoughGasInGasTank { .. }
            | DexOrderError::InvalidAmount
            | DexOrderError::InvalidMaxUsdFeeAmount(_)
            | DexOrderError::InvalidRecipient(_)
    )
}
