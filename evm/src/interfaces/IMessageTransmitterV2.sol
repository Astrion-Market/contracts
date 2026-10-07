// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

/// @notice Subset of Circle CCTP V2 MessageTransmitterV2 used by accounts.
interface IMessageTransmitterV2 {
    function receiveMessage(bytes calldata message, bytes calldata attestation)
        external
        returns (bool success);

    function usedNonces(bytes32 nonce) external view returns (uint256);
}
