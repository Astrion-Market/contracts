# Roadmap

## Milestone 1: Accounting Hardening

- Expand CorePool invariant tests into random operation sequences.
- Add integration tests for CorePool, OracleAdapter, and InterestRateModel.
- Add reserve withdrawal design and tests before exposing any reserve movement.

## Milestone 2: Liquidation Completeness

- Add CorePool collateral seizure API with a liquidation-engine whitelist.
- Complete end-to-end liquidation tests with price shock scenarios.
- Add keeper examples using `liquidate_with_limits`.

## Milestone 3: Governance And Upgrades

- Replace direct admin upgrades with timelock or multisig-controlled flows.
- Add storage migration tests for every contract family.
- Publish upgrade runbooks per contract.

## Milestone 4: Simulation And Fuzzing

- Build deterministic economic model scenarios for utilization, rates, and
  reserves.
- Add fuzz harnesses for public entrypoints and oracle prices.
- Save failing seeds as regression tests.

## Milestone 5: External Integrations

- Stabilize event schemas.
- Publish SDK examples for common flows.
- Add gas estimates and WASM size regression reports for frontends.
