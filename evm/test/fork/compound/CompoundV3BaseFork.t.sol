// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AccountForkBase} from "../AccountForkBase.sol";
import {AstrionAccount} from "../../../src/account/AstrionAccount.sol";
import {CompoundV3Module} from "../../../src/modules/CompoundV3Module.sol";
import {CompoundV3Lens} from "../../../src/lens/CompoundV3Lens.sol";
import {IComet} from "../../../src/interfaces/compound/IComet.sol";
import {ProtocolIds} from "../../../src/libraries/ProtocolIds.sol";

contract CompoundV3BaseForkTest is AccountForkBase {
    IComet constant COMET = IComet(0xb125E6687d4313864e53df431d5425969c15Eb2F);

    CompoundV3Module module;
    CompoundV3Lens lens;
    AstrionAccount account;
    IERC20 usdc = IERC20(BASE_USDC);
    IERC20 weth = IERC20(BASE_WETH);

    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
        if (!forked) return;
        _setUpAccounts(address(0), BASE_USDC, BASE_DOMAIN);
        module = new CompoundV3Module(COMET, BASE_WETH);
        lens = new CompoundV3Lens();
        account = _account(ProtocolIds.COMPOUND_V3, module.marketScope());
    }

    function _act(CompoundV3Module.Kind kind, uint256 amount) internal {
        _exec(account, address(module), address(COMET), abi.encode(kind, amount));
    }

    function _actReverts(CompoundV3Module.Kind kind, uint256 amount) internal {
        _execExpectRevert(account, address(module), address(COMET), abi.encode(kind, amount));
    }

    function _pos() internal view returns (CompoundV3Lens.Position memory) {
        return lens.position(COMET, address(account), BASE_WETH);
    }

    function _collateral(uint256 amount) internal {
        deal(BASE_WETH, address(account), amount);
        _act(CompoundV3Module.Kind.SupplyCollateral, amount);
    }

    function test_baseTokenIsNativeUsdc() public view {
        assertEq(module.baseToken(), BASE_USDC);
    }

    function test_positiveToNegativeBaseTransition() public {
        _collateral(5 ether);
        deal(BASE_USDC, address(account), 100e6);
        _act(CompoundV3Module.Kind.SupplyBase, 100e6);
        assertApproxEqAbs(_pos().baseSupplied, 100e6, 1);

        uint256 minimum = COMET.baseBorrowMin();
        uint256 supplied = _pos().baseSupplied;
        _act(CompoundV3Module.Kind.WithdrawBase, supplied + minimum * 2);
        CompoundV3Lens.Position memory p = _pos();
        assertEq(p.baseSupplied, 0);
        assertApproxEqAbs(p.baseBorrowed, minimum * 2, 1);
        assertTrue(p.borrowCollateralized);
    }

    function test_minimumBorrowIsRejectedBeforeComet() public {
        _collateral(5 ether);
        uint256 minimum = COMET.baseBorrowMin();
        _actReverts(CompoundV3Module.Kind.WithdrawBase, minimum - 1);
        _act(CompoundV3Module.Kind.WithdrawBase, minimum);
        assertApproxEqAbs(_pos().baseBorrowed, minimum, 1);
    }

    function test_repayAllIsExactAfterInterest() public {
        _collateral(5 ether);
        _act(CompoundV3Module.Kind.WithdrawBase, 1000e6);
        vm.warp(block.timestamp + 30 days);
        uint256 debt = COMET.borrowBalanceOf(address(account));
        assertGt(debt, 1000e6);
        deal(BASE_USDC, address(account), debt);
        _act(CompoundV3Module.Kind.RepayAll, 0);
        assertEq(COMET.borrowBalanceOf(address(account)), 0);
        assertEq(usdc.balanceOf(address(account)), 0);
        assertEq(usdc.allowance(address(account), address(COMET)), 0);
    }

    function test_partialRepayLeavesDustDebtVisible() public {
        _collateral(5 ether);
        _act(CompoundV3Module.Kind.WithdrawBase, 1000e6);
        vm.warp(block.timestamp + 1 days);
        uint256 debt = COMET.borrowBalanceOf(address(account));
        deal(BASE_USDC, address(account), debt - 1);
        _act(CompoundV3Module.Kind.RepayAvailable, 0);
        assertGt(_pos().baseBorrowed, 0, "dust debt stays visible");
        assertLt(_pos().baseBorrowed, COMET.baseBorrowMin());
    }

    function test_collateralWithdrawalSafety() public {
        _collateral(1 ether);
        _act(CompoundV3Module.Kind.WithdrawBase, 1000e6);
        _actReverts(CompoundV3Module.Kind.WithdrawCollateral, type(uint256).max);

        deal(BASE_USDC, address(account), 2000e6);
        _act(CompoundV3Module.Kind.RepayAll, 0);
        _act(CompoundV3Module.Kind.WithdrawCollateral, type(uint256).max);
        assertEq(weth.balanceOf(address(account)), 1 ether);
    }

    function test_collateralEarnsNoInterest() public {
        _collateral(3 ether);
        vm.warp(block.timestamp + 365 days);
        assertEq(_pos().collateral, 3 ether);
        assertEq(_pos().baseSupplied, 0);
    }

    function test_withdrawPauseBlocksBorrowButNotRepay() public {
        _collateral(5 ether);
        _act(CompoundV3Module.Kind.WithdrawBase, 1000e6);
        vm.prank(COMET.pauseGuardian());
        COMET.pause(false, false, true, false, false);
        _actReverts(CompoundV3Module.Kind.WithdrawBase, 100e6);
        deal(BASE_USDC, address(account), 2000e6);
        _act(CompoundV3Module.Kind.RepayAll, 0);
        assertEq(_pos().baseBorrowed, 0);
        assertTrue(lens.market(COMET, BASE_WETH).withdrawPaused);
    }

    function test_wrongCometScopeIsRejected() public {
        AstrionAccount other = _account(ProtocolIds.COMPOUND_V3, bytes32(uint256(1)));
        deal(BASE_USDC, address(other), 10e6);
        _execExpectRevert(
            other,
            address(module),
            address(COMET),
            abi.encode(CompoundV3Module.Kind.SupplyBase, uint256(10e6))
        );
    }
}
