// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "src/abstract/TokenManager.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

contract IcpEvmBridge is TokenManager, Ownable, Pausable {
    using SafeERC20 for IERC20;

    
    // Custom errors
    error InvalidICPAddress();
    error InvalidRecipient();
    error TransferFailed();
    error ZeroAmount();
    error InsufficientNativeToken();
    error InvalidTokenIdentifier();

    
    event TokenBurn(
        address indexed fromAddress,        
        uint256 amount,              
        bytes32 indexed icpRecipient,
        address wrappedToken
    );

    event FeeWithdrawal(address indexed collector, uint256 amount, uint256 timestamp);

    struct BurnParams {
    uint256 amount;
    bytes32 icpRecipient;
    bytes32 principal;    
    }

    constructor(
        address _minterAddress
    ) TokenManager(_minterAddress)   Ownable(msg.sender) {

       
    }

    /**
        * @dev Burns/Locks tokens to bridge them to ICP
        * For native tokens (address(0)):
        * - Locks the native token by sending to minter
        * - Requires msg.value to cover amount + burn fee
        * 
        * For ERC20 tokens:
        * - Burns the wrapped token by sending to minter (using WrappedToken burn mechanism)
        * - Requires msg.value to cover burn fee
        * - Requires token approval
        *
        * @param params BurnParams containing:
        * - amount: Amount of tokens to burn/lock
        * - icpRecipient: ICP recipient address as bytes32
        * - Principal: : ICP token identifier
        */
    function burn(
        BurnParams calldata params
    ) external payable whenNotPaused {
        if (params.amount == 0) revert ZeroAmount();
        if (params.icpRecipient == bytes32(0)) revert InvalidICPAddress();
        address wrappedToken = _baseToWrapped[params.principal];
        if (wrappedToken == address(0)) revert InvalidTokenIdentifier();

        // Handle native token burn/lock
        if (wrappedToken == NATIVE_TOKEN_ADDRESS) {
            
            // Transfer to minter
            (bool success,) = minterAddress.call{value: params.amount}("");
            if (!success) revert TransferFailed();
            
        } 
        // Handle ERC20 token burn
        else {
            
            // Transfer tokens to minter (will automatically burn due to WrappedToken logic)
            IERC20(wrappedToken).safeTransferFrom(msg.sender, minterAddress, params.amount);
        }

         emit TokenBurn(
            msg.sender,
            params.amount,
            params.icpRecipient,
            wrappedToken
        );
    }
    
    function deployERC20(
    string memory name,
    string memory symbol,
    uint8 decimals,
    bytes32 baseToken
    ) public onlyOwner returns (address) {
        return _deployERC20(name, symbol, decimals, baseToken);
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    receive() external payable {
        if (msg.value == 0) revert ZeroAmount();
    }

    fallback() external payable {
        revert("Unsupported operation");
    }
}



