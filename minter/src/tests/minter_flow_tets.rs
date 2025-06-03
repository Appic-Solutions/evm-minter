use std::time::Duration;

use candid::{Nat, Principal};

use crate::{
    endpoints::{
        DepositStatus, Eip1559TransactionPrice, RequestScrapingError, RetrieveErc20Request,
        RetrieveNativeRequest, RetrieveWithdrawalStatus, TxFinalizedStatus, WithdrawErc20Arg,
        WithdrawErc20Error, WithdrawalArg, WithdrawalError,
    },
    evm_config::EvmNetwork,
    tests::{
        lsm_types::{AddErc20Arg, AddErc20Error, Erc20Contract, LedgerInitArg, LedgerManagerInfo},
        pocket_ic_helpers::{
            five_ticks, icp_principal, lsm_principal, native_ledger_principal, update_call,
        },
    },
    PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL, SCRAPING_contract_logs_INTERVAL,
};

use icrc_ledger_types::icrc1::account::Account;
use icrc_ledger_types::icrc2::approve::{ApproveArgs, ApproveError};

use super::pocket_ic_helpers::{
    create_pic, initialize_minter::create_and_install_minter_plus_dependency_canisters,
    minter_principal, query_call,
};

use mock_rpc_https_responses::{
    generate_and_submit_mock_http_response, MOCK_BLOCK_NUMBER, MOCK_FEE_HISTORY_RESPONSE,
    MOCK_GET_LOGS, MOCK_GET_LOGS_ERC20, MOCK_HIGHER_BLOCK_NUMBER,
    MOCK_SECOND_NATIVE_TRANSACTION_RECEIPT, MOCK_SEND_TRANSACTION_ERROR,
    MOCK_SEND_TRANSACTION_SUCCESS, MOCK_TRANSACTION_COUNT_FINALIZED,
    MOCK_TRANSACTION_COUNT_FINALIZED_ERC20, MOCK_TRANSACTION_COUNT_LATEST,
    MOCK_TRANSACTION_COUNT_LATEST_ERC20, MOCK_TRANSACTION_RECEIPT, MOCK_TRANSACTION_RECEIPT_ERC20,
};

#[test]
fn should_get_estimated_eip1559_transaction_price() {
    let pic = create_pic();
    create_and_install_minter_plus_dependency_canisters(&pic);

    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_FEE_HISTORY_RESPONSE,
    );

    five_ticks(&pic);

    // Get eip1559 transaction price
    let transaction_price = query_call::<(), Eip1559TransactionPrice>(
        &pic,
        minter_principal(),
        "eip_1559_transaction_price",
        (),
    );
    let expected_price = Eip1559TransactionPrice {
        gas_limit: Nat::from(21000_u64),
        max_fee_per_gas: Nat::from(3000000000_u64),
        max_priority_fee_per_gas: Nat::from(3000000000_u64),
        max_transaction_fee: Nat::from(63000000000000_u64),
        timestamp: Some(1620328630000000061_u64),
    };
    assert_eq!(expected_price.gas_limit, transaction_price.gas_limit);
    assert_eq!(
        expected_price.max_fee_per_gas,
        transaction_price.max_fee_per_gas
    );
    assert_eq!(
        expected_price.max_priority_fee_per_gas,
        transaction_price.max_priority_fee_per_gas
    );

    assert_eq!(
        expected_price.max_transaction_fee,
        transaction_price.max_transaction_fee
    );
}

