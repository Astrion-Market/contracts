// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {AstrionAccount} from "../../src/account/AstrionAccount.sol";
import {RoutePolicy} from "../../src/account/RoutePolicy.sol";
import {CctpReturnModule} from "../../src/modules/CctpReturnModule.sol";
import {ProtocolIds} from "../../src/libraries/ProtocolIds.sol";
import {MockERC20} from "../mocks/MockERC20.sol";
import {MockLendingPool} from "../mocks/MockLendingPool.sol";
import {MockPoolModule} from "../mocks/MockPoolModule.sol";
import {MockTokenMessengerV2} from "../mocks/MockTokenMessengerV2.sol";
import {CctpMessages} from "../utils/CctpMessages.sol";

contract AccountHandler is Test {
    struct Inbound {
        bytes message;
        bytes32 transferId;
    }

    uint8 constant SUPPLY = 0;
    uint8 constant BORROW = 1;
    uint8 constant REPAY = 2;
    uint8 constant WITHDRAW = 3;
    string constant G = "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI";

    AstrionAccount public immutable account;
    MockERC20 public immutable usdc;
    MockLendingPool public immutable pool;
    MockPoolModule public immutable module;
    MockTokenMessengerV2 public immutable messenger;
    CctpReturnModule public immutable returnModule;
    RoutePolicy public immutable policy;
    uint256 internal immutable ownerPk;
    address public immutable owner;
    address public immutable attacker;
    address public immutable relayer;
    address internal immutable guardian;
    bytes internal attestation = "valid-attestation";

    Inbound[] internal inbounds;
    uint256 internal cctpNonce = 1;
    uint256 internal intentNonce = 1;

    uint256 public minted;
    uint256 public burned;
    uint256 public ownerOut;
    uint256 public fees;
    uint256 public borrowed;
    uint256 public repaid;
    uint256 public fundedExecuted;
    string public violation;

    constructor(
        AstrionAccount account_,
        MockERC20 usdc_,
        MockLendingPool pool_,
        MockTokenMessengerV2 messenger_,
        RoutePolicy policy_,
        uint256 ownerPk_,
        address guardian_
    ) {
        account = account_;
        usdc = usdc_;
        pool = pool_;
        messenger = messenger_;
        policy = policy_;
        ownerPk = ownerPk_;
        owner = vm.addr(ownerPk_);
        guardian = guardian_;
        attacker = makeAddr("attacker");
        relayer = makeAddr("relayer");
        module = new MockPoolModule(address(pool_), address(usdc_));
        returnModule = new CctpReturnModule(
            ProtocolIds.AAVE_V3, messenger_, address(usdc_), bytes32(uint256(1))
        );
    }

    function inboundCount() external view returns (uint256) {
        return inbounds.length;
    }

    function inboundId(uint256 i) external view returns (bytes32) {
        return inbounds[i].transferId;
    }

    function _newInbound(uint256 amountSeed, uint256 feeSeed)
        internal
        returns (Inbound memory ib, uint256 net)
    {
        uint256 amount = bound(amountSeed, 1e6, 1000e6);
        uint256 fee = bound(feeSeed, 0, amount / 100);
        bytes32 n = bytes32(cctpNonce++);
        ib = Inbound(
            CctpMessages.burn(27, 6, n, address(account), amount, fee), account.transferIdOf(27, n)
        );
        net = amount - fee;
    }

    function _intent(
        address mod,
        address target,
        bytes memory action,
        bytes32 transferId,
        uint256 maxFee
    ) internal returns (AstrionAccount.ExecutionIntent memory i) {
        i = AstrionAccount.ExecutionIntent({
            module: mod,
            moduleCodeHash: mod.codehash,
            target: target,
            actionHash: keccak256(action),
            recipient: owner,
            relayer: address(0),
            feeToken: address(usdc),
            maxFee: maxFee,
            nonce: intentNonce++,
            deadline: block.timestamp + 1 hours,
            transferId: transferId
        });
    }

    function _sign(AstrionAccount.ExecutionIntent memory i, uint256 pk)
        internal
        view
        returns (bytes memory)
    {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, account.intentDigest(i));
        return abi.encodePacked(r, s, v);
    }

    function _relay(bytes memory action) internal returns (bool ok) {
        AstrionAccount.ExecutionIntent memory i =
            _intent(address(module), address(pool), action, 0, 0);
        bytes memory sig = _sign(i, ownerPk);
        vm.prank(relayer);
        try account.executeIntent(i, action, sig, 0) {
            ok = true;
        } catch {}
    }

    function receiveTransfer(uint256 amountSeed, uint256 feeSeed, address caller) external {
        (Inbound memory ib, uint256 net) = _newInbound(amountSeed, feeSeed);
        vm.prank(caller);
        account.receiveTransfer(ib.message, attestation);
        inbounds.push(ib);
        minted += net;
    }

    function duplicateDelivery(uint256 idx, address caller) external {
        if (inbounds.length == 0) return;
        Inbound memory ib = inbounds[bound(idx, 0, inbounds.length - 1)];
        vm.prank(caller);
        try account.receiveTransfer(ib.message, attestation) {
            violation = "duplicate receipt accepted";
        } catch {}
    }

    function fundedSupply(
        uint256 amountSeed,
        uint256 feeSeed,
        uint256 supplySeed,
        uint256 relayerFeeSeed
    ) external {
        (Inbound memory ib, uint256 net) = _newInbound(amountSeed, feeSeed);
        uint256 supply = bound(supplySeed, 1, net * 2);
        uint256 fee = bound(relayerFeeSeed, 0, 1e6);
        bytes memory action = abi.encode(SUPPLY, supply);
        AstrionAccount.ExecutionIntent memory i =
            _intent(address(module), address(pool), action, ib.transferId, 1e6);
        bytes memory sig = _sign(i, ownerPk);
        vm.prank(relayer);
        bool executed = account.executeFundedIntent(i, action, sig, fee, ib.message, attestation);
        inbounds.push(ib);
        minted += net;
        if (executed) {
            fundedExecuted++;
            fees += fee;
        }
    }

    function fundedLater(uint256 idx, uint256 supplySeed) external {
        if (inbounds.length == 0) return;
        Inbound memory ib = inbounds[bound(idx, 0, inbounds.length - 1)];
        (bool recorded, bool consumed,,,,,) = account.receipts(ib.transferId);
        if (!recorded || consumed) return;
        bytes memory action = abi.encode(SUPPLY, bound(supplySeed, 1, 100e6));
        AstrionAccount.ExecutionIntent memory i =
            _intent(address(module), address(pool), action, ib.transferId, 0);
        bytes memory sig = _sign(i, ownerPk);
        vm.prank(relayer);
        if (account.executeFundedIntent(i, action, sig, 0, "", "")) fundedExecuted++;
    }

    function forgedFunded(uint256 amountSeed, uint256 attackerPkSeed) external {
        (Inbound memory ib, uint256 net) = _newInbound(amountSeed, 0);
        uint256 pk = bound(attackerPkSeed, 1, 1e30);
        if (vm.addr(pk) == owner) return;
        bytes memory action = abi.encode(WITHDRAW, uint256(1));
        AstrionAccount.ExecutionIntent memory i =
            _intent(address(module), address(pool), action, ib.transferId, 1e6);
        i.recipient = attacker;
        bytes memory sig = _sign(i, pk);
        vm.prank(attacker);
        bool executed = account.executeFundedIntent(i, action, sig, 1e6, ib.message, attestation);
        if (executed) violation = "forged intent executed";
        inbounds.push(ib);
        minted += net;
    }

    function borrow(uint256 amountSeed) external {
        uint256 amount = bound(amountSeed, 1, 500e6);
        if (_relay(abi.encode(BORROW, amount))) borrowed += amount;
    }

    function repay(uint256 amountSeed) external {
        uint256 debt = pool.debt(address(usdc), address(account));
        uint256 cap =
            debt < usdc.balanceOf(address(account)) ? debt : usdc.balanceOf(address(account));
        if (cap == 0) return;
        uint256 amount = bound(amountSeed, 1, cap);
        if (_relay(abi.encode(REPAY, amount))) repaid += amount;
    }

    function withdrawFromPool(uint256 amountSeed) external {
        uint256 supplied = pool.supplied(address(usdc), address(account));
        if (supplied == 0) return;
        uint256 amount = bound(amountSeed, 1, supplied);
        if (_relay(abi.encode(WITHDRAW, amount))) ownerOut += amount;
    }

    function ownerSweep(uint256 amountSeed) external {
        uint256 bal = usdc.balanceOf(address(account));
        if (bal == 0) return;
        uint256 amount = bound(amountSeed, 1, bal);
        vm.prank(owner);
        account.execute(address(usdc), 0, abi.encodeCall(IERC20.transfer, (owner, amount)));
        ownerOut += amount;
    }

    function returnToStellar(uint256 amountSeed) external {
        uint256 bal = usdc.balanceOf(address(account));
        if (bal == 0) return;
        uint256 amount = bound(amountSeed, 1, bal);
        bytes memory action = abi.encode(amount, uint256(0), uint32(2000), G);
        AstrionAccount.ExecutionIntent memory i =
            _intent(address(returnModule), address(messenger), action, 0, 0);
        bytes memory sig = _sign(i, ownerPk);
        vm.prank(relayer);
        account.executeIntent(i, action, sig, 0);
        burned += amount;
    }

    function togglePause(bool paused) external {
        bytes32 id = policy.routeId(block.chainid, ProtocolIds.AAVE_V3, bytes32(0));
        vm.prank(guardian);
        policy.setPaused(id, paused);
    }

    function warp(uint256 secondsSeed) external {
        vm.warp(block.timestamp + bound(secondsSeed, 1, 3 days));
    }

    function attackerDrain(uint256 amountSeed) external {
        uint256 bal = usdc.balanceOf(address(account));
        if (bal == 0) return;
        vm.prank(attacker);
        try account.execute(
            address(usdc), 0, abi.encodeCall(IERC20.transfer, (attacker, bound(amountSeed, 1, bal)))
        ) {
            violation = "attacker executed";
        } catch {}
    }
}
