# Morpho V2 Soroban Port Plan

This document maps Morpho Blue and Morpho Vault V2 concepts onto the current Astrion Soroban contracts. It is an implementation guide, not a literal Solidity translation.

Sources studied:
- `/home/sunny/0xsteins/contracts/dev_docs/morpho-blue-whitepaper.pdf`
- `/home/sunny/0xsteins/vault-v2/src/VaultV2.sol`
- `/home/sunny/0xsteins/vault-v2/src/VaultV2Factory.sol`
- `/home/sunny/0xsteins/vault-v2/src/interfaces/IVaultV2.sol`
- `/home/sunny/0xsteins/vault-v2/src/interfaces/IAdapter.sol`
- `/home/sunny/0xsteins/vault-v2/src/adapters/MorphoMarketV1AdapterV2.sol`
- Current Astrion contracts in `contracts/market`, `contracts/market-factory`, `contracts/core-pool`, `contracts/oracle-adapter`, `contracts/interest-rate-model`, `contracts/liquidation-engine`, and `libs/math`.

## Target Architecture

Morpho Blue's whitepaper describes a minimalist lending primitive: isolated markets, permissionless market creation, oracle-agnostic pricing, no embedded rehypothecation of collateral, no DAO-managed risk, and explicit bad-debt accounting. Morpho Vault V2 is a separate passive-management layer that accepts one asset, mints shares, and allocates that asset through adapters into lending markets.

The Soroban port should therefore be:

```text
VaultFactory
  deploys Vault
    holds one asset
    mints internal vault shares
    allocates through adapters

VaultAdapterRegistry
  allowlists adapter implementations when a vault wants adapter restrictions

MarketAdapter
  bridges Vault allocation calls to Astrion isolated markets

MarketFactory
  deploys/records isolated markets

Market
  Morpho-Blue-like isolated loan/collateral pair
  loan asset supplied by lenders
  collateral asset posted by borrowers
  oracle + IRM per market
```

`core-pool` should not be the main target for the Morpho port. It is a shared-liquidity pool, while Morpho Blue deliberately moves risk into isolated markets. Keep `core-pool` as a legacy/shared-pool product or deprecate it after the isolated-market and vault stack is complete.

## Step 1: Rename The Lending Model Around Morpho Terms

Current `contracts/market` uses `collateral_asset` as the supplied asset and `debt_asset` as the borrowed asset. Morpho's model is the opposite:

- `loan_asset`: asset supplied by lenders and borrowed by borrowers.
- `collateral_asset`: asset posted by borrowers and never lent out by default.

Update `contracts/market/src/types.rs`:

- Replace `debt_asset` with `loan_asset`.
- Keep `collateral_asset`.
- Replace `ltv` and `liquidation_threshold` with `lltv` if the market follows Morpho exactly. If keeping both `ltv` and `liquidation_threshold` for UX, treat `lltv = liquidation_threshold` in liquidation logic.
- Remove `supply_cap` and `borrow_cap` from the core market if the goal is a strict Morpho Blue primitive. Caps belong in vaults/adapters, not the market. If caps are retained during migration, mark them as non-Morpho compatibility controls.
- Add `fee_rate` or keep `reserve_factor` only as the governance-set fee on borrower interest, capped by policy.

Rename every function, event, and comment to Morpho terminology. **Step 1 is a
terminology change only** — it renames fields and consolidates `lltv`, but does
*not* change behavior. After Step 1, `supply` still moves `collateral_asset` and
stores `scaled_supply`; the market is still not a working lender. The semantic
change — making `supply` move `loan_asset` and adding separate borrower collateral
accounting — happens in Step 2. Do not conflate the two: Step 1 must be safely
mergeable on its own without altering accounting.

## Step 2: Split Lender Supply From Borrower Collateral

Current `contracts/market/src/lib.rs` stores one `scaled_supply` and one `scaled_borrow` per user. That makes supplied funds and collateral the same asset. Morpho requires separate accounting:

```rust
pub struct MarketPosition {
    pub supply_shares: i128,
    pub borrow_shares: i128,
    pub collateral: i128,
}

pub struct MarketState {
    pub total_supply_assets: i128,
    pub total_supply_shares: i128,
    pub total_borrow_assets: i128,
    pub total_borrow_shares: i128,
    pub total_collateral: i128,
    pub fee_assets: i128,
    pub last_update_timestamp: u64,
}
```

Implement these user entrypoints on `market`:

- `supply(env, supplier, assets, on_behalf) -> shares`
  - `supplier.require_auth()`.
  - Accrue interest.
  - Transfer `loan_asset` from supplier to market.
  - Mint supply shares to `on_behalf`.

- `withdraw(env, caller, assets, shares, on_behalf, receiver) -> (assets, shares)`
  - Allow caller only if `caller == on_behalf` or if an authorization map allows the caller.
  - Burn supply shares from `on_behalf`.
  - Transfer `loan_asset` to receiver.
  - Check market liquidity.

- `supply_collateral(env, supplier, assets, on_behalf)`
  - Transfer `collateral_asset` from supplier to market.
  - Increase `position.collateral`.
  - Do not mint interest-bearing collateral shares.

- `withdraw_collateral(env, caller, assets, on_behalf, receiver)`
  - Require caller auth/authorization.
  - Decrease collateral and check position remains healthy.
  - Transfer `collateral_asset` to receiver.

- `borrow(env, caller, assets, on_behalf, receiver) -> shares`
  - Require caller auth/authorization.
  - Accrue interest.
  - Mint borrow shares to `on_behalf`.
  - Check `LTV <= LLTV`.
  - Transfer `loan_asset` to receiver.

- `repay(env, payer, assets, shares, on_behalf) -> (assets, shares)`
  - Transfer `loan_asset` from payer.
  - Burn borrow shares from `on_behalf`.

This step is the most important correctness change. Without it, the current isolated market cannot behave like Morpho Blue.

## Step 3: Replace Index Accounting With Morpho Share Accounting

The current code uses Aave-style supply and borrow indexes. Morpho-style accounting is simpler for isolated markets:

- Supply shares represent a pro-rata claim on `total_supply_assets`.
- Borrow shares represent a pro-rata obligation against `total_borrow_assets`.
- Interest accrual increases `total_borrow_assets`.
- Lender assets increase by the borrower interest minus protocol fee.

Add math helpers to `libs/math/src/lib.rs`:

```text
to_shares_down(assets, total_assets, total_shares)
to_shares_up(assets, total_assets, total_shares)
to_assets_down(shares, total_assets, total_shares)
to_assets_up(shares, total_assets, total_shares)
mul_div_down(x, y, denominator)
mul_div_up(x, y, denominator)
zero_floor_sub(x, y)
```

**Virtual shares/assets are mandatory in the market, not only in the vault.** Morpho
Blue's `SharesMathLib` adds `VIRTUAL_SHARES = 1e6` and `VIRTUAL_ASSETS = 1` to every
market-level conversion. This is not cosmetic: it prevents the first-depositor share
inflation attack and removes division-by-zero on an empty market. Implement the helpers
with the offset baked in:

```text
to_shares_down(assets, total_assets, total_shares) =
    mul_div_down(assets, total_shares + VIRTUAL_SHARES, total_assets + VIRTUAL_ASSETS)
to_assets_down(shares, total_assets, total_shares) =
    mul_div_down(shares, total_assets + VIRTUAL_ASSETS, total_shares + VIRTUAL_SHARES)
```

(`*_up` variants use `mul_div_up`.) Use the same `VIRTUAL_SHARES`/`VIRTUAL_ASSETS`
constants the market relies on. The vault's virtual protection in Step 7 is a separate,
decimals-derived offset and does not replace this.

Use conservative rounding:

- Deposits mint shares rounded down.
- Withdrawals burn shares rounded up when assets are specified.
- Borrows mint debt shares rounded up.
- Repays burn debt shares rounded down/up according to whether the caller specifies assets or shares, always favoring solvency.

