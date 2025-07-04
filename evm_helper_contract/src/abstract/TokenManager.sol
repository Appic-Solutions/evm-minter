// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "src/libraries/StringUtils.sol";
import "src/WrappedToken.sol";

abstract contract TokenManager {
    using SafeERC20 for IERC20;

    // Address representing the native token
    address public constant NATIVE_TOKEN_ADDRESS = address(0);

    /// List of wrapped tokens
    address[] private _wrappedTokenList;
    mapping(bytes32 => address) internal _baseToWrapped;

    // Minter address
    address public minterAddress;

    // Custom errors
    error WrapperAlreadyExists();
    error InvalidBaseToken();
    error NotController();
    error InvalidMinter();

    /// Events
    event WrappedTokenDeployed(
        bytes32 indexed baseToken,
        address indexed wrappedERC20
    );

    /// Token metadata
    struct TokenMetadata {
        bytes32 name;
        bytes16 symbol;
        uint8 decimals;
    }

    /**
     * @dev Constructor to initialize the TokenManager
     * @param _minterAddress The address of the minter
     */
    constructor(address _minterAddress) {
        if (_minterAddress == address(0)) revert InvalidMinter();
        minterAddress = _minterAddress;
    }

    /**
     * @dev Creates a new ERC20 compatible token contract as a wrapper
     * @param name Token name
     * @param symbol Token symbol
     * @param decimals Token decimals
     * @param baseToken ICP token identifier as bytes32
     */
    function _deployERC20(
        string memory name,
        string memory symbol,
        uint8 decimals,
        bytes32 baseToken
    ) internal returns (address) {
        require(
            _baseToWrapped[baseToken] == address(0),
            "Wrapper already exist"
        );

        WrappedToken wrappedERC20 = new WrappedToken(
            name,
            symbol,
            decimals,
            minterAddress
        );
        address tokenAddress = address(wrappedERC20);
        _wrappedTokenList.push(tokenAddress);

        _baseToWrapped[baseToken] = tokenAddress;

        // Update event to use bytes32
        emit WrappedTokenDeployed(baseToken, tokenAddress);

        return tokenAddress;
    }

    /**
     * @dev Query token metadata
     * @param token Address of the token to query
     */
    function getTokenMetadata(
        address token
    ) internal view returns (TokenMetadata memory meta) {
        try IERC20Metadata(token).name() returns (string memory _name) {
            meta.name = StringUtils.truncateUTF8(_name);
        } catch {}
        try IERC20Metadata(token).symbol() returns (string memory _symbol) {
            meta.symbol = bytes16(StringUtils.truncateUTF8(_symbol));
        } catch {}
        try IERC20Metadata(token).decimals() returns (uint8 _decimals) {
            meta.decimals = _decimals;
        } catch {}
    }

    /**
     * @dev Returns wrapped token for the given base token
     * @param baseToken ICP token identifier as bytes32
     */
    function getWrappedToken(
        bytes32 baseToken
    ) external view returns (address) {
        return _baseToWrapped[baseToken];
    }

    /**
     * @dev Returns list of all wrapped tokens
     */
    function listTokens() external view returns (address[] memory) {
        return _wrappedTokenList;
    }
}
