// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

/// @notice A protocol action module PLANS calls; the account executes them in
/// its own context with a plain CALL (never delegatecall).
///
/// Modules must be stateless: configuration lives in immutables so that the
/// owner-signed `moduleCodeHash` (EXTCODEHASH) pins both logic and config.
interface IActionModule {
    struct PlannedCall {
        address target;
        bytes data;
    }

    struct Plan {
        PlannedCall[] calls;
        /// Tokens the plan approves to the intent target; each allowance must
        /// be fully consumed (zero) after execution.
        address[] approvalTokens;
        /// True when the action adds protocol risk (borrow, collateral
        /// withdrawal, new supply). Route pauses block only these.
        bool increasesRisk;
    }

    /// @notice Protocol id this module serves (ProtocolIds).
    function protocol() external view returns (bytes32);

    /// @notice Whether `selector` may be called on the intent target.
    function isAllowedSelector(bytes4 selector) external view returns (bool);

    /// @notice Translate an owner-signed action into calls.
    function plan(address account, bytes32 marketScope, address recipient, bytes calldata action)
        external
        view
        returns (Plan memory);
}
