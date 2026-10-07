// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {LifecycleForkBase} from "./LifecycleForkBase.sol";
import {AaveV3Module} from "../../../src/modules/AaveV3Module.sol";
import {MorphoBlueModule} from "../../../src/modules/MorphoBlueModule.sol";
import {CompoundV3Module} from "../../../src/modules/CompoundV3Module.sol";
import {IAavePoolAddressesProvider} from "../../../src/interfaces/aave/IAaveV3.sol";
import {IMorphoBlue, MarketParams} from "../../../src/interfaces/morpho/IMorphoBlue.sol";
import {IComet} from "../../../src/interfaces/compound/IComet.sol";
import {MorphoBalances} from "../../../src/libraries/MorphoBalances.sol";
import {ProtocolIds} from "../../../src/libraries/ProtocolIds.sol";
import {FixedRateIrm, MockMorphoOracle} from "../../mocks/MorphoMocks.sol";

contract AaveV3LifecycleForkTest is LifecycleForkBase {
    AaveV3Module module;

    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
        if (!forked) return;
        _setUpLifecycle();
        module = new AaveV3Module(
            IAavePoolAddressesProvider(0xe20fCBdBfFC4Dd138cE8b2E6FBb6CB49777ad64D), BASE_USDC, BASE_WETH
        );
        _createAccount();
    }

    function _module() internal view override returns (address) {
        return address(module);
    }

    function _target() internal view override returns (address) {
        return address(module.pool());
    }

    function _protocolId() internal pure override returns (bytes32) {
        return ProtocolIds.AAVE_V3;
    }

    function _marketScope() internal view override returns (bytes32) {
        return module.marketScopeFor(BASE_USDC);
    }

    function _collateralToken() internal pure override returns (address) {
        return BASE_WETH;
    }

    function _supplyCollateral(uint256 a) internal pure override returns (bytes memory) {
        return abi.encode(AaveV3Module.Kind.SupplyCollateral, BASE_WETH, a);
    }

    function _borrow(uint256 a) internal pure override returns (bytes memory) {
        return abi.encode(AaveV3Module.Kind.Borrow, BASE_USDC, a);
    }

    function _repayAvailable() internal pure override returns (bytes memory) {
        return abi.encode(AaveV3Module.Kind.RepayAvailable, BASE_USDC, uint256(0));
    }

    function _repayAll() internal pure override returns (bytes memory) {
        return abi.encode(AaveV3Module.Kind.RepayAll, BASE_USDC, uint256(0));
    }

    function _withdrawAllCollateral() internal pure override returns (bytes memory) {
        return abi.encode(AaveV3Module.Kind.Withdraw, BASE_WETH, type(uint256).max);
    }

    function _debt() internal view override returns (uint256) {
        return module.variableDebtOf(address(account));
    }

    function _collateral() internal view override returns (uint256) {
        (address aWeth,,) = module.dataProvider().getReserveTokensAddresses(BASE_WETH);
        return IERC20(aWeth).balanceOf(address(account));
    }

    function test_fullLifecycle() public {
        _runLifecycle(5 ether, 1_000e6);
    }
}

