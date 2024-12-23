// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/Deposit.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract MockERC20 is ERC20 {
    constructor(string memory name, string memory symbol) ERC20(name, symbol) {
        _mint(msg.sender, 1_000_000 * 10 ** decimals());
    }
}

contract DepositTest is Test {
    DepositHelper public deposit;
    MockERC20 public token;
    address public minter = address(1);
    address public user = address(2);

    function setUp() public {
        // Deploy the contract
        deposit = new DepositHelper(minter);

        // Deploy the mock token and assign balances
        token = new MockERC20("Mock Token", "MTKN");

        // Allocate tokens to user for testing
        token.transfer(user, 1000 * 10 ** token.decimals());
        token.transfer(address(4), 1000 * 10 ** token.decimals());
        token.transfer(address(5), 1000 * 10 ** token.decimals());
    }

    function testDepositErc20() public {
        // Arrange
        uint256 amount = 100 * 10 ** token.decimals();
        vm.startPrank(user);
        token.approve(address(deposit), amount);

        // Act
        bytes32 principalId = "testPrincipalId";
        bytes32 subaccount = "TestSubaccount";
        deposit.deposit(address(token), amount, principalId, subaccount);

        // Assert
        assertEq(token.balanceOf(minter), amount);
    }

    function testDepositNative() public {
        // Arrange
        uint256 amount = 1 ether;

        // Act
        bytes32 principalId = "testPrincipalId";
        bytes32 subaccount = "TestSubaccount";

        vm.deal(user, amount);
        vm.prank(user);
        deposit.deposit{value: amount}(address(0), amount, principalId, subaccount);

        // Assert
        assertEq(address(minter).balance, amount);
    }
}
