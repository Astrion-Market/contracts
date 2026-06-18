# Isolated Market

An isolated market is a two-asset pool with one collateral asset and one debt
asset. Risk is contained to the pair, so a bad asset or oracle in one market
does not affect the shared CorePool or other isolated markets.

## Minimal Flow

```rust
let config = IsolatedMarketConfig {
    collateral_asset,
    debt_asset,
    oracle_adapter,
    ltv: 700_000_000_000_000_000,
    liquidation_threshold: 800_000_000_000_000_000,
    liquidation_bonus: 50_000_000_000_000_000,
    reserve_factor: 100_000_000_000_000_000,
    supply_cap: 0,
    borrow_cap: 0,
    rate_model,
    treasury,
};

market.initialize(&config);
market.supply(&supplier, &1_000_000);
market.borrow(&borrower, &100_000);
market.repay(&payer, &borrower, &50_000);
market.withdraw(&supplier, &100_000);
```

All balances use raw token units. Risk parameters and rates use WAD precision.
