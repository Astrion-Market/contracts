// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockERC20} from "../mocks/MockERC20.sol";
import {MockLendingPool} from "../mocks/MockLendingPool.sol";
import {MockMessageTransmitterV2} from "../mocks/MockMessageTransmitterV2.sol";
import {MockTokenMessengerV2} from "../mocks/MockTokenMessengerV2.sol";
import {AccountHandler} from "./AccountHandler.sol";

/// @notice Cross-chain failure invariants over random sequences of mints,
/// duplicate/out-of-order deliveries, funded and forged intents, borrows,
/// repays, exits, return burns, pauses and time warps. No auth is mocked.
contract AccountInvariantsTest is Test {
    AccountHandler handler;
    AstrionAccount account;
    MockERC20 usdc;
    MockLendingPool pool;
    MockTokenMessengerV2 messenger;
    MockMessageTransmitterV2 transmitter;

    function setUp() public {
        usdc = new MockERC20("USD Coin", "USDC", 6);
        transmitter = new MockMessageTransmitterV2(usdc);
        pool = new MockLendingPool();
        usdc.mint(address(pool), 10_000_000_000e6);
        messenger = new MockTokenMessengerV2();
        address guardian = makeAddr("guardian");
        RoutePolicy policy = new RoutePolicy(guardian);
        AstrionAccountFactory factory = new AstrionAccountFactory(
            policy, IMessageTransmitterV2(address(transmitter)), IERC20(address(usdc)), 6
        );
        uint256 ownerPk = 0xA11CE;
        account = AstrionAccount(
            payable(factory.createAccount(vm.addr(ownerPk), ProtocolIds.AAVE_V3, bytes32(0), 1))
        );
        handler = new AccountHandler(account, usdc, pool, messenger, policy, ownerPk, guardian);
        targetContract(address(handler));
    }

    function invariant_noAdversarialSuccess() public view {
        assertEq(bytes(handler.violation()).length, 0, handler.violation());
    }

    function invariant_valueIsConserved() public view {
        uint256 held = usdc.balanceOf(address(account)) + pool.supplied(address(usdc), address(account));
        uint256 debt = pool.debt(address(usdc), address(account));
        assertEq(
            held + handler.burned() + handler.ownerOut() + handler.fees(),
            handler.minted() + debt,
            "no value created, lost or double-counted"
        );
        assertEq(usdc.balanceOf(address(messenger)), handler.burned(), "burned == sent to Stellar");
    }

    function invariant_noUnauthorizedValueMovement() public view {
        assertEq(usdc.balanceOf(handler.attacker()), 0);
        assertEq(usdc.balanceOf(handler.owner()), handler.ownerOut());
        assertEq(usdc.balanceOf(handler.relayer()), handler.fees());
    }

    function invariant_noDuplicateDebt() public view {
        assertEq(pool.debt(address(usdc), address(account)), handler.borrowed() - handler.repaid());
    }

    function invariant_noLeftoverAllowance() public view {
        assertEq(usdc.allowance(address(account), address(pool)), 0);
        assertEq(usdc.allowance(address(account), address(messenger)), 0);
    }

    function invariant_eachReceiptFundsAtMostOneAction() public view {
        uint256 consumed;
        for (uint256 i = 0; i < handler.inboundCount(); i++) {
            (bool recorded, bool isConsumed,,,,,) = account.receipts(handler.inboundId(i));
            assertTrue(recorded, "every delivered transfer has a receipt");
            if (isConsumed) consumed++;
        }
        assertEq(consumed, handler.fundedExecuted());
    }

    function invariant_ownerCanAlwaysRecoverMintedFunds() public {
        uint256 bal = usdc.balanceOf(address(account));
        uint256 snapshot = vm.snapshotState();
        address owner = handler.owner();
        uint256 before = usdc.balanceOf(owner);
        vm.prank(owner);
        account.execute(address(usdc), 0, abi.encodeCall(IERC20.transfer, (owner, bal)));
        assertEq(usdc.balanceOf(owner) - before, bal);
        assertEq(usdc.balanceOf(address(account)), 0);
        vm.revertToState(snapshot);
    }
}
