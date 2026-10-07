# Legacy Soroban engine: review findings

Source-level findings from the 2026-10-07 architecture review at `5910a30`.
These are **not demonstrated theft exploits**. Each has a reproduction test that
states the *expected* behaviour. The tests are `#[ignore]`d because they fail
against the current code:

```bash
cargo test -p market-adapter     -- --ignored
cargo test -p vault              -- --ignored finding_
cargo test -p liquidation-engine -- --ignored finding_
```

The cross-chain release path does not use these components. Any continued
legacy deployment needs remediation first. The remediating commit removes the
`#[ignore]`.

## LEGACY-F1: market-adapter allocation does not authenticate the vault

- **Where:** `contracts/market-adapter/src/lib.rs` `allocate` and `deallocate`.
- **Issue:** both compare the caller-supplied `sender` with `parent_vault`, but
  neither calls `require_auth()`. A matching argument is not caller
  authentication.
- **Impact (expected):** anyone can force-deallocate vault liquidity out of
  markets (funds go to the vault, so this is griefing of allocation and yield),
  and anyone can supply the adapter's idle balance.
- **Expected behaviour:** `config.parent_vault.require_auth()` in both
  entrypoints. The vault already authorizes its sub-call.
- **Tests:** `finding_f1_deallocate_requires_vault_auth`,
  `finding_f1_allocate_requires_vault_auth`.

## LEGACY-F2: market-adapter does not authenticate markets

- **Where:** `contracts/market-adapter/src/lib.rs` (`market_factory` stored at
  `initialize`, never read).
- **Issue:** the market address comes from encoded `data`. Any contract that
  returns a config with the right loan asset is accepted, and the adapter
  pre-authorizes a token `transfer` to it.
- **Impact (expected):** combined with F1, idle adapter balance can be supplied
  into an attacker-controlled contract.
- **Expected behaviour:** reject markets the configured factory did not create
  (`market_factory.get_market_by_id` / membership check).
- **Test:** `finding_f2_allocate_rejects_market_not_from_factory`.

## LEGACY-F3: vault allocate path lacks adapter membership check

- **Where:** `contracts/vault/src/lib.rs` `allocate_internal` (the
  `deallocate_internal` path checks `read_is_adapter`).
- **Issue:** an allocator can transfer idle vault assets to any address that
  reports a positive `AdapterChange`.
- **Impact (expected):** the allocator role becomes able to drain idle assets.
  The trust boundary for allocators is undefined.
- **Expected behaviour:** `AdapterNotEnabled` before any transfer, plus a
  documented allocator trust boundary.
- **Test:** `finding_f3_allocate_rejects_disabled_adapter`.

## LEGACY-F4: liquidation-engine does not seize collateral

- **Where:** `contracts/liquidation-engine/src/lib.rs` `execute_liquidation`.
- **Issue:** it calls `core.repay` and emits a computed seizure amount, but no
  collateral is transferred or seized. `core-pool` exposes no seizure API.
- **Impact:** the legacy liquidation flow is incomplete. A liquidator repays
  debt and receives nothing.
- **Expected behaviour:** do not deploy this flow. Remediation requires a
  core-pool seizure API restricted to the engine.
- **Test:** `finding_f4_liquidation_seizes_collateral`.
