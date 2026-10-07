// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

/// @notice Reads CCTP V2 MessageV2 + BurnMessageV2 fields from calldata.
/// Offsets match libs/crosschain-types/src/cctp.rs and spec/fixtures/cctp.
///
/// MessageV2 header (148 bytes): version u32 | sourceDomain u32 |
/// destinationDomain u32 | nonce b32 | sender b32 | recipient b32 |
/// destinationCaller b32 | minFinalityThreshold u32 | finalityThresholdExecuted u32
/// BurnMessageV2 body: version u32 | burnToken b32 | mintRecipient b32 |
/// amount u256 | messageSender b32 | maxFee u256 | feeExecuted u256 |
/// expirationBlock u256 | hookData
library CctpMessageV2 {
    uint256 internal constant HEADER_LEN = 148;
    uint256 internal constant BURN_BODY_LEN = 228;
    uint32 internal constant MESSAGE_VERSION = 1;
    uint32 internal constant BURN_MESSAGE_VERSION = 1;

    struct Burn {
        uint32 sourceDomain;
        uint32 destinationDomain;
        bytes32 nonce;
        bytes32 destinationCaller;
        bytes32 mintRecipient;
        uint256 amount;
        uint256 feeExecuted;
    }

    error MalformedMessage();

    function decode(bytes calldata m) internal pure returns (Burn memory b) {
        if (m.length < HEADER_LEN + BURN_BODY_LEN) revert MalformedMessage();
        if (
            uint32(bytes4(m[0:4])) != MESSAGE_VERSION
                || uint32(bytes4(m[148:152])) != BURN_MESSAGE_VERSION
        ) revert MalformedMessage();
        b.sourceDomain = uint32(bytes4(m[4:8]));
        b.destinationDomain = uint32(bytes4(m[8:12]));
        b.nonce = bytes32(m[12:44]);
        b.destinationCaller = bytes32(m[108:140]);
        b.mintRecipient = bytes32(m[184:216]);
        b.amount = uint256(bytes32(m[216:248]));
        b.feeExecuted = uint256(bytes32(m[312:344]));
        if (b.feeExecuted > b.amount) revert MalformedMessage();
    }

    function toBytes32(address a) internal pure returns (bytes32) {
        return bytes32(uint256(uint160(a)));
    }
}
