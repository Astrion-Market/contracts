// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Address} from "@openzeppelin/contracts/utils/Address.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title AstrionAccount
/// @notice A minimal execution account owned by one user for one scope
/// `(owner, chainId, protocol, marketScope, version)`. The account itself
/// holds the user's USDC, collateral, supply claims and debt for that scope;
/// no shared router or custodian ever owns a position.
///
/// Ownership semantics (frozen for alpha):
/// - `owner` is set in the constructor (atomic with deployment) and is
///   immutable. There is no initializer to front-run and no ownership transfer.
/// - The owner can always act directly through `execute`/`executeBatch`; this
///   path does not depend on Astrion's UI, relayer, registry or any pause.
/// - The account never uses delegatecall.
contract AstrionAccount is ReentrancyGuard {
    struct Call {
        address target;
        uint256 value;
        bytes data;
    }

    address public immutable owner;
    address public immutable factory;
    bytes32 public immutable protocol;
    bytes32 public immutable marketScope;
    uint32 public immutable version;

    event Executed(address indexed target, uint256 value, bytes4 selector);

    error NotOwner();
    error ZeroOwner();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor(address owner_, bytes32 protocol_, bytes32 marketScope_, uint32 version_) {
        if (owner_ == address(0)) revert ZeroOwner();
        owner = owner_;
        factory = msg.sender;
        protocol = protocol_;
        marketScope = marketScope_;
        version = version_;
    }

    receive() external payable {}

    /// @notice Owner-only direct call. Always available; used for recovery,
    /// repayment and exits even when application policy pauses new risk.
    function execute(address target, uint256 value, bytes calldata data)
        external
        onlyOwner
        nonReentrant
        returns (bytes memory)
    {
        return _call(target, value, data);
    }

    /// @notice Owner-only batch of direct calls, executed atomically.
    function executeBatch(Call[] calldata calls)
        external
        onlyOwner
        nonReentrant
        returns (bytes[] memory results)
    {
        results = new bytes[](calls.length);
        for (uint256 i = 0; i < calls.length; i++) {
            results[i] = _call(calls[i].target, calls[i].value, calls[i].data);
        }
    }

    /// @dev Plain CALL in the account's own context; reverts bubble up.
    function _call(address target, uint256 value, bytes memory data)
        internal
        returns (bytes memory result)
    {
        result = Address.functionCallWithValue(target, data, value);
        emit Executed(target, value, data.length >= 4 ? bytes4(data) : bytes4(0));
    }
}
