# Advanced Security Plan

This document turns the senior-contributor issue list into executable review
tracks. It is intentionally conservative: protocol changes that alter economics
or storage layout must land behind tests, migration notes, and explicit review.

## Upgrade And Migration

- Keep storage keys append-only. Never reuse a `DataKey` discriminant for a new
  meaning.
- Every upgrade PR must include a before/after state preservation test.
- Prefer adding new view functions over changing return types of existing views.
- Treat `upgrade` as a high-risk admin action until governance or multisig hooks
  are integrated.
- For coordinated multi-contract upgrades, deploy new WASM, run dry-run checks,
  upgrade passive contracts first, then upgrade contracts that initiate
  cross-contract calls.

## Runtime Invariants

CorePool exposes `assert_market_invariants(asset)` for cheap post-operation
checks. Simulation and fuzz harnesses should call it after every supply, borrow,
repay, withdraw, and accrual step.

Required invariants:

- Supply and borrow indexes never fall below WAD.
- Scaled totals and protocol reserves never become negative.
- Real total borrow never exceeds real total supply.
- Risk parameters remain within validated ranges.

## Oracle Safety

The oracle adapter supports per-asset WAD sanity bounds. Use them for assets
with known operational ranges, especially stablecoins and wrapped assets. Bounds
must be wide enough to avoid halting during normal volatility but tight enough
to catch decimal mistakes and obvious outliers.

## Liquidation Safety

Keeper calls should prefer `liquidate_with_limits`, which adds:

- `max_collateral_seized` slippage protection.
- `deadline` timeout protection.
- per-liquidator `nonce` replay protection.

## Formal Specification Targets

The first formal or semi-formal specs should cover:

- Interest accrual monotonicity.
- Debt share burn behavior on repay.
- Health factor projection checks on borrow and withdraw.
- Liquidation close-factor and collateral-seizure bounds.

## Audit Readiness

Before an external audit, freeze public APIs, export a WASM size report, run the
full test suite, and include:

- `SECURITY_CHECKLIST.md`
- `PR_REVIEW_CHECKLIST.md`
- this plan
- the latest deployment and upgrade notes
- known limitations and unresolved economic assumptions