Keep WAD precision, but use checked arithmetic in all share conversion paths.

### Interest accrual precision

When porting accrual, do not reuse the current market's linear formula
`WAD + (rate / SECONDS_PER_YEAR) * dt`. Dividing the WAD-scaled rate by
`SECONDS_PER_YEAR` *before* multiplying by `dt` truncates to near-zero for realistic
rates. Always multiply before dividing (`rate * dt / SECONDS_PER_YEAR`), and prefer a
Taylor-compounded per-second rate to match Morpho Blue. In the share model interest is
applied by increasing `total_borrow_assets`, then crediting lenders with
`interest - fee` on `total_supply_assets`; there is no supply/borrow index to drift.

## Step 4: Implement Morpho Liquidation And Bad Debt Accounting

Current liquidation uses a fixed 50% close factor and rejects collateral seizure above user collateral. Morpho Blue has no close factor and explicitly accounts for bad debt.

Signature (must match the bot spec in `dev_docs/bots.md`):

```text
liquidate(
    env,
    liquidator,
    borrower,
    seized_assets,      // collateral to seize; 0 if specifying repaid_shares
    repaid_shares,      // borrow shares to repay; 0 if specifying seized_assets
    min_collateral_out, // slippage floor for the liquidator
    deadline,           // ledger timestamp after which the call reverts
) -> (seized_assets, repaid_assets)
preview_liquidate(borrower, repay_assets) -> LiquidationPreview
```

Follow Morpho Blue: the caller specifies **exactly one** of `seized_assets` or
`repaid_shares` (the other must be zero); the contract derives the counterpart from the
incentive factor. `min_collateral_out` and `deadline` protect liquidators from price
moves and stale simulations and are required, not optional.

Update `liquidate`:

- A position is liquidatable when `borrow_value / collateral_value > lltv`.
- Compute liquidation incentive factor:
  - `lif = min(max_lif, 1 / (1 - cursor * (1 - lltv)))`
  - Use `max_lif = 1.15 WAD` and `cursor = 0.3 WAD` unless governance chooses different constants.
- Allow full liquidation (no close factor).
- If repay amount plus incentive seizes less than all collateral, reduce debt and collateral normally.
- If the position is deeply underwater, seize all collateral and repay only the debt amount economically required by the incentive formula.
- Bad debt is recognized **only at liquidation time**, never lazily. When seizing all
  collateral still leaves borrow shares outstanding, in the same call:
  - compute `bad_debt_assets = remaining borrow assets`,
  - reduce `total_borrow_assets` and `total_supply_assets` by `bad_debt_assets`,
  - burn the borrower's remaining `borrow_shares` and the corresponding `total_borrow_shares`,
  - emit `bad_debt_realized`.
  This lowers the supply share price for all lenders (loss socialization) atomically.

Add tests for:

- Healthy position cannot be liquidated.
- Normal liquidation repays full debt and leaves no bad debt.
- Deep underwater liquidation seizes all collateral and socializes bad debt.
- Bad debt recognition lowers supply share price for all lenders.

## Step 5: Make Market Creation Permissionless But Parameter-Bounded

Morpho allows anyone to create markets, while governance only approves LLTVs and IRMs.

Update `contracts/market-factory`:

- Remove admin-only requirement from `create_market`.
- Add governance/admin only for:
  - `enable_irm(rate_model)`
  - `enable_lltv(lltv)`
  - `set_market_wasm_hash(hash)`
  - optional `set_fee_recipient`
- Validate market config:
  - `loan_asset != collateral_asset`
  - `rate_model` is enabled
  - `lltv` is enabled
  - oracle address is nonzero and explicit per market
- Derive market ID from `(loan_asset, collateral_asset, oracle, irm, lltv)` rather than only `(collateral, debt)`.
- Store `MarketById(BytesN<32>) -> Address`.
- Permit multiple markets with the same asset pair if oracle, IRM, or LLTV differs.

