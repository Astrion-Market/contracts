// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Ownable2Step, Ownable} from "@openzeppelin/contracts/access/Ownable2Step.sol";

/// @title RoutePolicy
/// @notice Application-policy pauses per route. A pause blocks NEW RISK
/// executed through signed intents only. It cannot move funds and cannot
/// block owner recovery, repayment or protocol-valid exits through the
/// account's direct `execute` path.
contract RoutePolicy is Ownable2Step {
    mapping(bytes32 routeId => bool) public isPaused;

    event RoutePaused(bytes32 indexed routeId, bool paused);

    constructor(address guardian) Ownable(guardian) {}

    /// @notice routeId = keccak256(abi.encode(chainId, protocol, marketScope)).
    function routeId(uint256 chainId, bytes32 protocol, bytes32 marketScope)
        public
        pure
        returns (bytes32)
    {
        return keccak256(abi.encode(chainId, protocol, marketScope));
    }

    function setPaused(bytes32 id, bool paused) external onlyOwner {
        isPaused[id] = paused;
        emit RoutePaused(id, paused);
    }
}
