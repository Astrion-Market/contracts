// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {AstrionAccount} from "../../src/account/AstrionAccount.sol";

/// @notice A "pool" whose supply() re-enters the calling account.
contract ReentrantPool {
    function supply(address, uint256, address, uint16) external {
        AstrionAccount.ExecutionIntent memory empty;
        AstrionAccount(payable(msg.sender)).executeIntent(empty, "", "", 0);
    }
}
