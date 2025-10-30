use crate::{
    candid_types::{
        chain_data::ChainData,
        withdraw_erc20::{RetrieveErc20Request, WithdrawErc20Arg, WithdrawErc20Error},
        withdraw_native::{WithdrawalArg, WithdrawalError},
        ActivateSwapReqest, DepositStatus, Eip1559TransactionPrice, MinterInfo,
        RequestScrapingError, RetrieveNativeRequest, RetrieveWithdrawalStatus, TxFinalizedStatus,
    },
    evm_config::EvmNetwork,
    tests::{
        lsm_types::{AddErc20Arg, AddErc20Error, Erc20Contract, LedgerInitArg, LedgerManagerInfo},
        minter_flow_tets::mock_rpc_https_responses::{
            MOCK_BSC_FEE_HISTORY_INNER, MOCK_TRANSACTION_RECEIPT_APPROVE_ERC20,
        },
        pocket_ic_helpers::{
            five_ticks, icp_principal, lsm_principal, native_ledger_principal, update_call,
        },
    },
    APPIC_CONTROLLER_PRINCIPAL, RPC_HELPER_PRINCIPAL, SCRAPING_CONTRACT_LOGS_INTERVAL,
};
use candid::{Nat, Principal};
use evm_rpc_client::eth_types::Address;
use std::{str::FromStr, time::Duration};

use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};
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
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    print!("{:?}", canister_http_requests);

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
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

