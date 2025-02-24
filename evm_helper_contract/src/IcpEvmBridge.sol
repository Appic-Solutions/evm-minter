// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "src/abstract/TokenManager.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

contract IcpEvmBridge is TokenManager, Ownable, Pausable {
    using SafeERC20 for IERC20;

    // State variables
    mapping(address => bool) public controllerAccessList;
    uint256 public burnFeeInWei;
    uint256 public collectedBurnFees;
    address public feeCollector;

    // Custom errors
    error InvalidICPAddress();
    error InvalidRecipient();
    error TransferFailed();
    error ZeroAmount();
    error InsufficientNativeToken();
    error InvalidFeeCollector();
    error InvalidTokenIdentifier();

    event TokenBurn(
        address indexed fromAddress,        
        uint256 amount,              
        bytes32 indexed icpRecipient,
        address wrappedToken,
        uint256 burnFee       
    );

    event FeeWithdrawal(address indexed collector, uint256 amount, uint256 timestamp);

    struct BurnParams {
    uint256 amount;
    bytes32 icpRecipient;
    bytes32 principal;    
    }

    constructor(
        address _minterAddress,
        address _feeCollector,
        uint256 _burnFeeInWei,
        address[] memory _controllers,
        address initialOwner
    ) TokenManager(_minterAddress) Ownable(initialOwner) {
        if (_feeCollector == address(0)) revert InvalidFeeCollector();
        feeCollector = _feeCollector;
        burnFeeInWei = _burnFeeInWei;

        controllerAccessList[msg.sender] = true;
        for (uint256 i = 0; i < _controllers.length; ++i) {
            if (_controllers[i] != address(0)) {
                controllerAccessList[_controllers[i]] = true;
            }
        }
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
            if (msg.value < params.amount + burnFeeInWei) revert InsufficientNativeToken();
            
            // Transfer to minter
            (bool success,) = minterAddress.call{value: params.amount}("");
            if (!success) revert TransferFailed();
            
            collectedBurnFees += burnFeeInWei;
        } 
        // Handle ERC20 token burn
        else {
            if (msg.value < burnFeeInWei) revert InsufficientNativeToken();
            
            // Transfer tokens to minter (will automatically burn due to WrappedToken logic)
            IERC20(wrappedToken).safeTransferFrom(msg.sender, minterAddress, params.amount);
            collectedBurnFees += burnFeeInWei;
        }

         emit TokenBurn(
            msg.sender,
            params.amount,
            params.icpRecipient,
            wrappedToken,
            burnFeeInWei
        );
    }
    
    function withdrawFees() external onlyOwner {
        uint256 feesToWithdraw = collectedBurnFees;
        if (feesToWithdraw == 0) revert ZeroAmount();
        
        collectedBurnFees = 0;
        (bool success,) = feeCollector.call{value: feesToWithdraw}("");
        if (!success) revert TransferFailed();
        
        emit FeeWithdrawal(feeCollector, feesToWithdraw, block.timestamp);
    }
    
    
    function isController(address account) internal view override returns (bool) {
        return controllerAccessList[account];
    }

    function addController(address controller) external onlyOwner {
        if (controller == address(0)) revert InvalidRecipient();
        controllerAccessList[controller] = true;
    }

    function removeController(address controller) external onlyOwner {
        controllerAccessList[controller] = false;
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