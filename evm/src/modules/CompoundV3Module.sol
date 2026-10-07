// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IActionModule} from "../interfaces/IActionModule.sol";
import {IComet} from "../interfaces/compound/IComet.sol";
import {ProtocolIds} from "../libraries/ProtocolIds.sol";

/// @title CompoundV3Module
/// @notice Plans Compound III actions for one Comet (marketScope =
/// bytes32(comet)). The base position is a single signed balance: supplying
/// base repays debt first; withdrawing past the supply borrows and must stay
/// above `baseBorrowMin`. Collateral earns no interest.
contract CompoundV3Module is IActionModule {
    enum Kind {
        SupplyBase,
        WithdrawBase,
        SupplyCollateral,
        WithdrawCollateral,
        RepayAll,
        RepayAvailable
    }

    IComet public immutable comet;
    address public immutable baseToken;
    address public immutable collateralAsset;

    error WrongMarket();
    error UnsupportedCollateral(address asset);
    error BorrowTooSmall(uint256 borrow, uint256 minimum);
    error NoDebt();
    error ZeroAmount();

    constructor(IComet comet_, address collateralAsset_) {
        comet = comet_;
        baseToken = comet_.baseToken();
        collateralAsset = collateralAsset_;
        if (collateralAsset_ != address(0)) comet_.getAssetInfoByAddress(collateralAsset_);
    }

    function protocol() external pure returns (bytes32) {
        return ProtocolIds.COMPOUND_V3;
    }

    function marketScope() public view returns (bytes32) {
        return bytes32(uint256(uint160(address(comet))));
    }

    function isAllowedSelector(bytes4 s) external pure returns (bool) {
        return s == IComet.supply.selector || s == IComet.withdraw.selector;
    }

    function plan(address account, bytes32 scope, address, bytes calldata action)
        external
        view
        returns (Plan memory p)
    {
        if (scope != marketScope()) revert WrongMarket();
        (Kind kind, uint256 amount) = abi.decode(action, (Kind, uint256));
        uint256 supplied = comet.balanceOf(account);
        uint256 borrowed = comet.borrowBalanceOf(account);

        if (kind == Kind.SupplyBase) {
            _requireAmount(amount);
            p = _supply(baseToken, amount, amount);
            p.increasesRisk = borrowed == 0;
        } else if (kind == Kind.WithdrawBase) {
            _requireAmount(amount);
            if (amount > supplied) {
                uint256 newBorrow = borrowed + amount - supplied;
                uint256 minimum = comet.baseBorrowMin();
                if (newBorrow < minimum) revert BorrowTooSmall(newBorrow, minimum);
                p.increasesRisk = true;
            }
            p = _withdraw(baseToken, amount, p.increasesRisk);
        } else if (kind == Kind.SupplyCollateral) {
            _requireAmount(amount);
            _requireCollateral();
            p = _supply(collateralAsset, amount, amount);
            p.increasesRisk = borrowed == 0;
        } else if (kind == Kind.WithdrawCollateral) {
            _requireAmount(amount);
            _requireCollateral();
            uint256 assets = amount == type(uint256).max
                ? comet.collateralBalanceOf(account, collateralAsset)
                : amount;
            p = _withdraw(collateralAsset, assets, borrowed > 0);
        } else {
            if (borrowed == 0) revert NoDebt();
            uint256 pay = kind == Kind.RepayAll ? borrowed : IERC20(baseToken).balanceOf(account);
            _requireAmount(pay);
            p = pay >= borrowed
                ? _supply(baseToken, borrowed, type(uint256).max)
                : _supply(baseToken, pay, pay);
        }
    }

    function _supply(address asset, uint256 approval, uint256 amount)
        internal
        view
        returns (Plan memory p)
    {
        p.calls = new PlannedCall[](2);
        p.calls[0] = PlannedCall(asset, abi.encodeCall(IERC20.approve, (address(comet), approval)));
        p.calls[1] = PlannedCall(address(comet), abi.encodeCall(IComet.supply, (asset, amount)));
        p.approvalTokens = new address[](1);
        p.approvalTokens[0] = asset;
    }

    function _withdraw(address asset, uint256 amount, bool risky)
        internal
        view
        returns (Plan memory p)
    {
        p.calls = new PlannedCall[](1);
        p.calls[0] = PlannedCall(address(comet), abi.encodeCall(IComet.withdraw, (asset, amount)));
        p.increasesRisk = risky;
    }

    function _requireCollateral() internal view {
        if (collateralAsset == address(0)) revert UnsupportedCollateral(address(0));
    }

    function _requireAmount(uint256 amount) internal pure {
        if (amount == 0) revert ZeroAmount();
    }
}