// if there is a block scrape request that is not scraped yet after chain data update, in case the
// block is in scraping range(it should be between last_observed_block and last_scraped_block) the
// scaping should start
#[test]
fn should_start_log_scraping_after_chain_data_update() {
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

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

    five_ticks(&pic);

    let minter_info = query_call::<(), MinterInfo>(&pic, minter_principal(), "get_minter_info", ());
    assert_eq!(
        minter_info.last_scraped_block_number,
        Some(Nat::from(45944644_u128))
    );

    // request a block to be scraped
    update_call::<Nat, ()>(
        &pic,
        minter_principal(),
        "request_block_scrape",
        Nat::from(45944645_u128),
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    five_ticks(&pic);

    update_call::<ChainData, ()>(
        &pic,
        minter_principal(),
        "update_chain_data",
        ChainData {
            latest_block_number: Nat::from(45944646_u128),
            fee_history: MOCK_BSC_FEE_HISTORY_INNER.to_string(),
            native_token_usd_price: None,
        },
        Some(Principal::from_text(RPC_HELPER_PRINCIPAL).unwrap()),
    );

    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    assert_eq!(canister_http_requests.len(), 1);
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

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

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

    assert_eq!(balance, Nat::from(100_000_000_000_000_000_u128));

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
                99_990_000_000_000_000_u128, // Users balance - approval fee => 100_000_000_000_000_000_u128 - 10_000_000_000_000_u128
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

    assert_eq!(balance, Nat::from(99_990_000_000_000_000_u128));

    // Making the withdrawal request to minter
    let withdrawal_request_result = update_call::<
        WithdrawalArg,
        Result<RetrieveNativeRequest, WithdrawalError>,
    >(
        &pic,
        minter_principal(),
        "withdraw_native_token",
        WithdrawalArg {
            amount: Nat::from(99_990_000_000_000_000_u128),
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
    assert_eq!(withdrawal_request_result.block_index, Nat::from(2_u64));

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

    five_ticks(&pic);

    // 5th getting the finalized transaction count after sending transaction was successful.
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
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
            transaction_hash: "0x23e4ac0e4bde9f2c12a3906d7145769a52d96456fca38f3de399a1c0038309fb"
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

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

    five_ticks(&pic);

    // There should be a gap of at least one minute between each log scraping so we advance time for 1 min
    pic.advance_time(Duration::from_secs(1 * 60));

    // Requesting for another log_scrapping
    let request_result = update_call::<(), Result<(), RequestScrapingError>>(
        &pic,
        minter_principal(),
        "request_scraping_logs",
        (),
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
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

    assert_eq!(balance, Nat::from(100000000000000000_u128));
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
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

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
    update_call::<AddErc20Arg, Result<(), AddErc20Error>>(
        &pic,
        lsm_principal(),
        "add_erc20_ls",
        AddErc20Arg {
            contract: Erc20Contract {
                chain_id: EvmNetwork::BSC.chain_id().into(),
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
        SCRAPING_CONTRACT_LOGS_INTERVAL
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
    assert_eq!(withdrawal_request_result.block_index, Nat::from(2_u64));

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
            transaction_hash: "0x1bf19dee9c59944ddaed2252ad483a3df892a009e245330bb681481350926422"
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
        Nat::from(3_u64)
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

    five_ticks(&pic);

    // getting the finalized transaction count after sending transaction was successful.
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
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

    five_ticks(&pic);
    five_ticks(&pic);

    // The transaction should be included into finalized transaction list.
    let get_withdrawal_transaction_by_block_index = update_call::<u64, RetrieveWithdrawalStatus>(
        &pic,
        minter_principal(),
        "retrieve_withdrawal_status",
        3_u64,
        None,
    );
    let expected_transaction_result =
        RetrieveWithdrawalStatus::TxFinalized(TxFinalizedStatus::Success {
            transaction_hash: "0x54a97b762eca864e89a680c1e116632600dfc634ba80c8bd89689920e1ae99f3"
                .to_string(),
            effective_transaction_fee: Some(Nat::from(63000000000000_u128)),
        });

    assert_eq!(
        get_withdrawal_transaction_by_block_index,
        expected_transaction_result
    );
}

#[test]
fn should_activate_swap_feature() {
    let pic = create_pic();
    create_and_install_minter_plus_dependency_canisters(&pic);

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

    // 3rd generating mock response for eth_getLogs
    // At this time there should be 2 http requests:
    // [0] is for public_node eth_getLogs
    // [1] is for ankr eth_getLogs
    let canister_http_requests = pic.get_canister_http();

    // public_node mock submission
    generate_and_submit_mock_http_response(&pic, &canister_http_requests, 0, MOCK_GET_LOGS);

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

    assert_eq!(balance, Nat::from(100_000_000_000_000_000_u128));

    five_ticks(&pic);

    let transfer_result = update_call::<TransferArg, Result<Nat, TransferError>>(
        &pic,
        native_ledger_principal(),
        "icrc1_transfer",
        TransferArg {
            from_subaccount: None,
            to: Principal::from_text(APPIC_CONTROLLER_PRINCIPAL)
                .unwrap()
                .into(),
            fee: None,
            created_at_time: None,
            memo: None,
            amount: Nat::from(1_990_000_000_000_000_u128),
        },
        Some(
            Principal::from_text("b4any-vxcgx-dm654-xhumb-4pl7k-5kysk-qnjlt-w7hcb-2hd2h-ttzpz-fqe")
                .unwrap(),
        ),
    );

    assert!(transfer_result.is_ok());

    five_ticks(&pic);

    // Calling icrc2_approve and giving the permission to minter for taking funds from users principal
    // for sending the swap activation request
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
            amount: Nat::from(1_000_000_000_000_000_u128),
            expected_allowance: None,
            expires_at: None,
            fee: None,
            memo: None,
            created_at_time: None,
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    )
    .unwrap();

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
            amount: Nat::from(1_000_000_000_000_000_u128),
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

    // Add icUSDC to lsm
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

    update_call::<AddErc20Arg, Result<(), AddErc20Error>>(
        &pic,
        lsm_principal(),
        "add_erc20_ls",
        AddErc20Arg {
            contract: Erc20Contract {
                chain_id: EvmNetwork::BSC.chain_id().into(),
                address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(),
            },
            ledger_init_arg: LedgerInitArg {
                transfer_fee: Nat::from(10_000_u128),
                decimals: 6,
                token_name: "USDC on icp".to_string(),
                token_symbol: "icUSDC".to_string(),
                token_logo: "".to_string(),
            },
        },
        None,
    )
    .unwrap();

    five_ticks(&pic);

    // Advance time for 1 min.
    pic.advance_time(Duration::from_secs(60));

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

    five_ticks(&pic);
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
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

    five_ticks(&pic);

    //Get icLink ledger id
    let ic_usdc_ledger_id =
        match query_call::<(), LedgerManagerInfo>(&pic, lsm_principal(), "get_lsm_info", ())
            .managed_canisters
            .into_iter()
            .find(|canister| canister.twin_erc20_token_symbol == "icUSDC")
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

    println!("ic_usdc ledger id:{},", ic_usdc_ledger_id);

    // swap activation request
    let swap_contract_address =
        Address::from_str("0xa72ab997CCd4C55a7aDc049df8057D577f5322a8").unwrap();

    let dex_canister_id: Principal = Principal::from_text("nbepk-iyaaa-aaaad-qhlma-cai").unwrap();

    update_call::<ActivateSwapReqest, Nat>(
        &pic,
        minter_principal(),
        "activate_swap_feature",
        ActivateSwapReqest {
            twin_usdc_ledger_id: ic_usdc_ledger_id,
            swap_contract_address: swap_contract_address.to_string(),
            twin_usdc_decimals: 6,
            dex_canister_id,
            canister_signing_fee_twin_usdc_value: Nat::from(50_000_u32),
        },
        Some(Principal::from_text(APPIC_CONTROLLER_PRINCIPAL).unwrap()),
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let canister_http_requests = pic.get_canister_http();

    // Generating the latest transaction count for inserting the correct nonce
    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_LATEST_ERC20,
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

    five_ticks(&pic);
    let canister_http_requests = pic.get_canister_http();

    generate_and_submit_mock_http_response(
        &pic,
        &canister_http_requests,
        0,
        MOCK_TRANSACTION_COUNT_FINALIZED_ERC20,
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
        MOCK_TRANSACTION_RECEIPT_APPROVE_ERC20,
    );

    five_ticks(&pic);
    five_ticks(&pic);

    let minter_info = query_call::<(), MinterInfo>(&pic, minter_principal(), "get_minter_info", ());

    assert!(minter_info.is_swapping_active);
    assert_eq!(minter_info.clone().twin_usdc_info.unwrap().decimals, 6);
    assert_eq!(
        minter_info.clone().twin_usdc_info.unwrap().address,
        "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"
    );
    assert_eq!(
        minter_info.clone().twin_usdc_info.unwrap().ledger_id,
        ic_usdc_ledger_id
    );
    assert_eq!(
        minter_info.clone().swap_contract_address.unwrap(),
        swap_contract_address.to_string()
    );
    assert_eq!(
        minter_info.clone().dex_canister_id.unwrap(),
        dex_canister_id
    );

    println!("{minter_info:?}");
}

pub mod mock_rpc_https_responses {
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

    pub const MOCK_BSC_FEE_HISTORY_RESPONSE: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"oldestBlock":"0x3af1ef1","reward":[["0x5f5e100","0x68e7780","0x7735940"],["0x68e7780","0x68e7780","0x69f4060"],["0x5f5e100","0x68e7780","0x7270e01"],["0x5f5e100","0x68e7780","0x7735940"],["0x5f5e100","0x68e7780","0x69f4060"]],"baseFeePerGas":["0x0","0x0","0x0","0x0","0x0","0x0"],"gasUsedRatio":[0.27619985333333336,0.21652034666666667,0.32256104,0.28686824,0.2847872],"baseFeePerBlobGas":["0x1","0x1","0x1","0x1","0x1","0x1"],"blobGasUsedRatio":[0,0,0,0.16666666666666666,0]}}"#;

    pub const MOCK_BSC_FEE_HISTORY_INNER: &str = r#"{"oldestBlock":"0x3af1ef1","reward":[["0x5f5e100","0x68e7780","0x7735940"],["0x68e7780","0x68e7780","0x69f4060"],["0x5f5e100","0x68e7780","0x7270e01"],["0x5f5e100","0x68e7780","0x7735940"],["0x5f5e100","0x68e7780","0x69f4060"]],"baseFeePerGas":["0x0","0x0","0x0","0x0","0x0","0x0"],"gasUsedRatio":[0.27619985333333336,0.21652034666666667,0.32256104,0.28686824,0.2847872],"baseFeePerBlobGas":["0x1","0x1","0x1","0x1","0x1","0x1"],"blobGasUsedRatio":[0,0,0,0.16666666666666666,0]}"#;

    pub const MOCK_BSC_BLOCK_NUMBER: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"baseFeePerGas":"0xe128a69","blobGasUsed":"0x120000","difficulty":"0x0","excessBlobGas":"0x0","extraData":"0xda83010f0b846765746888676f312e32342e328777696e646f7773","gasLimit":"0x2aca2c9","gasUsed":"0x3a9b15","hash":"0xe703eaa3a31ab4377d5637b493ee854c98d599aaf1585f86f901a4ff2338846c","logsBloom":"0x40220115402210404a484004e9200d480392a0000003601018234004c01620028242a48288450a468014820000020514429002690a02131132420000a6251281404080480100962088600008280093700182002000509102a4426c1800402045ac16502482400a30381446404600883018003082035cf6c00050523400111002023808a20410d0041040802010027702011102200530412904800c5820300e13e240022c08082d81500880a118a801262810006210ce800880006a022010c024608038060819040d2461dc18002600000000005101180110c040a0a231802041013060680238010180a04880c088000c040828451110082800204010020a0582","miner":"0x6641a9df47ab895f93fa97d11a1afbb2c63d6e99","mixHash":"0x20b4f4e0822f4732db0e1eab23cc71edf4b90c8cc8b628300ed92aa755acd792","nonce":"0x0000000000000000","number":"0x16519d2","parentBeaconBlockRoot":"0xa15a9b5d93796347caa38eaa42123b1dff469d9483a670b04a3c880335f5e955","parentHash":"0x9904c8c8d192fe186d057996e9cc9969386c325bb30fe994d61e8db7c3cc6d5a","receiptsRoot":"0x6ba9eb62fd81f8788bbe340eb853e898920ced9e2657c5ffdd7810468360561d","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","size":"0x56f0","stateRoot":"0x2aab024c5f62088dc57840ead429f5388327278da6eb09e4c0838d320fa0b4b8","timestamp":"0x68ce5f8f","transactions":["0xc8ff6848c970a169f230a848131eb301edbbd2c06324a39a00f1eba0cac55fa3"],"transactionsRoot":"0x4d771a34d66d35a625b34540f0ac96baaf089161bc4e820ade26428fe316c04c","uncles":[]}}"#;

    pub const MOCK_BSC_HIGHER_BLOCK_NUMBER: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"baseFeePerGas":"0xe128a69","blobGasUsed":"0x120000","difficulty":"0x0","excessBlobGas":"0x0","extraData":"0xda83010f0b846765746888676f312e32342e328777696e646f7773","gasLimit":"0x2aca2c9","gasUsed":"0x3a9b15","hash":"0xe703eaa3a31ab4377d5637b493ee854c98d599aaf1585f86f901a4ff2338846c","logsBloom":"0x40220115402210404a484004e9200d480392a0000003601018234004c01620028242a48288450a468014820000020514429002690a02131132420000a6251281404080480100962088600008280093700182002000509102a4426c1800402045ac16502482400a30381446404600883018003082035cf6c00050523400111002023808a20410d0041040802010027702011102200530412904800c5820300e13e240022c08082d81500880a118a801262810006210ce800880006a022010c024608038060819040d2461dc18002600000000005101180110c040a0a231802041013060680238010180a04880c088000c040828451110082800204010020a0582","miner":"0x6641a9df47ab895f93fa97d11a1afbb2c63d6e99","mixHash":"0x20b4f4e0822f4732db0e1eab23cc71edf4b90c8cc8b628300ed92aa755acd792","nonce":"0x0000000000000000","number":"0x16519de","parentBeaconBlockRoot":"0xa15a9b5d93796347caa38eaa42123b1dff469d9483a670b04a3c880335f5e955","parentHash":"0x9904c8c8d192fe186d057996e9cc9969386c325bb30fe994d61e8db7c3cc6d5a","receiptsRoot":"0x6ba9eb62fd81f8788bbe340eb853e898920ced9e2657c5ffdd7810468360561d","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","size":"0x56f0","stateRoot":"0x2aab024c5f62088dc57840ead429f5388327278da6eb09e4c0838d320fa0b4b8","timestamp":"0x68ce5f8f","transactions":["0xc8ff6848c970a169f230a848131eb301edbbd2c06324a39a00f1eba0cac55fa3"],"transactionsRoot":"0x4d771a34d66d35a625b34540f0ac96baaf089161bc4e820ade26428fe316c04c","uncles":[]}}"#;

    pub const MOCK_BASE_FEE_HISTORY_RESPONSE: &str = r#"{"jsonrpc":"2.0","result":{"baseFeePerBlobGas":["0x1","0x1","0x1","0x1","0x1","0x1"],"baseFeePerGas":["0x28995e","0x287804","0x2855e5","0x288e3b","0x289148","0x286974"],"blobGasUsedRatio":[0,0,0,0,0],"gasUsedRatio":[0.27984945333333333,0.2784370133333333,0.4242639733333333,0.33823486,0.26941264666666664],"oldestBlock":"0x22200ce","reward":[["0x1388","0xf4240","0x120555"],["0xcf850","0xf4240","0x110b48"],["0x1388","0xcf850","0xf4240"],["0x3e988","0xf09ea","0x116d35"],["0x1c8e8","0xf3f33","0x14d40c"]]},"id":1}"#;

    pub const MOCK_BASE_FEE_HISTORY_INNER: &str = r#"{"baseFeePerBlobGas":["0x1","0x1","0x1","0x1","0x1","0x1"],"baseFeePerGas":["0x28995e","0x287804","0x2855e5","0x288e3b","0x289148","0x286974"],"blobGasUsedRatio":[0,0,0,0,0],"gasUsedRatio":[0.27984945333333333,0.2784370133333333,0.4242639733333333,0.33823486,0.26941264666666664],"oldestBlock":"0x22200ce","reward":[["0x1388","0xf4240","0x120555"],["0xcf850","0xf4240","0x110b48"],["0x1388","0xcf850","0xf4240"],["0x3e988","0xf09ea","0x116d35"],["0x1c8e8","0xf3f33","0x14d40c"]]}"#;

    pub const MOCK_BASE_BLOCK_NUMBER: &str = r#"{"jsonrpc":"2.0","result":{"baseFeePerGas":"0x2b9f2d","blobGasUsed":"0x0","difficulty":"0x0","excessBlobGas":"0x0","extraData":"0x000000003200000003","gasLimit":"0x8f0d180","gasUsed":"0x2982f92","hash":"0x8879750e4c4d90d75a782bffb2e76406011d9ec5aa423a1cf5f8048136ce151d","logsBloom":"0x3cfd7faaf1eebfd9dff81ad5ffffbbf7fbfddbbfbdbbf3bf6fdfff1ffb336f7ee76ff9bddde6fe55d7f7f7f7773ff7fbedfdbfde97b3ffe27f7eff3f4efcf4bef5afeefe4cf95f6db3bcff3ddebfe8b6cedfa5e7ce7fbff7ddf95a1f97f57beffab4773fffbf4f7effaddfebfec6ee6fdbe37d7ffcffb6faaa7fd79fdf6ffbff1fbfffdf9a4efdfe7dbdffbbee8e1c7fb7a79efbff3f7acff75fdf5759f4ddfabaeef97ffbf7fc69cfeef3fff7fbffb3fef7eeeab9f3dd7f5cf7afbfe5fe3fb67fb7bcff6ffbdfddbf7f1dfe4f36fffefffef7ffc7fb1eff7bf53cfe9dfbebed63fe1b1b7cfebe5eb8f7f2de5bf3bfffbffeff3defcfffebfb2f7e8f77984e77","miner":"0x4200000000000000000000000000000000000011","mixHash":"0xad01bff52072db4cb559d8c10896b05a888ab2d1d6b380674873b177660b6707","nonce":"0x0000000000000000","number":"0x22202050","parentBeaconBlockRoot":"0x3253811d0f0da0f8a53e35779616f26af5590c0a0efce44e25e5997e6ab37476","parentHash":"0xfcce45dee3ee577e7c8b89beb08a0c36db9c6fc15f003d8b7bb85b7ebfba0452","receiptsRoot":"0xe878356a28e394bb7e04a9ffb70ba667d0dc176f6a007139a110e5cda018adc9","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","size":"0x1cb3e","stateRoot":"0x36f1d129e7398444e6eb9505464f8d4a9f1ff2d58127a797776fe4ea0c6962c6","timestamp":"0x68ce5ff3","transactions":["0xdae86681341440c523c62eb76c9eb5f07bc5f4fe520a10b2f6af8bd23e92b55c"],"transactionsRoot":"0xf5281f791a550fedfc6abab76a0ecf111efcfda34149d1eacdf11e5ae4317662","uncles":[]},"id":1}"#;

    pub const MOCK_BASE_HIGHER_BLOCK_NUMBER: &str = r#"{"jsonrpc":"2.0","result":{"baseFeePerGas":"0x2b9f2d","blobGasUsed":"0x0","difficulty":"0x0","excessBlobGas":"0x0","extraData":"0x000000003200000003","gasLimit":"0x8f0d180","gasUsed":"0x2982f92","hash":"0x8879750e4c4d90d75a782bffb2e76406011d9ec5aa423a1cf5f8048136ce151d","logsBloom":"0x3cfd7faaf1eebfd9dff81ad5ffffbbf7fbfddbbfbdbbf3bf6fdfff1ffb336f7ee76ff9bddde6fe55d7f7f7f7773ff7fbedfdbfde97b3ffe27f7eff3f4efcf4bef5afeefe4cf95f6db3bcff3ddebfe8b6cedfa5e7ce7fbff7ddf95a1f97f57beffab4773fffbf4f7effaddfebfec6ee6fdbe37d7ffcffb6faaa7fd79fdf6ffbff1fbfffdf9a4efdfe7dbdffbbee8e1c7fb7a79efbff3f7acff75fdf5759f4ddfabaeef97ffbf7fc69cfeef3fff7fbffb3fef7eeeab9f3dd7f5cf7afbfe5fe3fb67fb7bcff6ffbdfddbf7f1dfe4f36fffefffef7ffc7fb1eff7bf53cfe9dfbebed63fe1b1b7cfebe5eb8f7f2de5bf3bfffbffeff3defcfffebfb2f7e8f77984e77","miner":"0x4200000000000000000000000000000000000011","mixHash":"0xad01bff52072db4cb559d8c10896b05a888ab2d1d6b380674873b177660b6707","nonce":"0x0000000000000000","number":"0x222020B0","parentBeaconBlockRoot":"0x3253811d0f0da0f8a53e35779616f26af5590c0a0efce44e25e5997e6ab37476","parentHash":"0xfcce45dee3ee577e7c8b89beb08a0c36db9c6fc15f003d8b7bb85b7ebfba0452","receiptsRoot":"0xe878356a28e394bb7e04a9ffb70ba667d0dc176f6a007139a110e5cda018adc9","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","size":"0x1cb3e","stateRoot":"0x36f1d129e7398444e6eb9505464f8d4a9f1ff2d58127a797776fe4ea0c6962c6","timestamp":"0x68ce5ff3","transactions":["0xdae86681341440c523c62eb76c9eb5f07bc5f4fe520a10b2f6af8bd23e92b55c"],"transactionsRoot":"0xf5281f791a550fedfc6abab76a0ecf111efcfda34149d1eacdf11e5ae4317662","uncles":[]},"id":1}"#;

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

    pub const MOCK_GET_BSC_LOGS_ERC20: &str = r#"{
        "jsonrpc": "2.0",
        "id": 3,
        "result": [
            {
                "address": "0x733a1beef5a02990aad285d7ed93fc1b622eef1d",
                "topics": [
                    "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                    "0x0000000000000000000000008AC76a51cc950d9822D68b83fE1Ad97B32Cd580d",
                    "0x00000000000000000000000000000000000000000000011129a2241af62c0000",
                    "0x1d811078ea0d8563f526b6697c5871ee8aab60b058b0aabb5c31ab17de020000"
                ],
                "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
                "blockNumber": "0x16519d2",
                "transactionHash": "0x0ce8486575f4a3fe725c463ad0c9a3da2484f68305edcec7bea5db26c95aa18c",
                "transactionIndex": "0x4",
                "blockHash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
                "logIndex": "0x4",
                "removed": false
            },
            {
                "address": "0x733a1beef5a02990aad285d7ed93fc1b622eef1d",
                "topics": [
                    "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "0x00000000000000000000000000000000000000000000000000a2241af62c0000",
                    "0x1d811078ea0d8563f526b6697c5871ee8aab60b058b0aabb5c31ab17de020000"
                ],
                "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
                "blockNumber": "0x2220188",
                "transactionHash": "0x0a655f518091572886dbb4c18169b00e47d2324b33e9d3f90f3ab6cc06ca4d5a",
                "transactionIndex": "0x5",
                "blockHash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
                "logIndex": "0x5",
                "removed": false
            }
        ]
    }"#;

    pub const MOCK_GET_BASE_LOGS_ERC20: &str = r#"{
        "jsonrpc": "2.0",
        "id": 3,
        "result": [
            {
                "address": "0x733a1beef5a02990aad285d7ed93fc1b622eef1d",
                "topics": [
                    "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                    "0x000000000000000000000000833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
                    "0x00000000000000000000000000000000000000000000000000a2241af62c0000",
                    "0x1d811078ea0d8563f526b6697c5871ee8aab60b058b0aabb5c31ab17de020000"
                ],
                "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
                "blockNumber": "0x2220188",
                "transactionHash": "0x0ce8486575f4a3fe725c463ad0c9a3da2484f68305edcec7bea5db26c95aa18c",
                "transactionIndex": "0x4",
                "blockHash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
                "logIndex": "0x4",
                "removed": false
            },
            {
                "address": "0x733a1beef5a02990aad285d7ed93fc1b622eef1d",
                "topics": [
                    "0xdeaddf8708b62ae1bf8ec4693b523254aa961b2da6bc5be57f3188ee784d6275",
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "0x00000000000000000000000000000000000000000000000000a2241af62c0000",
                    "0x1d811078ea0d8563f526b6697c5871ee8aab60b058b0aabb5c31ab17de020000"
                ],
                "data": "0x0000000000000000000000005d737f982696fe2fe4ef1c7584e914c3a8e44d540000000000000000000000000000000000000000000000000000000000000000",
                "blockNumber": "0x2220188",
                "transactionHash": "0x0a655f518091572886dbb4c18169b00e47d2324b33e9d3f90f3ab6cc06ca4d5a",
                "transactionIndex": "0x5",
                "blockHash": "0xc1ff7931ceab1152c911cbb033bb5f6dad378263e3849cb7c5d90711fcbe352c",
                "logIndex": "0x5",
                "removed": false
            }
        ]
    }"#;

    pub const MOCK_SWAP_BASE_BLOCK_NUMBER: &str = r#"{"jsonrpc":"2.0","result":{"baseFeePerGas":"0x2b9f2d","blobGasUsed":"0x0","difficulty":"0x0","excessBlobGas":"0x0","extraData":"0x000000003200000003","gasLimit":"0x8f0d180","gasUsed":"0x2982f92","hash":"0x8879750e4c4d90d75a782bffb2e76406011d9ec5aa423a1cf5f8048136ce151d","logsBloom":"0x3cfd7faaf1eebfd9dff81ad5ffffbbf7fbfddbbfbdbbf3bf6fdfff1ffb336f7ee76ff9bddde6fe55d7f7f7f7773ff7fbedfdbfde97b3ffe27f7eff3f4efcf4bef5afeefe4cf95f6db3bcff3ddebfe8b6cedfa5e7ce7fbff7ddf95a1f97f57beffab4773fffbf4f7effaddfebfec6ee6fdbe37d7ffcffb6faaa7fd79fdf6ffbff1fbfffdf9a4efdfe7dbdffbbee8e1c7fb7a79efbff3f7acff75fdf5759f4ddfabaeef97ffbf7fc69cfeef3fff7fbffb3fef7eeeab9f3dd7f5cf7afbfe5fe3fb67fb7bcff6ffbdfddbf7f1dfe4f36fffefffef7ffc7fb1eff7bf53cfe9dfbebed63fe1b1b7cfebe5eb8f7f2de5bf3bfffbffeff3defcfffebfb2f7e8f77984e77","miner":"0x4200000000000000000000000000000000000011","mixHash":"0xad01bff52072db4cb559d8c10896b05a888ab2d1d6b380674873b177660b6707","nonce":"0x0000000000000000","number":"0x222020CA","parentBeaconBlockRoot":"0x3253811d0f0da0f8a53e35779616f26af5590c0a0efce44e25e5997e6ab37476","parentHash":"0xfcce45dee3ee577e7c8b89beb08a0c36db9c6fc15f003d8b7bb85b7ebfba0452","receiptsRoot":"0xe878356a28e394bb7e04a9ffb70ba667d0dc176f6a007139a110e5cda018adc9","requestsHash":"0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347","size":"0x1cb3e","stateRoot":"0x36f1d129e7398444e6eb9505464f8d4a9f1ff2d58127a797776fe4ea0c6962c6","timestamp":"0x68ce5ff3","transactions":["0xdae86681341440c523c62eb76c9eb5f07bc5f4fe520a10b2f6af8bd23e92b55c"],"transactionsRoot":"0xf5281f791a550fedfc6abab76a0ecf111efcfda34149d1eacdf11e5ae4317662","uncles":[]},"id":1}"#;

    pub const MOCK_GET_SWAP_CONTRACT_BASE_LOGS: &str = r#"{
  "jsonrpc": "2.0",
  "id": 1,
  "result": [
    {
      "address": "0xa72ab997ccd4c55a7adc049df8057d577f5322a8",
      "topics": [
        "0xc33dada04354dd803ea44b93af35ba61d4bfa477f5f06c86b6a00cfc0c261bea",
        "0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace",
        "0x0000000000000000000000004200000000000000000000000000000000000006",
        "0x000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda02913"
      ],
      "data": "0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace00000000000000000000000000000000000000000000000000037235b96ea0000000000000000000000000000000000000000000000000000000000000423ce6000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000497f904948f3937303030303030303030303030309334323534303032363739383931383236333238933431393837303036343530353332333236303084312e3325f90454f901b784383435338f3937303030303030303030303030308734333430393636873433313932363184302e35258633343938373088313136393736323388302e30313933343730f85cf85aaa307834323030303030303030303030303030303030303030303030303030303030303030303030303036aa30783833333538396643443665446236453038663463374333324434663731623534626441303239313383313030c131f90105b901023078303030303030303030303030303030303030303030303030343230303030303030303030303030303030303030303030303030303030303030303030303030363030303030303030303030303030303030303030303030303833333538396663643665646236653038663463376333326434663731623534626461303239313330303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303431653831648a31373538343638323431f8bf8369637087343334303936369334333331323335383335383832333333393631933432393635383539343931393532373533303084302e382530303030f87df83c9b716b7277702d7a696161612d61616161672d6175656d712d6361699b7865766e6d2d67616161612d61616161722d7161666e712d63616983313030f83d9b7865766e6d2d67616161612d61616161722d7161666e712d6361699b7a326979652d66796161612d61616161672d61743270612d6361698431303030c0c030f901d682353693343235373038333833353838323333333936319334323534303032363739383931383236333238933431393837303036343530353332333236303084312e3325863334393837308931303030303030303088302e30333637363084302e3035f85cf85aaa307838414337366135316363393530643938323244363862383366453141643937423332436435383064aa30783535643339383332366639393035396646373735343835323436393939303237423331393739353583313030c131f90105b901023078303030303030303030303030303030303030303030303030386163373661353163633935306439383232643638623833666531616439376233326364353830643030303030303030303030303030303030303030303030303535643339383332366639393035396666373735343835323436393939303237623331393739353530303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030336162646166656632373738333039388a31373538343638323432000000000000000000",
      "blockNumber": "0x222020CA",
      "transactionHash": "0xdae86681341440c523c62eb76c9eb5f07bc5f4fe520a10b2f6af8bd23e92b55c",
      "transactionIndex": "0x5",
      "blockHash": "0xfcce45dee3ee577e7c8b89beb08a0c36db9c6fc15f003d8b7bb85b7ebfba0452",
      "logIndex": "0x6",
      "removed": false
    }
  ]
}"#;

    pub const MOCK_GET_SWAP_CONTRACT_BASE_LOGS_EVM_TO_ICP: &str = r#"{
