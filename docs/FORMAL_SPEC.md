# Formal Specification Notes

This document records preconditions and postconditions for the first critical
properties to verify manually, with property tests, or with a formal tool.

## Interest Accrual

Preconditions:

- Market exists.
- `supply_index >= WAD`.
- `borrow_index >= WAD`.
- `total_scaled_supply >= 0`.
- `total_scaled_borrow >= 0`.

Postconditions:

- `new_supply_index >= old_supply_index`.
- `new_borrow_index >= old_borrow_index`.
- `last_update_timestamp` equals the current ledger timestamp after accrual.
- If total supply or total borrow is zero, indexes do not change.
- Protocol reserves only increase.

## Borrow

Preconditions:

- Borrower authenticated.
- Market active and borrowable.
- Amount is positive.
- Borrow cap and liquidity are sufficient.

Postconditions:

- User scaled borrow increases by `to_scaled(amount, borrow_index)`.
- Market total scaled borrow increases by the same amount.
- Projected health factor is at least WAD.

## Repay

Preconditions:

- Payer authenticated.
- Amount is positive.
- Market exists.

Postconditions:

- Repay amount is capped to outstanding debt.
- User scaled borrow never becomes negative.
- Market total scaled borrow never becomes negative.

## Liquidation

Preconditions:

- Liquidator authenticated.
- Borrower health factor is below WAD.
- Repay amount is positive and within close factor.
- Collateral price and debt price pass oracle checks.

Postconditions:

- Repay call reduces borrower debt.
- Computed collateral seizure respects liquidation bonus and price ratio.
- `liquidate_with_limits` rejects expired deadlines, duplicate nonces, and
  collateral seizure above keeper-provided max.
