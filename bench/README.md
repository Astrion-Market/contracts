# Benchmark Harness Notes

This directory documents benchmark scenarios for future automated gas and CPU
measurement. CI currently publishes a WASM size artifact; runtime gas snapshots
should be added once the team settles on the canonical local network harness.

## Scenarios

- CorePool supply and withdraw round trip.
- CorePool supply, borrow, repay, withdraw with one asset.
- Multi-asset health factor calculation.
- Isolated market borrow and liquidation.
- Oracle price lookup with default source and per-asset override.

## Metrics

- WASM size.
- Host CPU instructions.
- Storage reads and writes.
- Event count.
- Cross-contract call count.

## Regression Policy

Any benchmark regression above 10% should include an explanation in the PR and
a note on whether the added cost is user-facing or admin-only.
