// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AccountForkBase} from "../AccountForkBase.sol";
import {AstrionAccount} from "../../../src/account/AstrionAccount.sol";
import {MorphoBlueModule} from "../../../src/modules/MorphoBlueModule.sol";
import {MorphoBlueLens} from "../../../src/lens/MorphoBlueLens.sol";
import {IMorphoBlue, MarketParams} from "../../../src/interfaces/morpho/IMorphoBlue.sol";
import {MorphoBalances} from "../../../src/libraries/MorphoBalances.sol";
import {ProtocolIds} from "../../../src/libraries/ProtocolIds.sol";
import {FixedRateIrm, MockMorphoOracle} from "../../mocks/MorphoMocks.sol";

/// @notice Real Morpho Blue bytecode on a Base fork, with a purpose-built
/// market fixture (our oracle and IRM) so boundaries are deterministic.
contract MorphoBlueBaseForkTest is AccountForkBase {
    IMorphoBlue constant MORPHO = IMorphoBlue(0xBBBBBbbBBb9cC5e90e3b3Af64bdAF62C37EEFFCb);
    uint256 constant LLTV = 0.86e18;
    uint256 constant WETH_PRICE = 3_000e24;
    uint256 constant LIQUIDITY = 100_000e6;

    MockMorphoOracle oracle;
    FixedRateIrm irm;
    MarketParams params;
    MorphoBlueModule module;
    MorphoBlueLens lens;
    AstrionAccount account;
    IERC20 usdc = IERC20(BASE_USDC);
    address lender = makeAddr("lender");

    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
        if (!forked) return;
        _setUpAccounts(address(0), BASE_USDC, BASE_DOMAIN);

        oracle = new MockMorphoOracle(WETH_PRICE);
        irm = new FixedRateIrm(uint256(0.1e18) / 365 days);
        vm.startPrank(MORPHO.owner());
        if (!MORPHO.isIrmEnabled(address(irm))) MORPHO.enableIrm(address(irm));
        if (!MORPHO.isLltvEnabled(LLTV)) MORPHO.enableLltv(LLTV);
        vm.stopPrank();

        params = MarketParams(BASE_USDC, BASE_WETH, address(oracle), address(irm), LLTV);
        MORPHO.createMarket(params);
        module = new MorphoBlueModule(MORPHO, params);
        lens = new MorphoBlueLens(MORPHO);
        account = _account(ProtocolIds.MORPHO_BLUE, module.marketId());

        deal(BASE_USDC, lender, LIQUIDITY);
        vm.startPrank(lender);
        usdc.approve(address(MORPHO), LIQUIDITY);
        MORPHO.supply(params, LIQUIDITY, 0, lender, "");
        vm.stopPrank();
    }

    function _act(MorphoBlueModule.Kind kind, uint256 amount) internal {
        _exec(account, address(module), address(MORPHO), abi.encode(kind, amount));
    }

    function _actReverts(MorphoBlueModule.Kind kind, uint256 amount) internal {
        _execExpectRevert(account, address(module), address(MORPHO), abi.encode(kind, amount));
    }

    function _maxBorrowFor(uint256 collateral) internal pure returns (uint256) {
        return collateral * WETH_PRICE / 1e36 * LLTV / 1e18;
    }

    function test_marketIdIsTheCanonicalHashOfParams() public view {
        assertEq(module.marketId(), keccak256(abi.encode(params)));
        (address loan, address col,,, uint256 lltv) = MORPHO.idToMarketParams(module.marketId());
        assertEq(loan, BASE_USDC);
        assertEq(col, BASE_WETH);
        assertEq(lltv, LLTV);
    }

    function test_suppliedLoanTokensAreNotCollateral() public {
        deal(BASE_USDC, address(account), 10_000e6);
        _act(MorphoBlueModule.Kind.Supply, 10_000e6);
        MorphoBlueLens.Position memory p = lens.position(params, address(account));
        assertEq(p.collateral, 0);
        assertGt(p.supplyAssets, 0);
        _actReverts(MorphoBlueModule.Kind.Borrow, 1e6);
    }

    function test_lltvBoundary() public {
        deal(BASE_WETH, address(account), 1 ether);
        _act(MorphoBlueModule.Kind.SupplyCollateral, 1 ether);
        uint256 max = _maxBorrowFor(1 ether);
        _actReverts(MorphoBlueModule.Kind.Borrow, max + 1);
        _act(MorphoBlueModule.Kind.Borrow, max);
        assertEq(usdc.balanceOf(address(account)), max);
        assertTrue(lens.position(params, address(account)).healthy);
    }

    function test_debtSharesFixedWhileAssetsGrowAndRepayAllIsExact() public {
        deal(BASE_WETH, address(account), 2 ether);
        _act(MorphoBlueModule.Kind.SupplyCollateral, 2 ether);
        _act(MorphoBlueModule.Kind.Borrow, 1_000e6);
        uint256 sharesBefore = lens.position(params, address(account)).borrowShares;

        vm.warp(block.timestamp + 90 days);
        MorphoBlueLens.Position memory p = lens.position(params, address(account));
        assertEq(p.borrowShares, sharesBefore);
        assertGt(p.borrowAssets, 1_000e6);

        deal(BASE_USDC, address(account), p.borrowAssets);
        _act(MorphoBlueModule.Kind.RepayAll, 0);
        p = lens.position(params, address(account));
        assertEq(p.borrowShares, 0);
        assertEq(usdc.balanceOf(address(account)), 0, "approved exactly the accrued debt");
        assertEq(usdc.allowance(address(account), address(MORPHO)), 0);

        _act(MorphoBlueModule.Kind.WithdrawCollateral, type(uint256).max);
        assertEq(IERC20(BASE_WETH).balanceOf(address(account)), 2 ether);
    }

    function test_repayAvailableKeepsResidualDebt() public {
        deal(BASE_WETH, address(account), 2 ether);
        _act(MorphoBlueModule.Kind.SupplyCollateral, 2 ether);
        _act(MorphoBlueModule.Kind.Borrow, 1_000e6);
        vm.warp(block.timestamp + 30 days);
        deal(BASE_USDC, address(account), 300e6);
        _act(MorphoBlueModule.Kind.RepayAvailable, 0);
        assertGt(lens.position(params, address(account)).borrowAssets, 700e6);
    }

    function test_oracleFailureBlocksBorrowButNotRepay() public {
        deal(BASE_WETH, address(account), 2 ether);
        _act(MorphoBlueModule.Kind.SupplyCollateral, 2 ether);
        _act(MorphoBlueModule.Kind.Borrow, 1_000e6);
        oracle.setBroken(true);
        _actReverts(MorphoBlueModule.Kind.Borrow, 1e6);
        assertFalse(lens.position(params, address(account)).oracleOk);
        _act(MorphoBlueModule.Kind.RepayAll, 0);
        assertEq(lens.position(params, address(account)).borrowShares, 0);
    }

    function test_unsafeCollateralWithdrawalReverts() public {
        deal(BASE_WETH, address(account), 1 ether);
        _act(MorphoBlueModule.Kind.SupplyCollateral, 1 ether);
        _act(MorphoBlueModule.Kind.Borrow, _maxBorrowFor(1 ether) / 2);
        _actReverts(MorphoBlueModule.Kind.WithdrawCollateral, 0.6 ether);
    }

    function test_illiquidMarketRejectsBorrow() public {
        deal(BASE_WETH, address(account), 1_000 ether);
        _act(MorphoBlueModule.Kind.SupplyCollateral, 1_000 ether);
        _actReverts(MorphoBlueModule.Kind.Borrow, LIQUIDITY + 1);
    }

    function test_wrongMarketTupleIsRejected() public {
        MarketParams memory other = params;
        other.lltv = 0.77e18;
        AstrionAccount wrongScope = _account(ProtocolIds.MORPHO_BLUE, MorphoBalances.id(other));
        deal(BASE_USDC, address(wrongScope), 10e6);
        _execExpectRevert(
            wrongScope,
            address(module),
            address(MORPHO),
            abi.encode(MorphoBlueModule.Kind.Supply, uint256(10e6))
        );
    }

    function test_positionBelongsToTheAccount() public {
        deal(BASE_USDC, address(account), 5_000e6);
        _act(MorphoBlueModule.Kind.Supply, 5_000e6);
        (uint256 shares,,) = MORPHO.position(module.marketId(), address(account));
        (uint256 factoryShares,,) = MORPHO.position(module.marketId(), address(factory));
        (uint256 moduleShares,,) = MORPHO.position(module.marketId(), address(module));
        assertGt(shares, 0);
        assertEq(factoryShares, 0);
        assertEq(moduleShares, 0);

        _act(MorphoBlueModule.Kind.Withdraw, type(uint256).max);
        (shares,,) = MORPHO.position(module.marketId(), address(account));
        assertEq(shares, 0);
        assertGe(usdc.balanceOf(address(account)), 5_000e6 - 1);
    }
}
