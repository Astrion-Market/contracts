// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";
import {CctpMessageV2} from "../../src/libraries/CctpMessageV2.sol";
import {MockERC20} from "./MockERC20.sol";

/// @notice CCTP V2 transmitter stand-in: checks a fake attestation, enforces
/// destinationCaller and nonce reuse like the real contract, then mints
/// `amount - feeExecuted` to mintRecipient.
contract MockMessageTransmitterV2 is IMessageTransmitterV2 {
    bytes32 public constant VALID_ATTESTATION = keccak256("valid-attestation");

    MockERC20 public immutable usdc;
    mapping(bytes32 => uint256) public usedNonces;
    /// Test knob: mint this much less than the message says.
    uint256 public shortfall;

    error BadAttestation();
    error WrongCaller();
    error NonceUsed();

    constructor(MockERC20 usdc_) {
        usdc = usdc_;
    }

    function setShortfall(uint256 s) external {
        shortfall = s;
    }

    function receiveMessage(bytes calldata message, bytes calldata attestation)
        external
        returns (bool)
    {
        if (keccak256(attestation) != VALID_ATTESTATION) revert BadAttestation();
        CctpMessageV2.Burn memory b = CctpMessageV2.decode(message);
        if (b.destinationCaller != bytes32(0) && b.destinationCaller != bytes32(uint256(uint160(msg.sender)))) {
            revert WrongCaller();
        }
        if (usedNonces[b.nonce] != 0) revert NonceUsed();
        usedNonces[b.nonce] = 1;
        usdc.mint(address(uint160(uint256(b.mintRecipient))), b.amount - b.feeExecuted - shortfall);
        return true;
    }
}
