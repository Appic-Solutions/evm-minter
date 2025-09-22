use std::time::Duration;

use candid::{Nat, Principal};

use crate::{
    candid_types::{
        chain_data::ChainData,
        withdraw_native::{WithdrawalDetail, WithdrawalSearchParameter},
        MinterInfo, RequestScrapingError,
    },
    tests::{
        minter_flow_tets::mock_rpc_https_responses::{
            generate_and_submit_mock_http_response, MOCK_BSC_FEE_HISTORY_INNER,
            MOCK_FAILED_TRANSACTION_RECEIPT_SWAP_BSC, MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
            MOCK_SEND_TRANSACTION_ERROR, MOCK_SEND_TRANSACTION_SUCCESS,
            MOCK_SWAP_BASE_BLOCK_NUMBER, MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
            MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC, MOCK_TRANSACTION_RECEIPT_SWAP_BSC,
            MOCK_TRANSACTION_RECEIPT_SWAP_BSC_REFUND,
        },
        pocket_ic_helpers::{create_pic, five_ticks, query_call, update_call},
        swap::helpers::{
            base_minter_principal, bsc_minter_principal,
            create_and_install_minters_plus_dependency_canisters,
        },
    },
    RPC_HELPER_PRINCIPAL,
};

#[test]
fn evm_to_evm_swap_happy_path() {
    let pic = create_pic();
    create_and_install_minters_plus_dependency_canisters(&pic);

    pic.advance_time(Duration::from_secs(70));

    // Requesting for another log_scrapping
    update_call::<Nat, Result<(), RequestScrapingError>>(
        &pic,
        base_minter_principal(),
        "request_scraping_logs",
        Nat::from(45944845_u64),
        None,
    )
    .unwrap();

    // update the gas estimate, block number and native usdc price
    update_call::<ChainData, ()>(
        &pic,
        bsc_minter_principal(),
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(23_403_000_u128),
            fee_history: MOCK_BSC_FEE_HISTORY_INNER.into(),
            native_token_usd_price: Some(900.73),
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_SWAP_BASE_BLOCK_NUMBER,
    );

    let minter_info_before_succesful_swap =
        query_call::<(), MinterInfo>(&pic, bsc_minter_principal(), "get_minter_info", ());

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Ankr mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Drpc mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Alchemy mock submissios
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    //
    // At this point there should be 2 http_requests
    // [0] public_node eth_sendRawTransaction
    // [1] ankr eth_sendRawTransaction
    let canister_http_requests = pic.get_canister_http();

    // public_node request
    // Trying to simulate real sendrawtransaction since there will only be one successful result and the rest of the nodes will return
    // one of the failed responses(NonceTooLow,NonceTooHigh,etc..,)
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_SEND_TRANSACTION_SUCCESS,
    );

    // ankr request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Drpc request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Alchemy request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_SEND_TRANSACTION_ERROR,
    );
    // getting the finalized transaction count after sending transaction was successful.

    five_ticks(&pic);
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    // At this point there should be two requests for eth_getTransactionReceipt
    // [0] public_node
    // [1] ankr
    let canister_http_requests = pic.get_canister_http();

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let minter_info_after_succesful_swap =
        query_call::<(), MinterInfo>(&pic, bsc_minter_principal(), "get_minter_info", ());

    // now there should be usdc in the gas tank, the usdc balance should be deducted
    // the native value of gas should be changed, and the next native ledger burn index should be
    // increased
    //
    assert_eq!(
        minter_info_after_succesful_swap
            .next_swap_ledger_burn_index
            .unwrap(),
        minter_info_before_succesful_swap
            .next_swap_ledger_burn_index
            .unwrap()
            + Nat::from(1_u128)
    );

    assert_ne!(
        minter_info_after_succesful_swap
            .gas_tank
            .clone()
            .unwrap()
            .native_balance,
        minter_info_before_succesful_swap
            .gas_tank
            .clone()
            .unwrap()
            .native_balance
    );

    assert_eq!(
        minter_info_after_succesful_swap
            .gas_tank
            .unwrap()
            .usdc_balance,
        minter_info_before_succesful_swap
            .gas_tank
            .unwrap()
            .usdc_balance
        // usdc max gas fee plus the signing fee
            + Nat::from(86_760_000_000_000_000_u128)
    );

    assert_eq!(
        minter_info_after_succesful_swap.erc20_balances.unwrap()[0].balance,
        // amount of usdc balance minus the amount_in for the swap
        minter_info_before_succesful_swap.erc20_balances.unwrap()[0]
            .balance
            .clone()
            - Nat::from(4_244_475_835_882_333_960_u128)
    );
}

