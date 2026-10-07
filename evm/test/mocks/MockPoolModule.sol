// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IActionModule} from "../../src/interfaces/IActionModule.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockLendingPool} from "./MockLendingPool.sol";

/// @notice Honest module for MockLendingPool. Action = abi.encode(kind, amount).
contract MockPoolModule is IActionModule {
    uint8 internal constant SUPPLY = 0;
    uint8 internal constant BORROW = 1;
    uint8 internal constant REPAY = 2;
    uint8 internal constant WITHDRAW = 3;

    address public immutable pool;
    address public immutable asset;

    constructor(address pool_, address asset_) {
        pool = pool_;
        asset = asset_;
    }

    function protocol() external pure virtual returns (bytes32) {
        return ProtocolIds.AAVE_V3;
    }

    function isAllowedSelector(bytes4 s) external pure returns (bool) {
        return s == MockLendingPool.supply.selector || s == MockLendingPool.withdraw.selector
            || s == MockLendingPool.borrow.selector || s == MockLendingPool.repay.selector;
    }

    function plan(address account, bytes32, address recipient, bytes calldata action)
        external
        view
        virtual
        returns (Plan memory p)
    {
        (uint8 kind, uint256 amount) = abi.decode(action, (uint8, uint256));
        if (kind == SUPPLY) {
            p.calls = new PlannedCall[](2);
            p.calls[0] = PlannedCall(asset, abi.encodeCall(IERC20.approve, (pool, amount)));
            p.calls[1] = PlannedCall(
                pool, abi.encodeCall(MockLendingPool.supply, (asset, amount, account, 0))
            );
            p.approvalTokens = _one(asset);
            p.increasesRisk = true;
        } else if (kind == BORROW) {
            p.calls = new PlannedCall[](1);
            p.calls[0] = PlannedCall(
                pool, abi.encodeCall(MockLendingPool.borrow, (asset, amount, 2, 0, account))
            );
            p.increasesRisk = true;
        } else if (kind == REPAY) {
            p.calls = new PlannedCall[](2);
            p.calls[0] = PlannedCall(asset, abi.encodeCall(IERC20.approve, (pool, amount)));
            p.calls[1] = PlannedCall(
                pool, abi.encodeCall(MockLendingPool.repay, (asset, amount, 2, account))
            );
            p.approvalTokens = _one(asset);
        } else if (kind == WITHDRAW) {
            p.calls = new PlannedCall[](2);
            p.calls[0] = PlannedCall(
                pool, abi.encodeCall(MockLendingPool.withdraw, (asset, amount, account))
            );
            p.calls[1] = PlannedCall(asset, abi.encodeCall(IERC20.transfer, (recipient, amount)));
            p.approvalTokens = _one(asset);
        } else {
            revert("unknown action");
        }
    }

    function _one(address a) internal pure returns (address[] memory list) {
        list = new address[](1);
        list[0] = a;
    }
}

/// @notice Approves twice what it supplies (leftover allowance).
contract ExcessApproveModule is MockPoolModule {
    constructor(address pool_, address asset_) MockPoolModule(pool_, asset_) {}

    function plan(address account, bytes32, address, bytes calldata action)
        external
        view
        override
        returns (Plan memory p)
    {
        (, uint256 amount) = abi.decode(action, (uint8, uint256));
        p.calls = new PlannedCall[](2);
        p.calls[0] = PlannedCall(asset, abi.encodeCall(IERC20.approve, (pool, amount * 2)));
        p.calls[1] =
            PlannedCall(pool, abi.encodeCall(MockLendingPool.supply, (asset, amount, account, 0)));
        p.approvalTokens = _one(asset);
    }
}

/// @notice Routes the supply to a different contract than the signed target.
contract SubstituteTargetModule is MockPoolModule {
    address public immutable other;

    constructor(address pool_, address asset_, address other_) MockPoolModule(pool_, asset_) {
        other = other_;
    }

    function plan(address account, bytes32, address, bytes calldata action)
        external
        view
        override
        returns (Plan memory p)
    {
        (, uint256 amount) = abi.decode(action, (uint8, uint256));
        p.calls = new PlannedCall[](1);
        p.calls[0] =
            PlannedCall(other, abi.encodeCall(MockLendingPool.supply, (asset, amount, account, 0)));
    }
}

/// @notice Sends tokens to an address other than the signed recipient.
contract TransferOutModule is MockPoolModule {
    address public immutable thief;

    constructor(address pool_, address asset_, address thief_) MockPoolModule(pool_, asset_) {
        thief = thief_;
    }

    function plan(address, bytes32, address, bytes calldata action)
        external
        view
        override
        returns (Plan memory p)
    {
        (, uint256 amount) = abi.decode(action, (uint8, uint256));
        p.calls = new PlannedCall[](1);
        p.calls[0] = PlannedCall(asset, abi.encodeCall(IERC20.transfer, (thief, amount)));
        p.approvalTokens = _one(asset);
    }
}

/// @notice Tries to call the account itself (storage / ownership corruption).
contract SelfCallModule is MockPoolModule {
    constructor(address pool_, address asset_) MockPoolModule(pool_, asset_) {}

    function plan(address account, bytes32, address, bytes calldata)
        external
        pure
        override
        returns (Plan memory p)
    {
        p.calls = new PlannedCall[](1);
        p.calls[0] = PlannedCall(account, abi.encodeWithSignature("revokeNonce(uint256)", 999));
    }
}

/// @notice Claims a different protocol than the account's scope.
contract WrongProtocolModule is MockPoolModule {
    constructor(address pool_, address asset_) MockPoolModule(pool_, asset_) {}

    function protocol() external pure override returns (bytes32) {
        return ProtocolIds.MORPHO_BLUE;
    }
}
