// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IActionModule} from "../interfaces/IActionModule.sol";
import {IMorphoBlue, MarketParams} from "../interfaces/morpho/IMorphoBlue.sol";
import {MorphoBalances} from "../libraries/MorphoBalances.sol";
import {ProtocolIds} from "../libraries/ProtocolIds.sol";

/// @title MorphoBlueModule
/// @notice Plans actions on exactly one Morpho Blue market, pinned by its
/// full parameters (immutables, covered by the module code hash). Loan supply
/// and collateral are separate actions; supplied loan tokens never count as
/// collateral. There is no vault method.
contract MorphoBlueModule is IActionModule {
    enum Kind {
        Supply,
        SupplyCollateral,
        Withdraw,
        WithdrawCollateral,
        Borrow,
        Repay,
        RepayAll,
        RepayAvailable
    }

    IMorphoBlue public immutable morpho;
    address public immutable loanToken;
    address public immutable collateralToken;
    address public immutable oracle;
    address public immutable irm;
    uint256 public immutable lltv;
    bytes32 public immutable marketId;

    error WrongMarket();
    error MarketNotCreated();
    error NoDebt();
    error ZeroAmount();

    constructor(IMorphoBlue morpho_, MarketParams memory params) {
        morpho = morpho_;
        loanToken = params.loanToken;
        collateralToken = params.collateralToken;
        oracle = params.oracle;
        irm = params.irm;
        lltv = params.lltv;
        marketId = MorphoBalances.id(params);
    }

    function protocol() external pure returns (bytes32) {
        return ProtocolIds.MORPHO_BLUE;
    }

    function marketParams() public view returns (MarketParams memory) {
        return MarketParams(loanToken, collateralToken, oracle, irm, lltv);
    }

    function isAllowedSelector(bytes4 s) external pure returns (bool) {
        return s == IMorphoBlue.supply.selector || s == IMorphoBlue.withdraw.selector
            || s == IMorphoBlue.borrow.selector || s == IMorphoBlue.repay.selector
            || s == IMorphoBlue.supplyCollateral.selector
            || s == IMorphoBlue.withdrawCollateral.selector;
    }

    function plan(address account, bytes32 marketScope, address, bytes calldata action)
        external
        view
        returns (Plan memory)
    {
        if (marketScope != marketId) revert WrongMarket();
        (,,,, uint128 lastUpdate,) = morpho.market(marketId);
        if (lastUpdate == 0) revert MarketNotCreated();
        (Kind kind, uint256 amount) = abi.decode(action, (Kind, uint256));
        if (kind == Kind.Supply || kind == Kind.SupplyCollateral || kind == Kind.Borrow) {
            return _planIncrease(kind, account, amount);
        }
        if (kind == Kind.Withdraw || kind == Kind.WithdrawCollateral) {
            return _planWithdraw(kind, account, amount);
        }
        return _planRepay(kind, account, amount);
    }

    function _planIncrease(Kind kind, address account, uint256 amount)
        internal
        view
        returns (Plan memory p)
    {
        _requireAmount(amount);
        MarketParams memory params = marketParams();
        if (kind == Kind.Supply) {
            p = _withApproval(
                loanToken,
                amount,
                abi.encodeCall(IMorphoBlue.supply, (params, amount, 0, account, ""))
            );
            p.increasesRisk = true;
        } else if (kind == Kind.SupplyCollateral) {
            (, uint128 borrowShares,) = morpho.position(marketId, account);
            p = _withApproval(
                collateralToken,
                amount,
                abi.encodeCall(IMorphoBlue.supplyCollateral, (params, amount, account, ""))
            );
            p.increasesRisk = borrowShares == 0;
        } else {
            p = _single(abi.encodeCall(IMorphoBlue.borrow, (params, amount, 0, account, account)));
            p.increasesRisk = true;
        }
    }

    function _planWithdraw(Kind kind, address account, uint256 amount)
        internal
        view
        returns (Plan memory p)
    {
        _requireAmount(amount);
        MarketParams memory params = marketParams();
        (uint256 supplyShares, uint128 borrowShares, uint128 collateral) =
            morpho.position(marketId, account);
        if (kind == Kind.Withdraw) {
            p = _single(
                amount == type(uint256).max
                    ? abi.encodeCall(
                        IMorphoBlue.withdraw, (params, 0, supplyShares, account, account)
                    )
                    : abi.encodeCall(IMorphoBlue.withdraw, (params, amount, 0, account, account))
            );
        } else {
            uint256 assets = amount == type(uint256).max ? collateral : amount;
            p = _single(
                abi.encodeCall(IMorphoBlue.withdrawCollateral, (params, assets, account, account))
            );
            p.increasesRisk = borrowShares > 0;
        }
    }

    function _planRepay(Kind kind, address account, uint256 amount)
        internal
        view
        returns (Plan memory p)
    {
        MarketParams memory params = marketParams();
        (, uint128 borrowShares,) = morpho.position(marketId, account);
        if (borrowShares == 0) revert NoDebt();
        uint256 debt = MorphoBalances.expectedBorrowAssets(morpho, params, account);
        uint256 pay = kind == Kind.RepayAll
            ? debt
            : kind == Kind.Repay ? amount : IERC20(loanToken).balanceOf(account);
        _requireAmount(pay);
        if (pay >= debt) {
            return _withApproval(
                loanToken,
                debt,
                abi.encodeCall(IMorphoBlue.repay, (params, 0, borrowShares, account, ""))
            );
        }
        return _withApproval(
            loanToken, pay, abi.encodeCall(IMorphoBlue.repay, (params, pay, 0, account, ""))
        );
    }

    function _withApproval(address token, uint256 amount, bytes memory call)
        internal
        view
        returns (Plan memory p)
    {
        p.calls = new PlannedCall[](2);
        p.calls[0] = PlannedCall(token, abi.encodeCall(IERC20.approve, (address(morpho), amount)));
        p.calls[1] = PlannedCall(address(morpho), call);
        p.approvalTokens = new address[](1);
        p.approvalTokens[0] = token;
    }

    function _single(bytes memory call) internal view returns (Plan memory p) {
        p.calls = new PlannedCall[](1);
        p.calls[0] = PlannedCall(address(morpho), call);
    }

    function _requireAmount(uint256 amount) internal pure {
        if (amount == 0) revert ZeroAmount();
    }
}
