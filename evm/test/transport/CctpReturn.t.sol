// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";
import {CctpReturnModule} from "../../src/modules/CctpReturnModule.sol";
import {StellarStrkey} from "../../src/libraries/StellarStrkey.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockERC20} from "../mocks/MockERC20.sol";
import {MockLendingPool} from "../mocks/MockLendingPool.sol";
import {MockPoolModule} from "../mocks/MockPoolModule.sol";
import {MockTokenMessengerV2} from "../mocks/MockTokenMessengerV2.sol";
import {StrkeyHarness} from "../mocks/StrkeyHarness.sol";

contract CctpReturnTest is Test {
    string vectors;
    StrkeyHarness strkeys;
    MockERC20 usdc;
    MockTokenMessengerV2 messenger;
    CctpReturnModule module;
    AstrionAccountFactory factory;
    AstrionAccount account;
    bytes32 forwarder;

    uint256 ownerPk = 0xA11CE;
    address owner;
    uint256 nonce = 1;
    string constant G = "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI";

    function setUp() public {
        vectors = vm.readFile(string.concat(vm.projectRoot(), "/../spec/fixtures/cctp/vectors.json"));
        strkeys = new StrkeyHarness();
        owner = vm.addr(ownerPk);
        usdc = new MockERC20("USD Coin", "USDC", 6);
        messenger = new MockTokenMessengerV2();
        forwarder = bytes32(vm.parseJsonBytes(vectors, ".strkeys[1].hex"));
        module = new CctpReturnModule(ProtocolIds.AAVE_V3, messenger, address(usdc), forwarder);
        factory = new AstrionAccountFactory(
            new RoutePolicy(address(this)), IMessageTransmitterV2(address(0)), IERC20(address(usdc)), 6
        );
        account = AstrionAccount(payable(factory.createAccount(owner, ProtocolIds.AAVE_V3, bytes32(0), 1)));
        usdc.mint(address(account), 1_000e6);
    }

    function _intent(address mod, address target, bytes memory action)
        internal
        returns (AstrionAccount.ExecutionIntent memory i)
    {
        i = AstrionAccount.ExecutionIntent({
            module: mod,
            moduleCodeHash: mod.codehash,
            target: target,
            actionHash: keccak256(action),
            recipient: owner,
            relayer: address(0),
            feeToken: address(usdc),
            maxFee: 0,
            nonce: nonce++,
            deadline: block.timestamp + 1 hours,
            transferId: bytes32(0)
        });
    }

    function _run(address mod, address target, bytes memory action) internal {
        AstrionAccount.ExecutionIntent memory i = _intent(mod, target, action);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, account.intentDigest(i));
        account.executeIntent(i, action, abi.encodePacked(r, s, v), 0);
    }

    function _return(uint256 amount, uint256 maxFee, uint32 finality, string memory recipient)
        internal
        pure
        returns (bytes memory)
    {
        return abi.encode(amount, maxFee, finality, recipient);
    }

    function test_strkeyVectorsAgreeWithRustAndTs() public view {
        for (uint256 i = 0; i < 6; i++) {
            string memory s = vm.parseJsonString(vectors, string.concat(".strkeys[", vm.toString(i), "].strkey"));
            string memory kind = vm.parseJsonString(vectors, string.concat(".strkeys[", vm.toString(i), "].kind"));
            StellarStrkey.Kind expected = keccak256(bytes(kind)) == keccak256("account")
                ? StellarStrkey.Kind.Account
                : keccak256(bytes(kind)) == keccak256("contract")
                    ? StellarStrkey.Kind.Contract
                    : StellarStrkey.Kind.Muxed;
            assertEq(uint8(strkeys.validate(s)), uint8(expected), s);
        }
    }

    function test_invalidStrkeyVectorsRevert() public {
        for (uint256 i = 0; i < 6; i++) {
            string memory base = string.concat(".invalidStrkeys[", vm.toString(i), "]");
            string memory s = vm.parseJsonString(vectors, string.concat(base, ".strkey"));
            string memory err = vm.parseJsonString(vectors, string.concat(base, ".error"));
            bytes4 selector = keccak256(bytes(err)) == keccak256("UnsupportedStrkeyKind")
                ? StellarStrkey.UnsupportedStrkeyKind.selector
                : StellarStrkey.InvalidStrkey.selector;
            vm.expectRevert(selector);
            strkeys.validate(s);
        }
    }

    function test_hookDataMatchesVectors() public view {
        for (uint256 i = 0; i < 3; i++) {
            string memory base = string.concat(".hooks.valid[", vm.toString(i), "]");
            string memory recipient = vm.parseJsonString(vectors, string.concat(base, ".recipient"));
            assertEq(module.hookData(recipient), vm.parseJsonBytes(vectors, string.concat(base, ".hex")));
        }
    }

    function test_returnBurnUsesForwarderForBothFields() public {
        _run(address(module), address(messenger), _return(250e6, 1e6, 2000, G));
        MockTokenMessengerV2.Burn memory b = messenger.lastBurn();
        assertEq(b.sender, address(account));
        assertEq(b.amount, 250e6);
        assertEq(b.destinationDomain, 27);
        assertEq(b.mintRecipient, forwarder);
        assertEq(b.destinationCaller, forwarder);
        assertEq(b.burnToken, address(usdc));
        assertEq(b.maxFee, 1e6);
        assertEq(b.minFinalityThreshold, 2000);
        assertEq(b.hookData, module.hookData(G));
        assertEq(usdc.balanceOf(address(account)), 750e6);
        assertEq(usdc.allowance(address(account), address(messenger)), 0);
    }

    function test_malformedDestinationFailsBeforeBurn() public {
        string memory bad = vm.parseJsonString(vectors, ".invalidStrkeys[0].strkey");
        bytes memory action = _return(100e6, 0, 2000, bad);
        AstrionAccount.ExecutionIntent memory i = _intent(address(module), address(messenger), action);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, account.intentDigest(i));
        vm.expectRevert(StellarStrkey.InvalidStrkey.selector);
        account.executeIntent(i, action, abi.encodePacked(r, s, v), 0);
        assertEq(messenger.burnCount(), 0);
        assertEq(usdc.balanceOf(address(account)), 1_000e6);
    }

    function test_feeFinalityAndAmountBounds() public {
        vm.expectRevert(CctpReturnModule.FeeExceedsAmount.selector);
        module.plan(address(account), bytes32(0), owner, _return(100, 100, 2000, G));
        vm.expectRevert(abi.encodeWithSelector(CctpReturnModule.UnsupportedFinality.selector, 1500));
        module.plan(address(account), bytes32(0), owner, _return(100, 0, 1500, G));
        vm.expectRevert(CctpReturnModule.ZeroAmount.selector);
        module.plan(address(account), bytes32(0), owner, _return(0, 0, 2000, G));
        vm.expectRevert(CctpReturnModule.AmountTooLarge.selector);
        module.plan(address(account), bytes32(0), owner, _return(type(uint64).max, 0, 2000, G));
    }

    function test_relayerRetryCannotRedirect() public {
        bytes memory action = _return(100e6, 0, 2000, G);
        AstrionAccount.ExecutionIntent memory i = _intent(address(module), address(messenger), action);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, account.intentDigest(i));
        string memory other = vm.parseJsonString(vectors, ".strkeys[5].strkey");
        vm.expectRevert(AstrionAccount.ActionHashMismatch.selector);
        account.executeIntent(i, _return(100e6, 0, 2000, other), abi.encodePacked(r, s, v), 0);
    }

    function test_borrowDebtStaysVisibleWhileProceedsTravel() public {
        MockLendingPool pool = new MockLendingPool();
        usdc.mint(address(pool), 1_000_000e6);
        MockPoolModule lending = new MockPoolModule(address(pool), address(usdc));
        _run(address(lending), address(pool), abi.encode(uint8(1), uint256(400e6)));
        _run(address(module), address(messenger), _return(400e6, 0, 2000, G));
        assertEq(pool.debt(address(usdc), address(account)), 400e6);
        assertEq(messenger.lastBurn().amount, 400e6);
    }

    function test_returnModuleMustMatchAccountProtocol() public {
        CctpReturnModule morphoReturn =
            new CctpReturnModule(ProtocolIds.MORPHO_BLUE, messenger, address(usdc), forwarder);
        bytes memory action = _return(1e6, 0, 2000, G);
        AstrionAccount.ExecutionIntent memory i = _intent(address(morphoReturn), address(messenger), action);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, account.intentDigest(i));
        vm.expectRevert(AstrionAccount.WrongProtocol.selector);
        account.executeIntent(i, action, abi.encodePacked(r, s, v), 0);
    }
}
