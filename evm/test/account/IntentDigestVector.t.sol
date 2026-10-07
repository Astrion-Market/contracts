// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";

/// @notice The SDK (viem) and Solidity agree on the EIP-712 intent digest.
contract IntentDigestVectorTest is Test {
    bytes32 constant DOMAIN_TYPEHASH =
        keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");

    string json;

    function setUp() public {
        json = vm.readFile(string.concat(vm.projectRoot(), "/../spec/fixtures/evm/intent-digest.json"));
    }

    function _intent() internal view returns (AstrionAccount.ExecutionIntent memory i) {
        i.module = vm.parseJsonAddress(json, ".intent.module");
        i.moduleCodeHash = vm.parseJsonBytes32(json, ".intent.moduleCodeHash");
        i.target = vm.parseJsonAddress(json, ".intent.target");
        i.actionHash = vm.parseJsonBytes32(json, ".intent.actionHash");
        i.recipient = vm.parseJsonAddress(json, ".intent.recipient");
        i.relayer = vm.parseJsonAddress(json, ".intent.relayer");
        i.feeToken = vm.parseJsonAddress(json, ".intent.feeToken");
        i.maxFee = vm.parseJsonUint(json, ".intent.maxFee");
        i.nonce = vm.parseJsonUint(json, ".intent.nonce");
        i.deadline = vm.parseJsonUint(json, ".intent.deadline");
        i.transferId = vm.parseJsonBytes32(json, ".intent.transferId");
    }

    function _digest(uint256 chainId, address account, AstrionAccount.ExecutionIntent memory i)
        internal
        pure
        returns (bytes32)
    {
        bytes32 domain = keccak256(
            abi.encode(DOMAIN_TYPEHASH, keccak256("AstrionAccount"), keccak256("1"), chainId, account)
        );
        bytes32 typehash = keccak256(
            "ExecutionIntent(address module,bytes32 moduleCodeHash,address target,bytes32 actionHash,"
            "address recipient,address relayer,address feeToken,uint256 maxFee,uint256 nonce,"
            "uint256 deadline,bytes32 transferId)"
        );
        return keccak256(abi.encodePacked("\x19\x01", domain, keccak256(abi.encode(typehash, i))));
    }

    function test_sdkDigestMatchesSolidity() public view {
        bytes32 expected = vm.parseJsonBytes32(json, ".digest");
        uint256 chainId = vm.parseJsonUint(json, ".chainId");
        address account = vm.parseJsonAddress(json, ".account");
        assertEq(_digest(chainId, account, _intent()), expected);
    }

    function test_accountDigestMatchesTheFormula() public {
        AstrionAccountFactory factory = new AstrionAccountFactory(
            new RoutePolicy(address(this)), IMessageTransmitterV2(address(0)), IERC20(address(0)), 6
        );
        AstrionAccount account = AstrionAccount(
            payable(factory.createAccount(makeAddr("owner"), ProtocolIds.AAVE_V3, bytes32(0), 1))
        );
        AstrionAccount.ExecutionIntent memory i = _intent();
        assertEq(account.intentDigest(i), _digest(block.chainid, address(account), i));
        assertEq(account.EXECUTION_INTENT_TYPEHASH(), keccak256(
            "ExecutionIntent(address module,bytes32 moduleCodeHash,address target,bytes32 actionHash,"
            "address recipient,address relayer,address feeToken,uint256 maxFee,uint256 nonce,"
            "uint256 deadline,bytes32 transferId)"
        ));
    }
}
