# Keeper Economics

Liquidation should be profitable enough to keep the protocol solvent but not so
punitive that small price moves over-penalize borrowers.

## Default Assumptions

- Close factor: 50%.
- Liquidation bonus: asset-specific, usually 5% to 10%.
- Keeper uses `liquidate_with_limits` with a deadline and nonce.
- Oracle adapter bounds are configured for high-value collateral assets.

## Profit Model

```text
gross_reward = collateral_seized_value - repaid_debt_value
net_reward   = gross_reward - transaction_cost - slippage_cost
```

A keeper should only submit when `net_reward > 0` and the collateral seizure is
below the configured `max_collateral_seized`.

## Governance Inputs

- Volatility of collateral asset.
- Market liquidity for seized collateral.
- Average transaction costs during congestion.
- Oracle update cadence and staleness threshold.

## Attack Notes

- Very high bonuses can create incentives to manipulate prices.
- Missing deadlines expose keepers to stale off-chain decisions.
- Missing nonce protection can lead to duplicate worker submissions.
- Loose oracle bounds can amplify decimal or source failures.
