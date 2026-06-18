# Security Review Checklist

Use this checklist before audit, before testnet releases, and before every
material protocol change.

- Initialization cannot be replayed.
- Every privileged function authenticates the current admin.
- User-moving functions authenticate the user or payer before state changes.
- Interest accrual runs before reading or mutating indexed balances.
- WAD math uses `astrion-math`; no floats or ad hoc decimal scaling.
- Caps, LTV, liquidation threshold, bonus, and reserve factor are validated.
- Oracle prices reject stale, zero, negative, or missing values.
- Health factor checks run on projected post-action state.
- Liquidation close factor and seizure amount are capped.
- Token transfers happen after state checks and match accounting deltas.
- Storage TTL is extended for persistent user and market records.
- Events are emitted for state-changing operations.
- Upgrade paths are admin-only and documented.
- Tests include happy path, auth failures, invalid params, and edge cases.
