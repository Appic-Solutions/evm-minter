// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.20;
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/**
 * @title Token Locking Smart Contract
 * @notice This contract allows users to lock their tokens, with the contract owner having the exclusive right to withdraw the locked tokens.
 */
contract Deposit {
    using SafeERC20 for IERC20;

    address payable private immutable minterAddress;
    mapping(address => uint256) public tokenAmount;
    mapping(address => uint256) public feeTank;

    // Event to log token deposits into the contract
    event TokensLocked(
        address user,
        address indexed token,
        uint256 indexed amount,
        bytes32 indexed principal,
        bytes32 subaccount
    );

    // Event to log gas fees added
    event gasFeeAdded(address user, uint256 amount);

    /**
     * @dev Constructor initializes the contract.
     * Sets the contract deployer as the initial owner and grants them the `MINTER_ROLE`.
     */
    constructor(address _minterAddress) {
        minterAddress = payable(_minterAddress);
    }

    /**
     * @dev Locks the specified amount of tokens or native currency from the user.
     * Transfers the assets to the minter address.
     * @param token The address of the token to lock. Use `address(0)` for native currency.
     * @param amount The amount of tokens to lock.
     * @param principal A unique identifier associated with the lock operation.
     */
    function deposit(
        address token,
        uint256 amount,
        bytes32 principal,
        bytes32 subaccount
    ) external payable {
        if (msg.value > 0) {
            // Transfer native currency from contract to minter
            (bool success, ) = minterAddress.call{value: msg.value}("");
            if (!success) {
                revert("Transfer to minter failed!");
            }

            emit TokensLocked(
                msg.sender,
                address(0),
                msg.value,
                principal,
                subaccount
            );
        } else {
            IERC20 tokenContract = IERC20(token);

            tokenContract.safeTransferFrom(msg.sender, minterAddress, amount);

            emit TokensLocked(msg.sender, token, amount, principal, subaccount);
        }
    }
}