#[test]
fn should_deposit_and_withdrawal_native() {
    let pic = create_pic();
    create_and_install_minter_plus_dependency_canisters(&pic);

    // The deposit and withdrawal http mock flow is as follow
    // 1st Step: The mock response for get_blockbynumber is generated
    // 2nd Step: The response for eth_feehistory response is generated afterwards,
    // so in the time of withdrawal transaction the eip1559 transaction price is available
    // Not to forget that the price should be refreshed through a second call at the time
    // 3rd Step: There should two mock responses be generated, one for ankr and the other one for public node
    // 4th Step: The response for sendrawtransaction
    // 5th Step: An http-outcall for getting the finalized transaction count.
    // 5th Step: and in the end get transaction receipt should be generate

    // At this time there should be 2 http requests:
    // [0] is for eth_getBlockByNumber
    // [1] is for eth_feeHistory
    let canister_http_requests = pic.get_canister_http();

    // 1st Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_BLOCK_NUMBER);

    // 2nd Generating mock response for eth_feehistory
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_FEE_HISTORY_RESPONSE,
    );

    five_ticks(&pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

    // Ankr mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 1, MOCK_GET_LOGS);

    // Drpc mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 2, MOCK_GET_LOGS);

    // Alchemy mock submissios
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 3, MOCK_GET_LOGS);

    five_ticks(&pic);

    // Check deposit
    // Based on the logs there should be 100_000_000_000_000_000 - deposit fees(50_000_000_000_000_u64)= 99_950_000_000_000_000 icBNB minted for Native to b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe
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

    // 99_840_000_000_000_000

    assert_eq!(balance, Nat::from(99_950_000_000_000_000_u128));

    // Withdrawal Section
    // Calling icrc2_approve and giving the permission to minter for taking funds from users principal
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
                99_940_000_000_000_000_u128, // Users balance - approval fee => 99_950_000_000_000_000_u128 - 10_000_000_000_000_u128
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

    // Check balance after approval
    // Based on the logs there should be 100_000_000_000_000_000 - deposit fees(50_000_000_000_000_u64)= 99_950_000_000_000_000 icBNB minted for Native to b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe
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

    assert_eq!(balance, Nat::from(99_940_000_000_000_000_u128));

    // Making the withdrawal request to minter
    let withdrawal_request_result = update_call::<
        WithdrawalArg,
        Result<RetrieveNativeRequest, WithdrawalError>,
    >(
        &pic,
        minter_principal(),
        "withdraw_native_token",
        WithdrawalArg {
            amount: Nat::from(99_940_000_000_000_000_u128),
            recipient: "0x3bcE376777eCFeb93953cc6C1bB957fbAcb1A261".to_string(),
        },
        Some(
            Principal::from_text("b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe")
                .unwrap(),
        ),
    )
    .unwrap();

    // Minting deposit block 0
    // Minting deposit fee block 1
    // Transfer
    assert_eq!(withdrawal_request_result.block_index, Nat::from(4_u64));

    five_ticks(&pic);
    five_ticks(&pic);

    // At this point there should be an http request for refreshing the fee history
    // Once there is a withdrawal request, The first attempt should be updating fee history
    // Cause there should be a maximum gap of 30 seconds between the previous gas fee estimate
    // we just advance time for amount
    //let canister_http_requests = pic.get_canister_http();
    //generate_and_submit_mock_http_response(
    //    &pic,
    //    &canister_http_requests,
    //    0,
    //    MOCK_FEE_HISTORY_RESPONSE,
    //);

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

    // 4th https out call for sending raw transaction.
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

    // drpc request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    // alchemy request
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_SEND_TRANSACTION_ERROR,
    );

    five_ticks(&pic);

    // 5th getting the finalized transaction count after sending transaction was successful.
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

    // 6th Getting the transaction receipt.
    // At this point there should be two requests for eth_getTransactionReceipt
    // [0] public_node
    // [1] ankr
    // drpc
    // alchemy
    let canister_http_requests = pic.get_canister_http();

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_RECEIPT,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_RECEIPT,
    );

    // drpc
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_RECEIPT,
    );

    // alchemy
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_RECEIPT,
    );

    five_ticks(&pic);

    // The transaction should be included into finalized transaction list.
    let get_withdrawal_transaction_by_block_index = update_call::<u64, RetrieveWithdrawalStatus>(
        &pic,
        minter_principal(),
        "retrieve_withdrawal_status",
        4_u64,
        None,
    );
    let expected_transaction_result =
        RetrieveWithdrawalStatus::TxFinalized(TxFinalizedStatus::Success {
            transaction_hash: "0x7176ed5bd7b639277afa2796148b7b10129c1d98a20ebfc2409606c13606be81"
                .to_string(),
            effective_transaction_fee: Some(Nat::from(63000000000000_u128)),
        });

    assert_eq!(
        get_withdrawal_transaction_by_block_index,
        expected_transaction_result
    );
}

