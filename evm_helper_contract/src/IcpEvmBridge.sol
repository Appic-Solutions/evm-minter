// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "src/abstract/TokenManager.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

contract IcpEvmbridge is TokenManager, Ownable, Pausable {


    address payable private immutable minterAddress;

    // Access control
    mapping(address => bool) public controllerAccessList;

    // Event to log token deposits into the contract
    event DepositLog(
        address from_address,
        address indexed token,
        uint256 indexed amount,
        bytes32 indexed principal,
        bytes32 subaccount
    );

     // Events
    event TokenBurn(
        address indexed fromAddress,
        uint256 amount,
        address indexed fromERC20,
        bytes32 toTokenId,
        bytes32 indexed recipientID,
        uint256 destinationMintFee
    );

    event TokenMint(
        bytes32 baseTokenID,
        address WrapErc2o,
        uint256 amount,
        address ToAddress
    );
    event FeeWithdrawal(address indexed collector, uint256 amount, uint256 timestamp);
    event FeeCollectorUpdate(address indexed oldCollector, address indexed newCollector);
    event BurnFeeUpdate(uint256 oldFee, uint256 newFee);
    event MinterDeposit(address indexed sender, uint256 amount);


    /**
     * @dev Constructor initializes the contract.
     * Sets the contract deployer as the initial owner and grants them the `MINTER_ROLE`.
     */
    constructor(address _minterAddress) {
        minterAddress = payable(_minterAddress);
    }

    /**
     * @dev Return minter main address.
     * @return address of minter main address.
     */
    function getMinterAddress() public view returns (address) {
        return minterAddress;
    }

    /**
     * @dev Locks the specified amount of tokens or native currency from the user.
     * Transfers the assets to the minter address.
     * @param token The address of the token to lock. Use `address(0)` for native currency.
     * @param amount The amount of tokens to lock.
     * @param principal A unique identifier associated with the lock operation.
     */
    function deposit(address token, uint256 amount, bytes32 principal, bytes32 subaccount) external payable {
        if (msg.value > 0) {
            // Transfer native currency from contract to minter
            (bool success,) = minterAddress.call{value: msg.value}("");
            if (!success) {
                revert("Transfer to minter failed!");
            }

            emit DepositLog(msg.sender, address(0), msg.value, principal, subaccount);
        } else {
            IERC20 tokenContract = IERC20(token);

            tokenContract.safeTransferFrom(msg.sender, minterAddress, amount);

            emit DepositLog(msg.sender, token, amount, principal, subaccount);
        }
    }

    function pause() external onlyController {
        _pause();
    }

    function unpause() external onlyController {
        _unpause();
    }

    receive() external payable {
        if (msg.value == 0) revert ZeroAmount();
    }

    fallback() external payable {
        revert("Unsupported operation");
    }
}

