// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import {IcpEvmBridge} from "../src/IcpEvmBridge.sol";
import {WrappedToken} from "../src/WrappedToken.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";


contract IcpEvmBridgeTest is Test {
    IcpEvmBridge bridge;
    address minter = address(0xBEEF);
    address owner = address(this);
    address attacker = address(0xBAD);

    function setUp() public {
        bridge = new IcpEvmBridge(minter);
    }

    function testOwnerCanDeployERC20() public {
        address deployed = bridge.deployERC20("TestToken", "TT", 18, bytes32("base1"));
        assertTrue(deployed != address(0), "Token deployment failed");
    }

    function testFailNonOwnerCannotDeployERC20() public {
        vm.prank(attacker);
        bridge.deployERC20("EvilToken", "EVIL", 18, bytes32("evil1"));
    }

    function testDuplicateBaseTokenFails() public {
        bridge.deployERC20("TokenA", "A", 18, bytes32("token1"));

        vm.expectRevert("Wrapper already exist");
        bridge.deployERC20("TokenA", "A", 18, bytes32("token1"));
    }

    function testERC20Burn() public {
    bytes32 baseToken = bytes32("baseERC20");
    address wrapped = bridge.deployERC20("TestWrapped", "TW", 18, baseToken);

    uint256 mintAmount = 100 ether;
    address user = address(0xCAFE);

    vm.prank(minter);
    WrappedToken(wrapped).transfer(user, mintAmount);

    vm.startPrank(user);
    IERC20(wrapped).approve(address(bridge), mintAmount);

    IcpEvmBridge.BurnParams memory burnParams = IcpEvmBridge.BurnParams({
        amount: mintAmount,
        icpRecipient: bytes32("recipientICP"),
        principal: baseToken
    });

    bridge.burn{value: 0}(burnParams);
    vm.stopPrank();

    uint256 finalBalance = IERC20(wrapped).balanceOf(user);
    assertEq(finalBalance, 0, "User should have 0 after burn");
}


function testFailBurnZeroAmount() public {
    IcpEvmBridge.BurnParams memory params = IcpEvmBridge.BurnParams({
        amount: 0,
        icpRecipient: bytes32("recipient"),
        principal: bytes32("base")
    });

    bridge.burn(params);
}

function testFailBurnWithInvalidTokenIdentifier() public {
    IcpEvmBridge.BurnParams memory params = IcpEvmBridge.BurnParams({
        amount: 1 ether,
        icpRecipient: bytes32("recipient"),
        principal: bytes32("nonexistent")
    });

    bridge.burn(params); 
}


}

