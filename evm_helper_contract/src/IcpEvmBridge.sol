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
 * For native tokens (ETH, BNB, etc.):
 * - Detects by checking msg.value > 0
 * - Locks by transferring to minter
 * 
 * For wrapped tokens (created by this bridge):
 * - Looks up in _baseToWrapped mapping
 * - Burns by transferring to minter (WrappedToken handles burning)
 * 
 * For external ERC20 tokens:
 * - Uses params.principal directly as token address when not found in mapping
 * - Locks by transferring to minter
 *
 * @param params BurnParams containing:
 * - amount: Amount of tokens to burn/lock
 * - icpRecipient: ICP recipient address as bytes32
 * - principal: ICP token identifier or ERC20 token address
 */
function burn(
    BurnParams calldata params
) external payable whenNotPaused {
    if (params.amount == 0) revert ZeroAmount();
    if (params.icpRecipient == bytes32(0)) revert InvalidICPAddress();
    
    if (msg.value > 0) {
        if (msg.value != params.amount) revert InsufficientNativeToken();
        
        (bool success,) = minterAddress.call{value: params.amount}("");
        if (!success) revert TransferFailed();
        
        emit TokenBurn(
            msg.sender,
            params.amount,
            params.icpRecipient,
            NATIVE_TOKEN_ADDRESS
        );
        return;
    }
    
    address wrappedToken = _baseToWrapped[params.principal];
    if (wrappedToken != address(0)) {
        IERC20(wrappedToken).safeTransferFrom(msg.sender, minterAddress, params.amount);
        
        emit TokenBurn(
            msg.sender,
            params.amount,
            params.icpRecipient,
            wrappedToken
        );
        return;
    }
    
    address externalToken = address(uint160(uint256(params.principal)));
    if (externalToken == address(0)) revert InvalidTokenIdentifier();
    
    IERC20(externalToken).safeTransferFrom(msg.sender, minterAddress, params.amount);
    
    emit TokenBurn(
        msg.sender,
        params.amount,
        params.icpRecipient,
        externalToken
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



