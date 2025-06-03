// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

import "forge-std/Script.sol";
import "../src/Deposit.sol";

contract DeployDeposit is Script {
    DepositHelper public deposit;

    function run() public {
        uint256 pk = vm.envUint("PRIVATE_KEY");
        address minter_address = vm.envAddress("BSCTEST_MINTER_ADDRESS");
        console.log(
            "Deploying DepositHelper contract with address",
            vm.addr(pk)
        );
        vm.startBroadcast(pk);
        _deployDeposit(minter_address);
        vm.stopBroadcast();
        console.log(
            "DepositHelper contract deployed at address",
            address(deposit)
        );
    }

    function _deployDeposit(address minter_address) internal {
        // Deploy the Deposit contract
        deposit = new DepositHelper(minter_address);
        console.log("DepositHelper contract initialized");
    }
}
