# Gas And Footprint Guidelines

Use these rules when changing hot paths or storage layouts.

## Hot Paths

- Accrue interest before state-changing operations, but avoid duplicate accrual
  in the same call path.
- Keep view functions bounded by market count. Expensive portfolio views should
  document iteration behavior.
- Prefer WAD helpers from `astrion-math` over custom arithmetic.
- Avoid storing values that can be derived cheaply from existing state.

## Storage

- Use instance storage for singleton configuration.
- Use persistent storage for user, market, and oracle override records.
- Bump TTL when reading or writing persistent records.
- Add new storage keys append-only; do not repurpose existing keys.

## WASM Size

CI uploads a `wasm-size-report` artifact after release builds. Review it for
unexpected growth when adding dependencies or large helper modules.

## Review Checklist

- Does this add a new cross-contract call?
- Does it increase the number of storage reads/writes in supply, borrow, repay,
  withdraw, liquidation, or price lookup?
- Does it add an unbounded loop?
- Does it add a new dependency to a contract crate?
- Does it change event or storage compatibility?
