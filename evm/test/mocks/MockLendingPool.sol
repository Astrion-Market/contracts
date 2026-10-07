// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/// @notice Minimal Aave-like pool for unit tests: positions are keyed by the
/// account that owns them (`onBehalfOf` for supply/repay, msg.sender otherwise).
contract MockLendingPool {
    using SafeERC20 for IERC20;

    mapping(address asset => mapping(address user => uint256)) public supplied;
    mapping(address asset => mapping(address user => uint256)) public debt;
    bool public paused;

    error Paused();
    error Insufficient();

    function setPaused(bool p) external {
        paused = p;
    }

    function supply(address asset, uint256 amount, address onBehalfOf, uint16) external {
        if (paused) revert Paused();
        IERC20(asset).safeTransferFrom(msg.sender, address(this), amount);
        supplied[asset][onBehalfOf] += amount;
    }

    function withdraw(address asset, uint256 amount, address to) external returns (uint256) {
        if (supplied[asset][msg.sender] < amount) revert Insufficient();
        supplied[asset][msg.sender] -= amount;
        IERC20(asset).safeTransfer(to, amount);
        return amount;
    }

    function borrow(address asset, uint256 amount, uint256, uint16, address onBehalfOf) external {
        if (paused) revert Paused();
        require(onBehalfOf == msg.sender, "no delegation");
        debt[asset][onBehalfOf] += amount;
        IERC20(asset).safeTransfer(msg.sender, amount);
    }

    function repay(address asset, uint256 amount, uint256, address onBehalfOf)
        external
        returns (uint256)
    {
        uint256 owed = debt[asset][onBehalfOf];
        uint256 pay = amount > owed ? owed : amount;
        IERC20(asset).safeTransferFrom(msg.sender, address(this), pay);
        debt[asset][onBehalfOf] = owed - pay;
        return pay;
    }
}
