// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {IActionModule} from "../../src/interfaces/IActionModule.sol";
import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";
import {CctpReturnModule} from "../../src/modules/CctpReturnModule.sol";
import {PlanCalls} from "../../src/libraries/PlanCalls.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockERC20} from "../mocks/MockERC20.sol";
import {MockLendingPool} from "../mocks/MockLendingPool.sol";
import {MockPoolModule} from "../mocks/MockPoolModule.sol";
import {MockTokenMessengerV2} from "../mocks/MockTokenMessengerV2.sol";

contract OwnerDirectPlanTest is Test {
    uint8 constant SUPPLY = 0;
    uint8 constant BORROW = 1;
    uint8 constant REPAY = 2;
    uint8 constant WITHDRAW = 3;

    RoutePolicy policy;
    MockERC20 usdc;
    MockLendingPool pool;
    MockPoolModule module;
    AstrionAccount account;
    address owner = makeAddr("owner");

    function setUp() public {
        policy = new RoutePolicy(address(this));
        usdc = new MockERC20("USD Coin", "USDC", 6);
        pool = new MockLendingPool();
        usdc.mint(address(pool), 1_000_000e6);
        module = new MockPoolModule(address(pool), address(usdc));
        AstrionAccountFactory factory = new AstrionAccountFactory(
            policy, IMessageTransmitterV2(address(0)), IERC20(address(usdc)), 6
        );
        account = AstrionAccount(
            payable(factory.createAccount(owner, ProtocolIds.AAVE_V3, bytes32(0), 1))
        );
        usdc.mint(address(account), 1000e6);
    }

    function _direct(IActionModule mod, bytes memory action) internal {
        AstrionAccount.Call[] memory calls = PlanCalls.planDirect(mod, account, owner, action);
        vm.prank(owner);
        account.executeBatch(calls);
    }

    function test_directLendBorrowRepayWithdrawWithoutRelayer() public {
        _direct(module, abi.encode(SUPPLY, uint256(600e6)));
        _direct(module, abi.encode(BORROW, uint256(100e6)));
        _direct(module, abi.encode(REPAY, uint256(100e6)));
        _direct(module, abi.encode(WITHDRAW, uint256(600e6)));
        assertEq(pool.debt(address(usdc), address(account)), 0);
        assertEq(usdc.balanceOf(owner), 600e6);
    }

    function test_emergencyRepayWorksWhileRoutePaused() public {
        _direct(module, abi.encode(SUPPLY, uint256(500e6)));
        _direct(module, abi.encode(BORROW, uint256(200e6)));
        policy.setPaused(policy.routeId(block.chainid, ProtocolIds.AAVE_V3, bytes32(0)), true);
        _direct(module, abi.encode(REPAY, uint256(200e6)));
        assertEq(pool.debt(address(usdc), address(account)), 0);
    }

    function test_directReturnToStellar() public {
        MockTokenMessengerV2 messenger = new MockTokenMessengerV2();
        CctpReturnModule ret = new CctpReturnModule(
            ProtocolIds.AAVE_V3, messenger, address(usdc), bytes32(uint256(1))
        );
        _direct(
            ret,
            abi.encode(
                uint256(300e6),
                uint256(0),
                uint32(2000),
                "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI"
            )
        );
        assertEq(messenger.lastBurn().amount, 300e6);
        assertEq(usdc.allowance(address(account), address(messenger)), 0);
    }

    function test_strangerCannotUseTheDirectPath() public {
        AstrionAccount.Call[] memory calls =
            PlanCalls.planDirect(module, account, owner, abi.encode(SUPPLY, uint256(1e6)));
        vm.expectRevert(AstrionAccount.NotOwner.selector);
        account.executeBatch(calls);
    }
}