This aligns with the whitepaper's externalized risk model and avoids DAO bottlenecks.

## Step 6: Add Account Authorization

Morpho lets users authorize other accounts to manage positions. Soroban does not need EIP-712, but it should expose explicit account permissions.

Add to `market` persistent storage:

```rust
Authorization(owner: Address, operator: Address) -> bool
```

Do not add a `Nonce(owner)` key in the first port. Nonces only exist to replay-protect
signature-based authorization, which is a stated non-goal (see Non-Goals). Add it later
alongside signed-payload auth, not before.

Entry points:

- `set_authorization(env, owner, operator, authorized)`
  - `owner.require_auth()`.
- `is_authorized(env, owner, operator) -> bool`.

Use it in `borrow`, `withdraw`, and `withdraw_collateral`:

```text
caller == on_behalf || is_authorized(on_behalf, caller)
```

Do not add signature-based authorization until the basic auth path is stable. If needed later, design it around Stellar signed payloads and account contracts, not EIP-712.

## Step 7: Build Vault V2 As A New Contract

Add a new crate:

```text
contracts/vault/
  Cargo.toml
  src/lib.rs
  src/types.rs
  src/storage.rs
  src/errors.rs
  src/test.rs
```

The vault is a Soroban-native ERC-4626 analogue:

- It accepts exactly one SAC/token asset.
- It mints internal vault shares in persistent storage.
- It tracks `total_assets` from idle asset balance plus adapters' `real_assets`.
- It realizes gains and losses through `accrue_interest` (see asymmetric model below).
- It supports performance and management fees by minting shares to fee recipients.
- It does not need to be a SEP-41 token at first. Internal shares are enough for deposits/withdrawals. A share-token wrapper can be added later if composability requires transferability.

### Asymmetric interest model (do not skip — this is VaultV2's core protection)

`accrue_interest` must not simply set `total_assets = sum(real_assets)`. Port VaultV2's
rate-capped accrual exactly:

```text
real      = idle_balance + sum(adapter.real_assets())
max_total = total_assets + total_assets * max_rate * elapsed / WAD
new_total = min(real, max_total)
interest  = zero_floor_sub(new_total, total_assets)
```

- **Losses are realized immediately and fully** (`new_total` can drop to `real`).
- **Gains are capped at `max_rate` per second.** This is what prevents a donation or a
  `force_deallocate` penalty from spiking the share price in one ledger and letting an
  attacker front-run the jump. `max_rate` is governance/curator-set and capped by policy
  (VaultV2 uses 200% APR as the ceiling).
- Performance fee is taken on `interest`; management fee is taken on `new_total * elapsed`.
  Both are paid by minting shares to the fee recipients, diluting existing holders.

### Previews must run on projected state

`preview_*`, `convert_to_shares`, and `convert_to_assets` must compute against the
*projected* totals from an `accrue_interest_view()` (pending interest + fee shares folded
in), not the stored `total_assets`/`total_shares`. Otherwise previews disagree with the
state the matching write will produce. Expose `accrue_interest_view()` as a public view
for bots and frontends.

### Reentrancy

Adapters and gates are curator-chosen contracts and must be treated as adversarial. Guard
every entrypoint that makes a cross-contract call into an adapter, gate, or token
(`deposit`, `mint`, `withdraw`, `redeem`, `allocate`, `deallocate`, `force_deallocate`)
with a reentrancy lock, and follow strict checks-effects-interactions: update vault
shares and `total_assets` before the external call. Do not rely on "Soroban reentrancy is
different" — an adapter can still re-enter the vault during `real_assets`/allocate
callbacks and observe half-updated state.

Core storage:

