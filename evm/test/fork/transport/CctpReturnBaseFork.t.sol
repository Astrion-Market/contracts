// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AccountForkBase} from "../AccountForkBase.sol";
import {AstrionAccount} from "../../../src/account/AstrionAccount.sol";
import {CctpReturnModule} from "../../../src/modules/CctpReturnModule.sol";
import {ITokenMessengerV2} from "../../../src/interfaces/ITokenMessengerV2.sol";
import {ProtocolIds} from "../../../src/libraries/ProtocolIds.sol";

interface ITokenMessengerV2Remote {
    function remoteTokenMessengers(uint32 domain) external view returns (bytes32);
}

/// @notice Real TokenMessengerV2 burn to Stellar. Skips (never passes) when
/// the pinned block predates Stellar's domain registration.
contract CctpReturnBaseForkTest is AccountForkBase {
    bytes32 constant FORWARDER = 0x72bd20ff2f8281801bb05b7c29179026933256fabafeb13e94efd8ddbcfcf291;
    string constant G = "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI";

    CctpReturnModule module;
    AstrionAccount account;

    function setUp() public {
        _forkOrSkip(BASE_CHAIN_ID);
        if (!forked) return;
        if (ITokenMessengerV2Remote(BASE_TOKEN_MESSENGER_V2).remoteTokenMessengers(27) == bytes32(0)) {
            emit log("SKIPPED: Stellar domain 27 not registered at this fork block");
            vm.skip(true);
            return;
        }
        _setUpAccounts(address(0), BASE_USDC, BASE_DOMAIN);
        module = new CctpReturnModule(
            ProtocolIds.AAVE_V3, ITokenMessengerV2(BASE_TOKEN_MESSENGER_V2), BASE_USDC, FORWARDER
        );
        account = _account(ProtocolIds.AAVE_V3, bytes32(uint256(uint160(BASE_USDC))));
    }

    function test_realBurnToStellarDebitsExactly() public {
        deal(BASE_USDC, address(account), 100e6);
        _exec(account, address(module), BASE_TOKEN_MESSENGER_V2, abi.encode(uint256(100e6), uint256(1e6), uint32(2000), G));
        assertEq(IERC20(BASE_USDC).balanceOf(address(account)), 0);
        assertEq(IERC20(BASE_USDC).allowance(address(account), BASE_TOKEN_MESSENGER_V2), 0);
    }
}