#[test]
fn should_not_deposit_twice() {
    let pic = create_pic();
    create_and_install_minter_plus_dependency_canisters(&pic);

    // The deposit http mock flow is as follow
    // 1st Step: The mock response for get_blockbynumber is generated
    // 2nd Step: The response for eth_feehistory resonse is generated afterwards,
    // 3rd Step: The response for eth_getlogs response is generated,

    // At this time there should be 2 http requests:
    // [0] is for eth_getBlockByNumber
    // [1] is for eth_feeHistory
    let canister_http_requests = pic.get_canister_http();

    // 1st Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_BLOCK_NUMBER);

    // 2nd Generating mock response for eth_feehistory
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_FEE_HISTORY_RESPONSE,
    );

    five_ticks(&pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

    // Ankr mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 1, MOCK_GET_LOGS);

    // Drpc mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 2, MOCK_GET_LOGS);

    // Alchemy mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 3, MOCK_GET_LOGS);

    five_ticks(&pic);

    // There should be a gap of at least one minute between each log scraping so we advance time for 1 min
    pic.advance_time(Duration::from_secs(1 * 60));

    // Requesting for another log_scrapping
    let request_result = update_call::<Nat, Result<(), RequestScrapingError>>(
        &pic,
        minter_principal(),
        "request_scraping_logs",
        Nat::from(45944845_u64),
        None,
    );

    assert_eq!(request_result, Ok(()));

    five_ticks(&pic);

    // After requesting one more time there should be another log scraping request which means we have to
    // follow the same steps but this time we will mock http requests with incorrect responses
    // to check if minter, mints the same request twice or not.

    // At this time there should be 1 http requests:
    // [0] is for eth_getBlockByNumber
    let canister_http_requests = pic.get_canister_http();

    // 1st Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_HIGHER_BLOCK_NUMBER,
    );

    five_ticks(&pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // Generating the same mock eth_getlogs response and the minter should detect that these responses are not correct
    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

    // ankr mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 1, MOCK_GET_LOGS);

    // Drpc mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 2, MOCK_GET_LOGS);

    // Alchemy mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 3, MOCK_GET_LOGS);

    five_ticks(&pic);

    // Check balance
    // there should only be 100000000000000000 icBNB minted for Native to b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe
    // despite receiving two mint events.
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

    assert_eq!(balance, Nat::from(99950000000000000_u128));
}

