// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test, console2} from "forge-std/Test.sol";

/// @notice Base for fork suites. Pins a block per chain so results are
/// reproducible, and SKIPS (never passes) when no RPC URL is configured.
///
/// Override a pin locally with FORK_BLOCK_BASE / FORK_BLOCK_ETHEREUM; CI uses
/// the constants below. Update pins deliberately, in their own commit.
abstract contract ForkTest is Test {
    uint256 internal constant BASE_CHAIN_ID = 8453;
    uint256 internal constant ETHEREUM_CHAIN_ID = 1;

    uint256 internal constant BASE_FORK_BLOCK = 30_000_000;
    uint256 internal constant ETHEREUM_FORK_BLOCK = 22_500_000;

    /// @dev True when the fork was created; false when the suite was skipped.
    bool internal forked;

    /// @notice Select a fork of `chainId` at its pinned block, or skip the
    /// whole suite when the RPC env var is missing.
    function _forkOrSkip(uint256 chainId) internal {
        (string memory envName, uint256 pinned, string memory blockEnv) = _forkParams(chainId);
        string memory url = vm.envOr(envName, string(""));
        if (bytes(url).length == 0) {
            console2.log("SKIPPED: fork suite needs", envName);
            vm.skip(true);
            return;
        }
        uint256 forkBlock = vm.envOr(blockEnv, pinned);
        vm.createSelectFork(url, forkBlock);
        assertEq(block.chainid, chainId, "fork chain id");
        forked = true;
    }

    function _forkParams(uint256 chainId)
        private
        pure
        returns (string memory envName, uint256 pinned, string memory blockEnv)
    {
        if (chainId == BASE_CHAIN_ID) return ("BASE_RPC_URL", BASE_FORK_BLOCK, "FORK_BLOCK_BASE");
        if (chainId == ETHEREUM_CHAIN_ID) {
            return ("ETH_RPC_URL", ETHEREUM_FORK_BLOCK, "FORK_BLOCK_ETHEREUM");
        }
        revert("ForkTest: unsupported chain");
    }
}