"jsonrpc": "2.0",
"id": 1,
"result": [
{
"address": "0xa72ab997ccd4c55a7adc049df8057d577f5322a8",
"topics": [
"0xc33dada04354dd803ea44b93af35ba61d4bfa477f5f06c86b6a00cfc0c261bea",
"0x1d0b5ef2c95dcfe54bdbeed8236d2101c037c12e4cb0f1e70c6c5bcc03020000",
"0x0000000000000000000000004200000000000000000000000000000000000006",
"0x000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda02913"
],
"data": "0x000000000000000000000000daf40d6d8fcfbbffd1deba15990b7e08780f7ace000000000000000000000000000000000000000000000000002386f26fc1000000000000000000000000000000000000000000000000000000000000027ea2da000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000259f9025691313030303030303030303030303030303088343138383936353388343135353435333584302e3825f9022af901ba843834353391313030303030303030303030303030303088343138353336353888343136343433383984302e352586333439383730873535333330373188302e30303832303230f85cf85aaa307834323030303030303030303030303030303030303030303030303030303030303030303030303036aa30783833333538396643443665446236453038663463374333324434663731623534626441303239313383313030c131f90105b901023078303030303030303030303030303030303030303030303030343230303030303030303030303030303030303030303030303030303030303030303030303030363030303030303030303030303030303030303030303030303833333538396663643665646236653038663463376333326434663731623534626461303239313330303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030626238303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030323762373136358a31373538353736363739f86b8369637088343138353336353888343138383936353388343135353435333584302e382530303030f83ef83c9b716b7277702d7a696161612d61616161672d6175656d712d6361699b7865766e6d2d67616161612d61616161722d7161666e712d63616983313030c0c03000000000000000",
"blockNumber": "0x222020CA",
"transactionHash": "0xdae86681341440c523c62eb76c9eb5f07bc5f4fe520a10b2f6af8bd23e92b55c",
"transactionIndex": "0x5",
"blockHash": "0xfcce45dee3ee577e7c8b89beb08a0c36db9c6fc15f003d8b7bb85b7ebfba0452",
"logIndex": "0x6",
"removed": false
}
]
}"#;

    pub const MOCK_GET_LOGS_EMPTY: &str = r#"{
      "jsonrpc": "2.0",
        "id": 3,
        "result": []
    }"#;

    pub const MOCK_TRANSACTION_COUNT_LATEST: &str = r#"{"id":1,"jsonrpc":"2.0","result":"0x0"}"#;

    pub const MOCK_TRANSACTION_COUNT_BSC_LATEST: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x0"}"#;
    pub const MOCK_TRANSACTION_COUNT_BASE_LATEST: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x0"}"#;

    pub const MOCK_TRANSACTION_COUNT_LATEST_ERC20: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;

    pub const MOCK_TRANSACTION_COUNT_LATEST_SWAP_BSC: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;

    pub const MOCK_TRANSACTION_COUNT_LATEST_SWAP_BASE: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;

    pub const MOCK_TRANSACTION_COUNT_FINALIZED: &str = r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;

    pub const MOCK_TRANSACTION_COUNT_BSC_FINALIZED: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;
    pub const MOCK_TRANSACTION_COUNT_BASE_FINALIZED: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x1"}"#;

    pub const MOCK_TRANSACTION_COUNT_FINALIZED_ERC20: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x2"}"#;

    pub const MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BSC: &str =
        r#"{"id":1,"jsonrpc":"2.0","result":"0x2"}"#;

    pub const MOCK_TRANSACTION_COUNT_FINALIZED_SWAP_BASE: &str =
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
            "transactionHash": "0x23e4ac0e4bde9f2c12a3906d7145769a52d96456fca38f3de399a1c0038309fb",
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
            "transactionHash": "0x1bf19dee9c59944ddaed2252ad483a3df892a009e245330bb681481350926422",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_MINT_WRAPPED_ICRC_RECEIPT: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xe06f75670e6fdc6a7d988cff3227cd1d8b767ad37f9c2bb57b2bcaef7abcef31",
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
            "transactionHash": "0x51a454c6e327aecd8fcf5c7db7a52e8df7119c9247db5e6c1c5f5eee3be794d1",
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
            "transactionHash": "0x54a97b762eca864e89a680c1e116632600dfc634ba80c8bd89689920e1ae99f3",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_TRANSACTION_RECEIPT_APPROVE_ERC20: &str = r#"{
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
            "transactionHash": "0x338927f24d89e7dbc8f0063c703f03360bcb6d21c7608c88a0641f7acd4d6999",
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

    pub const MOCK_WRAPPED_ICRC_DEPLOYED_AND_DEPOSIT: &str = r#"{
    "jsonrpc": "2.0",
    "id": 3,
    "result": [
        {
            "address": "0xabcdef1234567890abcdef1234567890abcdef12",
            "topics": [
                "0xe63ddf723173735772522be59b64b9c95be6eb8f14b87948f670ad6f8949ab2e",
                "0x0a00000000000000020101000000000000000000000000000000000000000000",
                "0x0000000000000000000000001234567890abcdef1234567890abcdef12345678"
            ],
            "data": "0x",
            "blockNumber": "0x1a2b3c",
            "transactionHash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            "transactionIndex": "0x0",
            "blockHash": "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "logIndex": "0x0",
            "removed": false
        },
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

    pub const MOCK_TRANSACTION_RECEIPT_APPROVE_BSC_ERC20: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0x5f5e100",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0xea68753ef2b082afa2faecd7b071058da413e35cb8f2d59714f282a09a96d80f",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_TRANSACTION_RECEIPT_APPROVE_BASE_ERC20: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0x28995e",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0xc45362a05ab691709b829651292c07377249f303e1b5043a8218106aedcc00f5",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_TRANSACTION_RECEIPT_SWAP_BSC: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0x5f5e100",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0x7a7898b55fff40cb69f8443b6f541ed7c45de60feed4a5df6219e418f9dacd2d",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_TRANSACTION_RECEIPT_SWAP_BASE: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0x28995e",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0x3ef35a6e2c40c73fe054ed1795b42d32076f39688134029b830bf689926cb2c6",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_TRANSACTION_RECEIPT_SWAP_BSC_REFUND: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0x5f5e100",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0x91f05903165e76de4e526fc66e338fba7da295fad97d1662935e51bbe44dd04c",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_FAILED_TRANSACTION_RECEIPT_SWAP_BSC: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "blockHash": "0xa99ddaae8a1488af78eab4942d91e7c3640479ee7162c5ae3d1e3fe325599b9c",
            "blockNumber": "0x2bcf802",
            "contractAddress": null,
            "cumulativeGasUsed": "0x1f00c",
            "effectiveGasPrice": "0x5f5e100",
            "from": "0xffd465f2655e4ee9164856715518f4287b22a49d",
            "gasUsed": "0x5208",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x0",
            "to": "0x3bce376777ecfeb93953cc6c1bb957fbacb1a261",
            "transactionHash": "0x7a7898b55fff40cb69f8443b6f541ed7c45de60feed4a5df6219e418f9dacd2d",
            "transactionIndex": "0x3",
            "type": "0x2"
        }
    }"#;

    pub const MOCK_ICRC_RELEASE_REUQEST: &str = r#"{
    "jsonrpc": "2.0",
    "id": 3,
    "result": [
        {
            "address": "0xabcdef1234567890abcdef1234567890abcdef12",
            "topics": [
                "0x37199deebd336af9013dbddaaf9a68e337707bb4ed64cb45ed12841af85e0377",
                "0x000000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd",
                "0x0a00000000000000020101000000000000000000000000000000000000000000",
                "0x0000000000000000000000001234567890abcdef1234567890abcdef12345678"
            ],
            "data": "0x00000000000000000000000000000000000000000000000000000000000186a00000000000000000000000000000000000000000000000000000000000000000",
            "blockNumber": "0x1a2b3c",
            "transactionHash": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            "transactionIndex": "0x0",
            "blockHash": "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "logIndex": "0x0",
            "removed": false
        }
    ]
}"#;

    pub fn generate_and_submit_mock_http_response(
        pic: &PocketIc,
        http_requests_list: &[CanisterHttpRequest],
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