#[test]
fn should_deposit_and_withdrawal_erc20() {
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

    // 1st Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_BLOCK_NUMBER);

    // 2nd Generating mock response for eth_feehistory
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_FEE_HISTORY_RESPONSE,
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
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

    // Ankr mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 1, MOCK_GET_LOGS);

    // Drpc mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 2, MOCK_GET_LOGS);

    // Alchemy mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 3, MOCK_GET_LOGS);

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // Check Native deposit
    // Based on the logs there should be 100_000_000_000_000_000 - deposit fees(50_000_000_000_000_u64)= 99_950_000_000_000_000 icBNB minted for Native to b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe
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

    // 99_840_000_000_000_000

    assert_eq!(balance, Nat::from(99_950_000_000_000_000_u128));

    // Calling icrc2_approve and giving the permission to lsm for taking funds from users principal
    let _approve_result = update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        &pic,
        icp_principal(),
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: lsm_principal(),
                subaccount: None,
            },
            amount: Nat::from(
                2_500_000_000_u128, // Users balance - approval fee => 99_950_000_000_000_000_u128 - 10_000_000_000_000_u128
            ),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        None,
    )
    .unwrap();

    five_ticks(&pic);

    // Add icUSDC to lsm
    let _result = update_call::<AddErc20Arg, Result<(), AddErc20Error>>(
        &pic,
        lsm_principal(),
        "add_erc20_ls",
        AddErc20Arg {
            contract: Erc20Contract {
                chain_id: EvmNetwork::BSCTestnet.chain_id().into(),
                address: "0x84b9B910527Ad5C03A9Ca831909E21e236EA7b06".to_string(),
            },
            ledger_init_arg: LedgerInitArg {
                transfer_fee: Nat::from(100_000_000_000_000_u128),
                decimals: 18,
                token_name: "Chain Link on icp".to_string(),
                token_symbol: "icLINK".to_string(),
                token_logo: "".to_string(),
            },
        },
        None,
    )
    .unwrap();

    five_ticks(&pic);

    // Advance time for 1 hour.
    pic.advance_time(Duration::from_secs(1 * 60));

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // Get icLink ledger id
    let chain_link_ledger_id =
        match query_call::<(), LedgerManagerInfo>(&pic, lsm_principal(), "get_lsm_info", ())
            .managed_canisters
            .into_iter()
            .find(|canister| canister.twin_erc20_token_symbol == "icLINK")
            .unwrap()
            .ledger
            .unwrap()
        {
            crate::tests::lsm_types::ManagedCanisterStatus::Created { canister_id: _ } => {
                panic!("Link canister id should be available")
            }
            crate::tests::lsm_types::ManagedCanisterStatus::Installed {
                canister_id,
                installed_wasm_hash: _,
            } => canister_id,
        };

    pic.advance_time(
        SCRAPING_contract_logs_INTERVAL
            .checked_sub(Duration::from_secs(1 * 60))
            .unwrap(),
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // 4th
    let canister_http_requests = pic.get_canister_http();

    // Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_HIGHER_BLOCK_NUMBER,
    );

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // 5th generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS_ERC20);

    // Ankr mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 1, MOCK_GET_LOGS_ERC20);

    // Drpc mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 2, MOCK_GET_LOGS_ERC20);

    // Alchemy mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 3, MOCK_GET_LOGS_ERC20);

    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);
    five_ticks(&pic);

    // Check the deposit status
    let status = query_call::<String, Option<DepositStatus>>(
        &pic,
        minter_principal(),
        "retrieve_deposit_status",
        String::from("0x0ce8486575f4a3fe725c463ad0c9a3da2484f68305edcec7bea5db26c95aa18c"),
    );

    assert_eq!(status, Some(DepositStatus::Minted));

    // Check Erc20 icLINK deposit
    // Based on the logs there should be 3_000_000_000_000_000_000 icLINK minted to b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe
    let balance = query_call::<Account, Nat>(
        &pic,
        chain_link_ledger_id,
        "icrc1_balance_of",
        Account {
            owner: Principal::from_text(
                "b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe",
            )
            .unwrap(),
            subaccount: None,
        },
    );

    assert_eq!(balance, Nat::from(3_000_000_000_000_000_000_u128));
    // assert_eq!(balance, Nat::from(99_950_000_000_000_000_u128));

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
                99_940_000_000_000_000_u128, // Users balance - approval fee => 99_950_000_000_000_000_u128 - 10_000_000_000_000_u128
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

    // Withdrawal Section
    // Calling icrc2_approve and giving the permission to minter for taking funds from users principal ERC20_LEDGER
    let _approve_result = update_call::<ApproveArgs, Result<Nat, ApproveError>>(
        &pic,
        chain_link_ledger_id,
        "icrc2_approve",
        ApproveArgs {
            from_subaccount: None,
            spender: Account {
                owner: minter_principal(),
                subaccount: None,
            },
            amount: Nat::from(
                3_000_000_000_000_000_000_u128 - 100_000_000_000_000_u128, // Users balance - approval fee => 3_000_000_000_000_000_000_u128 - 100_000_000_000_000_u128
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

    // Check balance after approval ERC20_LEDGER
    let balance = query_call::<Account, Nat>(
        &pic,
        chain_link_ledger_id,
        "icrc1_balance_of",
        Account {
            owner: Principal::from_text(
                "b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe",
            )
            .unwrap(),
            subaccount: None,
        },
    );

    assert_eq!(
        balance,
        Nat::from(3_000_000_000_000_000_000_u128 - 100_000_000_000_000_u128)
    );

    five_ticks(&pic);

    // Making Native the withdrawal request to minter
    let withdrawal_request_result = update_call::<
        WithdrawalArg,
        Result<RetrieveNativeRequest, WithdrawalError>,
    >(
        &pic,
        minter_principal(),
        "withdraw_native_token",
        WithdrawalArg {
            amount: Nat::from(940_000_000_000_000_u128),
            recipient: "0x3bcE376777eCFeb93953cc6C1bB957fbAcb1A261".to_string(),
        },
        Some(
            Principal::from_text("b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe")
                .unwrap(),
        ),
    )
    .unwrap();

    // Minting deposit block 0
    // Minting deposit fee block 1
    // Transfer
    assert_eq!(withdrawal_request_result.block_index, Nat::from(4_u64));

    five_ticks(&pic);

    // Advance time for PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL amount.
    //pic.advance_time(PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL);

    five_ticks(&pic);

    // At this point there should be an http request for refreshing the fee history
    // Once there is a withdrawal request, The first attempt should be updating fee history
    // Cause there should be a maximum gap of 30 seconds between the previous gas fee estimate
    // we just advance time for amount
    let canister_http_requests = pic.get_canister_http();
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_FEE_HISTORY_RESPONSE,
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

    println!("Hellooooooo {}", canister_http_requests.len());
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
        MOCK_SECOND_NATIVE_TRANSACTION_RECEIPT,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_SECOND_NATIVE_TRANSACTION_RECEIPT,
    );

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_SECOND_NATIVE_TRANSACTION_RECEIPT,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_SECOND_NATIVE_TRANSACTION_RECEIPT,
    );

    five_ticks(&pic);

    // The transaction should be included into finalized transaction list.
    let get_withdrawal_transaction_by_block_index = update_call::<u64, RetrieveWithdrawalStatus>(
        &pic,
        minter_principal(),
        "retrieve_withdrawal_status",
        4_u64,
        None,
    );
    let expected_transaction_result =
        RetrieveWithdrawalStatus::TxFinalized(TxFinalizedStatus::Success {
            transaction_hash: "0x846d40fd70184f891cbe42ea3738505a43d57e72e410bb707493f833cc0670c1"
                .to_string(),
            effective_transaction_fee: Some(Nat::from(63000000000000_u128)),
        });

    assert_eq!(
        get_withdrawal_transaction_by_block_index,
        expected_transaction_result
    );

    // Making the Erc20 withdrawal request to minter
    let withdrawal_request_result = update_call::<
        WithdrawErc20Arg,
        Result<RetrieveErc20Request, WithdrawErc20Error>,
    >(
        &pic,
        minter_principal(),
        "withdraw_erc20",
        WithdrawErc20Arg {
            amount: Nat::from(3_000_000_000_000_000_000_u128 - 100_000_000_000_000_u128),
            recipient: "0x3bcE376777eCFeb93953cc6C1bB957fbAcb1A261".to_string(),
            erc20_ledger_id: chain_link_ledger_id,
        },
        Some(
            Principal::from_text("b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe")
                .unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(
        withdrawal_request_result.native_block_index,
        Nat::from(6_u64)
    );
    assert_eq!(
        withdrawal_request_result.erc20_block_index,
        Nat::from(2_u64)
    );

    // Advance time for PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL amount.
    //pic.advance_time(PROCESS_TOKENS_RETRIEVE_TRANSACTIONS_INTERVAL);

    five_ticks(&pic);

    // At this point there should be an http request for refreshing the fee history
    // Once there is a withdrawal request, The first attempt should be updating fee history
    // Cause there should be a maximum gap of 30 seconds between the previous gas fee estimate
    // we just advance time for amount
    //let canister_http_requests = pic.get_canister_http();
    //generate_and_submit_mock_http_response(
    //    &pic,
    //    &canister_http_requests,
    //    0,
    //    MOCK_FEE_HISTORY_RESPONSE,
    //);

    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_LATEST_ERC20,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_LATEST_ERC20,
    );

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_LATEST_ERC20,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_LATEST_ERC20,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    // https out call for sending raw transaction.
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
        MOCK_TRANSACTION_COUNT_FINALIZED_ERC20,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_COUNT_FINALIZED_ERC20,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_COUNT_FINALIZED_ERC20,
    );

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_COUNT_FINALIZED_ERC20,
    );

    five_ticks(&pic);

    // Getting the transaction receipt.
    // At this point there should be two requests for eth_getTransactionReceipt
    // [0] public_node
    // [1] ankr
    let canister_http_requests = pic.get_canister_http();

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_RECEIPT_ERC20,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_TRANSACTION_RECEIPT_ERC20,
    );

    // public_node
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        2,
        MOCK_TRANSACTION_RECEIPT_ERC20,
    );

    // ankr
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        3,
        MOCK_TRANSACTION_RECEIPT_ERC20,
    );

    five_ticks(&pic);

    // The transaction should be included into finalized transaction list.
    let get_withdrawal_transaction_by_block_index = update_call::<u64, RetrieveWithdrawalStatus>(
        &pic,
        minter_principal(),
        "retrieve_withdrawal_status",
        6_u64,
        None,
    );
    let expected_transaction_result =
        RetrieveWithdrawalStatus::TxFinalized(TxFinalizedStatus::Success {
            transaction_hash: "0xbb61f6de6191e08bc5925af9b91ca98347e94307c32c73b7c68ee78e6b1fe580"
                .to_string(),
            effective_transaction_fee: Some(Nat::from(63000000000000_u128)),
        });

    assert_eq!(
        get_withdrawal_transaction_by_block_index,
        expected_transaction_result
    );
}

