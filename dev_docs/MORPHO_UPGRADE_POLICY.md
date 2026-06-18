# Morpho Soroban Port Upgrade Policy

Status: pragmatic Stellar launch mode.

This port intentionally starts upgradeable while the Soroban implementation is
still pre-audit and testnet-facing, then moves isolated markets toward the
Morpho Blue trust model once accounting, liquidation, and oracle boundaries are
audited.

## Market Contracts

Markets keep `pause`, `upgrade`, `AppVersion`, and `migrate` during the launch
phase.

- `upgrade(admin, new_wasm_hash)` is gated by the market treasury/governance
  address.
- `migrate(admin, target_version)` is gated by the same address and only accepts
  forward app-version moves.
- `AppVersion` starts at `1`.
- `pause` remains an incident-response tool only during the launch phase.

Target freeze date: 2026-09-30 UTC.

By that date, or earlier after an external audit is complete, new production
markets should be deployed from an immutable market contract that removes
`pause`, `upgrade`, and mutable migration paths. Existing launch-phase markets
should either be migrated to audited immutable replacements through vault
allocator actions or be clearly labeled as upgradeable legacy markets.

Governance should remain unable to change a market's `loan_asset`,
`collateral_asset`, `oracle_adapter`, `rate_model`, or `lltv` after creation.
Factories may only expand the enabled IRM and LLTV sets for new markets.

## Vault Contracts

Vaults are management-layer contracts and may remain upgradeable permanently,
but upgrades must be treated as high-risk curator/governance actions.

- Risk changes should use the existing timelock flow.
- Adapter registry changes are timelocked.
- Vault users and frontends must display whether a vault has opted into an
  adapter registry.
- Vault upgrades should preserve share balances, total assets, caps, adapters,
  gates, timelocks, and fee state.

## Registry And Factory Contracts

`vault-factory` and `adapter-registry` are operational infrastructure. They may
remain upgradeable behind their owner/governance address, but registry entries
are add-only by design.

The vault factory records canonical vault deployments by `(owner, asset, salt)`
and should not be used as a hidden upgrade router. A vault address is stable
after creation.

## Release Checklist

- Publish the market WASM hash and app version.
- Publish whether a market is launch-phase upgradeable or immutable.
- Publish enabled IRMs and LLTVs for each factory.
- Publish each vault's adapter registry setting, if any.
- Run migration-state preservation tests before any app-version increase.
- Re-run decimals-mismatch health tests before deploying a market WASM.