contract MorphoBlueLifecycleForkTest is LifecycleForkBase {
    IMorphoBlue constant MORPHO = IMorphoBlue(0xBBBBBbbBBb9cC5e90e3b3Af64bdAF62C37EEFFCb);
    MorphoBlueModule module;
    MarketParams params;

    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
        if (!forked) return;
        _setUpLifecycle();
        MockMorphoOracle oracle = new MockMorphoOracle(3_000e24);
        FixedRateIrm irm = new FixedRateIrm(uint256(0.1e18) / 365 days);
        vm.startPrank(MORPHO.owner());
        MORPHO.enableIrm(address(irm));
        if (!MORPHO.isLltvEnabled(0.86e18)) MORPHO.enableLltv(0.86e18);
        vm.stopPrank();
        params = MarketParams(BASE_USDC, BASE_WETH, address(oracle), address(irm), 0.86e18);
        MORPHO.createMarket(params);
        address lender = makeAddr("lender");
        deal(BASE_USDC, lender, 100_000e6);
        vm.startPrank(lender);
        IERC20(BASE_USDC).approve(address(MORPHO), 100_000e6);
        MORPHO.supply(params, 100_000e6, 0, lender, "");
        vm.stopPrank();
        module = new MorphoBlueModule(MORPHO, params);
        _createAccount();
    }

    function _module() internal view override returns (address) {
        return address(module);
    }

    function _target() internal pure override returns (address) {
        return address(MORPHO);
    }

    function _protocolId() internal pure override returns (bytes32) {
        return ProtocolIds.MORPHO_BLUE;
    }

    function _marketScope() internal view override returns (bytes32) {
        return module.marketId();
    }

    function _collateralToken() internal pure override returns (address) {
        return BASE_WETH;
    }

    function _supplyCollateral(uint256 a) internal pure override returns (bytes memory) {
        return abi.encode(MorphoBlueModule.Kind.SupplyCollateral, a);
    }

    function _borrow(uint256 a) internal pure override returns (bytes memory) {
        return abi.encode(MorphoBlueModule.Kind.Borrow, a);
    }

    function _repayAvailable() internal pure override returns (bytes memory) {
        return abi.encode(MorphoBlueModule.Kind.RepayAvailable, uint256(0));
    }

    function _repayAll() internal pure override returns (bytes memory) {
        return abi.encode(MorphoBlueModule.Kind.RepayAll, uint256(0));
    }

    function _withdrawAllCollateral() internal pure override returns (bytes memory) {
        return abi.encode(MorphoBlueModule.Kind.WithdrawCollateral, type(uint256).max);
    }

    function _debt() internal view override returns (uint256) {
        return MorphoBalances.expectedBorrowAssets(MORPHO, params, address(account));
    }

    function _collateral() internal view override returns (uint256) {
        (,, uint128 collateral) = MORPHO.position(module.marketId(), address(account));
        return collateral;
    }

    function test_fullLifecycle() public {
        _runLifecycle(5 ether, 1_000e6);
    }
}

contract CompoundV3LifecycleForkTest is LifecycleForkBase {
    IComet constant COMET = IComet(0xb125E6687d4313864e53df431d5425969c15Eb2F);
    CompoundV3Module module;

    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
        if (!forked) return;
        _setUpLifecycle();
        module = new CompoundV3Module(COMET, BASE_WETH);
        _createAccount();
    }

    function _module() internal view override returns (address) {
        return address(module);
    }

    function _target() internal pure override returns (address) {
        return address(COMET);
    }

    function _protocolId() internal pure override returns (bytes32) {
        return ProtocolIds.COMPOUND_V3;
    }

    function _marketScope() internal view override returns (bytes32) {
        return module.marketScope();
    }

    function _collateralToken() internal pure override returns (address) {
        return BASE_WETH;
    }

    function _supplyCollateral(uint256 a) internal pure override returns (bytes memory) {
        return abi.encode(CompoundV3Module.Kind.SupplyCollateral, a);
    }

    function _borrow(uint256 a) internal pure override returns (bytes memory) {
        return abi.encode(CompoundV3Module.Kind.WithdrawBase, a);
    }

    function _repayAvailable() internal pure override returns (bytes memory) {
        return abi.encode(CompoundV3Module.Kind.RepayAvailable, uint256(0));
    }

    function _repayAll() internal pure override returns (bytes memory) {
        return abi.encode(CompoundV3Module.Kind.RepayAll, uint256(0));
    }

    function _withdrawAllCollateral() internal pure override returns (bytes memory) {
        return abi.encode(CompoundV3Module.Kind.WithdrawCollateral, type(uint256).max);
    }

    function _debt() internal view override returns (uint256) {
        return COMET.borrowBalanceOf(address(account));
    }

    function _collateral() internal view override returns (uint256) {
        return COMET.collateralBalanceOf(address(account), BASE_WETH);
    }

    function test_fullLifecycle() public {
        _runLifecycle(5 ether, 1_000e6);
    }
}
