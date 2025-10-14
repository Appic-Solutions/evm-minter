// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import {IcpEvmBridge} from "../src/IcpEvmBridge.sol";

contract DeployEthBridge is Script {
    function run() external {
        address minter = vm.envAddress("ETH_MINTER_ADDRESS");

        uint256 pk = vm.envUint("PRIVATE_KEY");

        console.log(
            "Deploying contract with address",
            vm.addr(pk)
        );

        vm.startBroadcast(pk);

        IcpEvmBridge bridge = new IcpEvmBridge(minter);
        console.log("Deployed IcpEvmBridge at:", address(bridge));

        vm.stopBroadcast();
    }
}