#[test]
fn evm_to_evm_swap_refund_path() {
    // there is a spike in the gas price and the dedicated gas can not support the swap anymore so
    // we create a usdc refund request instead of swap request
    let pic = create_pic();
    create_and_install_minters_plus_dependency_canisters(&pic);

    pic.advance_time(Duration::from_secs(70));

    // Requesting for another log_scrapping
    update_call::<Nat, Result<(), RequestScrapingError>>(
        &pic,
        base_minter_principal(),
        "request_scraping_logs",
        Nat::from(45944845_u64),
        None,
    )
    .unwrap();

    // update the gas estimate, block number and native usdc price
    update_call::<ChainData, ()>(
        &pic,
        bsc_minter_principal(),
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(23_403_000_u128),
            fee_history: MOCK_BSC_FEE_HISTORY_INNER.into(),
            native_token_usd_price: Some(3000.73),
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_SWAP_BASE_BLOCK_NUMBER,
    );

    let minter_info_before_succesful_swap =
        query_call::<(), MinterInfo>(&pic, bsc_minter_principal(), "get_minter_info", ());

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Ankr mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Drpc mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Alchemy mock submissios
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    //
    // At this point there should be 2 http_requests
    // [0] public_node eth_sendRawTransaction
    // [1] ankr eth_sendRawTransaction
    let canister_http_requests = pic.get_canister_http();

    // public_node request
    // Trying to simulate real sendrawtransaction since there will only be one successful result and the rest of the nodes will return
    // one of the failed responses(NonceTooLow,NonceTooHigh,etc..,)
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_SEND_TRANSACTION_SUCCESS,
    );

    // ankr request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Drpc request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Alchemy request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_SEND_TRANSACTION_ERROR,
    );
    // getting the finalized transaction count after sending transaction was successful.

    five_ticks(&pic);
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    // At this point there should be two requests for eth_getTransactionReceipt
    // [0] public_node
    // [1] ankr
    let canister_http_requests = pic.get_canister_http();

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC_REFUND,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC_REFUND,
    );

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC_REFUND,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_RECEIPT_SWAP_BSC_REFUND,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let minter_info_after_succesful_swap =
        query_call::<(), MinterInfo>(&pic, bsc_minter_principal(), "get_minter_info", ());

    // now there should be usdc in the gas tank, the usdc balance should be deducted
    // the native value of gas should be changed, and the next native ledger burn index should be
    // increased
    //
    assert_eq!(
        minter_info_after_succesful_swap
            .next_swap_ledger_burn_index
            .unwrap(),
        minter_info_before_succesful_swap
            .next_swap_ledger_burn_index
            .unwrap()
            + Nat::from(1_u128)
    );

    assert_ne!(
        minter_info_after_succesful_swap
            .gas_tank
            .clone()
            .unwrap()
            .native_balance,
        minter_info_before_succesful_swap
            .gas_tank
            .clone()
            .unwrap()
            .native_balance
    );

    assert_eq!(
        minter_info_after_succesful_swap
            .gas_tank
            .unwrap()
            .usdc_balance,
        minter_info_before_succesful_swap
            .gas_tank
            .unwrap()
            .usdc_balance
        // usdc max gas fee plus the signing fee
            + Nat::from(63_008_030_000_000_000_u128)
    );

    assert_eq!(
        minter_info_after_succesful_swap.erc20_balances.unwrap()[0].balance,
        // amount of usdc balance minus the amount_in for the swap
        minter_info_before_succesful_swap.erc20_balances.unwrap()[0]
            .balance
            .clone()
            - Nat::from(4_268_227_805_882_333_960_u128)
    );
}

#[test]
fn evm_to_evm_swap_refund_after_failed_evm_swap() {
    let pic = create_pic();
    create_and_install_minters_plus_dependency_canisters(&pic);

    pic.advance_time(Duration::from_secs(70));

    // Requesting for another log_scrapping
    update_call::<Nat, Result<(), RequestScrapingError>>(
        &pic,
        base_minter_principal(),
        "request_scraping_logs",
        Nat::from(45944845_u64),
        None,
    )
    .unwrap();

    // update the gas estimate, block number and native usdc price
    update_call::<ChainData, ()>(
        &pic,
        bsc_minter_principal(),
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(23_403_000_u128),
            fee_history: MOCK_BSC_FEE_HISTORY_INNER.into(),
            native_token_usd_price: Some(900.73),
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_SWAP_BASE_BLOCK_NUMBER,
    );

    let minter_info_before_succesful_swap =
        query_call::<(), MinterInfo>(&pic, bsc_minter_principal(), "get_minter_info", ());

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Ankr mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Drpc mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    // Alchemy mock submissios
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    //
    // At this point there should be 2 http_requests
    // [0] public_node eth_sendRawTransaction
    // [1] ankr eth_sendRawTransaction
    let canister_http_requests = pic.get_canister_http();

    // public_node request
    // Trying to simulate real sendrawtransaction since there will only be one successful result and the rest of the nodes will return
    // one of the failed responses(NonceTooLow,NonceTooHigh,etc..,)
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_SEND_TRANSACTION_SUCCESS,
    );

    // ankr request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Drpc request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // Alchemy request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_SEND_TRANSACTION_ERROR,
    );
    // getting the finalized transaction count after sending transaction was successful.

    five_ticks(&pic);
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    // At this point there should be two requests for eth_getTransactionReceipt
    // [0] public_node
    // [1] ankr
    let canister_http_requests = pic.get_canister_http();

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_FAILED_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_FAILED_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_FAILED_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_FAILED_TRANSACTION_RECEIPT_SWAP_BSC,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    pic.advance_time(Duration::from_secs(10));

    // update the gas estimate, block number and native usdc price
    update_call::<ChainData, ()>(
        &pic,
        bsc_minter_principal(),
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(23_403_000_u128),
            fee_history: MOCK_BSC_FEE_HISTORY_INNER.into(),
            native_token_usd_price: Some(900.73),
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // now we should check if the reund swap requet is created
    let details = query_call::<WithdrawalSearchParameter, Vec<WithdrawalDetail>>(
        &pic,
        bsc_minter_principal(),
        "withdrawal_status",
        WithdrawalSearchParameter::ByWithdrawalId(10000000000000000001),
    )[0]
    .clone();

    assert_eq!(
        details.recipient_address,
        "0xdAf40D6d8FCFBbFfd1deBA15990B7e08780F7ACe".to_string()
    )
}
