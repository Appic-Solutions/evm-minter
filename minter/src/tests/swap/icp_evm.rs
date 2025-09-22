use std::time::Duration;

use candid::{Nat, Principal};

use crate::{
    candid_types::{chain_data::ChainData, MinterInfo, RequestScrapingError},
    tests::{
        dex_types::{CrosschainSwapArgs, CrosschainSwapError},
        minter_flow_tets::mock_rpc_https_responses::{
            generate_and_submit_mock_http_response, MOCK_BASE_FEE_HISTORY_INNER,
            MOCK_BSC_FEE_HISTORY_INNER, MOCK_GET_SWAP_CONTRACT_BASE_LOGS,
            MOCK_SEND_TRANSACTION_ERROR, MOCK_SEND_TRANSACTION_SUCCESS,
            MOCK_SWAP_BASE_BLOCK_NUMBER, MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BASE,
            MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC, MOCK_TRANSACTION_COUNT_LATEST_SWAP_BASE,
            MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC, MOCK_TRANSACTION_RECEIPT_SWAP_BASE,
            MOCK_TRANSACTION_RECEIPT_SWAP_BSC, MOCK_TRANSACTION_RECEIPT_SWAP_BSC_REFUND,
        },
        pocket_ic_helpers::{create_pic, five_ticks, query_call, update_call},
        swap::helpers::{
            base_minter_principal, bsc_minter_principal,
            create_and_install_minters_plus_dependency_canisters, dex_principal_id,
        },
    },
    APPIC_CONTROLLER_PRINCIPAL, RPC_HELPER_PRINCIPAL,
};

#[test]
fn icp_to_evm_swap_happy_path() {
    let pic = create_pic();
    create_and_install_minters_plus_dependency_canisters(&pic);

    // update the gas estimate, block number and native usdc price
    update_call::<ChainData, ()>(
        &pic,
        base_minter_principal(),
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(572_530_960_u128),
            fee_history: MOCK_BASE_FEE_HISTORY_INNER.into(),
            native_token_usd_price: Some(4021.73),
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    let minter_info_before_succesful_swap =
        query_call::<(), MinterInfo>(&pic, base_minter_principal(), "get_minter_info", ());

    // add the swap request to the dex from ckUSDC to WETH on bsc
    let result=update_call::<CrosschainSwapArgs, Result<String, CrosschainSwapError>>(
        &pic,
        dex_principal_id(),
        "cross_chain_swap",
        CrosschainSwapArgs {
            encoded_swap_data: "0xf902628831303030303030309032333439313439363935363838393830903233333033353634393831323334363884302e3825f9022ff869836963708831303030303030308739393633383332873939333339343084302e332530303030f83ef83c9b7865766e6d2d67616161612d61616161722d7161666e712d6361699b716b7277702d7a696161612d61616161672d6175656d712d63616983313030c0c030f901c1843834353387393838343632329032333439313439363935363838393830903233333033353634393831323334363884302e38258633343938373088323935363739363688302e30343434353930f85cf85aaa307838333335383966434436654462364530386634633743333244346637316235346264413032393133aa30783432303030303030303030303030303030303030303030303030303030303030303030303030303683313030c131f90105b901023078303030303030303030303030303030303030303030303030383333353839666364366564623665303866346337633332643466373162353462646130323931333030303030303030303030303030303030303030303030303432303030303030303030303030303030303030303030303030303030303030303030303030303630303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030383464646234336638353732378a31373538353531313333".to_string(),
            recipient:"0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace".to_string(),
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    ).unwrap();

    println!("{result}");

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BASE,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BASE,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BASE,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_LATEST_SWAP_BASE,
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
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BASE,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BASE,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BASE,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BASE,
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
        MOCK_TRANSACTION_RECEIPT_SWAP_BASE,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_RECEIPT_SWAP_BASE,
    );

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_RECEIPT_SWAP_BASE,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_RECEIPT_SWAP_BASE,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let minter_info_after_succesful_swap =
        query_call::<(), MinterInfo>(&pic, base_minter_principal(), "get_minter_info", ());

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
            + Nat::from(74_459_u128)
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
