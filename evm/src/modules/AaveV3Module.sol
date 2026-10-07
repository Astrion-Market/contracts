// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IActionModule} from "../interfaces/IActionModule.sol";
import {
    IAavePool,
    IAavePoolAddressesProvider,
    IAavePoolDataProvider
} from "../interfaces/aave/IAaveV3.sol";
import {ProtocolIds} from "../libraries/ProtocolIds.sol";

/// @title AaveV3Module
/// @notice Plans Aave V3 actions for an account scoped to one loan reserve
/// (marketScope = bytes32(loanAsset)) with at most one collateral reserve.
/// Supported settings are pinned: eMode category 0, no isolation-mode
/// collateral, variable-rate debt only.
contract AaveV3Module is IActionModule {
    enum Kind {
        Supply,
        SupplyCollateral,
        Withdraw,
        Borrow,
        Repay,
        RepayAll,
        RepayAvailable
    }

    uint256 internal constant VARIABLE_RATE = 2;

    IAavePool public immutable pool;
    IAavePoolDataProvider public immutable dataProvider;
    address public immutable loanAsset;
    address public immutable collateralAsset;

    error WrongMarket();
    error UnsupportedAsset(address asset);
    error UnsupportedEMode(uint256 category);
    error IsolationModeUnsupported(address asset);
    error NoDebt();
    error ZeroAmount();

    constructor(IAavePoolAddressesProvider provider, address loanAsset_, address collateralAsset_) {
        pool = IAavePool(provider.getPool());
        dataProvider = IAavePoolDataProvider(provider.getPoolDataProvider());
        loanAsset = loanAsset_;
        collateralAsset = collateralAsset_;
    }

    function protocol() external pure returns (bytes32) {
        return ProtocolIds.AAVE_V3;
    }

    function marketScopeFor(address asset) public pure returns (bytes32) {
        return bytes32(uint256(uint160(asset)));
    }

    function isAllowedSelector(bytes4 s) external pure returns (bool) {
        return s == IAavePool.supply.selector || s == IAavePool.withdraw.selector
            || s == IAavePool.borrow.selector || s == IAavePool.repay.selector
            || s == IAavePool.setUserUseReserveAsCollateral.selector;
    }

    function plan(address account, bytes32 marketScope, address, bytes calldata action)
        external
        view
        returns (Plan memory)
    {
        if (marketScope != marketScopeFor(loanAsset)) revert WrongMarket();
        (Kind kind, address asset, uint256 amount) = abi.decode(action, (Kind, address, uint256));
        _checkAsset(kind, asset);
        uint256 eMode = pool.getUserEMode(account);
        if (eMode != 0) revert UnsupportedEMode(eMode);
        (, uint256 debtBase,,,,) = pool.getUserAccountData(account);

        if (kind == Kind.Supply || kind == Kind.SupplyCollateral) {
            return _planSupply(account, asset, amount, kind == Kind.SupplyCollateral, debtBase);
        }
        if (kind == Kind.Withdraw) return _planWithdraw(account, asset, amount, debtBase);
        if (kind == Kind.Borrow) return _planBorrow(account, asset, amount);
        return _planRepay(kind, account, asset, amount);
    }

    function _planSupply(
        address account,
        address asset,
        uint256 amount,
        bool collateral,
        uint256 debtBase
    ) internal view returns (Plan memory p) {
        if (amount == 0) revert ZeroAmount();
        if (collateral && dataProvider.getDebtCeiling(asset) != 0) {
            revert IsolationModeUnsupported(asset);
        }
        p.calls = new PlannedCall[](collateral ? 3 : 2);
        p.calls[0] = PlannedCall(asset, abi.encodeCall(IERC20.approve, (address(pool), amount)));
        p.calls[1] = PlannedCall(
            address(pool), abi.encodeCall(IAavePool.supply, (asset, amount, account, 0))
        );
        if (collateral) {
            p.calls[2] = PlannedCall(
                address(pool),
                abi.encodeCall(IAavePool.setUserUseReserveAsCollateral, (asset, true))
            );
        }
        p.approvalTokens = _one(asset);
        p.increasesRisk = collateral ? debtBase == 0 : true;
    }

    function _planWithdraw(address account, address asset, uint256 amount, uint256 debtBase)
        internal
        view
        returns (Plan memory p)
    {
        if (amount == 0) revert ZeroAmount();
        p.calls = new PlannedCall[](1);
        p.calls[0] = PlannedCall(
            address(pool), abi.encodeCall(IAavePool.withdraw, (asset, amount, account))
        );
        p.increasesRisk = debtBase > 0;
    }

    function _planBorrow(address account, address asset, uint256 amount)
        internal
        view
        returns (Plan memory p)
    {
        if (amount == 0) revert ZeroAmount();
        p.calls = new PlannedCall[](1);
        p.calls[0] = PlannedCall(
            address(pool),
            abi.encodeCall(IAavePool.borrow, (asset, amount, VARIABLE_RATE, 0, account))
        );
        p.increasesRisk = true;
    }

    function _planRepay(Kind kind, address account, address asset, uint256 amount)
        internal
        view
        returns (Plan memory p)
    {
        uint256 debt = variableDebtOf(account);
        if (debt == 0) revert NoDebt();
        uint256 pay;
        uint256 repayArg;
        if (kind == Kind.Repay) {
            if (amount == 0) revert ZeroAmount();
            pay = amount < debt ? amount : debt;
            repayArg = pay;
        } else if (kind == Kind.RepayAll) {
            pay = debt;
            repayArg = type(uint256).max;
        } else {
            uint256 balance = IERC20(asset).balanceOf(account);
            pay = balance < debt ? balance : debt;
            if (pay == 0) revert ZeroAmount();
            repayArg = pay;
        }
        p.calls = new PlannedCall[](2);
        p.calls[0] = PlannedCall(asset, abi.encodeCall(IERC20.approve, (address(pool), pay)));
        p.calls[1] = PlannedCall(
            address(pool),
            abi.encodeCall(IAavePool.repay, (asset, repayArg, VARIABLE_RATE, account))
        );
        p.approvalTokens = _one(asset);
    }

    function variableDebtOf(address account) public view returns (uint256) {
        (,, address variableDebt) = dataProvider.getReserveTokensAddresses(loanAsset);
        return IERC20(variableDebt).balanceOf(account);
    }

    function _checkAsset(Kind kind, address asset) internal view {
        bool isLoan = asset == loanAsset;
        bool isCollateral = asset == collateralAsset && asset != address(0);
        bool ok = kind == Kind.SupplyCollateral
            ? isCollateral
            : kind == Kind.Withdraw ? (isLoan || isCollateral) : isLoan;
        if (!ok) revert UnsupportedAsset(asset);
    }

    function _one(address a) internal pure returns (address[] memory list) {
        list = new address[](1);
        list[0] = a;
    }
}
