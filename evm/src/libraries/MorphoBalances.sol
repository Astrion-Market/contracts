// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {
    IMorphoBlue,
    IMorphoIrm,
    MarketParams,
    MorphoMarket
} from "../interfaces/morpho/IMorphoBlue.sol";

/// @notice Mirrors Morpho Blue's interest accrual and share math so a plan
/// can approve exactly what a share-based repay will pull in the same block.
library MorphoBalances {
    uint256 internal constant WAD = 1e18;
    uint256 internal constant VIRTUAL_SHARES = 1e6;
    uint256 internal constant VIRTUAL_ASSETS = 1;

    function id(MarketParams memory params) internal pure returns (bytes32) {
        return keccak256(abi.encode(params));
    }

    function marketOf(IMorphoBlue morpho, bytes32 marketId)
        internal
        view
        returns (MorphoMarket memory m)
    {
        (
            m.totalSupplyAssets,
            m.totalSupplyShares,
            m.totalBorrowAssets,
            m.totalBorrowShares,
            m.lastUpdate,
            m.fee
        ) = morpho.market(marketId);
    }

    function expectedMarket(IMorphoBlue morpho, MarketParams memory params)
        internal
        view
        returns (MorphoMarket memory m)
    {
        m = marketOf(morpho, id(params));
        uint256 elapsed = block.timestamp - m.lastUpdate;
        if (elapsed == 0 || params.irm == address(0) || m.totalBorrowAssets == 0) return m;
        uint256 rate = IMorphoIrm(params.irm).borrowRateView(params, m);
        uint256 interest = _wMulDown(m.totalBorrowAssets, _wTaylorCompounded(rate, elapsed));
        m.totalBorrowAssets += uint128(interest);
        m.totalSupplyAssets += uint128(interest);
        if (m.fee != 0) {
            uint256 feeAmount = _wMulDown(interest, m.fee);
            uint256 feeShares =
                toSharesDown(feeAmount, m.totalSupplyAssets - feeAmount, m.totalSupplyShares);
            m.totalSupplyShares += uint128(feeShares);
        }
        m.lastUpdate = uint128(block.timestamp);
    }

    function expectedBorrowAssets(IMorphoBlue morpho, MarketParams memory params, address user)
        internal
        view
        returns (uint256)
    {
        (, uint128 borrowShares,) = morpho.position(id(params), user);
        MorphoMarket memory m = expectedMarket(morpho, params);
        return toAssetsUp(borrowShares, m.totalBorrowAssets, m.totalBorrowShares);
    }

    function expectedSupplyAssets(IMorphoBlue morpho, MarketParams memory params, address user)
        internal
        view
        returns (uint256)
    {
        (uint256 supplyShares,,) = morpho.position(id(params), user);
        MorphoMarket memory m = expectedMarket(morpho, params);
        return toAssetsDown(supplyShares, m.totalSupplyAssets, m.totalSupplyShares);
    }

    function toAssetsUp(uint256 shares, uint256 totalAssets, uint256 totalShares)
        internal
        pure
        returns (uint256)
    {
        return Math.mulDiv(
            shares, totalAssets + VIRTUAL_ASSETS, totalShares + VIRTUAL_SHARES, Math.Rounding.Ceil
        );
    }

    function toAssetsDown(uint256 shares, uint256 totalAssets, uint256 totalShares)
        internal
        pure
        returns (uint256)
    {
        return Math.mulDiv(shares, totalAssets + VIRTUAL_ASSETS, totalShares + VIRTUAL_SHARES);
    }

    function toSharesDown(uint256 assets, uint256 totalAssets, uint256 totalShares)
        internal
        pure
        returns (uint256)
    {
        return Math.mulDiv(assets, totalShares + VIRTUAL_SHARES, totalAssets + VIRTUAL_ASSETS);
    }

    function _wMulDown(uint256 x, uint256 y) private pure returns (uint256) {
        return x * y / WAD;
    }

    function _wTaylorCompounded(uint256 x, uint256 n) private pure returns (uint256) {
        uint256 firstTerm = x * n;
        uint256 secondTerm = Math.mulDiv(firstTerm, firstTerm, 2 * WAD);
        uint256 thirdTerm = Math.mulDiv(secondTerm, firstTerm, 3 * WAD);
        return firstTerm + secondTerm + thirdTerm;
    }
}
