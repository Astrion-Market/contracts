// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {AstrionAccount} from "./AstrionAccount.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {RoutePolicy} from "./RoutePolicy.sol";
import {IMessageTransmitterV2} from "../interfaces/IMessageTransmitterV2.sol";
import {ProtocolIds} from "../libraries/ProtocolIds.sol";

/// @title AstrionAccountFactory
/// @notice Deploys one AstrionAccount per `(owner, chainId, protocol,
/// marketScope, version)` with CREATE2. Deployment is permissionless and
/// idempotent: the owner is part of both the salt and the constructor
/// arguments, so anyone deploying "first" can only deploy the owner's own
/// account. The factory holds no assets and has no admin. Every account it
/// deploys reads route pauses from the same immutable `policy`.
contract AstrionAccountFactory {
    RoutePolicy public immutable policy;
    /// CCTP wiring shared by every account on this chain (from the manifest).
    IMessageTransmitterV2 public immutable messageTransmitter;
    IERC20 public immutable usdc;
    uint32 public immutable localDomain;

    event AccountCreated(
        address indexed account,
        address indexed owner,
        bytes32 indexed protocol,
        bytes32 marketScope,
        uint32 version
    );

    error UnsupportedProtocol(bytes32 protocol);
    error ZeroVersion();

    constructor(
        RoutePolicy policy_,
        IMessageTransmitterV2 messageTransmitter_,
        IERC20 usdc_,
        uint32 localDomain_
    ) {
        policy = policy_;
        messageTransmitter = messageTransmitter_;
        usdc = usdc_;
        localDomain = localDomain_;
    }

    /// @notice Deploy (or return the existing) account for a scope.
    function createAccount(address owner, bytes32 protocol, bytes32 marketScope, uint32 version)
        external
        returns (address account)
    {
        if (!ProtocolIds.isSupported(protocol)) revert UnsupportedProtocol(protocol);
        if (version == 0) revert ZeroVersion();
        account = predictAccount(owner, protocol, marketScope, version);
        if (account.code.length > 0) return account;
        AstrionAccount deployed = new AstrionAccount{
            salt: salt(owner, protocol, marketScope, version)
        }(
            owner, policy, messageTransmitter, usdc, localDomain, protocol, marketScope, version
        );
        assert(address(deployed) == account);
        emit AccountCreated(account, owner, protocol, marketScope, version);
    }

    /// @notice The scope key. Includes the chain id so scopes never collide
    /// across chains even if the factory shares an address.
    function salt(address owner, bytes32 protocol, bytes32 marketScope, uint32 version)
        public
        view
        returns (bytes32)
    {
        return keccak256(abi.encode(owner, block.chainid, protocol, marketScope, version));
    }

    function predictAccount(address owner, bytes32 protocol, bytes32 marketScope, uint32 version)
        public
        view
        returns (address)
    {
        bytes32 initCodeHash = keccak256(
            abi.encodePacked(
                type(AstrionAccount).creationCode,
                abi.encode(
                    owner,
                    policy,
                    messageTransmitter,
                    usdc,
                    localDomain,
                    protocol,
                    marketScope,
                    version
                )
            )
        );
        return address(
            uint160(
                uint256(
                    keccak256(
                        abi.encodePacked(
                            bytes1(0xff),
                            address(this),
                            salt(owner, protocol, marketScope, version),
                            initCodeHash
                        )
                    )
                )
            )
        );
    }
}
