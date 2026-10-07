// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {StellarStrkey} from "../../src/libraries/StellarStrkey.sol";

contract StrkeyHarness {
    function validate(string calldata s) external pure returns (StellarStrkey.Kind) {
        return StellarStrkey.validate(s);
    }
}
