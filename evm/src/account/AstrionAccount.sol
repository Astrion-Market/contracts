// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {Address} from "@openzeppelin/contracts/utils/Address.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {SignatureChecker} from "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";

import {IActionModule} from "../interfaces/IActionModule.sol";
import {IMessageTransmitterV2} from "../interfaces/IMessageTransmitterV2.sol";
import {CctpMessageV2} from "../libraries/CctpMessageV2.sol";
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
///
/// Bridge-funded execution (C10): CCTP mints USDC to this account with the
/// account as `destinationCaller`, so only the account can complete its own
/// mint. Each mint is recorded once as a receipt keyed by (sourceDomain,
/// nonce) with the measured received amount. A funded intent is bound to one
/// exact receipt; if the action fails or has expired, the mint still stands
/// and the USDC stays here, recoverable by the owner.
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

    /// @notice One recorded CCTP mint into this account.
    struct TransferReceipt {
        bool recorded;
        bool consumed; // bound to an executed intent
        uint32 sourceDomain;
        bytes32 nonce;
        uint256 burnedAmount; // message amount (6 decimals)
        uint256 feeExecuted;
        uint256 received; // measured balance delta
    }

    address public immutable owner;
    address public immutable factory;
    RoutePolicy public immutable policy;
    IMessageTransmitterV2 public immutable messageTransmitter;
    IERC20 public immutable usdc;
    uint32 public immutable localDomain;
    bytes32 public immutable protocol;
    bytes32 public immutable marketScope;
    uint32 public immutable version;

    /// @notice Consumed or revoked intent nonces.
    mapping(uint256 nonce => bool) public nonceUsed;

    /// @notice CCTP receipts by transferId = keccak256(abi.encode(sourceDomain, nonce)).
    mapping(bytes32 transferId => TransferReceipt) public receipts;

    event Executed(address indexed target, uint256 value, bytes4 selector);
    event IntentExecuted(uint256 indexed nonce, address indexed module, address relayer, uint256 fee);
    event NonceRevoked(uint256 indexed nonce);
    event TransferReceived(
        bytes32 indexed transferId,
        uint32 sourceDomain,
        bytes32 nonce,
        uint256 burnedAmount,
        uint256 feeExecuted,
        uint256 received
    );
    event FundedActionFailed(bytes32 indexed transferId, uint256 indexed nonce, bytes reason);

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
    error NotSelf();
    error BadTransferFields();
    error AlreadyReceived(bytes32 transferId);
    error MintFailed();
    error ReceiptMismatch(uint256 expected, uint256 received);
    error TransferMismatch(bytes32 expected, bytes32 actual);
    error UnknownTransfer();
    error TransferAlreadyConsumed();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor(
        address owner_,
        RoutePolicy policy_,
        IMessageTransmitterV2 messageTransmitter_,
        IERC20 usdc_,
        uint32 localDomain_,
        bytes32 protocol_,
        bytes32 marketScope_,
        uint32 version_
    ) EIP712("AstrionAccount", "1") {
        if (owner_ == address(0)) revert ZeroOwner();
        owner = owner_;
        factory = msg.sender;
        policy = policy_;
        messageTransmitter = messageTransmitter_;
        usdc = usdc_;
        localDomain = localDomain_;
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
        _consumeIntent(intent, action, signature, fee, msg.sender);
        _runPlan(intent, action);
        _payFee(intent, fee, msg.sender);
    }

    // ─── bridge-funded path (C10) ────────────────────────────────────────────

    /// @notice Complete a CCTP mint into this account and record its receipt.
    /// Permissionless: the USDC can only land here. Duplicate receipts revert.
    function receiveTransfer(bytes calldata message, bytes calldata attestation)
        external
        nonReentrant
        returns (bytes32 transferId)
    {
        return _receiveTransfer(message, attestation);
    }

    /// @notice Mint (if not yet recorded) and execute an intent bound to that
    /// exact transfer. If the action fails (revert, expiry, bad signature,
    /// pause) the mint still stands, the receipt stays unconsumed and the
    /// funds remain in this account. Returns whether the action executed.
    function executeFundedIntent(
        ExecutionIntent calldata intent,
        bytes calldata action,
        bytes calldata signature,
        uint256 fee,
        bytes calldata message,
        bytes calldata attestation
    ) external nonReentrant returns (bool executed) {
        if (intent.transferId == bytes32(0)) revert TransferBindingRequired();
        if (!receipts[intent.transferId].recorded) {
            bytes32 received = _receiveTransfer(message, attestation);
            if (received != intent.transferId) revert TransferMismatch(intent.transferId, received);
        }
        if (receipts[intent.transferId].consumed) revert TransferAlreadyConsumed();
        try this.runFundedIntent(intent, action, signature, fee, msg.sender) {
            executed = true;
        } catch (bytes memory reason) {
            emit FundedActionFailed(intent.transferId, intent.nonce, reason);
        }
    }

    /// @dev Self-call so a failing action rolls back only itself, never the
    /// mint. Runs inside executeFundedIntent's reentrancy lock.
    function runFundedIntent(
        ExecutionIntent calldata intent,
        bytes calldata action,
        bytes calldata signature,
        uint256 fee,
        address submitter
    ) external {
        if (msg.sender != address(this)) revert NotSelf();
        _consumeIntent(intent, action, signature, fee, submitter);
        receipts[intent.transferId].consumed = true;
        _runPlan(intent, action);
        _payFee(intent, fee, submitter);
    }

    function transferIdOf(uint32 sourceDomain, bytes32 nonce) public pure returns (bytes32) {
        return keccak256(abi.encode(sourceDomain, nonce));
    }

    function _receiveTransfer(bytes calldata message, bytes calldata attestation)
        internal
        returns (bytes32 transferId)
    {
        CctpMessageV2.Burn memory b = CctpMessageV2.decode(message);
        bytes32 self = CctpMessageV2.toBytes32(address(this));
        if (
            b.destinationDomain != localDomain || b.mintRecipient != self
                || b.destinationCaller != self
        ) revert BadTransferFields();

        transferId = transferIdOf(b.sourceDomain, b.nonce);
        if (receipts[transferId].recorded) revert AlreadyReceived(transferId);

        uint256 before = usdc.balanceOf(address(this));
        if (!messageTransmitter.receiveMessage(message, attestation)) revert MintFailed();
        uint256 received = usdc.balanceOf(address(this)) - before;
        uint256 expected = b.amount - b.feeExecuted;
        if (received != expected) revert ReceiptMismatch(expected, received);

        receipts[transferId] = TransferReceipt({
            recorded: true,
            consumed: false,
            sourceDomain: b.sourceDomain,
            nonce: b.nonce,
            burnedAmount: b.amount,
            feeExecuted: b.feeExecuted,
            received: received
        });
        emit TransferReceived(transferId, b.sourceDomain, b.nonce, b.amount, b.feeExecuted, received);
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
        uint256 fee,
        address submitter
    ) internal {
        if (block.timestamp > intent.deadline) revert IntentExpired();
        if (nonceUsed[intent.nonce]) revert NonceAlreadyUsed();
        if (intent.relayer != address(0) && submitter != intent.relayer) revert WrongRelayer();
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

    function _payFee(ExecutionIntent calldata intent, uint256 fee, address submitter) internal {
        if (fee > 0) IERC20(intent.feeToken).safeTransfer(submitter, fee);
        emit IntentExecuted(intent.nonce, intent.module, submitter, fee);
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
