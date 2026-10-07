// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {ForkTest} from "./ForkTest.sol";

/// @notice Proves the fork plumbing: correct chain, pinned block. Reports
/// SKIPPED without an RPC URL.
contract BaseForkSmokeTest is ForkTest {
    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
    }

    function test_forkIsPinned() public view {
        assertTrue(forked);
        assertEq(block.number, vm.envOr("FORK_BLOCK_BASE", BASE_FORK_BLOCK));
    }
}

contract EthereumForkSmokeTest is ForkTest {
    function setUp() public {
        _forkOrSkip(ETHEREUM_CHAIN_ID);
    }

    function test_forkIsPinned() public view {
        assertTrue(forked);
        assertEq(block.number, vm.envOr("FORK_BLOCK_ETHEREUM", ETHEREUM_FORK_BLOCK));
    }
}
