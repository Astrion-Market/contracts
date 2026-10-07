// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

/// @notice Canonical protocol identifiers used in account scopes. They match
/// the `protocol` enum in spec/schemas/common.schema.json.
library ProtocolIds {
    bytes32 internal constant AAVE_V3 = keccak256("aave-v3");
    bytes32 internal constant MORPHO_BLUE = keccak256("morpho-blue");
    bytes32 internal constant COMPOUND_V3 = keccak256("compound-v3");

    function isSupported(bytes32 protocol) internal pure returns (bool) {
        return protocol == AAVE_V3 || protocol == MORPHO_BLUE || protocol == COMPOUND_V3;
    }
}
