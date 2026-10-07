// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {MarketParams, MorphoMarket} from "../../src/interfaces/morpho/IMorphoBlue.sol";

contract MockMorphoOracle {
    uint256 public storedPrice;
    bool public broken;

    constructor(uint256 price_) {
        storedPrice = price_;
    }

    function setPrice(uint256 p) external {
        storedPrice = p;
    }

    function setBroken(bool b) external {
        broken = b;
    }

    function price() external view returns (uint256) {
        require(!broken, "oracle down");
        return storedPrice;
    }
}

contract FixedRateIrm {
    uint256 public immutable ratePerSecond;

    constructor(uint256 ratePerSecond_) {
        ratePerSecond = ratePerSecond_;
    }

    function borrowRate(MarketParams memory, MorphoMarket memory) external view returns (uint256) {
        return ratePerSecond;
    }

    function borrowRateView(MarketParams memory, MorphoMarket memory)
        external
        view
        returns (uint256)
    {
        return ratePerSecond;
    }
}