```rust
VaultConfig {
    owner,
    curator,
    asset,
    name,
    symbol,
    decimals,
    virtual_shares,
    performance_fee,
    performance_fee_recipient,
    management_fee,
    management_fee_recipient,
    max_rate,
}

VaultState {
    total_assets,
    total_shares,
    last_update_timestamp,
}

ShareBalance(user) -> i128
Allowance(owner, spender) -> i128
Adapter(address) -> bool
Adapters -> Vec<Address>
Caps(id) -> Caps { allocation, absolute_cap, relative_cap }
Roles: sentinel, allocator
LiquidityAdapter -> Option<Address>
LiquidityData -> Bytes
```

Initial user functions:

- `deposit(caller, assets, receiver) -> shares`
- `mint(caller, shares, receiver) -> assets`
- `withdraw(caller, assets, receiver, owner) -> shares`
- `redeem(caller, shares, receiver, owner) -> assets`
- `preview_deposit`, `preview_mint`, `preview_withdraw`, `preview_redeem`
- `convert_to_shares`, `convert_to_assets`
- `balance_of`, `total_supply`, `total_assets`

Use Morpho's virtual asset/share protection:

- `virtual_assets = 1`
- `virtual_shares = 10 ^ max(0, 18 - asset_decimals)`

## Step 8: Add Vault Roles, Timelocks, And Abdication

Port Morpho Vault V2's role model:

- `owner`: sets curator, sentinels, display metadata.
- `curator`: submits timelocked risk/adapter changes.
- `sentinel`: can revoke pending changes and decrease caps quickly.
- `allocator`: moves assets among adapters within caps.

Soroban operation names are symbols/strings, not Solidity selectors. Use a stable operation key:

```rust
#[contracttype]
pub enum TimelockKey {
    SetAllocator(Address),
    AddAdapter(Address),
    RemoveAdapter(Address),
    IncreaseAbsoluteCap(BytesN<32>),
    IncreaseRelativeCap(BytesN<32>),
    SetPerformanceFee,
    SetManagementFee,
    SetGate(Symbol),
    SetAdapterRegistry,
    IncreaseTimelock(Symbol),
    DecreaseTimelock(Symbol),
    Abdicate(Symbol),
}
```

Entry points:

- `submit(curator, action_key, action_args_hash)`
- `revoke(caller, action_key, action_args_hash)`
- privileged execution functions check `executable_at <= ledger.timestamp`.
- `abdicate(action_key)` permanently disables an action.

Keep zero timelocks allowed at deployment so a vault can be configured in one setup phase.

## Step 9: Add Adapter Interface And Market Adapter

Define a Soroban adapter trait equivalent to Morpho's `IAdapter`:

```rust
pub trait Adapter {
    fn allocate(env: Env, data: Bytes, assets: i128, selector: Symbol, sender: Address) -> AdapterChange;
    fn deallocate(env: Env, data: Bytes, assets: i128, selector: Symbol, sender: Address) -> AdapterChange;
    fn real_assets(env: Env) -> i128;
}

pub struct AdapterChange {
    pub ids: Vec<BytesN<32>>,
    pub change: i128,
}
```

Add `contracts/market-adapter`:

- Constructor/config stores `parent_vault`, `asset`, and `market_factory`.
- `allocate` decodes a market ID or market address.
- Verifies the target market's `loan_asset == vault.asset`.
- Transfers vault asset to the market via `market.supply(adapter, assets, adapter)`.
- Tracks supply shares per market.
- Returns IDs:
  - adapter ID
  - collateral asset ID
  - exact market ID
- `deallocate` calls market withdraw and gives the vault access to returned assets.
- `real_assets` sums expected supplied assets across active market IDs.

This is the Soroban equivalent of `MorphoMarketV1AdapterV2`.

## Step 10: Add Vault Caps And Liquidity Adapter Behavior

Port the cap model into `vault`:

- `absolute_cap(id)`
- `relative_cap(id)`
- `allocation(id)`

Rules:

- Caps are checked only when allocations increase.
- Relative caps are relative to `first_total_assets`: the total assets snapshotted at the
  first accrual of the current transaction.
