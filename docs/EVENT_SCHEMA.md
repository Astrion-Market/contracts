# Event Schema

Events are part of the protocol integration surface. Indexers should key events
by contract ID, topic tuple, and ledger sequence, and consumers should treat
replayed event processing as idempotent.

## CorePool

- `init`: pool initialization.
- `mkt_add`: market registration with risk parameters.
- `mkt_upd`: market risk config update.
- `supply`: user supplied an asset.
- `withdr`: user withdrew an asset.
- `borrow`: user borrowed an asset.
- `repay`: debt repayment.
- `col_on`: collateral enabled.
- `col_off`: collateral disabled.
- `pause`: pool paused.
- `unpause`: pool unpaused.
- `accrue`: market indexes updated.

## Market

- `init`: isolated market initialization.
- `supply`: collateral supplied.
- `withdr`: collateral withdrawn.
- `borrow`: debt borrowed.
- `repay`: debt repaid.
- `liq`: isolated-market liquidation.

## MarketFactory

- `init`: factory initialized.
- `market`: isolated market deployed and registered.

## LiquidationEngine

- `init`: engine initialized.
- `liq`: CorePool liquidation repayment accepted.

## OracleAdapter

The oracle adapter uses typed contract events for initialization, price queries,
oracle updates, and admin transfers. Price query events include the requested
asset, WAD price, timestamp, and oracle source.

## Compatibility Rules

- Do not change existing topic names without a migration note.
- Additive tuple fields require a documented version bump.
- Prefer new event names for materially different semantics.