#[test]
fn should_fail_log_scrapping_request_without_proper_gap() {
    let pic = create_pic();
    create_and_install_minter_plus_dependency_canisters(&pic);

    // The deposit http mock flow is as follow
    // 1st Step: The mock response for get_blockbynumber is generated
    // 2nd Step: The response for eth_feehistory resonse is generated afterwards,
    // 3rd Step: The response for eth_getlogs response is generated,

    // At this time there should be 2 http requests:
    // [0] is for eth_getBlockByNumber
    // [1] is for eth_feeHistory
    let canister_http_requests = pic.get_canister_http();

    // 1st Generating mock response for eth_getBlockByNumber
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_BLOCK_NUMBER);

    // 2nd Generating mock response for eth_feehistory
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        1,
        MOCK_FEE_HISTORY_RESPONSE,
    );

    five_ticks(&pic);

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

    // Ankr mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 1, MOCK_GET_LOGS);

    // drpc mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 2, MOCK_GET_LOGS);

    // Alchemy mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 3, MOCK_GET_LOGS);

    five_ticks(&pic);

    // There should be a gap of at least one minute between each log scraping so we advance time for 1 min
    // So we only add 30 seconds to make the process fail
    pic.advance_time(Duration::from_secs(1 * 30));

    // Requesting for another log_scrapping
    let request_result = update_call::<Nat, Result<(), RequestScrapingError>>(
        &pic,
        minter_principal(),
        "request_scraping_logs",
        Nat::from(4594464100_u64),
        None,
    );

    assert_eq!(
        request_result,
        Err(RequestScrapingError::CalledTooManyTimes)
    );
}

