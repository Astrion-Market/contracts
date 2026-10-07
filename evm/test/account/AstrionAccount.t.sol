// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockERC20} from "../mocks/MockERC20.sol";
import {MockLendingPool} from "../mocks/MockLendingPool.sol";

/// @dev Calls back into the account from inside an execute().
contract Reenterer {
    function poke(AstrionAccount account) external {
        account.execute(address(this), 0, "");
    }
}

contract AstrionAccountTest is Test {
    AstrionAccountFactory factory;
    MockERC20 usdc;
    MockLendingPool pool;

    address alice = makeAddr("alice");
    address bob = makeAddr("bob");
    address attacker = makeAddr("attacker");
    bytes32 constant MARKET = bytes32(uint256(0xbeef));

    function setUp() public {
        factory = new AstrionAccountFactory();
        usdc = new MockERC20("USD Coin", "USDC", 6);
        pool = new MockLendingPool();
        usdc.mint(address(pool), 1_000_000e6);
    }

    function _account(address owner) internal returns (AstrionAccount) {
        return AstrionAccount(payable(factory.createAccount(owner, ProtocolIds.AAVE_V3, MARKET, 1)));
    }

    function _call(address target, bytes memory data) internal pure returns (AstrionAccount.Call memory) {
        return AstrionAccount.Call({target: target, value: 0, data: data});
    }

    // ─── deterministic scoped identity ───────────────────────────────────────

    function test_createMatchesPredictionAndIsIdempotent() public {
        address predicted = factory.predictAccount(alice, ProtocolIds.AAVE_V3, MARKET, 1);
        address created = factory.createAccount(alice, ProtocolIds.AAVE_V3, MARKET, 1);
        assertEq(created, predicted);
        assertEq(factory.createAccount(alice, ProtocolIds.AAVE_V3, MARKET, 1), created);

        AstrionAccount account = AstrionAccount(payable(created));
        assertEq(account.owner(), alice);
        assertEq(account.factory(), address(factory));
        assertEq(account.protocol(), ProtocolIds.AAVE_V3);
        assertEq(account.marketScope(), MARKET);
        assertEq(account.version(), 1);
    }

    function test_everyScopeComponentChangesTheAccount() public view {
        address base = factory.predictAccount(alice, ProtocolIds.AAVE_V3, MARKET, 1);
        assertTrue(base != factory.predictAccount(bob, ProtocolIds.AAVE_V3, MARKET, 1));
        assertTrue(base != factory.predictAccount(alice, ProtocolIds.MORPHO_BLUE, MARKET, 1));
        assertTrue(base != factory.predictAccount(alice, ProtocolIds.AAVE_V3, bytes32(uint256(1)), 1));
        assertTrue(base != factory.predictAccount(alice, ProtocolIds.AAVE_V3, MARKET, 2));
    }

    function test_chainIdIsPartOfTheScope() public {
        bytes32 here = factory.salt(alice, ProtocolIds.AAVE_V3, MARKET, 1);
        vm.chainId(8453);
        assertTrue(here != factory.salt(alice, ProtocolIds.AAVE_V3, MARKET, 1));
    }

    function test_rejectsUnsupportedProtocolZeroVersionAndZeroOwner() public {
        vm.expectRevert(
            abi.encodeWithSelector(AstrionAccountFactory.UnsupportedProtocol.selector, keccak256("x"))
        );
        factory.createAccount(alice, keccak256("x"), MARKET, 1);
        vm.expectRevert(AstrionAccountFactory.ZeroVersion.selector);
        factory.createAccount(alice, ProtocolIds.AAVE_V3, MARKET, 0);
        vm.expectRevert(AstrionAccount.ZeroOwner.selector);
        factory.createAccount(address(0), ProtocolIds.AAVE_V3, MARKET, 1);
    }

    // ─── front-running cannot steal ownership ────────────────────────────────

    function test_frontRunnerDeploysOnlyTheOwnersAccount() public {
        vm.prank(attacker);
        address deployed = factory.createAccount(alice, ProtocolIds.AAVE_V3, MARKET, 1);
        AstrionAccount account = AstrionAccount(payable(deployed));
        assertEq(account.owner(), alice, "owner fixed by salt + constructor");

        usdc.mint(deployed, 100e6);
        vm.prank(attacker);
        vm.expectRevert(AstrionAccount.NotOwner.selector);
        account.execute(address(usdc), 0, abi.encodeCall(usdc.transfer, (attacker, 100e6)));
        assertEq(usdc.balanceOf(deployed), 100e6);
    }

    // ─── isolation between users ─────────────────────────────────────────────

    function test_usersCannotTouchEachOthersAccounts() public {
        AstrionAccount a = _account(alice);
        AstrionAccount b = _account(bob);
        usdc.mint(address(a), 50e6);
        usdc.mint(address(b), 70e6);

        vm.prank(bob);
        vm.expectRevert(AstrionAccount.NotOwner.selector);
        a.execute(address(usdc), 0, abi.encodeCall(usdc.transfer, (bob, 50e6)));

        AstrionAccount.Call[] memory calls = new AstrionAccount.Call[](1);
        calls[0] = _call(address(usdc), abi.encodeCall(usdc.transfer, (alice, 70e6)));
        vm.prank(alice);
        vm.expectRevert(AstrionAccount.NotOwner.selector);
        b.executeBatch(calls);

        assertEq(usdc.balanceOf(address(a)), 50e6);
        assertEq(usdc.balanceOf(address(b)), 70e6);
    }

    function testFuzz_nonOwnerCannotExecute(address caller) public {
        AstrionAccount a = _account(alice);
        vm.assume(caller != alice);
        vm.prank(caller);
        vm.expectRevert(AstrionAccount.NotOwner.selector);
        a.execute(address(usdc), 0, "");
    }

    // ─── owner operates the protocol directly, no relayer ────────────────────

    function test_ownerSuppliesBorrowsRepaysAndWithdrawsDirectly() public {
        AstrionAccount a = _account(alice);
        usdc.mint(address(a), 1_000e6);

        AstrionAccount.Call[] memory supply = new AstrionAccount.Call[](2);
        supply[0] = _call(address(usdc), abi.encodeCall(usdc.approve, (address(pool), 600e6)));
        supply[1] = _call(address(pool), abi.encodeCall(pool.supply, (address(usdc), 600e6, address(a), 0)));
        vm.prank(alice);
        a.executeBatch(supply);

        vm.startPrank(alice);
        a.execute(address(pool), 0, abi.encodeCall(pool.borrow, (address(usdc), 100e6, 2, 0, address(a))));
        a.execute(address(usdc), 0, abi.encodeCall(usdc.approve, (address(pool), 100e6)));
        a.execute(address(pool), 0, abi.encodeCall(pool.repay, (address(usdc), 100e6, 2, address(a))));
        a.execute(address(pool), 0, abi.encodeCall(pool.withdraw, (address(usdc), 600e6, address(a))));
        a.execute(address(usdc), 0, abi.encodeCall(usdc.transfer, (alice, 1_000e6)));
        vm.stopPrank();

        assertEq(usdc.balanceOf(alice), 1_000e6);
        assertEq(pool.supplied(address(usdc), address(a)), 0);
        assertEq(pool.debt(address(usdc), address(a)), 0);
    }

    function test_positionIsOwnedByTheAccountNotAShared_router() public {
        AstrionAccount a = _account(alice);
        usdc.mint(address(a), 10e6);
        vm.startPrank(alice);
        a.execute(address(usdc), 0, abi.encodeCall(usdc.approve, (address(pool), 10e6)));
        a.execute(address(pool), 0, abi.encodeCall(pool.supply, (address(usdc), 10e6, address(a), 0)));
        vm.stopPrank();

        assertEq(pool.supplied(address(usdc), address(a)), 10e6);
        assertEq(pool.supplied(address(usdc), address(factory)), 0);
        assertEq(usdc.balanceOf(address(factory)), 0, "factory never custodies assets");
    }

    // ─── call semantics ──────────────────────────────────────────────────────

    function test_revertsBubbleUpAndBatchIsAtomic() public {
        AstrionAccount a = _account(alice);
        usdc.mint(address(a), 10e6);
        pool.setPaused(true);

        AstrionAccount.Call[] memory calls = new AstrionAccount.Call[](2);
        calls[0] = _call(address(usdc), abi.encodeCall(usdc.approve, (address(pool), 10e6)));
        calls[1] = _call(address(pool), abi.encodeCall(pool.supply, (address(usdc), 10e6, address(a), 0)));
        vm.prank(alice);
        vm.expectRevert(MockLendingPool.Paused.selector);
        a.executeBatch(calls);
        assertEq(usdc.allowance(address(a), address(pool)), 0, "first call rolled back");
    }

    function test_reentrantCallbackIsRejected() public {
        AstrionAccount a = _account(alice);
        Reenterer r = new Reenterer();
        vm.prank(alice);
        vm.expectRevert(AstrionAccount.NotOwner.selector);
        a.execute(address(r), 0, abi.encodeCall(Reenterer.poke, (a)));
    }

    function test_receivesEtherForGasRecovery() public {
        AstrionAccount a = _account(alice);
        vm.deal(address(this), 1 ether);
        (bool ok,) = address(a).call{value: 1 ether}("");
        assertTrue(ok);
        vm.prank(alice);
        a.execute(alice, 1 ether, "");
        assertEq(alice.balance, 1 ether);
    }
}
