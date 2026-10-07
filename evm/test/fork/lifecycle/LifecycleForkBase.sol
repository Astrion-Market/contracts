// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AccountForkBase} from "../AccountForkBase.sol";
import {AstrionAccount} from "../../../src/account/AstrionAccount.sol";
import {CctpReturnModule} from "../../../src/modules/CctpReturnModule.sol";
import {PrefundedMessageTransmitter} from "../../mocks/PrefundedMessageTransmitter.sol";
import {MockTokenMessengerV2} from "../../mocks/MockTokenMessengerV2.sol";
import {CctpMessages} from "../../utils/CctpMessages.sol";

/// @notice Full lifecycle against real protocol bytecode on a Base fork:
/// collateral already on EVM -> borrow -> proceeds burned to Stellar ->
/// interest accrues -> Stellar-funded repayment -> payoff -> collateral back
/// -> residual USDC returned. Inbound mints come from a prefunded transmitter
/// stand-in; the return leg records burns. Both CCTP legs are proven
/// separately on testnet (C07); this suite proves lending behaviour.
abstract contract LifecycleForkBase is AccountForkBase {
    uint32 internal constant STELLAR_DOMAIN = 27;
    bytes32 internal constant FORWARDER =
        0x72bd20ff2f8281801bb05b7c29179026933256fabafeb13e94efd8ddbcfcf291;
    string internal constant STELLAR_RECIPIENT =
        "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI";

    PrefundedMessageTransmitter internal transmitter;
    MockTokenMessengerV2 internal messenger;
    CctpReturnModule internal returnModule;
    AstrionAccount internal account;
    uint256 internal cctpNonce = 1;

    function _module() internal view virtual returns (address);
    function _target() internal view virtual returns (address);
    function _protocolId() internal view virtual returns (bytes32);
    function _marketScope() internal view virtual returns (bytes32);
    function _collateralToken() internal view virtual returns (address);
    function _supplyCollateral(uint256 amount) internal view virtual returns (bytes memory);
    function _borrow(uint256 amount) internal view virtual returns (bytes memory);
    function _repayAvailable() internal view virtual returns (bytes memory);
    function _repayAll() internal view virtual returns (bytes memory);
    function _withdrawAllCollateral() internal view virtual returns (bytes memory);
    function _debt() internal view virtual returns (uint256);
    function _collateral() internal view virtual returns (uint256);

    function _setUpLifecycle() internal {
        transmitter = new PrefundedMessageTransmitter(IERC20(BASE_USDC));
        deal(BASE_USDC, address(transmitter), 10_000_000e6);
        _setUpAccounts(address(transmitter), BASE_USDC, BASE_DOMAIN);
        messenger = new MockTokenMessengerV2();
        returnModule = new CctpReturnModule(_protocolId(), messenger, BASE_USDC, FORWARDER);
    }

    function _createAccount() internal {
        account = _account(_protocolId(), _marketScope());
    }

    function _act(bytes memory action) internal {
        _exec(account, _module(), _target(), action);
    }

    function _returnToStellar(uint256 amount) internal {
        _exec(
            account,
            address(returnModule),
            address(messenger),
            abi.encode(amount, uint256(0), uint32(2000), STELLAR_RECIPIENT)
        );
    }

    function _fundedAct(uint256 inbound, bytes memory action) internal returns (bool executed) {
        bytes32 nonce = bytes32(cctpNonce++);
        bytes memory message =
            CctpMessages.burn(STELLAR_DOMAIN, BASE_DOMAIN, nonce, address(account), inbound, 0);
        AstrionAccount.ExecutionIntent memory intent = _intent(_module(), _target(), action);
        intent.transferId = account.transferIdOf(STELLAR_DOMAIN, nonce);
        bytes memory sig = _sign(account, intent);
        vm.prank(relayer);
        executed = account.executeFundedIntent(intent, action, sig, 0, message, "");
    }

    function _runLifecycle(uint256 collateral, uint256 borrowAmount) internal {
        IERC20 usdc = IERC20(BASE_USDC);
        deal(_collateralToken(), address(account), collateral);
        _act(_supplyCollateral(collateral));
        assertApproxEqAbs(_collateral(), collateral, 1, "collateral posted");

        _act(_borrow(borrowAmount));
        assertEq(usdc.balanceOf(address(account)), borrowAmount, "borrow proceeds");
        _returnToStellar(borrowAmount);
        assertEq(usdc.balanceOf(address(account)), 0);
        assertEq(messenger.lastBurn().amount, borrowAmount);
        assertGe(_debt(), borrowAmount, "debt visible while proceeds travel");

        vm.warp(block.timestamp + 30 days);
        uint256 accrued = _debt();
        assertGt(accrued, borrowAmount, "interest accrued during transit");

        assertTrue(_fundedAct(borrowAmount, _repayAvailable()), "repay principal");
        uint256 residual = _debt();
        assertGt(residual, 0, "debt not silently marked closed");

        uint256 topUp = residual + 25e6;
        assertTrue(_fundedAct(topUp, _repayAll()), "pay off");
        assertEq(_debt(), 0, "debt closed");
        assertEq(usdc.allowance(address(account), _target()), 0);

        _act(_withdrawAllCollateral());
        assertApproxEqAbs(IERC20(_collateralToken()).balanceOf(address(account)), collateral, 1);

        uint256 leftover = usdc.balanceOf(address(account));
        assertGt(leftover, 0);
        _returnToStellar(leftover);
        assertEq(usdc.balanceOf(address(account)), 0, "residual USDC returned");
    }
}