mod mock_rpc_https_responses {
    use pocket_ic::{common::rest::CanisterHttpRequest, PocketIc};

    use crate::tests::pocket_ic_helpers::generate_successful_mock_response;

    pub const MOCK_FEE_HISTORY_RESPONSE: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "oldestBlock": "0x2be4eb6",
            "reward": [
                ["0xb2d05e00"]
            ],
            "baseFeePerGas": [
                "0x0",
                "0x0"
            ],
            "gasUsedRatio": [
                0.01189926
            ]
        }
    }"#;

    pub const MOCK_BLOCK_NUMBER: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "baseFeePerGas": "0x0",
            "blobGasUsed": "0x0",
            "difficulty": "0x2",
            "excessBlobGas": "0x0",
            "extraData": "0xd98301040d846765746889676f312e32312e3132856c696e757800000299d9bcf8b23fb860a6069a9c8823266060b144139b402fed5a7c6cfa64adbe236bdaf57abf6f9b826936bdbdd7b544ffba345fbd06bfdd0012edb5d44efb53d04773bebe33d108c631ba5a6e1c1258daafe10785cb919d0683068fa18a6e55ccfcf08c7c917ccce6f84c8402bd0f43a0e87d3407a7a51cc5ce929008888b5e53f8609cf0d1479e873d8e329c237d55308402bd0f44a09180e661bde5e71fbc1fa8fde5b8faafaeaefd8ef6db52290ac21cd7230f7fef806844d3d19ba58d09bf4dc94bb250903644e0dd43e0b78522be95d95dff16e9eb4eb686a35d9a069987c1361b5275e7ed7c468b8d97c6014d55ccded79c6961f101",
            "gasLimit": "0x5f5e100",
            "gasUsed": "0x4995b",
            "hash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
            "logsBloom": "0x04000000800000004000004000000000000000000000000080000000000000000100300000010000008000000000000000800000000000000000004000200000000000200000002010000008002000002010000002000000000000000000000a00081020828200000000000000000800080000000000008020000010000000000000000000000000000000000000000040000400040000000000000080400020020010001000002008000000028000000000000000000000000000000040011002000002001000000000000000000000000000000000000100104002000020000010000000000000010000040000010000008000000000004000000000102000",
            "miner": "0x1a3d9d7a717d64e6088ac937d5aacdd3e20ca963",
            "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "nonce": "0x0000000000000000",
            "number": "0x2bd0f45",
            "parentBeaconBlockRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "parentHash": "0x9180e661bde5e71fbc1fa8fde5b8faafaeaefd8ef6db52290ac21cd7230f7fef",
            "receiptsRoot": "0x1191695d554680c98e403b2e730e6dd3cd0a7732a3f305425c001e70cfd86095",
            "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "size": "0x7f4",
            "stateRoot": "0xa361889a0c1a6446cd37b308cf6cc3ffc6b8b4eaf9d01afe541bb80a9b2ab911",
            "timestamp": "0x6744b156",
            "totalDifficulty": "0x5767939",
            "transactions": [
                "0x92f77e7cd263c5f41d724180ab9ae40f273d601dcfb6d1ce1a4a2c9a44e96061",
                "0xcf9b50e1871932d3ce16af58dc2db2bd9ec6ec70f2dc6ac24197a95ab1f663f0",
                "0xf9617ed37c4fef311da4c37d29adc2ad9e0a8289cea8b365257b23b2a531dfa7",
                "0x5ace3bb62c01dc21e0ed3289181d8a67ce13606ea34d730f6e1983b5cec80ec0",
                "0xcde530df6850bd19f822264791dac4f6730caa8642f65bd3810389bf982babfe",
                "0xf8c98fefa467d3e3b1c4d260feefd58856904ff05c266f92b4cb662eb07801a5",
                "0xbd662557953a0e892e276ab586e2ea0dee9ed8c1ba3c129788216942e8367888"
            ],
            "transactionsRoot": "0x7a4a90d5244d734440282ca816aab466ad480bb05dace99ea23f1ac26749351c",
            "uncles": [],
            "withdrawals": [],
            "withdrawalsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        }
    }"#;

    pub const MOCK_HIGHER_BLOCK_NUMBER: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "baseFeePerGas": "0x0",
            "blobGasUsed": "0x0",
            "difficulty": "0x2",
            "excessBlobGas": "0x0",
            "extraData": "0xd98301040d846765746889676f312e32312e3132856c696e757800000299d9bcf8b23fb860a6069a9c8823266060b144139b402fed5a7c6cfa64adbe236bdaf57abf6f9b826936bdbdd7b544ffba345fbd06bfdd0012edb5d44efb53d04773bebe33d108c631ba5a6e1c1258daafe10785cb919d0683068fa18a6e55ccfcf08c7c917ccce6f84c8402bd0f43a0e87d3407a7a51cc5ce929008888b5e53f8609cf0d1479e873d8e329c237d55308402bd0f44a09180e661bde5e71fbc1fa8fde5b8faafaeaefd8ef6db52290ac21cd7230f7fef806844d3d19ba58d09bf4dc94bb250903644e0dd43e0b78522be95d95dff16e9eb4eb686a35d9a069987c1361b5275e7ed7c468b8d97c6014d55ccded79c6961f101",
            "gasLimit": "0x5f5e100",
            "gasUsed": "0x4995b",
            "hash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
            "logsBloom": "0x04000000800000004000004000000000000000000000000080000000000000000100300000010000008000000000000000800000000000000000004000200000000000200000002010000008002000002010000002000000000000000000000a00081020828200000000000000000800080000000000008020000010000000000000000000000000000000000000000040000400040000000000000080400020020010001000002008000000028000000000000000000000000000000040011002000002001000000000000000000000000000000000000100104002000020000010000000000000010000040000010000008000000000004000000000102000",
            "miner": "0x1a3d9d7a717d64e6088ac937d5aacdd3e20ca963",
            "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "nonce": "0x0000000000000000",
            "number": "0x2BD103A",
            "parentBeaconBlockRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "parentHash": "0x9180e661bde5e71fbc1fa8fde5b8faafaeaefd8ef6db52290ac21cd7230f7fef",
            "receiptsRoot": "0x1191695d554680c98e403b2e730e6dd3cd0a7732a3f305425c001e70cfd86095",
            "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "size": "0x7f4",
            "stateRoot": "0xa361889a0c1a6446cd37b308cf6cc3ffc6b8b4eaf9d01afe541bb80a9b2ab911",
            "timestamp": "0x6744b156",
            "totalDifficulty": "0x5767939",
            "transactions": [
                "0x92f77e7cd263c5f41d724180ab9ae40f273d601dcfb6d1ce1a4a2c9a44e96061",
                "0xcf9b50e1871932d3ce16af58dc2db2bd9ec6ec70f2dc6ac24197a95ab1f663f0",
                "0xf9617ed37c4fef311da4c37d29adc2ad9e0a8289cea8b365257b23b2a531dfa7",
                "0x5ace3bb62c01dc21e0ed3289181d8a67ce13606ea34d730f6e1983b5cec80ec0",
                "0xcde530df6850bd19f822264791dac4f6730caa8642f65bd3810389bf982babfe",
                "0xf8c98fefa467d3e3b1c4d260feefd58856904ff05c266f92b4cb662eb07801a5",
                "0xbd662557953a0e892e276ab586e2ea0dee9ed8c1ba3c129788216942e8367888"
            ],
            "transactionsRoot": "0x7a4a90d5244d734440282ca816aab466ad480bb05dace99ea23f1ac26749351c",
            "uncles": [],
            "withdrawals": [],
            "withdrawalsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        }
    }"#;

    pub const MOCK_GET_LOGS: &str = r#"{
        "jsonrpc": "2.0",
        "id": 3,
        "result": [
            {
                "address": "0x733a1beef5a02990aad285d7ed93fc1b622eef1d",
                "topics": [
                    "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "0x000000000000000000000000000000000000000000000000016345785d8a0000",
                    "0x1de235c6cf77973d181e3d7f5755892a0d4ae76f9c41d1c7a3ce797e4b020000"
                ],
                "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
                "blockNumber": "0x2bd0f45",
                "transactionHash": "0xcde530df6850bd19f822264791dac4f6730caa8642f65bd3810389bf982babfe",
                "transactionIndex": "0x4",
                "blockHash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
                "logIndex": "0x3",
                "removed": false
            }
        ]
    }"#;

    pub const MOCK_GET_LOGS_ERC20: &str = r#"{
        "jsonrpc": "2.0",
        "id": 3,
        "result": [
            {
                "address": "0x733a1beef5a02990aad285d7ed93fc1b622eef1d",
                "topics": [
                    "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                    "0x00000000000000000000000084b9b910527ad5c03a9ca831909e21e236ea7b06",
                    "0x00000000000000000000000000000000000000000000000029a2241af62c0000",
                    "0x1de235c6cf77973d181e3d7f5755892a0d4ae76f9c41d1c7a3ce797e4b020000"
                ],
                "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
                "blockNumber": "0x2BD103A",
                "transactionHash": "0x0ce8486575f4a3fe725c463ad0c9a3da2484f68305edcec7bea5db26c95aa18c",
                "transactionIndex": "0x4",
                "blockHash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
                "logIndex": "0x4",
                "removed": false
            }
        ]
    }"#;

    pub const MOCK_TRANSACTION_COUNT_LATEST: &str = r#"{"id":1,"jsonrpc":"2.0","result":"0x0"}"#;
    pub const MOCK_TRANSACTION_COUNT_LATEST_ERC20: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;

    pub const MOCK_TRANSACTION_COUNT_FINALIZED: &str = r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;
    pub const MOCK_TRANSACTION_COUNT_FINALIZED_ERC20: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x2"}"#;

    pub const MOCK_TRANSACTION_RECEIPT: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0xb2d05e00",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0x7176ed5bd7b639277afa2796148b7b10129c1d98a20ebfc2409606c13606be81",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_SECOND_NATIVE_TRANSACTION_RECEIPT: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0xb2d05e00",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0x846d40fd70184f891cbe42ea3738505a43d57e72e410bb707493f833cc0670c1",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_TRANSACTION_RECEIPT_ERC20: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0xb2d05e00",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0xbb61f6de6191e08bc5925af9b91ca98347e94307c32c73b7c68ee78e6b1fe580",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_SEND_TRANSACTION_ERROR: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "error": {
            "code": -32000,
            "message": "already known"
        }
    }"#;

    pub const MOCK_SEND_TRANSACTION_SUCCESS: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": "0x7176ed5bd7b639277afa2796148b7b10129c1d98a20ebfc2409606c13606be81"
    }"#;

    pub fn generate_and_submit_mock_http_response(
        pic: &PocketIc,
        http_requests_list: &Vec<CanisterHttpRequest>,
        https_request_index: usize,
        http_json_response: &str,
    ) {
        let http_request = &http_requests_list[https_request_index];

        let generated_mock_response = generate_successful_mock_response(
            http_request.subnet_id,
            http_request.request_id,
            http_json_response.as_bytes().to_vec(),
        );

        pic.mock_canister_http_response(generated_mock_response);
    }
}
