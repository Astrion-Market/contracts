// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

interface IAavePoolAddressesProvider {
    function getPool() external view returns (address);
    function getPoolDataProvider() external view returns (address);
    function getPoolConfigurator() external view returns (address);
    function getACLManager() external view returns (address);
    function getACLAdmin() external view returns (address);
}

interface IAavePool {
    function supply(address asset, uint256 amount, address onBehalfOf, uint16 referralCode) external;
    function withdraw(address asset, uint256 amount, address to) external returns (uint256);
    function borrow(
        address asset,
        uint256 amount,
        uint256 interestRateMode,
        uint16 referralCode,
        address onBehalfOf
    ) external;
    function repay(address asset, uint256 amount, uint256 interestRateMode, address onBehalfOf)
        external
        returns (uint256);
    function setUserUseReserveAsCollateral(address asset, bool useAsCollateral) external;
    function setUserEMode(uint8 categoryId) external;
    function getUserEMode(address user) external view returns (uint256);
    function getUserAccountData(address user)
        external
        view
        returns (
            uint256 totalCollateralBase,
            uint256 totalDebtBase,
            uint256 availableBorrowsBase,
            uint256 currentLiquidationThreshold,
            uint256 ltv,
            uint256 healthFactor
        );
}

interface IAavePoolDataProvider {
    function getReserveTokensAddresses(address asset)
        external
        view
        returns (
            address aTokenAddress,
            address stableDebtTokenAddress,
            address variableDebtTokenAddress
        );
    function getReserveConfigurationData(address asset)
        external
        view
        returns (
            uint256 decimals,
            uint256 ltv,
            uint256 liquidationThreshold,
            uint256 liquidationBonus,
            uint256 reserveFactor,
            bool usageAsCollateralEnabled,
            bool borrowingEnabled,
            bool stableBorrowRateEnabled,
            bool isActive,
            bool isFrozen
        );
    function getPaused(address asset) external view returns (bool);
    function getReserveCaps(address asset)
        external
        view
        returns (uint256 borrowCap, uint256 supplyCap);
    function getDebtCeiling(address asset) external view returns (uint256);
}

interface IAavePoolConfigurator {
    function setReserveFreeze(address asset, bool freeze) external;
    function setReservePause(address asset, bool paused) external;
    function setSupplyCap(address asset, uint256 newSupplyCap) external;
    function setBorrowCap(address asset, uint256 newBorrowCap) external;
}

interface IAaveACLManager {
    function addPoolAdmin(address admin) external;
    function addRiskAdmin(address admin) external;
    function addEmergencyAdmin(address admin) external;
}
