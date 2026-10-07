// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

library CctpMessages {
    function burn(
        uint32 sourceDomain,
        uint32 destinationDomain,
        bytes32 nonce,
        address recipientAccount,
        uint256 amount,
        uint256 feeExecuted
    ) internal pure returns (bytes memory) {
        bytes32 self = bytes32(uint256(uint160(recipientAccount)));
        bytes memory header = abi.encodePacked(
            uint32(1), sourceDomain, destinationDomain, nonce, bytes32(0), bytes32(0), self, uint32(1000), uint32(2000)
        );
        bytes memory body = abi.encodePacked(
            uint32(1), bytes32(0), self, amount, bytes32(0), feeExecuted, feeExecuted, uint256(0)
        );
        return bytes.concat(header, body);
    }
}