- Soroban has no EVM transient (transaction-scoped) storage, so Morpho's `firstTotalAssets`
  cannot be reproduced by storage TTL. **Do not key it by ledger sequence and do not rely
  on TTL to clear it** — a Soroban ledger can hold many transactions, so a ledger-keyed or
  TTL-cleared snapshot would be shared across unrelated transactions and the relative cap
  would be measured against a stale base.
- Instead, treat the top-level vault entrypoint as the transaction scope:
  - Store `FirstTotalAssets -> i128` in a single instance/temporary key.
  - At the start of each top-level entrypoint, if it is unset, set it during the first
    `accrue_interest`.
  - **Explicitly remove the key before that top-level call returns** (success or via a
    cleanup that runs on every exit path). This mirrors EVM transient auto-clear at tx end.
  - Because Soroban entrypoints are atomic and there is no cross-transaction reentrancy,
    one entrypoint = one transaction scope, which is the granularity Morpho assumes.
- Keep adapter ID lists bounded. If a vault has many adapters/markets, `real_assets` can become too expensive.

Liquidity adapter behavior:

- On deposit/mint, optionally allocate newly received assets to the liquidity adapter.
- On withdraw/redeem, if idle balance is insufficient, deallocate the shortfall from the liquidity adapter.
- Always allow in-kind/forced deallocation path so users are not trapped by an illiquid liquidity adapter.

## Step 11: Add Gates As Optional Contracts

Morpho Vault V2 has four gates:

- receive shares
- send shares
- receive assets
- send assets

Add a simple gate trait:

```rust
pub trait VaultGate {
    fn can_receive_shares(env: Env, account: Address) -> bool;
    fn can_send_shares(env: Env, account: Address) -> bool;
    fn can_receive_assets(env: Env, account: Address) -> bool;
    fn can_send_assets(env: Env, account: Address) -> bool;
}
```

Vault config may store each gate independently as `Option<Address>`.

Rules:

- Gate calls must be view-only by convention and should not be allowed to mutate vault state.
- If a gate call fails, the vault should fail closed for shares/assets movement.
- The vault itself must always be allowed to receive assets so force-deallocation penalties and internal flows work.

## Step 12: Add Vault Factory And Registry

Add `contracts/vault-factory`:

- Stores `vault_wasm_hash`.
- `create_vault(owner, asset, salt, constructor_args) -> Address`.
- Stores:
  - `IsVault(address) -> bool`
  - `VaultByOwnerAssetSalt(owner, asset, salt) -> Address`
- Emits `vault_created`.

Add `contracts/adapter-registry`:

- `owner` or governance can add adapters/adapter implementations.
- For a strict Morpho-like registry, make it add-only.
- Vaults can opt out by setting registry to none.

## Step 13: Adjust Oracle And IRM Boundaries

The existing `oracle-adapter` and `interest-rate-model` are good building blocks. Update integration boundaries:

- Market config stores its own oracle and IRM.
- Factory validates IRM against enabled set.
- Factory validates LLTV against enabled set.
- Factory does not bless oracle quality. Users/vault curators choose oracle risk.
- Oracle adapter must continue enforcing staleness and bounds per asset.

Add optional `get_market_params(market) -> MarketParams` so adapters and frontends can verify `loan_asset`, `collateral_asset`, `oracle`, `irm`, and `lltv`.

### Token decimals must be normalized in value math (fixes a latent bug)

The current health-factor path computes `collateral_value = raw_amount * price_wad` using
raw token units and ignores decimals entirely. SAC assets are 7 decimals and arbitrary
SEP-41 tokens differ, so when collateral and loan decimals differ the value is wrong by
orders of magnitude — and a market can be drained or wrongly liquidated. Choose one model
and apply it consistently:

- **Morpho Blue model (preferred):** the oracle returns a single price expressing the
  collateral/loan exchange rate scaled to `36 + loan_decimals - collateral_decimals`, and
  the market does one multiplication. This needs only one feed per market and no separate
  decimal handling in the market.
