// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {Address} from "@openzeppelin/contracts/utils/Address.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {SignatureChecker} from "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";

import {IActionModule} from "../interfaces/IActionModule.sol";
import {RoutePolicy} from "./RoutePolicy.sol";

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
///
/// Relayed execution (ADR-0002): a relayer submits an owner-signed
/// `ExecutionIntent`. The account checks it and runs the module plan in its
/// own context under target, selector, approval, recipient and fee bounds.
contract AstrionAccount is ReentrancyGuard, EIP712 {
    using SafeERC20 for IERC20;

    struct Call {
        address target;
        uint256 value;
        bytes data;
    }

    /// @notice Everything the relayer cannot change.
    struct ExecutionIntent {
        address module; // action module to plan with
        bytes32 moduleCodeHash; // EXTCODEHASH the owner approved (pins logic + immutables)
        address target; // the protocol contract the owner expects (pool, Morpho, Comet)
        bytes32 actionHash; // keccak256(action)
        address recipient; // only allowed destination of token transfers out
        address relayer; // address(0) = any submitter
        address feeToken; // token the relayer fee is paid in
        uint256 maxFee; // fee cap in feeToken units
        uint256 nonce; // unordered; consumed or revoked once
        uint256 deadline; // action must not execute after this timestamp
        bytes32 transferId; // CCTP transfer binding (C10); 0 = not bridge-funded
    }

    bytes32 public constant EXECUTION_INTENT_TYPEHASH = keccak256(
        "ExecutionIntent(address module,bytes32 moduleCodeHash,address target,bytes32 actionHash,"
        "address recipient,address relayer,address feeToken,uint256 maxFee,uint256 nonce,"
        "uint256 deadline,bytes32 transferId)"
    );

    address public immutable owner;
    address public immutable factory;
    RoutePolicy public immutable policy;
    bytes32 public immutable protocol;
    bytes32 public immutable marketScope;
    uint32 public immutable version;

    /// @notice Consumed or revoked intent nonces.
    mapping(uint256 nonce => bool) public nonceUsed;

    event Executed(address indexed target, uint256 value, bytes4 selector);
    event IntentExecuted(uint256 indexed nonce, address indexed module, address relayer, uint256 fee);
    event NonceRevoked(uint256 indexed nonce);

    error NotOwner();
    error ZeroOwner();
    error InvalidSignature();
    error IntentExpired();
    error NonceAlreadyUsed();
    error WrongRelayer();
    error FeeAboveCap();
    error ActionHashMismatch();
    error ModuleCodeMismatch();
    error WrongProtocol();
    error RoutePaused();
    error TargetNotAllowed(address target);
    error SelectorNotAllowed(address target, bytes4 selector);
    error RecipientNotAllowed(address to);
    error ExcessAllowance(address token);
    error InvalidRecipient();
    error TransferBindingRequired();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor(
        address owner_,
        RoutePolicy policy_,
        bytes32 protocol_,
        bytes32 marketScope_,
        uint32 version_
    ) EIP712("AstrionAccount", "1") {
        if (owner_ == address(0)) revert ZeroOwner();
        owner = owner_;
        factory = msg.sender;
        policy = policy_;
        protocol = protocol_;
        marketScope = marketScope_;
        version = version_;
    }

    receive() external payable {}

    // ─── direct owner path (always available) ────────────────────────────────

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

    /// @notice Revoke an intent before it is executed.
    function revokeNonce(uint256 nonce) external onlyOwner {
        nonceUsed[nonce] = true;
        emit NonceRevoked(nonce);
    }

    // ─── relayed path ────────────────────────────────────────────────────────

    /// @notice Execute an owner-signed intent. `fee` is what the relayer
    /// charges, bounded by the signed `maxFee`.
    function executeIntent(
        ExecutionIntent calldata intent,
        bytes calldata action,
        bytes calldata signature,
        uint256 fee
    ) external nonReentrant {
        // Bridge-funded intents must go through the transfer-bound path.
        if (intent.transferId != bytes32(0)) revert TransferBindingRequired();
        _consumeIntent(intent, action, signature, fee);
        _runPlan(intent, action);
        _payFee(intent, fee);
    }

    function intentDigest(ExecutionIntent calldata intent) public view returns (bytes32) {
        return _hashTypedDataV4(keccak256(abi.encode(EXECUTION_INTENT_TYPEHASH, intent)));
    }

    function domainSeparator() external view returns (bytes32) {
        return _domainSeparatorV4();
    }

    /// @dev Checks that do not depend on the plan, then consumes the nonce
    /// before any external call (checks-effects-interactions).
    function _consumeIntent(
        ExecutionIntent calldata intent,
        bytes calldata action,
        bytes calldata signature,
        uint256 fee
    ) internal {
        if (block.timestamp > intent.deadline) revert IntentExpired();
        if (nonceUsed[intent.nonce]) revert NonceAlreadyUsed();
        if (intent.relayer != address(0) && msg.sender != intent.relayer) revert WrongRelayer();
        if (fee > intent.maxFee) revert FeeAboveCap();
        if (keccak256(action) != intent.actionHash) revert ActionHashMismatch();
        if (intent.recipient == address(0)) revert InvalidRecipient();
        if (intent.target == address(this) || intent.target == factory) {
            revert TargetNotAllowed(intent.target);
        }
        if (!SignatureChecker.isValidSignatureNow(owner, intentDigest(intent), signature)) {
            revert InvalidSignature();
        }
        if (intent.module.codehash != intent.moduleCodeHash) revert ModuleCodeMismatch();
        if (IActionModule(intent.module).protocol() != protocol) revert WrongProtocol();
        nonceUsed[intent.nonce] = true;
    }

    /// @dev Plan with the pinned module, enforce bounds, execute, then check
    /// that every approval was consumed exactly.
    function _runPlan(ExecutionIntent calldata intent, bytes calldata action) internal {
        IActionModule.Plan memory p =
            IActionModule(intent.module).plan(address(this), marketScope, intent.recipient, action);
        if (
            p.increasesRisk
                && policy.isPaused(policy.routeId(block.chainid, protocol, marketScope))
        ) revert RoutePaused();

        for (uint256 i = 0; i < p.calls.length; i++) {
            _checkCall(intent, p, p.calls[i]);
            _call(p.calls[i].target, 0, p.calls[i].data);
        }
        for (uint256 i = 0; i < p.approvalTokens.length; i++) {
            if (IERC20(p.approvalTokens[i]).allowance(address(this), intent.target) != 0) {
                revert ExcessAllowance(p.approvalTokens[i]);
            }
        }
    }

    function _checkCall(
        ExecutionIntent calldata intent,
        IActionModule.Plan memory p,
        IActionModule.PlannedCall memory c
    ) internal view {
        bytes4 selector = c.data.length >= 4 ? bytes4(c.data) : bytes4(0);
        if (c.target == intent.target) {
            if (!IActionModule(intent.module).isAllowedSelector(selector)) {
                revert SelectorNotAllowed(c.target, selector);
            }
            return;
        }
        if (!_contains(p.approvalTokens, c.target)) revert TargetNotAllowed(c.target);
        if (selector == IERC20.approve.selector) {
            (address spender,) = abi.decode(_args(c.data), (address, uint256));
            if (spender != intent.target) revert TargetNotAllowed(spender);
        } else if (selector == IERC20.transfer.selector) {
            (address to,) = abi.decode(_args(c.data), (address, uint256));
            if (to != intent.recipient) revert RecipientNotAllowed(to);
        } else {
            revert SelectorNotAllowed(c.target, selector);
        }
    }

    function _payFee(ExecutionIntent calldata intent, uint256 fee) internal {
        if (fee > 0) IERC20(intent.feeToken).safeTransfer(msg.sender, fee);
        emit IntentExecuted(intent.nonce, intent.module, msg.sender, fee);
    }

    // ─── helpers ─────────────────────────────────────────────────────────────

    /// @dev Plain CALL in the account's own context; reverts bubble up.
    function _call(address target, uint256 value, bytes memory data)
        internal
        returns (bytes memory result)
    {
        result = Address.functionCallWithValue(target, data, value);
        emit Executed(target, value, data.length >= 4 ? bytes4(data) : bytes4(0));
    }

    function _contains(address[] memory list, address item) private pure returns (bool) {
        for (uint256 i = 0; i < list.length; i++) {
            if (list[i] == item) return true;
        }
        return false;
    }

    /// @dev Calldata without the 4-byte selector.
    function _args(bytes memory data) private pure returns (bytes memory args) {
        if (data.length < 4) revert SelectorNotAllowed(address(0), bytes4(0));
        args = new bytes(data.length - 4);
        for (uint256 i = 0; i < args.length; i++) {
            args[i] = data[i + 4];
        }
    }
}
