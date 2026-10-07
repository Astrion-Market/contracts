// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {AstrionAccount} from "../account/AstrionAccount.sol";
import {IActionModule} from "../interfaces/IActionModule.sol";

/// @notice Converts a module plan into calls the owner can run directly with
/// `AstrionAccount.executeBatch`: the emergency path that needs no relayer,
/// no signature and ignores application pauses.
library PlanCalls {
    function toCalls(IActionModule.Plan memory p)
        internal
        pure
        returns (AstrionAccount.Call[] memory calls)
    {
        calls = new AstrionAccount.Call[](p.calls.length);
        for (uint256 i = 0; i < p.calls.length; i++) {
            calls[i] = AstrionAccount.Call({target: p.calls[i].target, value: 0, data: p.calls[i].data});
        }
    }

    function planDirect(
        IActionModule module,
        AstrionAccount account,
        address recipient,
        bytes memory action
    ) internal view returns (AstrionAccount.Call[] memory) {
        return toCalls(module.plan(address(account), account.marketScope(), recipient, action));
    }
}