- **Two-feed model:** if keeping separate `get_price(collateral)` and `get_price(loan)`
  calls against a common numeraire (e.g. USD WAD), normalize each amount by its token
  decimals (`amount * WAD / 10^decimals`) before multiplying by the WAD price.

Document which model the oracle adapter implements, and make the market read token
decimals from the token contract — never assume 7 or 18.

## Step 14: Decide Upgrade Policy Explicitly

Morpho Blue is immutable. Current Astrion contracts expose `upgrade`.

Choose one:

- Strict Morpho mode:
  - Remove `upgrade` and `pause` from `market`.
  - Keep factory governance limited to enabling LLTVs and IRMs.
  - Vaults may remain configurable through timelocks because Vault V2 is a management layer.

- Pragmatic Stellar launch mode:
  - Keep `upgrade` temporarily behind governance/multisig.
  - Add `AppVersion`, `migrate`, and a written freeze date.
  - Once audited, deploy immutable markets with no upgrade entrypoint.

Do not leave this ambiguous. Trust assumptions are part of the product.

**Recommendation:** launch in pragmatic mode, finish in strict mode. Ship markets with a
governance/multisig-gated `upgrade` plus `AppVersion`/`migrate` and a published freeze
date; once audited, redeploy markets with the upgrade entrypoint removed so they are
immutable like Morpho Blue. Keep the vault upgradeable behind timelocks permanently —
Vault V2 is a management layer and is expected to evolve. Liquidation, accrual, and
share/collateral accounting must be correct under the immutable target from day one;
do not rely on a future upgrade to fix accounting.

Written policy: see `dev_docs/MORPHO_UPGRADE_POLICY.md`.

## Step 15: Testing Order

Implement tests in this order:

1. Math share conversions and rounding.
2. Market initialization and market ID determinism.
3. Supply/withdraw loan asset shares.
4. Supply/withdraw borrower collateral.
5. Borrow/repay with LLTV health checks.
6. Interest accrual updates total borrow and total supply assets.
7. Protocol fee accrual.
8. Full and partial liquidations.
9. Bad-debt socialization.
10. Account authorization.
11. Permissionless factory creation with enabled LLTV/IRM checks.
12. Vault deposit/mint/withdraw/redeem previews and execution.
13. Vault fee share minting.
14. Adapter allocate/deallocate into one market.
15. Adapter `real_assets` loss realization.
16. Absolute and relative cap checks.
17. Liquidity adapter entry/exit behavior.
18. Force deallocation path.
19. Gates.
20. Timelocks, revoke, and abdication.

Add property tests for:

- `total_supply_assets >= total_borrow_assets` except inside explicit bad-debt recognition paths.
- User supply share claims sum to `total_supply_assets` within rounding bounds.
- Borrow shares sum to `total_borrow_assets` within rounding bounds.
- A healthy account cannot become liquidatable without price, interest, borrow, or collateral withdrawal changes.
- Vault share price changes only through interest, loss realization, fees, donations, or rounding.
- A first depositor cannot inflate market or vault share price to grief later depositors (virtual shares/assets hold).
- Vault share price cannot increase faster than `max_rate` per second in a single accrual, regardless of donations or force-deallocate penalties.
- Value math is decimals-correct: a market with mismatched collateral/loan decimals produces the same health factor as an equivalent same-decimals market.
- `first_total_assets` is unset before and after every top-level vault entrypoint (no leak across transactions).
- An adversarial adapter re-entering during `real_assets`/allocate cannot extract more than it deposited or corrupt `total_assets`.

## Step 16: Suggested Implementation Milestones

Milestone 1: Morpho-like isolated market
- Rename debt/loan asset semantics.
- Add separate collateral accounting.
- Replace index accounting with share accounting.
- Implement borrow, repay, withdraw, collateral withdraw.
- Implement Morpho liquidation and bad debt.

Milestone 2: Permissionless market factory
- Market ID from full params.
- Enabled LLTV/IRM registries.
- Permissionless `create_market`.

