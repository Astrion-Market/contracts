# Astrion cross-chain spec (v1)

Versioned JSON schemas and shared test vectors for cross-chain lending
operations. Every consumer (Rust `libs/crosschain-types`, the TypeScript
`sdk/`, Solidity tests and the interface) validates against the same fixtures.

| File | Purpose |
|---|---|
| `networks.json` | Canonical network registry: chain IDs, CCTP domains and USDC decimals are separate fields |
| `schemas/common.schema.json` | Shared definitions (amounts, addresses, ids) |
| `schemas/intent.schema.json` | What the user asked for; bound by `intentId` |
| `schemas/quote.schema.json` | Route quote with per-leg fees and ETA |
| `schemas/status.schema.json` | UI lifecycle projection |
| `schemas/receipt.schema.json` | One on-chain leg with its own id and finality |
| `schemas/scoped-account.schema.json` | `(owner, network, protocol, marketScope, version)` account |
| `schemas/failure-codes.json` | Error codes and which are retryable |
| `fixtures/` | Valid and invalid vectors with expected outcome |

## Units

- Amounts are raw integers in the token's smallest unit, encoded as decimal
  strings with no sign and no leading zeros. They are never floats.
- Stellar USDC has **7** decimals and EVM USDC has **6**. CCTP message amounts
  are always 6. A Stellar burn of `sent` raw units burns `floor(sent / 10)` and
  leaves `sent mod 10` as dust on Stellar.
- Maximum raw amount: `i64::MAX` (Stellar classic asset limit).
- An intent's `amount` and `maxFee` are in **source-network** units.

## Integrity: `intentId`

`intentId = keccak256(canonical JSON of the intent object without intentId)`

Canonical JSON means UTF-8, object keys sorted by code point at every level, no
insignificant whitespace, integers without exponent or fraction, and ASCII-only
content (`NonCanonicalEncoding` otherwise). The hash covers every field,
including fields unknown to an older consumer. Changing the recipient, network,
amount or fee therefore fails with `IntentIdMismatch`.

`intentId` is an integrity check on the document, not an authorization. EVM
actions are authorized by the owner's domain-separated signature (C09).
Stellar burns are authorized by the Stellar transaction itself.

## Semantic rules (enforced by `libs/crosschain-types`)

Checked in this order; each failing fixture maps to exactly one code.

1. `schemaVersion` is a supported major → `UnsupportedSchemaVersion`.
2. `intentId` matches the canonical hash → `IntentIdMismatch`.
3. Asset is USDC; both networks are in the registry → `UnknownNetwork`.
4. Source and destination share an environment (no testnet ↔ mainnet) →
   `EnvironmentMismatch`.
5. Route:
   - `transfer` has no protocol and different networks.
   - Protocol actions need `protocol` + `marketScope` and cross only
     Stellar ↔ EVM (or stay on one EVM network).
   - `lend`, `supply_collateral` and `repay` execute on an EVM destination.
   - `withdraw`, `withdraw_collateral` and `borrow` execute on an EVM source.
   - Otherwise → `InvalidRoute`.
6. `amount` is canonical and positive (`InvalidAmount`) and ≤ i64::MAX
   (`AmountTooLarge`).
7. `amount` and `maxFee` decimals equal the source network's USDC decimals →
   `DecimalsMismatch`.
8. `maxFee ≤ amount` → `FeeExceedsAmount`.
9. `positionOwner` is EVM. `recipient` matches the destination kind and
   `refundRecipient` matches the source kind. `marketScope` is a lowercase
   bytes32. Otherwise → `InvalidAddress`.
10. `actionDeadline > createdAt` and `deliveryTimeoutSecs ≥ 1` →
    `InvalidDeadline`.
11. `nonce` is a canonical uint256 → `InvalidNonce`.
12. In a batch, `intentId` and `(positionOwner, nonce)` are unique →
    `DuplicateIntent`.

Receipts reconcile units: a burn satisfies `burned × scale + dust = sent` with
`dust < scale`, and a mint satisfies `received = (burned − fee) × scale`. Here
`scale` is 10 on Stellar and 1 on EVM, and CCTP domains must match the leg's
network (`ReconciliationMismatch`).

## Action deadline vs delivery timeout

- `actionDeadline`: after it, the destination **action** must not execute. It
  never cancels a burn. Once burned, delivery continues and minted funds stay
  recoverable by the owner.
- `deliveryTimeoutSecs`: when an undelivered transfer is escalated to recovery
  in the UI. It is not a cancellation, and it is not a liquidation buffer.

## Versioning

- `schemaVersion` is a major version. Consumers reject unknown majors.
- Within a major, changes are additive only: new **optional** fields. Required
  fields, enums' meanings and units never change in place.
- Removing or re-meaning anything is a new major, with new fixtures alongside
  the old ones.

## Regenerating fixtures

Fixtures are checked in. `intentId` values were computed with Foundry's
`cast keccak` over the canonical JSON, independently of the Rust validator.
