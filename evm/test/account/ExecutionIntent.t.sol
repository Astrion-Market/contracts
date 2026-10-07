// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockERC20} from "../mocks/MockERC20.sol";
import {MockLendingPool} from "../mocks/MockLendingPool.sol";
import {ReentrantPool} from "../mocks/ReentrantPool.sol";
import {
    MockPoolModule,
    ExcessApproveModule,
    SubstituteTargetModule,
    TransferOutModule,
    SelfCallModule,
    WrongProtocolModule
} from "../mocks/MockPoolModule.sol";

contract ExecutionIntentTest is Test {
    RoutePolicy policy;
    AstrionAccountFactory factory;
    MockERC20 usdc;
    MockLendingPool pool;
    MockPoolModule module;
    AstrionAccount account;

    uint256 alicePk = 0xA11CE;
    address alice;
    address relayer = makeAddr("relayer");
    address guardian = makeAddr("guardian");
    bytes32 constant MARKET = bytes32(uint256(0xbeef));

    uint8 constant SUPPLY = 0;
    uint8 constant BORROW = 1;
    uint8 constant REPAY = 2;
    uint8 constant WITHDRAW = 3;

    function setUp() public {
        alice = vm.addr(alicePk);
        policy = new RoutePolicy(guardian);
        factory = new AstrionAccountFactory(policy);
        usdc = new MockERC20("USD Coin", "USDC", 6);
        pool = new MockLendingPool();
        usdc.mint(address(pool), 1_000_000e6);
        module = new MockPoolModule(address(pool), address(usdc));
        account = AstrionAccount(payable(factory.createAccount(alice, ProtocolIds.AAVE_V3, MARKET, 1)));
        usdc.mint(address(account), 1_000e6);
    }

    // ─── helpers ─────────────────────────────────────────────────────────────

    function _action(uint8 kind, uint256 amount) internal pure returns (bytes memory) {
        return abi.encode(kind, amount);
    }

    function _intent(address mod, bytes memory action, uint256 nonce)
        internal
        view
        returns (AstrionAccount.ExecutionIntent memory)
    {
        return AstrionAccount.ExecutionIntent({
            module: mod,
            moduleCodeHash: mod.codehash,
            target: address(pool),
            actionHash: keccak256(action),
            recipient: alice,
            relayer: address(0),
            feeToken: address(usdc),
            maxFee: 1e6,
            nonce: nonce,
            deadline: block.timestamp + 1 hours,
            transferId: bytes32(0)
        });
    }

    function _sign(AstrionAccount acct, AstrionAccount.ExecutionIntent memory intent, uint256 pk)
        internal
        view
        returns (bytes memory)
    {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, acct.intentDigest(intent));
        return abi.encodePacked(r, s, v);
    }

    function _run(AstrionAccount.ExecutionIntent memory intent, bytes memory action, uint256 fee)
        internal
    {
        bytes memory sig = _sign(account, intent, alicePk);
        vm.prank(relayer);
        account.executeIntent(intent, action, sig, fee);
    }

    function _routeId() internal view returns (bytes32) {
        return policy.routeId(block.chainid, ProtocolIds.AAVE_V3, MARKET);
    }

    // ─── happy paths ─────────────────────────────────────────────────────────

    function test_relayedSupplyPaysBoundedFee() public {
        bytes memory action = _action(SUPPLY, 600e6);
        _run(_intent(address(module), action, 1), action, 0.5e6);

        assertEq(pool.supplied(address(usdc), address(account)), 600e6);
        assertEq(usdc.balanceOf(relayer), 0.5e6);
        assertEq(usdc.allowance(address(account), address(pool)), 0);
        assertTrue(account.nonceUsed(1));
    }

    function test_withdrawGoesOnlyToSignedRecipient() public {
        bytes memory supply = _action(SUPPLY, 100e6);
        _run(_intent(address(module), supply, 1), supply, 0);
        bytes memory withdraw = _action(WITHDRAW, 100e6);
        _run(_intent(address(module), withdraw, 2), withdraw, 0);
        assertEq(usdc.balanceOf(alice), 100e6);
    }

    // ─── authorization ───────────────────────────────────────────────────────

    function test_rejectsWrongOwnerSignature() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        bytes memory sig = _sign(account, intent, 0xB0B);
        vm.expectRevert(AstrionAccount.InvalidSignature.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_rejectsSignatureFromAnotherChain() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.chainId(8453);
        vm.expectRevert(AstrionAccount.InvalidSignature.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_rejectsSignatureForAnotherAccountOrMarket() public {
        AstrionAccount other = AstrionAccount(
            payable(factory.createAccount(alice, ProtocolIds.AAVE_V3, bytes32(uint256(0xcafe)), 1))
        );
        usdc.mint(address(other), 10e6);
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        bytes memory sigForOther = _sign(other, intent, alicePk);
        vm.expectRevert(AstrionAccount.InvalidSignature.selector);
        account.executeIntent(intent, action, sigForOther, 0);
    }

    function test_rejectsReplay() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 7);
        bytes memory sig = _sign(account, intent, alicePk);
        account.executeIntent(intent, action, sig, 0);
        vm.expectRevert(AstrionAccount.NonceAlreadyUsed.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_rejectsRevokedIntent() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 9);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.prank(alice);
        account.revokeNonce(9);
        vm.expectRevert(AstrionAccount.NonceAlreadyUsed.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_rejectsExpiredIntent() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.warp(intent.deadline + 1);
        vm.expectRevert(AstrionAccount.IntentExpired.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_relayerCannotChangeActionModuleOrRecipient() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        bytes memory sig = _sign(account, intent, alicePk);

        vm.expectRevert(AstrionAccount.ActionHashMismatch.selector);
        account.executeIntent(intent, _action(SUPPLY, 999e6), sig, 0);

        AstrionAccount.ExecutionIntent memory swapped = intent;
        swapped.recipient = relayer;
        vm.expectRevert(AstrionAccount.InvalidSignature.selector);
        account.executeIntent(swapped, action, sig, 0);

        swapped = intent;
        swapped.module = address(new MockPoolModule(address(pool), address(usdc)));
        swapped.moduleCodeHash = swapped.module.codehash;
        vm.expectRevert(AstrionAccount.InvalidSignature.selector);
        account.executeIntent(swapped, action, sig, 0);
    }

    function test_feeCapAndRelayerBinding() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        intent.relayer = relayer;
        bytes memory sig = _sign(account, intent, alicePk);

        vm.prank(relayer);
        vm.expectRevert(AstrionAccount.FeeAboveCap.selector);
        account.executeIntent(intent, action, sig, intent.maxFee + 1);

        vm.prank(makeAddr("someoneElse"));
        vm.expectRevert(AstrionAccount.WrongRelayer.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_bridgeFundedIntentNeedsTransferPath() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        intent.transferId = keccak256("transfer");
        bytes memory sig = _sign(account, intent, alicePk);
        vm.expectRevert(AstrionAccount.TransferBindingRequired.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    // ─── module pinning and plan bounds ──────────────────────────────────────

    function test_moduleCodeHashIsPinned() public {
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(address(module), action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        // Same address, different code after signing.
        vm.etch(address(module), address(new SelfCallModule(address(pool), address(usdc))).code);
        vm.expectRevert(AstrionAccount.ModuleCodeMismatch.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_moduleMustMatchAccountProtocol() public {
        address wrong = address(new WrongProtocolModule(address(pool), address(usdc)));
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(wrong, action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.expectRevert(AstrionAccount.WrongProtocol.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    function test_substitutedTargetIsRejected() public {
        MockLendingPool other = new MockLendingPool();
        address mod = address(new SubstituteTargetModule(address(pool), address(usdc), address(other)));
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(mod, action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.expectRevert(abi.encodeWithSelector(AstrionAccount.TargetNotAllowed.selector, address(other)));
        account.executeIntent(intent, action, sig, 0);
    }

    function test_excessAllowanceIsRejected() public {
        address mod = address(new ExcessApproveModule(address(pool), address(usdc)));
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(mod, action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.expectRevert(abi.encodeWithSelector(AstrionAccount.ExcessAllowance.selector, address(usdc)));
        account.executeIntent(intent, action, sig, 0);
    }

    function test_transferToUnsignedRecipientIsRejected() public {
        address thief = makeAddr("thief");
        address mod = address(new TransferOutModule(address(pool), address(usdc), thief));
        bytes memory action = _action(WITHDRAW, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(mod, action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.expectRevert(abi.encodeWithSelector(AstrionAccount.RecipientNotAllowed.selector, thief));
        account.executeIntent(intent, action, sig, 0);
        assertEq(usdc.balanceOf(thief), 0);
    }

    function test_moduleCannotCallTheAccountItself() public {
        address mod = address(new SelfCallModule(address(pool), address(usdc)));
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(mod, action, 1);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.expectRevert(abi.encodeWithSelector(AstrionAccount.TargetNotAllowed.selector, address(account)));
        account.executeIntent(intent, action, sig, 0);
        assertFalse(account.nonceUsed(999));
        assertEq(account.owner(), alice);
    }

    function test_reentrancyIsBlocked() public {
        ReentrantPool evil = new ReentrantPool();
        address mod = address(new MockPoolModule(address(evil), address(usdc)));
        bytes memory action = _action(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(mod, action, 1);
        intent.target = address(evil);
        bytes memory sig = _sign(account, intent, alicePk);
        vm.expectRevert(ReentrancyGuard.ReentrancyGuardReentrantCall.selector);
        account.executeIntent(intent, action, sig, 0);
    }

    // ─── route pauses block new risk, never recovery ─────────────────────────

    function test_pauseBlocksRiskButNotRepayOrOwnerRecovery() public {
        bytes memory supply = _action(SUPPLY, 500e6);
        _run(_intent(address(module), supply, 1), supply, 0);
        bytes memory borrow = _action(BORROW, 100e6);
        _run(_intent(address(module), borrow, 2), borrow, 0);

        vm.prank(guardian);
        policy.setPaused(_routeId(), true);

        bytes memory moreBorrow = _action(BORROW, 1e6);
        AstrionAccount.ExecutionIntent memory risky = _intent(address(module), moreBorrow, 3);
        bytes memory sig = _sign(account, risky, alicePk);
        vm.expectRevert(AstrionAccount.RoutePaused.selector);
        account.executeIntent(risky, moreBorrow, sig, 0);

        // Repayment through an intent still works while paused.
        bytes memory repay = _action(REPAY, 100e6);
        _run(_intent(address(module), repay, 4), repay, 0);
        assertEq(pool.debt(address(usdc), address(account)), 0);

        // And the owner can always exit directly.
        vm.prank(alice);
        account.execute(address(pool), 0, abi.encodeCall(pool.withdraw, (address(usdc), 500e6, alice)));
        assertEq(pool.supplied(address(usdc), address(account)), 0);
    }

    function test_onlyGuardianCanPause() public {
        bytes32 id = _routeId();
        vm.prank(relayer);
        vm.expectRevert();
        policy.setPaused(id, true);
    }
}