Milestone 3: Vault core
- Vault shares, previews, deposits, withdrawals.
- Interest/loss realization from adapters.
- Management/performance fees.

Milestone 4: Market adapter
- Allocate/deallocate vault asset into isolated markets.
- Track per-market supply shares.
- Return allocation IDs.

Milestone 5: Risk controls
- Caps.
- Liquidity adapter.
- Force deallocate.
- Gates.
- Timelocks/abdication.

Milestone 6: Production hardening
- Snapshot tests.
- Fuzz/property tests.
- Resource profiling with realistic adapter counts.
- Audit-ready threat model.
- Decide immutable vs upgradeable deployment.

## Current Code Mapping

| Morpho concept | Current code | Required change |
|---|---|---|
| Isolated market | `contracts/market` | Good starting point, but asset semantics and accounting must change |
| Permissionless market creation | `contracts/market-factory` | Currently admin-gated and pair-keyed; make full-param keyed and permissionless |
| Oracle-agnostic market | `oracle-adapter` plus market config | Store oracle per isolated market and let users/curators choose |
| IRM allowlist | `interest-rate-model` | Add enabled IRM registry in factory |
| LLTV allowlist | not present | Add enabled LLTV registry in factory |
| Bad debt accounting | partial liquidation only | Add Morpho full liquidation and socialized loss |
| Market share math | index accounting, no virtual offset | Add `VIRTUAL_SHARES`/`VIRTUAL_ASSETS` to all market conversions (Step 3) |
| Decimals in value math | raw amounts × WAD price, decimals ignored | Normalize by token decimals or use single ratio oracle (Step 13) |
| Vault interest realization | not present | Asymmetric: full loss, `max_rate`-capped gain (Step 7) |
| Transaction-scoped state | not present | `first_total_assets` set per entrypoint, explicitly cleared (Step 10) |
| Reentrancy protection | CEI on token transfers only | Add reentrancy guard to vault adapter/gate flows (Step 7, Soroban notes) |
| Account management | not present | Add owner/operator authorization map |
| Vault V2 | not present | Add `contracts/vault` |
| Vault factory | not present | Add `contracts/vault-factory` |
| Adapter interface | not present | Add adapter trait and `contracts/market-adapter` |
| Vault caps | not present | Add cap storage and checks in vault |
| Gates | not present | Add optional gate contracts/trait |
| Timelocks | not present | Add action-key timelocks to vault and adapters |
| Core pool | `contracts/core-pool` | Keep separate; it is not Morpho-like |

## Soroban-Specific Notes

- Use `soroban_sdk::token::Client` for all SAC/SEP-41 token transfers.
- Use persistent storage for user balances, market positions, vault shares, caps, and adapter positions.
- Use instance storage for config, roles, and aggregate state.
- Extend TTL on every touched persistent key.
- Avoid unbounded loops in hot paths. Adapter and market lists must have practical limits or explicit resource warnings.
- Soroban has no ERC-2612 permit or EIP-712. Prefer explicit `require_auth` first.
- Soroban has no EVM transient storage. Use temporary storage keyed by ledger sequence where Morpho uses transaction-scoped transient state.
- Soroban has no delegatecall-style multicall. If batching is required, expose specific batch functions with bounded operation counts.
- Reentrancy differs from EVM but is not absent. Any vault or market flow that calls an
  adapter, gate, or token can be re-entered through callbacks. Add an explicit reentrancy
  guard to vault deposit/mint/withdraw/redeem/allocate/deallocate/force_deallocate, follow
  checks-effects-interactions everywhere (update shares/state before the external call —
  the current market already does this for token transfers), and test with an adversarial
  adapter that re-enters during `real_assets`/allocate.

## Non-Goals For The First Port

- ERC-20/SEP-41 transferable vault shares. Internal vault accounting is enough initially.
- Signature-based delegated authorization.
- Free flash loans.
- Callback hooks.
- Full MetaMorpho compatibility.
- Migrating existing `core-pool` liquidity automatically.

These can be added after the isolated market and vault adapter model is correct.
