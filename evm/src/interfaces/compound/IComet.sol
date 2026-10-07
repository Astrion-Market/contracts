// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

interface IComet {
    struct AssetInfo {
        uint8 offset;
        address asset;
        address priceFeed;
        uint64 scale;
        uint64 borrowCollateralFactor;
        uint64 liquidateCollateralFactor;
        uint64 liquidationFactor;
        uint128 supplyCap;
    }

    function supply(address asset, uint256 amount) external;
    function withdraw(address asset, uint256 amount) external;
    function baseToken() external view returns (address);
    function baseBorrowMin() external view returns (uint256);
    function balanceOf(address account) external view returns (uint256);
    function borrowBalanceOf(address account) external view returns (uint256);
    function collateralBalanceOf(address account, address asset) external view returns (uint128);
    function isBorrowCollateralized(address account) external view returns (bool);
    function isLiquidatable(address account) external view returns (bool);
    function isSupplyPaused() external view returns (bool);
    function isWithdrawPaused() external view returns (bool);
    function getUtilization() external view returns (uint256);
    function getAssetInfoByAddress(address asset) external view returns (AssetInfo memory);
    function totalsCollateral(address asset)
        external
        view
        returns (uint128 totalSupplyAsset, uint128 reserved);
    function pauseGuardian() external view returns (address);
    function pause(
        bool supplyPaused,
        bool transferPaused,
        bool withdrawPaused,
        bool absorbPaused,
        bool buyPaused
    ) external;
}
