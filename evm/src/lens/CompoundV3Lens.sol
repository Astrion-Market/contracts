// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IComet} from "../interfaces/compound/IComet.sol";

/// @title CompoundV3Lens
/// @notice Comet position and market state. Base supply and base debt are
/// mutually exclusive (one signed balance); collateral earns no interest.
contract CompoundV3Lens {
    struct Position {
        uint256 blockNumber;
        uint256 timestamp;
        uint256 baseSupplied;
        uint256 baseBorrowed;
        uint256 collateral;
        bool borrowCollateralized;
        bool liquidatable;
        uint256 baseBorrowMin;
    }

    struct MarketState {
        uint256 blockNumber;
        bool supplyPaused;
        bool withdrawPaused;
        uint256 utilization;
        uint256 borrowCollateralFactor;
        uint256 liquidateCollateralFactor;
        uint256 collateralSupplyCap;
        uint256 collateralTotalSupply;
    }

    function position(IComet comet, address account, address collateralAsset)
        external
        view
        returns (Position memory p)
    {
        p.blockNumber = block.number;
        p.timestamp = block.timestamp;
        p.baseSupplied = comet.balanceOf(account);
        p.baseBorrowed = comet.borrowBalanceOf(account);
        if (collateralAsset != address(0)) {
            p.collateral = comet.collateralBalanceOf(account, collateralAsset);
        }
        p.borrowCollateralized = comet.isBorrowCollateralized(account);
        p.liquidatable = comet.isLiquidatable(account);
        p.baseBorrowMin = comet.baseBorrowMin();
    }

    function market(IComet comet, address collateralAsset)
        external
        view
        returns (MarketState memory s)
    {
        s.blockNumber = block.number;
        s.supplyPaused = comet.isSupplyPaused();
        s.withdrawPaused = comet.isWithdrawPaused();
        s.utilization = comet.getUtilization();
        if (collateralAsset != address(0)) {
            IComet.AssetInfo memory info = comet.getAssetInfoByAddress(collateralAsset);
            s.borrowCollateralFactor = info.borrowCollateralFactor;
            s.liquidateCollateralFactor = info.liquidateCollateralFactor;
            s.collateralSupplyCap = info.supplyCap;
            (uint128 total,) = comet.totalsCollateral(collateralAsset);
            s.collateralTotalSupply = total;
        }
    }
}
