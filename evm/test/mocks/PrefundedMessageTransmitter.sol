// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";
import {CctpMessageV2} from "../../src/libraries/CctpMessageV2.sol";

/// @notice Fork stand-in for the inbound leg: pays real USDC it was funded
/// with, enforcing destinationCaller and nonce reuse like CCTP.
contract PrefundedMessageTransmitter is IMessageTransmitterV2 {
    IERC20 public immutable usdc;
    mapping(bytes32 => uint256) public usedNonces;

    constructor(IERC20 usdc_) {
        usdc = usdc_;
    }

    function receiveMessage(bytes calldata message, bytes calldata) external returns (bool) {
        CctpMessageV2.Burn memory b = CctpMessageV2.decode(message);
        require(b.destinationCaller == bytes32(uint256(uint160(msg.sender))), "caller");
        require(usedNonces[b.nonce] == 0, "nonce used");
        usedNonces[b.nonce] = 1;
        usdc.transfer(address(uint160(uint256(b.mintRecipient))), b.amount - b.feeExecuted);
        return true;
    }
}
