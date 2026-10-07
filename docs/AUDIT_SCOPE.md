# Audit scope: cross-chain alpha

Commit: the release candidate named in `CHANGELOG.md`. Language versions:
Solidity 0.8.30 (cancun), Rust stable, TypeScript (Bun 1.4.2).

## Priority 1: custody and authorization

| Path | Focus |
|---|---|
| `evm/src/account/AstrionAccount.sol` | EIP-712 domain/struct, nonce consumption order, call checks (target, selector, approval spender, transfer recipient), exact-allowance post-check, reentrancy, funded path (`receiveTransfer`, `executeFundedIntent` try/self-call, receipt binding, amount reconciliation) |
| `evm/src/account/AstrionAccountFactory.sol` | CREATE2 salt/initcode, front-running, idempotency |
| `evm/src/account/RoutePolicy.sol` | Pause semantics, cannot block exits |
| `evm/src/libraries/CctpMessageV2.sol`, `StellarStrkey.sol` | Offsets, bounds, checksum strictness |

## Priority 2: protocol modules

`evm/src/modules/*.sol`, `evm/src/libraries/MorphoBalances.sol`: plans can't
exceed signed intent, `increasesRisk` classification, exact repay-all
approvals (Morpho accrual mirror), Comet `baseBorrowMin`, Aave eMode and
isolation pinning, return module hook and forwarder fields.

## Priority 3: off-chain

`libs/crosschain-types`, `sdk/src` (codecs, intent hashing, account
prediction), `services/relayer/src` (signature validation before spend,
persistence-before-broadcast, reorg handling, budgets),
`ops/crosschain/*.sh` (no duplicate burns or mints on rerun).

## Out of scope

Third-party protocols and CCTP. Test fixtures. The frozen legacy Soroban engine.

## Provided evidence

`docs/evidence/INDEX.md`, the invariant list in
`docs/architecture/failure-invariants.md`, and ADRs 0001–0002.
