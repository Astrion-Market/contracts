// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Script, console2} from "forge-std/Script.sol";
import {AstrionAccount} from "../src/account/AstrionAccount.sol";
import {IActionModule} from "../src/interfaces/IActionModule.sol";
import {PlanCalls} from "../src/libraries/PlanCalls.sol";

/// @notice Owner-direct execution of any module action, with no relayer.
///
///   OWNER_KEY=0x.. ACCOUNT=0x.. MODULE=0x.. ACTION=0x<abi-encoded action> [RECIPIENT=0x..] \
///   forge script script/AccountOps.s.sol --rpc-url $RPC --broadcast
///
/// Action encodings: docs/ops/DIRECT_OWNER_ACTIONS.md.
contract AccountOps is Script {
    function run() external {
        AstrionAccount account = AstrionAccount(payable(vm.envAddress("ACCOUNT")));
        IActionModule module = IActionModule(vm.envAddress("MODULE"));
        bytes memory action = vm.envBytes("ACTION");
        address recipient = vm.envOr("RECIPIENT", account.owner());

        require(module.protocol() == account.protocol(), "module protocol != account protocol");
        AstrionAccount.Call[] memory calls =
            PlanCalls.planDirect(module, account, recipient, action);
        for (uint256 i = 0; i < calls.length; i++) {
            console2.log("call", i, calls[i].target);
        }

        uint256 ownerKey = vm.envUint("OWNER_KEY");
        require(vm.addr(ownerKey) == account.owner(), "OWNER_KEY is not the account owner");
        vm.startBroadcast(ownerKey);
        account.executeBatch(calls);
        vm.stopBroadcast();
    }
}
