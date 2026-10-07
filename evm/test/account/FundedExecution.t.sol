// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test, Vm} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";
import {CctpMessageV2} from "../../src/libraries/CctpMessageV2.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockERC20} from "../mocks/MockERC20.sol";
import {MockLendingPool} from "../mocks/MockLendingPool.sol";
import {MockMessageTransmitterV2} from "../mocks/MockMessageTransmitterV2.sol";
import {MockPoolModule} from "../mocks/MockPoolModule.sol";

contract FundedExecutionTest is Test {
    uint32 constant STELLAR = 27;
    uint32 constant BASE = 6;
    bytes32 constant MARKET = bytes32(uint256(0xbeef));
    uint8 constant SUPPLY = 0;

    MockERC20 usdc;
    MockMessageTransmitterV2 transmitter;
    MockLendingPool pool;
    MockPoolModule module;
    AstrionAccountFactory factory;
    AstrionAccount account;

    uint256 alicePk = 0xA11CE;
    address alice;
    address relayer = makeAddr("relayer");
    address observer = makeAddr("observer");
    bytes attestation = "valid-attestation";

    function setUp() public {
        alice = vm.addr(alicePk);
        usdc = new MockERC20("USD Coin", "USDC", 6);
        transmitter = new MockMessageTransmitterV2(usdc);
        pool = new MockLendingPool();
        module = new MockPoolModule(address(pool), address(usdc));
        factory = new AstrionAccountFactory(
            new RoutePolicy(address(this)),
            IMessageTransmitterV2(address(transmitter)),
            IERC20(address(usdc)),
            BASE
        );
        account = AstrionAccount(payable(factory.createAccount(alice, ProtocolIds.AAVE_V3, MARKET, 1)));
    }

    // ─── helpers ─────────────────────────────────────────────────────────────

    /// MessageV2 header (148 bytes) + BurnMessageV2 body (228 bytes).
    function _message(
        uint32 dst,
        bytes32 nonce,
        bytes32 caller,
        bytes32 mintRecipient,
        uint256 amount,
        uint256 fee
    ) internal pure returns (bytes memory) {
        bytes memory header = abi.encodePacked(
            uint32(1), STELLAR, dst, nonce, bytes32(0), bytes32(0), caller, uint32(1000), uint32(2000)
        );
        bytes memory body = abi.encodePacked(
            uint32(1), bytes32(0), mintRecipient, amount, bytes32(0), uint256(fee), fee, uint256(0)
        );
        return bytes.concat(header, body);
    }

    function _toAccount(bytes32 nonce, uint256 amount, uint256 fee) internal view returns (bytes memory) {
        bytes32 self = CctpMessageV2.toBytes32(address(account));
        return _message(BASE, nonce, self, self, amount, fee);
    }

    function _intent(bytes memory action, uint256 nonce, bytes32 transferId)
        internal
        view
        returns (AstrionAccount.ExecutionIntent memory)
    {
        return AstrionAccount.ExecutionIntent({
            module: address(module),
            moduleCodeHash: address(module).codehash,
            target: address(pool),
            actionHash: keccak256(action),
            recipient: alice,
            relayer: address(0),
            feeToken: address(usdc),
            maxFee: 1e6,
            nonce: nonce,
            deadline: block.timestamp + 1 hours,
            transferId: transferId
        });
    }

    function _sign(AstrionAccount.ExecutionIntent memory intent, uint256 pk)
        internal
        view
        returns (bytes memory)
    {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, account.intentDigest(intent));
        return abi.encodePacked(r, s, v);
    }

    function _id(bytes32 nonce) internal view returns (bytes32) {
        return account.transferIdOf(STELLAR, nonce);
    }

    function _receipt(bytes32 id) internal view returns (bool recorded, bool consumed, uint256 received) {
        (recorded, consumed,,,,, received) = account.receipts(id);
    }

    // ─── reconciliation ──────────────────────────────────────────────────────

    function test_receiptRecordsMeasuredAmountAfterFee() public {
        bytes32 id = account.receiveTransfer(_toAccount(bytes32(uint256(1)), 100e6, 0.01e6), attestation);
        assertEq(id, _id(bytes32(uint256(1))));
        (bool recorded, bool consumed, uint256 received) = _receipt(id);
        assertTrue(recorded);
        assertFalse(consumed);
        assertEq(received, 100e6 - 0.01e6);
        assertEq(usdc.balanceOf(address(account)), 100e6 - 0.01e6);
    }

    function test_duplicateReceiptIsRejected() public {
        bytes memory m = _toAccount(bytes32(uint256(1)), 10e6, 0);
        bytes32 id = account.receiveTransfer(m, attestation);
        vm.expectRevert(abi.encodeWithSelector(AstrionAccount.AlreadyReceived.selector, id));
        account.receiveTransfer(m, attestation);
    }

    function test_mintedAmountMustReconcile() public {
        transmitter.setShortfall(1);
        vm.expectRevert(abi.encodeWithSelector(AstrionAccount.ReceiptMismatch.selector, 10e6, 10e6 - 1));
        account.receiveTransfer(_toAccount(bytes32(uint256(1)), 10e6, 0), attestation);
    }

    function test_rejectsTransfersNotAddressedToThisAccount() public {
        bytes32 self = CctpMessageV2.toBytes32(address(account));
        bytes32 other = CctpMessageV2.toBytes32(observer);
        bytes32 n = bytes32(uint256(1));

        vm.expectRevert(AstrionAccount.BadTransferFields.selector);
        account.receiveTransfer(_message(STELLAR, n, self, self, 10e6, 0), attestation); // wrong domain

        vm.expectRevert(AstrionAccount.BadTransferFields.selector);
        account.receiveTransfer(_message(BASE, n, self, other, 10e6, 0), attestation); // mint elsewhere

        vm.expectRevert(AstrionAccount.BadTransferFields.selector);
        account.receiveTransfer(_message(BASE, n, bytes32(0), self, 10e6, 0), attestation); // anyone-can-mint
    }

    function test_observerCompletingTheMintGainsNothing() public {
        vm.prank(observer);
        account.receiveTransfer(_toAccount(bytes32(uint256(1)), 10e6, 0), attestation);
        assertEq(usdc.balanceOf(observer), 0);
        assertEq(usdc.balanceOf(address(account)), 10e6);
    }

    // ─── bound execution ─────────────────────────────────────────────────────

    function test_mintAndSupplyInOneCall() public {
        bytes32 n = bytes32(uint256(1));
        bytes memory action = abi.encode(SUPPLY, 99e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(action, 1, _id(n));
        bytes memory sig = _sign(intent, alicePk);

        vm.prank(relayer);
        bool executed = account.executeFundedIntent(
            intent, action, sig, 0.5e6, _toAccount(n, 100e6, 0.5e6), attestation
        );
        assertTrue(executed);
        (, bool consumed, uint256 received) = _receipt(_id(n));
        assertTrue(consumed);
        assertEq(received, 99.5e6);
        assertEq(pool.supplied(address(usdc), address(account)), 99e6);
        assertEq(usdc.balanceOf(relayer), 0.5e6);
        assertEq(usdc.balanceOf(address(account)), 0);
    }

    function test_failedActionLeavesMintRecoverable() public {
        pool.setPaused(true);
        bytes32 n = bytes32(uint256(2));
        bytes memory action = abi.encode(SUPPLY, 50e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(action, 1, _id(n));
        bytes memory sig = _sign(intent, alicePk);

        vm.recordLogs();
        bool executed =
            account.executeFundedIntent(intent, action, sig, 0, _toAccount(n, 50e6, 0), attestation);
        assertFalse(executed);
        assertTrue(_sawFailure());

        (bool recorded, bool consumed,) = _receipt(_id(n));
        assertTrue(recorded);
        assertFalse(consumed, "receipt stays available");
        assertFalse(account.nonceUsed(1), "intent nonce not burned by a failed action");
        assertEq(usdc.balanceOf(address(account)), 50e6);

        // Owner recovers directly.
        vm.prank(alice);
        account.execute(address(usdc), 0, abi.encodeCall(IERC20.transfer, (alice, 50e6)));
        assertEq(usdc.balanceOf(alice), 50e6);
    }

    function test_expiredIntentStillMintsAndCanBeRetriedWithAFreshIntent() public {
        bytes32 n = bytes32(uint256(3));
        bytes memory action = abi.encode(SUPPLY, 20e6);
        AstrionAccount.ExecutionIntent memory stale = _intent(action, 1, _id(n));
        bytes memory staleSig = _sign(stale, alicePk);
        vm.warp(stale.deadline + 1);

        assertFalse(
            account.executeFundedIntent(stale, action, staleSig, 0, _toAccount(n, 20e6, 0), attestation)
        );
        assertEq(usdc.balanceOf(address(account)), 20e6);

        // A fresh intent bound to the same receipt; no message needed now.
        AstrionAccount.ExecutionIntent memory fresh = _intent(action, 2, _id(n));
        assertTrue(account.executeFundedIntent(fresh, action, _sign(fresh, alicePk), 0, "", ""));
        assertEq(pool.supplied(address(usdc), address(account)), 20e6);
    }

    function test_actionIsBoundToTheExactTransfer() public {
        bytes32 signedFor = bytes32(uint256(4));
        bytes32 substituted = bytes32(uint256(5));
        bytes memory action = abi.encode(SUPPLY, 10e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(action, 1, _id(signedFor));
        bytes memory sig = _sign(intent, alicePk);

        vm.prank(relayer);
        vm.expectRevert(
            abi.encodeWithSelector(
                AstrionAccount.TransferMismatch.selector, _id(signedFor), _id(substituted)
            )
        );
        account.executeFundedIntent(intent, action, sig, 0, _toAccount(substituted, 10e6, 0), attestation);
    }

    function test_consumedTransferCannotFundASecondAction() public {
        bytes32 n = bytes32(uint256(6));
        bytes memory action = abi.encode(SUPPLY, 10e6);
        AstrionAccount.ExecutionIntent memory first = _intent(action, 1, _id(n));
        assertTrue(
            account.executeFundedIntent(
                first, action, _sign(first, alicePk), 0, _toAccount(n, 20e6, 0), attestation
            )
        );
        AstrionAccount.ExecutionIntent memory second = _intent(action, 2, _id(n));
        bytes memory sig = _sign(second, alicePk);
        vm.expectRevert(AstrionAccount.TransferAlreadyConsumed.selector);
        account.executeFundedIntent(second, action, sig, 0, "", "");
    }

    function test_observerCannotExecuteWithForgedIntentOrStealDust() public {
        bytes32 n = bytes32(uint256(7));
        bytes memory action = abi.encode(uint8(3), uint256(10e6)); // withdraw to recipient
        AstrionAccount.ExecutionIntent memory forged = _intent(action, 1, _id(n));
        forged.recipient = observer;
        bytes memory sig = _sign(forged, 0xBAD);

        vm.prank(observer);
        bool executed = account.executeFundedIntent(
            forged, action, sig, 1e6, _toAccount(n, 10e6 + 3, 0), attestation
        );
        assertFalse(executed);
        assertEq(usdc.balanceOf(observer), 0);
        assertEq(usdc.balanceOf(address(account)), 10e6 + 3, "including dust");
        (, bool consumed,) = _receipt(_id(n));
        assertFalse(consumed);
    }

    function test_runFundedIntentIsSelfOnly() public {
        AstrionAccount.ExecutionIntent memory intent = _intent("", 1, bytes32(uint256(1)));
        vm.expectRevert(AstrionAccount.NotSelf.selector);
        account.runFundedIntent(intent, "", "", 0, relayer);
    }

    function test_plainIntentPathRejectsTransferBinding() public {
        bytes memory action = abi.encode(SUPPLY, 1e6);
        AstrionAccount.ExecutionIntent memory intent = _intent(action, 1, bytes32(0));
        vm.expectRevert(AstrionAccount.TransferBindingRequired.selector);
        account.executeFundedIntent(intent, action, "", 0, "", "");
    }

    function _sawFailure() internal returns (bool) {
        Vm.Log[] memory logs = vm.getRecordedLogs();
        for (uint256 i = 0; i < logs.length; i++) {
            if (logs[i].topics[0] == AstrionAccount.FundedActionFailed.selector) return true;
        }
        return false;
    }
}
