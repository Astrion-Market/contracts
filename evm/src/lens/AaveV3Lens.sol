// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {
    IAavePool,
    IAavePoolAddressesProvider,
    IAavePoolDataProvider
} from "../interfaces/aave/IAaveV3.sol";

/// @title AaveV3Lens
/// @notice Read-only position and reserve risk state, stamped with the block
/// it was read at. Values are Aave's own; nothing is combined across protocols.
contract AaveV3Lens {
    struct Position {
        uint256 blockNumber;
        uint256 timestamp;
        uint256 totalCollateralBase;
        uint256 totalDebtBase;
        uint256 availableBorrowsBase;
        uint256 liquidationThreshold;
        uint256 ltv;
        uint256 healthFactor;
        uint256 loanSupplied;
        uint256 loanVariableDebt;
        uint256 collateralSupplied;
        uint256 eModeCategory;
    }

    struct ReserveState {
        bool isActive;
        bool isFrozen;
        bool isPaused;
        bool borrowingEnabled;
        bool usageAsCollateralEnabled;
        uint256 ltv;
        uint256 liquidationThreshold;
        uint256 supplyCap;
        uint256 borrowCap;
        uint256 debtCeiling;
    }

    IAavePool public immutable pool;
    IAavePoolDataProvider public immutable dataProvider;

    constructor(IAavePoolAddressesProvider provider) {
        pool = IAavePool(provider.getPool());
        dataProvider = IAavePoolDataProvider(provider.getPoolDataProvider());
    }

    function position(address account, address loanAsset, address collateralAsset)
        external
        view
        returns (Position memory p)
    {
        p.blockNumber = block.number;
        p.timestamp = block.timestamp;
        (
            p.totalCollateralBase,
            p.totalDebtBase,
            p.availableBorrowsBase,
            p.liquidationThreshold,
            p.ltv,
            p.healthFactor
        ) = pool.getUserAccountData(account);
        (address aLoan,, address vLoan) = dataProvider.getReserveTokensAddresses(loanAsset);
        p.loanSupplied = IERC20(aLoan).balanceOf(account);
        p.loanVariableDebt = IERC20(vLoan).balanceOf(account);
        if (collateralAsset != address(0)) {
            (address aCol,,) = dataProvider.getReserveTokensAddresses(collateralAsset);
            p.collateralSupplied = IERC20(aCol).balanceOf(account);
        }
        p.eModeCategory = pool.getUserEMode(account);
    }

    function reserve(address asset) external view returns (ReserveState memory r) {
        (
            ,
            r.ltv,
            r.liquidationThreshold,,,
            r.usageAsCollateralEnabled,
            r.borrowingEnabled,,
            r.isActive,
            r.isFrozen
        ) = dataProvider.getReserveConfigurationData(asset);
        r.isPaused = dataProvider.getPaused(asset);
        (r.borrowCap, r.supplyCap) = dataProvider.getReserveCaps(asset);
        r.debtCeiling = dataProvider.getDebtCeiling(asset);
    }
}
