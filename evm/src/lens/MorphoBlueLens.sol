// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {
    IMorphoBlue,
    IMorphoOracle,
    MarketParams,
    MorphoMarket
} from "../interfaces/morpho/IMorphoBlue.sol";
import {MorphoBalances} from "../libraries/MorphoBalances.sol";

/// @title MorphoBlueLens
/// @notice Position and market state for one Morpho Blue market, using the
/// market's own oracle and LLTV. An oracle failure is reported, not hidden.
contract MorphoBlueLens {
    uint256 internal constant ORACLE_PRICE_SCALE = 1e36;
    uint256 internal constant WAD = 1e18;

    struct Position {
        uint256 blockNumber;
        uint256 timestamp;
        uint256 supplyShares;
        uint256 borrowShares;
        uint256 collateral;
        uint256 supplyAssets;
        uint256 borrowAssets;
        bool oracleOk;
        uint256 oraclePrice;
        uint256 maxBorrow;
        uint256 lltv;
        bool healthy;
    }

    struct MarketState {
        uint256 blockNumber;
        uint256 totalSupplyAssets;
        uint256 totalBorrowAssets;
        uint256 liquidity;
        uint256 utilizationWad;
        uint256 fee;
    }

    IMorphoBlue public immutable morpho;

    constructor(IMorphoBlue morpho_) {
        morpho = morpho_;
    }

    function position(MarketParams memory params, address account)
        external
        view
        returns (Position memory p)
    {
        bytes32 marketId = MorphoBalances.id(params);
        p.blockNumber = block.number;
        p.timestamp = block.timestamp;
        (uint256 supplyShares, uint128 borrowShares, uint128 collateral) =
            morpho.position(marketId, account);
        p.supplyShares = supplyShares;
        p.borrowShares = borrowShares;
        p.collateral = collateral;
        MorphoMarket memory m = MorphoBalances.expectedMarket(morpho, params);
        p.supplyAssets =
            MorphoBalances.toAssetsDown(supplyShares, m.totalSupplyAssets, m.totalSupplyShares);
        p.borrowAssets =
            MorphoBalances.toAssetsUp(borrowShares, m.totalBorrowAssets, m.totalBorrowShares);
        p.lltv = params.lltv;
        try IMorphoOracle(params.oracle).price() returns (uint256 price) {
            p.oracleOk = true;
            p.oraclePrice = price;
            uint256 collateralValue = Math.mulDiv(collateral, price, ORACLE_PRICE_SCALE);
            p.maxBorrow = Math.mulDiv(collateralValue, params.lltv, WAD);
            p.healthy = p.borrowAssets <= p.maxBorrow;
        } catch {
            p.healthy = p.borrowAssets == 0;
        }
    }

    function market(MarketParams memory params) external view returns (MarketState memory s) {
        MorphoMarket memory m = MorphoBalances.expectedMarket(morpho, params);
        s.blockNumber = block.number;
        s.totalSupplyAssets = m.totalSupplyAssets;
        s.totalBorrowAssets = m.totalBorrowAssets;
        s.liquidity = m.totalSupplyAssets - m.totalBorrowAssets;
        s.utilizationWad = m.totalSupplyAssets == 0
            ? 0
            : Math.mulDiv(m.totalBorrowAssets, WAD, m.totalSupplyAssets);
        s.fee = m.fee;
    }
}
