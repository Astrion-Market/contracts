// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {ForkTest} from "./ForkTest.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {AstrionAccountFactory} from "../../src/account/AstrionAccountFactory.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {IMessageTransmitterV2} from "../../src/interfaces/IMessageTransmitterV2.sol";

abstract contract AccountForkBase is ForkTest {
    address internal constant BASE_USDC = 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913;
    address internal constant BASE_WETH = 0x4200000000000000000000000000000000000006;
    address internal constant BASE_CBBTC = 0xcbB7C0000aB88B473b1f5aFd9ef808440eed33Bf;
    address internal constant BASE_TOKEN_MESSENGER_V2 = 0x28b5a0e9C621a5BadaA536219b3a228C8168cf5d;
    uint32 internal constant BASE_DOMAIN = 6;

    uint256 internal ownerPk = 0xA11CE;
    address internal owner;
    address internal relayer = makeAddr("relayer");
    RoutePolicy internal policy;
    AstrionAccountFactory internal factory;
    uint256 internal nextNonce = 1;

    function _setUpAccounts(address messageTransmitter, address usdc, uint32 domain) internal {
        owner = vm.addr(ownerPk);
        policy = new RoutePolicy(address(this));
        factory = new AstrionAccountFactory(
            policy, IMessageTransmitterV2(messageTransmitter), IERC20(usdc), domain
        );
    }

    function _account(bytes32 protocolId, bytes32 marketScope) internal returns (AstrionAccount) {
        return AstrionAccount(payable(factory.createAccount(owner, protocolId, marketScope, 1)));
    }

    function _intent(address module, address target, bytes memory action)
        internal
        returns (AstrionAccount.ExecutionIntent memory intent)
    {
        intent = AstrionAccount.ExecutionIntent({
            module: module,
            moduleCodeHash: module.codehash,
            target: target,
            actionHash: keccak256(action),
            recipient: owner,
            relayer: address(0),
            feeToken: BASE_USDC,
            maxFee: 0,
            nonce: nextNonce++,
            deadline: block.timestamp + 1 hours,
            transferId: bytes32(0)
        });
    }

    function _sign(AstrionAccount account, AstrionAccount.ExecutionIntent memory intent)
        internal
        view
        returns (bytes memory)
    {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, account.intentDigest(intent));
        return abi.encodePacked(r, s, v);
    }

    function _exec(AstrionAccount account, address module, address target, bytes memory action)
        internal
    {
        AstrionAccount.ExecutionIntent memory intent = _intent(module, target, action);
        bytes memory sig = _sign(account, intent);
        vm.prank(relayer);
        account.executeIntent(intent, action, sig, 0);
    }

    function _execExpectRevert(
        AstrionAccount account,
        address module,
        address target,
        bytes memory action
    ) internal {
        AstrionAccount.ExecutionIntent memory intent = _intent(module, target, action);
        bytes memory sig = _sign(account, intent);
        vm.prank(relayer);
        vm.expectRevert();
        account.executeIntent(intent, action, sig, 0);
    }
}
