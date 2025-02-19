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
    address indexed burner, 
    address burnErc20,      
    uint256 amount,             
    bytes32 indexed icpRecipient,  
    uint256 burnFee              
    );

    event TokenMint(
    address indexed recipient,    
    uint256 amount,  
    address MintErc20,            
    bytes32 indexed icpIdentifier, 
    bytes32 indexed icpSender     
    );

    event FeeWithdrawal(address indexed collector, uint256 amount, uint256 timestamp);
    event FeeCollectorUpdate(address indexed oldCollector, address indexed newCollector);
    event BurnFeeUpdate(uint256 oldFee, uint256 newFee);
    event MinterDeposit(address indexed sender, uint256 amount);

    // Custom errors
    error InsufficientFee();
    error InvalidFeeCollector();
    error ZeroAmount();
    error InvalidICPAddress();
    error InvalidRecipient();
    error TransferFailed();

    /**
     * @dev Constructor initializes the contract.
     * Sets the contract deployer as the initial owner and grants them the `MINTER_ROLE`.
     */
    constructor(
        address _minterAddress,
        address _feeCollector,
        uint256 _FeeInWei,
        address[] memory _controllers,
        address initialOwner
        
        )TokenManager(_minterAddress) Ownable(initialOwner) {
        minterAddress = payable(_minterAddress);
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


    // When ICP tokens are locked, this mints wrapped tokens on EVM
    function mint(
        address evmRecipient,
        uint256 amount,
        bytes32 icpIdentifier,
        bytes32 icpSender,
        string memory name,
        string memory symbol,
        uint8 decimals
    ) external onlyController whenNotPaused returns (uint32) {
        if (amount == 0) revert ZeroAmount();
        if (evmRecipient == address(0)) revert InvalidRecipient();

        uint32 operationId = operationIDCounter++;

        // Deploy wrapped token if it doesn't exist
        address wrappedToken = _baseToWrapped[address(uint160(uint256(icpIdentifier)))];
        if (wrappedToken == address(0)) {
            wrappedToken = deployERC20(name, symbol, decimals, address(uint160(uint256(icpIdentifier))));
        }

        // Mint wrapped tokens to recipient
        WrappedToken(wrappedToken).transfer(evmRecipient, amount);

        emit TokenMint(
            evmRecipient,
            amount,
            icpIdentifier,
            icpSender,
            operationId
        );

        return operationId;
    }


    // When someone wants to go back to ICP
    function burn(
        uint256 amount,
        bytes32 icpRecipient,
        address wrappedToken
    ) external payable whenNotPaused returns (uint32) {
        if (amount == 0) revert ZeroAmount();
        if (icpRecipient == bytes32(0)) revert InvalidICPAddress();
        
        // Check if enough fee was sent
        if (msg.value < burnFeeInWei) revert InsufficientFee();

        uint32 operationId = operationIDCounter++;

        // Transfer tokens to bridge (will be burned)
        IERC20(wrappedToken).safeTransferFrom(msg.sender, minterAddress, amount);
        
        // Update collected fees
        collectedBurnFees += burnFeeInWei;

        emit TokenBurn(
            msg.sender,
            amount,
            icpRecipient,
            wrappedToken,
            operationId
        );

        return operationId;
    }

    function isController(address account) internal view override returns (bool) {
        return controllerAccessList[account];
    }

     // Admin functions
    function setFeeCollector(address newFeeCollector) external onlyOwner {
        if (newFeeCollector == address(0)) revert InvalidFeeCollector();
        
        address oldCollector = feeCollector;
        feeCollector = newFeeCollector;
        
        emit FeeCollectorUpdate(oldCollector, newFeeCollector);
    }

    function updateBurnFee(uint256 newFeeInWei) external onlyOwner {
        emit BurnFeeUpdate(burnFeeInWei, newFeeInWei);
        burnFeeInWei = newFeeInWei;
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

