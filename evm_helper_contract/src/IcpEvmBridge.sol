// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "src/abstract/TokenManager.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

contract IcpEvmBridge is TokenManager, Ownable, Pausable {
    using SafeERC20 for IERC20;

    // Access control
    mapping(address => bool) public controllerAccessList;

    // Custom errors
    error InvalidICPAddress();
    error InvalidRecipient();
    error TransferFailed();
    error ZeroAmount();
    error InvalidTokenIdentifier();

    // Events
    event DepositLog(
        address from_address,
        address indexed token,
        uint256 indexed amount,
        bytes32 indexed principal,
        bytes32 subaccount
    );

    event TokenMint(
        address indexed recipient,    
        uint256 amount,              
        bytes32 indexed icpIdentifier,
        bytes32 indexed icpSender     
    );

    event TokenBurn(
        address indexed burner,        
        uint256 amount,              
        bytes32 indexed icpRecipient, 
        address wrappedToken        
    );

    struct MintParams {
        address evmRecipient;
        uint256 amount;
        bytes32 icpIdentifier;
        bytes32 icpSender;
    }

    struct BurnParams {
        uint256 amount;
        bytes32 icpRecipient;
        address wrappedToken;
    }

    constructor(
        address _minterAddress,
        address[] memory _controllers,
        address initialOwner
    ) TokenManager(_minterAddress) Ownable(initialOwner) {
        // Set up controllers
        controllerAccessList[msg.sender] = true;
        for (uint256 i = 0; i < _controllers.length; ++i) {
            if (_controllers[i] != address(0)) {
                controllerAccessList[_controllers[i]] = true;
            }
        }
    }

    /**
     * @dev Locks the specified amount of tokens or native currency from the user.
     */
    function deposit(
        address token, 
        uint256 amount, 
        bytes32 principal, 
        bytes32 subaccount
    ) external payable {
        if (msg.value > 0) {
            (bool success,) = minterAddress.call{value: msg.value}("");
            if (!success) revert TransferFailed();

            emit DepositLog(msg.sender, address(0), msg.value, principal, subaccount);
        } else {
            IERC20(token).safeTransferFrom(msg.sender, minterAddress, amount);
            emit DepositLog(msg.sender, token, amount, principal, subaccount);
        }
    }
        
    function mint(
        MintParams calldata params
    ) external onlyController whenNotPaused {
        if (params.amount == 0) revert ZeroAmount();
        if (params.evmRecipient == address(0)) revert InvalidRecipient();

        address wrappedToken = _baseToWrapped[address(uint160(uint256(params.icpIdentifier)))];
        if (wrappedToken == address(0)) revert InvalidTokenIdentifier();

        WrappedToken(wrappedToken).transfer(params.evmRecipient, params.amount);

        emit TokenMint(
            params.evmRecipient,
            params.amount,
            params.icpIdentifier,
            params.icpSender
        );
    }

    function burn(
        BurnParams calldata params
    ) external payable whenNotPaused {
        if (params.amount == 0) revert ZeroAmount();
        if (params.icpRecipient == bytes32(0)) revert InvalidICPAddress();

        IERC20(params.wrappedToken).safeTransferFrom(msg.sender, minterAddress, params.amount);

        emit TokenBurn(
            msg.sender,
            params.amount,
            params.icpRecipient,
            params.wrappedToken
        );
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