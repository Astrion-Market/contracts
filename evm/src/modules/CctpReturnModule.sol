// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IActionModule} from "../interfaces/IActionModule.sol";
import {ITokenMessengerV2} from "../interfaces/ITokenMessengerV2.sol";
import {StellarStrkey} from "../libraries/StellarStrkey.sol";

/// @title CctpReturnModule
/// @notice Plans an owner-authorized CCTP V2 burn of the account's USDC to a
/// Stellar recipient through Circle's CctpForwarder: the forwarder is both
/// mintRecipient and destinationCaller, the final strkey goes in hook data.
/// The destination is validated before any burn. One deployment per
/// protocol id so it can serve accounts of that scope.
contract CctpReturnModule is IActionModule {
    uint32 public constant STELLAR_DOMAIN = 27;
    uint32 public constant FINALITY_FAST = 1000;
    uint32 public constant FINALITY_STANDARD = 2000;
    uint256 internal constant MAX_STELLAR_AMOUNT = uint256(uint64(type(int64).max));

    bytes32 public immutable protocolId;
    ITokenMessengerV2 public immutable tokenMessenger;
    address public immutable usdc;
    bytes32 public immutable forwarder;

    error ZeroAmount();
    error FeeExceedsAmount();
    error AmountTooLarge();
    error UnsupportedFinality(uint32 threshold);
    error ZeroForwarder();

    constructor(
        bytes32 protocolId_,
        ITokenMessengerV2 tokenMessenger_,
        address usdc_,
        bytes32 forwarder_
    ) {
        if (forwarder_ == bytes32(0)) revert ZeroForwarder();
        protocolId = protocolId_;
        tokenMessenger = tokenMessenger_;
        usdc = usdc_;
        forwarder = forwarder_;
    }

    function protocol() external view returns (bytes32) {
        return protocolId;
    }

    function isAllowedSelector(bytes4 s) external pure returns (bool) {
        return s == ITokenMessengerV2.depositForBurnWithHook.selector;
    }

    function encodeAction(
        uint256 amount,
        uint256 maxFee,
        uint32 minFinality,
        string calldata recipient
    ) external pure returns (bytes memory) {
        return abi.encode(amount, maxFee, minFinality, recipient);
    }

    function hookData(string memory recipient) public pure returns (bytes memory) {
        StellarStrkey.validate(recipient);
        bytes memory r = bytes(recipient);
        return abi.encodePacked(bytes24(0), uint32(0), uint32(r.length), r);
    }

    function plan(address, bytes32, address, bytes calldata action)
        external
        view
        returns (Plan memory p)
    {
        (uint256 amount, uint256 maxFee, uint32 minFinality, string memory recipient) =
            abi.decode(action, (uint256, uint256, uint32, string));
        if (amount == 0) revert ZeroAmount();
        if (maxFee >= amount) revert FeeExceedsAmount();
        if ((amount - maxFee) * 10 > MAX_STELLAR_AMOUNT) revert AmountTooLarge();
        if (minFinality != FINALITY_FAST && minFinality != FINALITY_STANDARD) {
            revert UnsupportedFinality(minFinality);
        }
        bytes memory hook = hookData(recipient);

        p.calls = new PlannedCall[](2);
        p.calls[0] =
            PlannedCall(usdc, abi.encodeCall(IERC20.approve, (address(tokenMessenger), amount)));
        p.calls[1] = PlannedCall(
            address(tokenMessenger),
            abi.encodeCall(
                ITokenMessengerV2.depositForBurnWithHook,
                (amount, STELLAR_DOMAIN, forwarder, usdc, forwarder, maxFee, minFinality, hook)
            )
        );
        p.approvalTokens = new address[](1);
        p.approvalTokens[0] = usdc;
    }
}
