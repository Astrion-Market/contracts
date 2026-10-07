// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AccountForkBase} from "../AccountForkBase.sol";
import {AstrionAccount} from "../../../src/account/AstrionAccount.sol";
import {AaveV3Module} from "../../../src/modules/AaveV3Module.sol";
import {AaveV3Lens} from "../../../src/lens/AaveV3Lens.sol";
import {
    IAaveACLManager,
    IAavePool,
    IAavePoolAddressesProvider,
    IAavePoolConfigurator
} from "../../../src/interfaces/aave/IAaveV3.sol";
import {ProtocolIds} from "../../../src/libraries/ProtocolIds.sol";

contract AaveV3BaseForkTest is AccountForkBase {
    IAavePoolAddressesProvider constant PROVIDER =
        IAavePoolAddressesProvider(0xe20fCBdBfFC4Dd138cE8b2E6FBb6CB49777ad64D);

    AaveV3Module module;
    AaveV3Lens lens;
    AstrionAccount account;
    address pool;
    IERC20 usdc = IERC20(BASE_USDC);
    IERC20 weth = IERC20(BASE_WETH);

    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
        if (!forked) return;
        _setUpAccounts(address(0), BASE_USDC, BASE_DOMAIN);
        module = new AaveV3Module(PROVIDER, BASE_USDC, BASE_WETH);
        lens = new AaveV3Lens(PROVIDER);
        pool = address(module.pool());
        account = _account(ProtocolIds.AAVE_V3, module.marketScopeFor(BASE_USDC));
    }

    function _act(AaveV3Module.Kind kind, address asset, uint256 amount) internal {
        _exec(account, address(module), pool, abi.encode(kind, asset, amount));
    }

    function _actReverts(AaveV3Module.Kind kind, address asset, uint256 amount) internal {
        _execExpectRevert(account, address(module), pool, abi.encode(kind, asset, amount));
    }

    function _collateralAndBorrow(uint256 borrowAmount) internal {
        deal(BASE_WETH, address(account), 10 ether);
        _act(AaveV3Module.Kind.SupplyCollateral, BASE_WETH, 10 ether);
        _act(AaveV3Module.Kind.Borrow, BASE_USDC, borrowAmount);
    }

    function _admin() internal returns (IAavePoolConfigurator configurator) {
        IAaveACLManager acl = IAaveACLManager(PROVIDER.getACLManager());
        vm.startPrank(PROVIDER.getACLAdmin());
        acl.addPoolAdmin(address(this));
        acl.addRiskAdmin(address(this));
        acl.addEmergencyAdmin(address(this));
        vm.stopPrank();
        configurator = IAavePoolConfigurator(PROVIDER.getPoolConfigurator());
    }

    function test_supplyAccruesInterestAndWithdrawsExactly() public {
        deal(BASE_USDC, address(account), 1000e6);
        _act(AaveV3Module.Kind.Supply, BASE_USDC, 1000e6);
        assertEq(usdc.balanceOf(address(account)), 0);
        assertEq(usdc.allowance(address(account), pool), 0);

        vm.warp(block.timestamp + 30 days);
        uint256 supplied = lens.position(address(account), BASE_USDC, BASE_WETH).loanSupplied;
        assertGt(supplied, 1000e6, "interest accrued");

        _act(AaveV3Module.Kind.Withdraw, BASE_USDC, type(uint256).max);
        assertEq(usdc.balanceOf(address(account)), supplied, "exact delta");
        assertEq(lens.position(address(account), BASE_USDC, BASE_WETH).loanSupplied, 0);
    }

    function test_collateralBorrowRepayAllAndWithdraw() public {
        _collateralAndBorrow(1000e6);
        AaveV3Lens.Position memory p = lens.position(address(account), BASE_USDC, BASE_WETH);
        assertEq(usdc.balanceOf(address(account)), 1000e6);
        assertGt(p.healthFactor, 1e18);
        assertGt(p.loanVariableDebt, 0);

        vm.warp(block.timestamp + 7 days);
        uint256 debt = module.variableDebtOf(address(account));
        assertGt(debt, 1000e6, "debt accrued");
        deal(BASE_USDC, address(account), debt);

        _act(AaveV3Module.Kind.RepayAll, BASE_USDC, 0);
        assertEq(module.variableDebtOf(address(account)), 0);
        assertEq(usdc.balanceOf(address(account)), 0, "approved and paid exactly the debt");
        assertEq(usdc.allowance(address(account), pool), 0);

        _act(AaveV3Module.Kind.Withdraw, BASE_WETH, type(uint256).max);
        assertApproxEqAbs(weth.balanceOf(address(account)), 10 ether, 1);
    }

    function test_repayAvailableLeavesRemainingDebtVisible() public {
        _collateralAndBorrow(1000e6);
        vm.warp(block.timestamp + 7 days);
        deal(BASE_USDC, address(account), 400e6);
        _act(AaveV3Module.Kind.RepayAvailable, BASE_USDC, 0);
        assertEq(usdc.balanceOf(address(account)), 0);
        assertGt(module.variableDebtOf(address(account)), 600e6, "not marked closed");
    }

    function test_insufficientCollateralBorrowReverts() public {
        deal(BASE_WETH, address(account), 0.01 ether);
        _act(AaveV3Module.Kind.SupplyCollateral, BASE_WETH, 0.01 ether);
        _actReverts(AaveV3Module.Kind.Borrow, BASE_USDC, 1_000_000e6);
    }

    function test_unsafeCollateralWithdrawalReverts() public {
        _collateralAndBorrow(5000e6);
        _actReverts(AaveV3Module.Kind.Withdraw, BASE_WETH, type(uint256).max);
    }

    function test_frozenReserveBlocksSupplyButAllowsRepay() public {
        _collateralAndBorrow(1000e6);
        _admin().setReserveFreeze(BASE_USDC, true);

        deal(BASE_USDC, address(account), 2000e6);
        _actReverts(AaveV3Module.Kind.Supply, BASE_USDC, 100e6);
        _act(AaveV3Module.Kind.RepayAll, BASE_USDC, 0);
        assertEq(module.variableDebtOf(address(account)), 0);
        assertTrue(lens.reserve(BASE_USDC).isFrozen);
    }

    function test_pausedReserveBlocksSupply() public {
        _admin().setReservePause(BASE_USDC, true);
        deal(BASE_USDC, address(account), 100e6);
        _actReverts(AaveV3Module.Kind.Supply, BASE_USDC, 100e6);
        assertTrue(lens.reserve(BASE_USDC).isPaused);
    }

    function test_supplyAndBorrowCapsAreEnforced() public {
        IAavePoolConfigurator configurator = _admin();
        configurator.setBorrowCap(BASE_USDC, 1);
        deal(BASE_WETH, address(account), 10 ether);
        _act(AaveV3Module.Kind.SupplyCollateral, BASE_WETH, 10 ether);
        _actReverts(AaveV3Module.Kind.Borrow, BASE_USDC, 10e6);

        configurator.setSupplyCap(BASE_USDC, 1);
        deal(BASE_USDC, address(account), 100e6);
        _actReverts(AaveV3Module.Kind.Supply, BASE_USDC, 100e6);
    }

    function test_eModeAndForeignAssetsAreRejected() public {
        vm.prank(owner);
        account.execute(pool, 0, abi.encodeCall(IAavePool.setUserEMode, (1)));
        deal(BASE_USDC, address(account), 10e6);
        _actReverts(AaveV3Module.Kind.Supply, BASE_USDC, 10e6);

        vm.prank(owner);
        account.execute(pool, 0, abi.encodeCall(IAavePool.setUserEMode, (0)));
        _actReverts(AaveV3Module.Kind.Supply, BASE_WETH, 1 ether);
    }

    function test_wrongMarketScopeIsRejected() public {
        AstrionAccount other = _account(ProtocolIds.AAVE_V3, module.marketScopeFor(BASE_WETH));
        deal(BASE_USDC, address(other), 10e6);
        _execExpectRevert(
            other, address(module), pool, abi.encode(AaveV3Module.Kind.Supply, BASE_USDC, 10e6)
        );
    }

    function test_ownerCanExitDirectlyWithoutRelayer() public {
        deal(BASE_USDC, address(account), 100e6);
        _act(AaveV3Module.Kind.Supply, BASE_USDC, 100e6);
        vm.prank(owner);
        account.execute(
            pool, 0, abi.encodeCall(IAavePool.withdraw, (BASE_USDC, type(uint256).max, owner))
        );
        assertGe(usdc.balanceOf(owner), 100e6);
    }
}
