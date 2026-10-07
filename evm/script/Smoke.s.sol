// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Script, console2} from "forge-std/Script.sol";
import {AstrionAccount} from "../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../src/account/AstrionAccountFactory.sol";
import {CctpReturnModule} from "../src/modules/CctpReturnModule.sol";
import {ProtocolIds} from "../src/libraries/ProtocolIds.sol";

/// @notice Read-only smoke check of a deployment (no broadcast):
///   NETWORK=base-sepolia forge script script/Smoke.s.sol --rpc-url $BASE_SEPOLIA_RPC_URL
contract Smoke is Script {
    function run() external {
        string memory net = vm.envString("NETWORK");
        string memory d = vm.readFile(
            string.concat(vm.projectRoot(), "/../deployments/crosschain/deployed/", net, ".json")
        );
        require(vm.parseJsonUint(d, ".chainId") == block.chainid, "wrong chain");
        AstrionAccountFactory factory =
            AstrionAccountFactory(vm.parseJsonAddress(d, ".accountFactory"));
        require(address(factory).code.length > 0, "factory missing");
        require(
            address(factory.policy()) == vm.parseJsonAddress(d, ".routePolicy"), "policy mismatch"
        );

        address probeOwner = address(0xA11CE);
        address predicted = factory.predictAccount(probeOwner, ProtocolIds.AAVE_V3, bytes32(0), 1);
        address created = factory.createAccount(probeOwner, ProtocolIds.AAVE_V3, bytes32(0), 1);
        require(created == predicted, "prediction mismatch");
        require(AstrionAccount(payable(created)).owner() == probeOwner, "owner mismatch");

        CctpReturnModule ret = CctpReturnModule(vm.parseJsonAddress(d, ".returnModuleAaveV3"));
        require(ret.protocol() == ProtocolIds.AAVE_V3, "return module protocol");
        require(ret.STELLAR_DOMAIN() == 27, "stellar domain");
        require(ret.usdc() == address(factory.usdc()), "usdc mismatch");
        console2.log("smoke ok on", net);
    }
}
