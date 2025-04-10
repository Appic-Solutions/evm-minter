// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import {IcpEvmBridge} from "../src/IcpEvmBridge.sol";

contract DeployBridge is Script {
    function run() external {
        address minter = vm.envAddress("MINTER_ADDRESS");
        address owner = vm.envAddress("OWNER_ADDRESS");

        vm.startBroadcast(owner);

        IcpEvmBridge bridge = new IcpEvmBridge(minter);
        console.log("Deployed IcpEvmBridge at:", address(bridge));

        vm.stopBroadcast();
    }
}

