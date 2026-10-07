# Cross-chain failure invariants

These invariants are checked automatically over random operation sequences.
The tests use no blanket auth mocks: every relayed action carries a real
ECDSA signature and every direct action is pranked from the real owner.

## Solidity (`evm/test/invariant/`)

The handler mixes CCTP mints from random callers, duplicate deliveries,
mint-and-act in one call, actions bound to earlier receipts (out of order),
forged intents, borrows, repays, pool withdrawals, owner sweeps, return burns,
route pauses and time warps (expiry).

| Invariant | Meaning |
|---|---|
| `valueIsConserved` | `account + supplied − debt + burned + ownerOut + fees == minted` and burns equal what reached the messenger. Nothing is created, lost or double-counted |
| `noUnauthorizedValueMovement` | Attacker holds 0; owner and relayer hold exactly what they were entitled to |
| `noDuplicateDebt` | Protocol debt equals recorded borrows minus repays |
| `noLeftoverAllowance` | No approval survives any action |
| `eachReceiptFundsAtMostOneAction` | Every delivered transfer is recorded once; consumed receipts equal executed funded actions |
| `ownerCanAlwaysRecoverMintedFunds` | From any state the owner can sweep the full balance directly |
| `noAdversarialSuccess` | Duplicate receipt, forged intent and attacker execute never succeed |

Failing seeds persist under `evm/cache/{fuzz,invariant}`. CI uploads them as an
artifact on failure, so they can be replayed.

## Relayer (`services/relayer/test/property.test.ts`)

Seeded random interleavings of submits (with duplicates), ticks, mining,
reorgs, broadcast failures and process restarts. Every job ends `final`, with
exactly one signed transaction per job. CI runs 200 seeds.

## Liquidation during repayment transit

Covered by the lifecycle fork suites (`evm/test/fork/lifecycle/`). Interest
accrues while repayment USDC is in transit, and a principal-only repayment
leaves visible residual debt. Protocol-side liquidation of the position remains
the protocol's behaviour. Astrion never assumes the bridge ETA protects it.
