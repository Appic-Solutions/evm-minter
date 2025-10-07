use std::time::Duration;

use candid::{Nat, Principal};
use icrc_ledger_types::icrc1::account::Account;

use crate::{
    candid_types::{chain_data::ChainData, RequestScrapingError},
    tests::{
        minter_flow_tets::mock_rpc_https_responses::{
            generate_and_submit_mock_http_response, MOCK_BSC_FEE_HISTORY_INNER,
            MOCK_GET_SWAP_CONTRACT_BASE_LOGS_EVM_TO_ICP, MOCK_SWAP_BASE_BLOCK_NUMBER,
        },
        pocket_ic_helpers::{create_pic, five_ticks, query_call, update_call},
        swap::helpers::{
            base_minter_principal, bsc_minter_principal, ck_usdc_principal,
            create_and_install_minters_plus_dependency_canisters,
        },
    },
    RPC_HELPER_PRINCIPAL,
};

#[test]
fn evm_to_icp_swap_happy_path() {
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

    let user_balance_before_swap = query_call::<Account, Nat>(
        &pic,
        ck_usdc_principal(),
        "icrc1_balance_of",
        Account::from(
            Principal::from_text("7qi53-mqll3-zmsxo-p4vf5-x3wye-nwsca-oag7a-s4tfq-6htqy-3c3zq-bqe")
                .unwrap(),
        ),
    );

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

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_GET_SWAP_CONTRACT_BASE_LOGS_EVM_TO_ICP,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    let user_balance_after_swap = query_call::<Account, Nat>(
        &pic,
        ck_usdc_principal(),
        "icrc1_balance_of",
        Account::from(
            Principal::from_text("7qi53-mqll3-zmsxo-p4vf5-x3wye-nwsca-oag7a-s4tfq-6htqy-3c3zq-bqe")
                .unwrap(),
        ),
    );

    assert_eq!(
        user_balance_before_swap + Nat::from(41_879_653_u128),
        user_balance_after_swap
    );
}
