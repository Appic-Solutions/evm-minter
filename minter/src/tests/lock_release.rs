use std::time::Duration;

use candid::{Nat, Principal};
use icrc_ledger_types::{
    icrc1::account::Account,
    icrc2::approve::{ApproveArgs, ApproveError},
};

use crate::{
    candid_types::{
        wrapped_icrc::{RetrieveWrapIcrcRequest, WrapIcrcArg, WrapIcrcError},
        MinterInfo, RetrieveWithdrawalStatus, TxFinalizedStatus,
    },
    tests::{
        minter_flow_tets::mock_rpc_https_responses::{
            generate_and_submit_mock_http_response, MOCK_BLOCK_NUMBER, MOCK_FEE_HISTORY_RESPONSE,
            MOCK_GET_LOGS, MOCK_HIGHER_BLOCK_NUMBER, MOCK_ICRC_RELEASE_REUQEST,
            MOCK_MINT_WRAPPED_ICRC_RECEIPT, MOCK_SECOND_NATIVE_TRANSACTION_RECEIPT,
            MOCK_SEND_TRANSACTION_ERROR, MOCK_SEND_TRANSACTION_SUCCESS,
            MOCK_TRANSACTION_COUNT_FINALIZED, MOCK_TRANSACTION_COUNT_LATEST,
            MOCK_WRAPPED_ICRC_DEPLOYED_AND_DEPOSIT,
        },
        pocket_ic_helpers::{
            create_pic, five_ticks, icp_principal,
            initialize_minter::create_and_install_minter_plus_dependency_canisters,
            minter_principal, native_ledger_principal, query_call, update_call,
        },
    },
    SCRAPING_CONTRACT_LOGS_INTERVAL,
};

#[test]
fn should_release_and_lock() {
    let pic = create_pic();
    create_and_install_minter_plus_dependency_canisters(&pic);

    // The deposit and withdrawal http mock flow is as follow
    // 1st Step: The mock response for get_blockbynumber is generated
    // 2nd Step: The response for eth_feehistory resonse is generated afterwards,
    // so in the time of withdrawal transaction the eip1559 transaction price is available
    // Not to forget that the price should be refreshed through a second call at the time
    // 3rd Step: There should two mock responses be generated for eth_getlogs, one for ankr and the other one for public node
    // 4th Step: After 10 min the response for eth_feehistory resonse is generated afterwards,
    // 5th Step: There should two mock responses be generated for eth_getlogs, one for ankr and the other one for public node this time with deposit logs
    // One for native and erc20
    // 6th Step: The response for sendrawtransaction
    // 7th Step: An http-outcall for getting the finalized transaction count.
    // 8th Step: and in the end get transaction receipt should be generate

    // At this time there should be 2 http requests:
    // [0] is for eth_getBlockByNumber
    // [1] is for eth_feeHistory

    let canister_http_requests = pic.get_canister_http();

    // 1st Generating mock response for eth_feehistory
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_FEE_HISTORY_RESPONSE,
    );

    // 2nd Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 1, MOCK_BLOCK_NUMBER);

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_WRAPPED_ICRC_DEPLOYED_AND_DEPOSIT,
    );

    // Ankr mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_WRAPPED_ICRC_DEPLOYED_AND_DEPOSIT,
    );

    // Drpc mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_WRAPPED_ICRC_DEPLOYED_AND_DEPOSIT,
    );

    // Alchemy mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_WRAPPED_ICRC_DEPLOYED_AND_DEPOSIT,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // Check Native deposit
    let balance = query_call::<Account, Nat>(
        &pic,
        native_ledger_principal(),
        "icrc1_balance_of",
        Account {
            owner: Principal::from_text(
                "b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe",
            )
            .unwrap(),
            subaccount: None,
        },
    );

    assert_eq!(balance, Nat::from(100_000_000_000_000_000_u128));

    // Withdrawal Section
    // Calling icrc2_approve and giving the permission to minter for taking funds from users principal NATIVE_LEDGER
    let _approve_result = update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        &pic,
        native_ledger_principal(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: minter_principal(),
                subaccount: None,
            },
            amount: Nat::from(
                99_990_000_000_000_000_u128, // Users balance - approval fee => 99_990_000_000_000_000_u128 - 10_000_000_000_000_u128
            ),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(
            Principal::from_text("b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe")
                .unwrap(),
        ),
    )
    .unwrap();

    five_ticks(&pic);

    // Calling icrc2_approve and giving the permission to lsm for taking funds from users principal
    let _approve_result = update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        &pic,
        icp_principal(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: minter_principal(),
                subaccount: None,
            },
            amount: Nat::from(
                5_000_000_000_u128, // Users balance - approval fee => 99_950_000_000_000_000_u128 - 10_000_000_000_000_u128
            ),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(
            Principal::from_text("b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe")
                .unwrap(),
        ),
    )
    .unwrap();

    five_ticks(&pic);
    five_ticks(&pic);

    let _lock_result = update_call::<WrapIcrcArg, Result<RetrieveWrapIcrcRequest, WrapIcrcError>>(
        &pic,
        minter_principal(),
        "wrap_icrc",
        WrapIcrcArg {
            amount: Nat::from(1_000_000_000_u128),
            icrc_ledger_id: icp_principal(),
            recipient: "0x3bcE376777eCFeb93953cc6C1bB957fbAcb1A261".to_string(),
        },
        Some(
            Principal::from_text("b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe")
                .unwrap(),
        ),
    );

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
        MOCK_TRANSACTION_COUNT_LATEST,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_LATEST,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_LATEST,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_LATEST,
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

    five_ticks(&pic);

    // getting the finalized transaction count after sending transaction was successful.
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_FINALIZED,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_FINALIZED,
    );

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
        MOCK_MINT_WRAPPED_ICRC_RECEIPT,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_MINT_WRAPPED_ICRC_RECEIPT,
    );

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_MINT_WRAPPED_ICRC_RECEIPT,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_MINT_WRAPPED_ICRC_RECEIPT,
    );

    five_ticks(&pic);

    // The transaction should be included into finalized transaction list.
    let get_withdrawal_transaction_by_block_index = update_call::<u64, RetrieveWithdrawalStatus>(
        &pic,
        minter_principal(),
        "retrieve_withdrawal_status",
        2_u64,
        None,
    );
    let expected_transaction_result =
        RetrieveWithdrawalStatus::TxFinalized(TxFinalizedStatus::Success {
            transaction_hash: "0x428a0f3575a0fa951224bed61e5665d9358476c474805f4b756fb9f150478f82"
                .to_string(),
            effective_transaction_fee: Some(Nat::from(63000000000000_u128)),
        });

    assert_eq!(
        get_withdrawal_transaction_by_block_index,
        expected_transaction_result
    );

    pic.advance_time(SCRAPING_CONTRACT_LOGS_INTERVAL);

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // 2nd Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_HIGHER_BLOCK_NUMBER,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_ICRC_RELEASE_REUQEST,
    );

    // Ankr mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_ICRC_RELEASE_REUQEST,
    );

    // Drpc mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_ICRC_RELEASE_REUQEST,
    );

    // Alchemy mock submission
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_ICRC_RELEASE_REUQEST,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    let minter_info = query_call::<_, MinterInfo>(&pic, minter_principal(), "get_minter_info", ());
}
