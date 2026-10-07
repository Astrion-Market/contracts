// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Script, console2} from "forge-std/Script.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AstrionAccount} from "../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../src/account/RoutePolicy.sol";
import {IMessageTransmitterV2} from "../src/interfaces/IMessageTransmitterV2.sol";
import {ITokenMessengerV2} from "../src/interfaces/ITokenMessengerV2.sol";
import {IAavePoolAddressesProvider} from "../src/interfaces/aave/IAaveV3.sol";
import {IComet} from "../src/interfaces/compound/IComet.sol";
import {IMorphoBlue} from "../src/interfaces/morpho/IMorphoBlue.sol";
import {AaveV3Lens} from "../src/lens/AaveV3Lens.sol";
import {CompoundV3Lens} from "../src/lens/CompoundV3Lens.sol";
import {MorphoBlueLens} from "../src/lens/MorphoBlueLens.sol";
import {AaveV3Module} from "../src/modules/AaveV3Module.sol";
import {CctpReturnModule} from "../src/modules/CctpReturnModule.sol";
import {CompoundV3Module} from "../src/modules/CompoundV3Module.sol";
import {ProtocolIds} from "../src/libraries/ProtocolIds.sol";

/// @notice Deploys the execution layer for one network from
/// deployments/crosschain/<ASTRION_ENV>.json. Mainnet chains require an
/// explicit release approval pinned to this commit.
///
///   ASTRION_ENV=testnet NETWORK=base-sepolia ROUTE_GUARDIAN=0x.. \
///   STELLAR_FORWARDER_BYTES32=0x.. GIT_COMMIT=$(git rev-parse HEAD) \
///   forge script script/Deploy.s.sol --rpc-url $BASE_SEPOLIA_RPC_URL --broadcast --private-key $DEPLOYER_KEY
contract Deploy is Script {
    string internal manifest;
    string internal net;

    function run() external {
        string memory envName = vm.envString("ASTRION_ENV");
        net = vm.envString("NETWORK");
        manifest = vm.readFile(
            string.concat(vm.projectRoot(), "/../deployments/crosschain/", envName, ".json")
        );
        require(
            keccak256(bytes(vm.parseJsonString(manifest, ".environment"))) == keccak256(bytes(envName)),
            "manifest environment mismatch"
        );
        require(block.chainid == _netUint(".chainId"), "RPC chain id does not match manifest");
        _requireReleaseApproval(envName);

        address usdc = _netAddress(".usdc.address");
        bytes32 forwarder = vm.envBytes32("STELLAR_FORWARDER_BYTES32");
        require(forwarder != bytes32(0), "forwarder required");

        vm.startBroadcast();
        RoutePolicy policy = new RoutePolicy(vm.envAddress("ROUTE_GUARDIAN"));
        AstrionAccountFactory factory = new AstrionAccountFactory(
            policy,
            IMessageTransmitterV2(_netAddress(".cctp.messageTransmitterV2")),
            IERC20(usdc),
            uint32(_netUint(".cctpDomain"))
        );
        ITokenMessengerV2 messenger = ITokenMessengerV2(_netAddress(".cctp.tokenMessengerV2"));
        address returnAave = address(new CctpReturnModule(ProtocolIds.AAVE_V3, messenger, usdc, forwarder));
        address returnMorpho =
            address(new CctpReturnModule(ProtocolIds.MORPHO_BLUE, messenger, usdc, forwarder));
        address returnCompound =
            address(new CctpReturnModule(ProtocolIds.COMPOUND_V3, messenger, usdc, forwarder));

        (address aaveModule, address aaveLens) = _deployAave(usdc);
        (address compoundModule, address compoundLens) = _deployCompound();
        address morphoLens = _deployMorphoLens();
        vm.stopBroadcast();

        string memory o = "deployment";
        vm.serializeString(o, "environment", envName);
        vm.serializeString(o, "network", net);
        vm.serializeUint(o, "chainId", block.chainid);
        vm.serializeUint(o, "deployedAtBlock", block.number);
        vm.serializeString(o, "gitCommit", vm.envOr("GIT_COMMIT", string("unknown")));
        vm.serializeAddress(o, "routePolicy", address(policy));
        vm.serializeAddress(o, "accountFactory", address(factory));
        vm.serializeBytes32(o, "accountCreationCodeHash", keccak256(type(AstrionAccount).creationCode));
        vm.serializeAddress(o, "returnModuleAaveV3", returnAave);
        vm.serializeAddress(o, "returnModuleMorphoBlue", returnMorpho);
        vm.serializeAddress(o, "returnModuleCompoundV3", returnCompound);
        vm.serializeAddress(o, "aaveV3Module", aaveModule);
        vm.serializeAddress(o, "aaveV3Lens", aaveLens);
        vm.serializeAddress(o, "compoundV3Module", compoundModule);
        vm.serializeAddress(o, "compoundV3Lens", compoundLens);
        string memory out = vm.serializeAddress(o, "morphoBlueLens", morphoLens);
        string memory path =
            string.concat(vm.projectRoot(), "/../deployments/crosschain/deployed/", net, ".json");
        vm.writeJson(out, path);
        console2.log("wrote", path);
    }

    function _deployAave(address usdc) internal returns (address module, address lens) {
        (bool found, string memory r) = _route(string.concat("aave-v3:", net));
        if (!found || !vm.keyExistsJson(manifest, string.concat(r, ".targets.poolAddressesProvider"))) {
            return (address(0), address(0));
        }
        IAavePoolAddressesProvider provider = IAavePoolAddressesProvider(
            vm.parseJsonAddress(manifest, string.concat(r, ".targets.poolAddressesProvider"))
        );
        module = address(new AaveV3Module(provider, usdc, vm.envOr("AAVE_COLLATERAL", address(0))));
        lens = address(new AaveV3Lens(provider));
    }

    function _deployCompound() internal returns (address module, address lens) {
        (bool found, string memory r) = _route(string.concat("compound-v3:", net));
        if (!found || !vm.keyExistsJson(manifest, string.concat(r, ".targets.comet"))) {
            return (address(0), address(0));
        }
        IComet comet = IComet(vm.parseJsonAddress(manifest, string.concat(r, ".targets.comet")));
        module = address(new CompoundV3Module(comet, vm.envOr("COMPOUND_COLLATERAL", address(0))));
        lens = address(new CompoundV3Lens());
    }

    function _deployMorphoLens() internal returns (address) {
        (bool found, string memory r) = _route(string.concat("morpho-blue:", net));
        if (!found || !vm.keyExistsJson(manifest, string.concat(r, ".targets.morpho"))) return address(0);
        return address(
            new MorphoBlueLens(IMorphoBlue(vm.parseJsonAddress(manifest, string.concat(r, ".targets.morpho"))))
        );
    }

    function _route(string memory id) internal view returns (bool, string memory) {
        for (uint256 i = 0; i < 6; i++) {
            string memory key = string.concat(".routes[", vm.toString(i), "]");
            if (keccak256(bytes(vm.parseJsonString(manifest, string.concat(key, ".id")))) == keccak256(bytes(id))) {
                return (true, key);
            }
        }
        return (false, "");
    }

    function _requireReleaseApproval(string memory envName) internal view {
        if (keccak256(bytes(envName)) != keccak256("mainnet")) return;
        string memory path =
            string.concat(vm.projectRoot(), "/../deployments/crosschain/release-approval.json");
        require(vm.exists(path), "MainnetReleaseNotApproved: no release-approval.json");
        string memory approval = vm.readFile(path);
        require(
            keccak256(bytes(vm.parseJsonString(approval, ".commit")))
                == keccak256(bytes(vm.envString("GIT_COMMIT"))),
            "MainnetReleaseNotApproved: commit mismatch"
        );
        require(
            bytes(vm.parseJsonString(approval, ".securityReview")).length > 0,
            "MainnetReleaseNotApproved: no security review"
        );
        uint256[] memory chains = vm.parseJsonUintArray(approval, ".chainIds");
        bool ok;
        for (uint256 i = 0; i < chains.length; i++) {
            if (chains[i] == block.chainid) ok = true;
        }
        require(ok, "MainnetReleaseNotApproved: chain not approved");
    }

    function _netAddress(string memory field) internal view returns (address) {
        return vm.parseJsonAddress(manifest, string.concat('.networks["', net, '"]', field));
    }

    function _netUint(string memory field) internal view returns (uint256) {
        return vm.parseJsonUint(manifest, string.concat('.networks["', net, '"]', field));
    }
}
