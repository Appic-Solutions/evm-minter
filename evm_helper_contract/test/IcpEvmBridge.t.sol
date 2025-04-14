// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/IcpEvmBridge.sol";
import "../src/WrappedToken.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract IcpEvmBridgeTest is Test {
    IcpEvmBridge bridge;
    address minter = address(0xBEEF);
    address owner = address(this);
    address user = address(0xCAFE);

    function setUp() public {
        bridge = new IcpEvmBridge(minter);
    }

    function testBurnNativeToken() public {
        vm.deal(user, 2 ether);

        vm.prank(user);
        bridge.burn{value: 1 ether}(
            IcpEvmBridge.BurnParams({
                amount: 1 ether,
                icpRecipient: bytes32("recipient"),
                TokenAddress: address(0) 
            })
        );

        
    }

    function testBurnERC20Token() public {
        bytes32 baseToken = bytes32("token1");
        address wrapped = bridge.deployERC20("Wrapped Gold", "wGOLD", 18, baseToken);

        uint256 amount = 100 ether;
        vm.prank(minter);
        WrappedToken(wrapped).transfer(user, amount);

        vm.startPrank(user);
        IERC20(wrapped).approve(address(bridge), amount);

        bridge.burn(
            IcpEvmBridge.BurnParams({
                amount: amount,
                icpRecipient: bytes32("recipient"),
                TokenAddress: wrapped
            })
        );

        uint256 userBalance = IERC20(wrapped).balanceOf(user);
        assertEq(userBalance, 0);
    }

    function testRevertIfZeroAmount() public {
        vm.expectRevert(IcpEvmBridge.ZeroAmount.selector);

        bridge.burn(
            IcpEvmBridge.BurnParams({
                amount: 0,
                icpRecipient: bytes32("abc"),
                TokenAddress: address(0)
            })
        );
    }

    function testRevertIfInvalidICPRecipient() public {
        vm.expectRevert(IcpEvmBridge.InvalidICPAddress.selector);

        bridge.burn(
            IcpEvmBridge.BurnParams({
                amount: 1 ether,
                icpRecipient: bytes32(0),
                TokenAddress: address(0)
            })
        );
    }

    function testRevertIfNativeAndAmountMismatch() public {
        vm.expectRevert(IcpEvmBridge.InsufficientNativeToken.selector);

        vm.deal(user, 2 ether);
        vm.prank(user);
        bridge.burn{value: 0.5 ether}(
            IcpEvmBridge.BurnParams({
                amount: 1 ether,
                icpRecipient: bytes32("bad"),
                TokenAddress: address(0)
            })
        );
    }

    function testRevertIfERC20TokenAddressZero() public {
        vm.expectRevert(IcpEvmBridge.InvalidTokenAddress.selector);

        bridge.burn(
            IcpEvmBridge.BurnParams({
                amount: 1 ether,
                icpRecipient: bytes32("invalid"),
                TokenAddress: address(0)
            })
        );
    }
}
