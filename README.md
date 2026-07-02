<p align="center">
  <img src="https://raw.githubusercontent.com/Astrion-Market/interface/main/apps/web/public/logo192.png" alt="Astrion" width="192" />
</p>

<h3 align="center">The credit layer for Stellar — smart contracts.</h3>

<p align="center">
  A <a href="https://github.com/morpho-org/morpho-blue">Morpho Blue</a> port to Stellar, written in Rust for Soroban.
</p>

<p align="center">
  Rust · Soroban · WASM · SEP-40 · SEP-41
</p>

<p align="center">
  <a href="https://astrion.market"><strong>astrion.market</strong></a> ·
  <a href="#stellar-integration">Stellar Integration</a> ·
  <a href="#integrating-with-astrion">Integrate</a> ·
  <a href="#getting-started">Get started</a> ·
  <a href="docs/CONTRIBUTING.md">Contribute</a>
</p>

---

## Overview

Astrion is a **noncustodial, permissionless lending protocol** for Stellar. It is a
faithful port of [Morpho Blue](https://github.com/morpho-org/morpho-blue) — the
trustless EVM lending primitive — to Rust/Soroban, plus an ERC-4626-style vault
layer modelled on Morpho's MetaMorpho.

The protocol keeps Morpho's core properties:

- **Isolated two-asset markets.** Each market is a self-contained pair of one
  *loan asset* (supplied by lenders, borrowed by borrowers) and one *collateral
  asset*. A bad oracle or an insolvent position in one market cannot touch any
  other market.
- **One risk parameter — LLTV.** A position is healthy while
  `borrow_value ≤ collateral_value × LLTV`, and liquidatable once it crosses that
  line. No separate liquidation threshold, no close factor.
- **Oracle-agnostic pricing.** Markets price assets through a pluggable
  [SEP-40](#1-sep-40-price-oracles-reflector) oracle adapter — anyone can point a
  market at any Reflector-compatible feed.
- **Permissionless but bounded market creation.** Anyone can create a market; the
  factory only allows governance-vetted LLTV values and interest-rate models.
- **Bad-debt socialization.** If a liquidation seizes all collateral while debt
  remains, the residual is written off at liquidation time and shared pro-rata
  across that market's lenders — no bank-run dynamics, markets run indefinitely.
- **Passive vaults.** ERC-4626 vaults let passive lenders deposit one asset and
  have a curator allocate it across many markets through adapters.

Everything is compiled to WASM and runs on Stellar's Soroban execution
environment. For the web app see the
[interface repo](https://github.com/Astrion-Market/interface).

> **Why "Morpho Blue"?** The Morpho Blue whitepaper (in
> [`dev_docs/morpho-blue-whitepaper.pdf`](dev_docs/morpho-blue-whitepaper.pdf)) is
> the design spec this port follows. The port plan and per-step mapping to the
> whitepaper live in
> [`dev_docs/MORPHO_V2_SOROBAN_PORT_PLAN.md`](dev_docs/MORPHO_V2_SOROBAN_PORT_PLAN.md).

---

## Stellar Integration

Astrion is not a standalone system — it is composed of, and interoperates with,
existing Stellar building blocks. Every asset, price, and signature flows through
a Stellar-native standard. This section is the map of what Astrion plugs into and
exactly how.

```
                         ┌───────────────────────────┐
   Freighter / SEP-43    │        Wallet             │
   wallet signs auth ──▶ │  (@stellar/stellar-sdk)   │
                         └─────────────┬─────────────┘
                                       │ Soroban RPC (submit / simulate)
                                       ▼
        ┌──────────────────────────────────────────────────────────┐
        │                    ASTRION CONTRACTS                       │
        │                                                            │
        │   market / vault  ──token.transfer()──▶  SEP-41 tokens     │
        │        │                                 (Stellar Asset    │
        │        │                                  Contract / SAC)   │
        │        │                                                    │
        │        └── get_price() ──▶ oracle-adapter ──lastprice()──▶ │
        │                                              SEP-40 oracle  │
        │                                              (Reflector)    │
        └──────────────────────────────────────────────────────────┘
```

### 1. SEP-40 price oracles (Reflector)

Markets are oracle-agnostic: pricing is delegated to
[**SEP-40**](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0040.md),
the Stellar price-feed contract interface implemented by
[**Reflector**](https://reflector.network) and other providers.

Astrion never calls an oracle directly. It calls its own `oracle-adapter`, a thin,
safe wrapper that speaks the SEP-40 interface to whatever feed a market is
configured with:

```rust
// The exact SEP-40 surface Astrion consumes (contracts/oracle-adapter):
#[contractclient(name = "OracleClient")]
pub trait OracleTrait {
    fn lastprice(env: Env, asset: Asset) -> Option<PriceData>;  // SEP-40
    fn decimals(env: Env) -> u32;                               // SEP-40
}

// SEP-40 asset addressing — Stellar contract assets or off-chain tickers:
pub enum Asset {
    Stellar(Address),   // a Soroban token / SAC contract
    Other(Symbol),      // e.g. "BTC", "USD"
}
```

The adapter adds the safety Morpho markets assume from an oracle:

- Resolves a per-asset feed override, falling back to a default oracle.
- Cross-contract-calls `lastprice(asset)` on the SEP-40 feed.
- **Staleness check** — rejects observations older than `max_staleness` (default
  300 s).
- **Bounds check** — optional per-asset floor/ceiling to reject absurd prints.
- **WAD normalization** — converts the feed's native `decimals()` to the
  protocol's 1e18 fixed point, so market math never touches raw oracle scale.

To integrate a market with a live feed, point the adapter at a Reflector contract
and (optionally) register per-asset overrides — see
[Wiring your own oracle](#wiring-a-market-to-an-oracle). On testnet a
`mock-oracle` implements the same SEP-40 interface so simulations don't depend on
external feeds.

### 2. SEP-41 tokens & the Stellar Asset Contract (SAC)

All value moves as [**SEP-41**](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)
tokens — the Soroban token interface that the **Stellar Asset Contract (SAC)**
implements. This means Astrion works out of the box with:

- Any classic Stellar asset wrapped as a SAC (native XLM, circle USDC, etc.).
- Any custom Soroban token that implements SEP-41.

Markets and vaults custody and move funds purely through the SEP-41 client:

```rust
// Every supply / borrow / repay / liquidate settles via SEP-41 transfer.
token::Client::new(&env, &config.loan_asset)
    .transfer(&from, &to, &amount);

// Asset scale is read from the token itself — no hardcoded decimals.
let scale = pow10(token::Client::new(&env, asset).decimals());
```

Because the market pulls funds inside the invocation via `transfer(from = user, …)`,
there is **no separate ERC-20-style `approve` step** — the wallet authorizes the
transfer as part of the same signed transaction (see
[The authorization model](#the-authorization-model)).

### 3. Soroban RPC, Stellar SDK & wallets

Off-chain integrations (frontends, bots, keepers) talk to Astrion the standard
Stellar way:

- **Soroban RPC** (`https://soroban-testnet.stellar.org`) for simulating and
  submitting transactions and reading contract state.
- **[`@stellar/stellar-sdk`](https://github.com/stellar/js-stellar-sdk)** to build
  `InvokeHostFunction` operations, assemble auth entries, and parse `ScVal`
  results. The Astrion interface uses a hand-rolled SDK layer (no generated
  bindings) so every call is explicit.
- **Wallets** — [Freighter](https://www.freighter.app/) and any
  [SEP-43](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0043.md)
  wallet — sign the Soroban authorization entries that `require_auth()` produces.
- **Contract auth** — Astrion relies on Soroban's native `require_auth()` /
  `authorize_as_current_contract` rather than reimplementing signatures. Account
  delegation (Morpho's "authorization" feature) is exposed as
  `set_authorization(owner, operator, …)` (see
  [The authorization model](#the-authorization-model)).

### Integration surface at a glance

| Stellar building block | Standard | How Astrion uses it | Where |
|---|---|---|---|
| Reflector price feeds | **SEP-40** | Oracle-agnostic market pricing via `oracle-adapter` | `contracts/oracle-adapter` |
| Stellar Asset Contract / tokens | **SEP-41** | All custody & settlement; decimals read from token | `contracts/market`, `contracts/vault` |
| Soroban RPC + Stellar SDK | — | Off-chain reads, simulate/submit | `interface`, `ops/`, `sim/` |
| Freighter / wallets | **SEP-43** | Signs `require_auth()` entries | `interface` |

---

## Protocol Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         ASTRION CONTRACTS                          │
│                                                                    │
│  libs/math              Fixed-point WAD arithmetic (1e18) +        │
│                         Morpho virtual-share conversion            │
│  libs/market-types      Shared isolated-market data types          │
│                                                                    │
│  oracle-adapter         SEP-40 price-feed wrapper                  │
│    ├── set_asset_oracle  per-asset feed override                   │
│    └── get_price         staleness-checked, WAD-normalised price   │
│                                                                    │
│  interest-rate-model    Kinked two-slope utilization curve         │
│                                                                    │
│  ── the Morpho primitive ───────────────────────────────────────  │
│  market                 Isolated two-asset market                  │
│    ├── supply / withdraw          (lender loan-asset pool)         │
│    ├── supply_collateral / withdraw_collateral                     │
│    ├── borrow / repay             (Morpho share accounting)        │
│    ├── liquidate                  (LIF, no close factor, bad debt) │
│    └── set_authorization          (operator delegation)            │
│  market-factory         Permissionless, LLTV/IRM-bounded creation  │
│                                                                    │
│  ── the vault layer (MetaMorpho-style) ─────────────────────────  │
│  vault                  ERC-4626 vault + roles/timelocks/gates     │
│  vault-factory          Deploys vaults                             │
│  market-adapter         Routes vault liquidity into markets        │
│  adapter-registry       Allow-list of vetted adapters              │
│                                                                    │
│  liquidation-engine     Keeper-callable solvency helper            │
│  core-pool              Legacy shared pool (deprecated, see below) │
│                                                                    │
│  ── testnet only ───────────────────────────────────────────────  │
│  mock-oracle            Fixed-price SEP-40 oracle (never mainnet)  │
│  test-token             Admin-mintable SEP-41 tokens (USDC, WBTC)  │
└──────────────────────────────────────────────────────────────────┘
```

### Contract inventory

| Contract | Status | Description |
|---|---|---|
| `libs/math` | Production | WAD (1e18) arithmetic + Morpho virtual-share conversion |
| `libs/market-types` | Production | Shared isolated-market config/state/position types |
| `contracts/oracle-adapter` | Production | SEP-40 oracle wrapper: staleness, bounds, WAD normalisation |
| `contracts/interest-rate-model` | Production | Kinked two-slope rate curve |
| `contracts/market` | **Production** | Isolated Morpho market — supply/borrow/liquidate on share accounting |
| `contracts/market-factory` | Production | Permissionless market creation, bounded by enabled LLTV + IRM |
| `contracts/vault` | **Production** | ERC-4626 vault with curator/allocator roles, timelocks, gates |
| `contracts/vault-factory` | Production | Deploys vaults deterministically |
| `contracts/market-adapter` | Production | Allocates vault liquidity into isolated markets |
| `contracts/adapter-registry` | Production | Governance allow-list of adapters a vault may use |
| `contracts/liquidation-engine` | Scaffold | Keeper-callable liquidation helper |
| `contracts/core-pool` | Deprecated | Legacy Aave-style shared pool; superseded by isolated markets |
| `contracts/mock-oracle` | Testnet only | Fixed-price SEP-40 oracle for simulation — never mainnet |
| `contracts/test-token` | Testnet only | Admin-mintable SEP-41 token (USDC, WBTC instances) |

> **On CorePool.** The original Astrion design was an Aave-style shared pool
> (`core-pool`) with index accounting. The Morpho port replaces it with isolated
> markets + vaults. CorePool remains deployed for backwards compatibility but
> holds no markets in the current model and should not be integrated against —
> build on `market` / `vault` instead.

### Live testnet addresses

Source of truth: [`deployments/testnet/addresses.env`](deployments/testnet/addresses.env).

| Contract | Address |
|---|---|
| oracle-adapter | `CACJW5GN3RDF5LH3HZNYFJHLY5B257E2TPYMHFMCDFERL7E3WXNRK7QO` |
| rate-model | `CBUDQ3AVT4KLA4RGIN5A5PBCBKHUKJ2Z6LI3ANIPJHVWOE54BPM3WTUV` |
| market-factory | `CBM4BW772GFRAKX233ZJHGTAWN3WC4WPWGJIUJAO66B67NR7PYZ4PCWH` |
| vault-factory | `CCGA3ZPHRLPSVVH6JD7KC27HYWRP6KDHK6Z2G7XHJHUJU4ILX4VPRWBQ` |
| adapter-registry | `CAG67HPHYQW6LUTQRURE36SCWDPDQCCTRFAFWGO7LOHDTX5S3IYCJECI` |
| liquidation-engine | `CD3LP3GPNSV2WROGZQ5JLIRW7UXKVPQGAGUFPXNNPG3OXGYMIJ3RYXPZ` |
| **demo market** (WBTC collateral / USDC loan, 70% LLTV) | `CCKXGK4SE3XW5M4MRRX3NKV5UOTQ57V73OVYUEFHAQ2GJOCYNTW36MRH` |
| reverse market (USDC collateral / WBTC loan, 70% LLTV) | `CCADAHEHDOQZXKZ6LVIWHDJW5MQL7NILNPRVJVB5KEKWT56TY2CEL6WS` |
| demo vault | `CDBJHSHCWGZ3DXRBL6K4IWP5BGWBIOU5PWBNLVAUT24RZNKZHTUFNO3A` |
| market-adapter | `CCHVHZFO5U74DT4JE2GHTFPKPWI5FIJ4IABRNVFB2OBGTR4DOZGD6YID` |
| mock-oracle *(testnet)* | `CCKZQRVYXA7C66LJ6FAPKWTAB3RZTRM5S2RL473ZZ73LQYKKEJONMINQ` |
| test-usdc *(testnet)* | `CDZ4L3GZH4TGMOQC7XXPO3IKYABJM2FB2OGYXLZ7SPFFHM5HCLME3J7D` |
| test-wbtc *(testnet)* | `CCBD6JIWJDHWSURR3NU42QRIWHGUHF4XLMPES7PI6RLTQUCEBG5MVP6T` |

---

## Core Concepts

### Fixed-point math (WAD = 1e18)

All protocol values use 1e18 precision. Do not use floats. See `libs/math`.

```rust
let five_pct: i128 = 5 * WAD / 100;   // 50_000_000_000_000_000
let result       = wad_mul(a, b);      // (a * b) / WAD
```

### Morpho share accounting (with virtual shares)

Astrion does **not** use Aave-style indexes. Each market tracks four running
totals and converts between assets and shares on the fly:

```
supply_shares : pro-rata claim on total_supply_assets   (what a lender can redeem)
borrow_shares : pro-rata obligation vs total_borrow_assets (what a borrower owes)

assets  = shares × (totalAssets + 1)        / (totalShares + VIRTUAL_SHARES)
shares  = assets × (totalShares + VIRTUAL)  / (totalAssets + 1)      // VIRTUAL_SHARES = 1e6
```

The `+1` / `+VIRTUAL_SHARES` virtual amounts are Morpho's inflation-attack
defence. Rounding always favours protocol solvency: deposits round shares **down**,
withdrawals/borrows round the debt side **up**, debt is valued **up** for health.

Interest accrual increases `total_borrow_assets`; the lender claim
(`total_supply_assets`) grows by that interest **minus** the protocol reserve fee,
which accumulates in `fee_assets`.

### Health factor & the single LLTV

```
HF = collateral_value × LLTV / debt_value        (WAD; i128::MAX when no debt)

HF ≥ 1.0  →  healthy
HF  < 1.0  →  liquidatable
```

Collateral and loan legs are valued **independently**, each through its own
SEP-40 price and its own token decimals. There is exactly one LLTV per market and
no separate liquidation threshold — a Morpho property that improves collateral
ratios for borrowers.

### Liquidation: LIF, no close factor, bad debt

When `HF < 1`, anyone may liquidate. The liquidator repays debt and seizes
collateral scaled by the **Liquidation Incentive Factor**:

```
LIF = min( 1.15 , 1 / (1 − 0.30 × (1 − LLTV)) )
```

Morpho uses **no close factor** — a liquidatable position can be repaid fully. If
a liquidation empties the borrower's collateral while borrow shares remain, the
residual is recognised as **bad debt right then**: `total_borrow_assets` and
`total_supply_assets` are both reduced, lowering the supply-share price for every
lender in that market (socialized loss). `preview_liquidate()` lets bots simulate
the outcome (including bad debt) before sending.

### Permissionless but bounded markets

Anyone can call `market_factory.create_market(config)`. The factory enforces that
`config.lltv` is in the governance-enabled LLTV set and `config.rate_model` is an
enabled IRM. Governance can *add* options but cannot halt a market, change its
params, or touch user funds — matching Morpho's governance-minimized design.

### Vaults (ERC-4626 / MetaMorpho-style)

A vault is a single-asset, share-based wrapper (`deposit`/`mint`/`withdraw`/
`redeem`) for passive lenders. A **curator** enables markets and sets caps; an
**allocator** moves idle liquidity into markets through a **market-adapter**; the
**adapter-registry** limits which adapters a vault may use. Roles are guarded by
**timelocks** and can be permanently **abdicated**. Vaults use an asymmetric
interest model and always quote via `preview_*` / `accrue_interest_view()`.

---

## Integrating with Astrion

This section is for developers building **on top of** Astrion — frontends, bots,
vaults, adapters, or other protocols. Everything below is exercised against the
live testnet deployment.

### The authorization model

Astrion uses Soroban-native auth, not token approvals:

- **No `approve`.** State-changing calls take an explicit signer address
  (`supplier`, `payer`, `caller`, `liquidator`). Inside the call the market does
  `token.transfer(from = that address, …)`. The wallet signs one authorization
  entry that covers both the contract call and the token movement — `simulate`
  produces the entries, the wallet signs, you submit.
- **Delegation.** `on_behalf` lets a caller act for another account. A caller may
  only act on `on_behalf`'s position if it *is* that account or the account ran
  `set_authorization(owner, operator, true)`. This powers bundlers/keepers acting
  for users (Morpho's account-management feature).

### Isolated market — write API

`assets`/`shares` are raw token units / raw shares. Note `assets` precedes
`on_behalf`. For a normal user `caller == on_behalf == receiver`.

```
supply(supplier, assets, on_behalf)                       -> shares
withdraw(caller, assets, shares, on_behalf, receiver)     -> (assets, shares)
supply_collateral(supplier, assets, on_behalf)            -> ()
withdraw_collateral(caller, assets, on_behalf, receiver)  -> ()          // health-checked
borrow(caller, assets, on_behalf, receiver)               -> shares      // health-checked
repay(payer, assets, shares, on_behalf)                   -> (assets, shares)
liquidate(liquidator, borrower, seized_assets, repaid_shares,
          min_collateral_out, deadline)                   -> (seized_assets, repaid_assets)
set_authorization(owner, operator, authorized)            -> ()
accrue_interest()                                         -> ()
```

- **`withdraw` / `repay` take exactly one of `assets` or `shares`** (the other is
  `0`). Pass `shares = position.*_shares` to exit fully without leaving interest
  dust; pass `assets` to act by amount. Withdraw rounds shares up, repay rounds
  shares down — both favour the pool.
- **`liquidate` takes exactly one of `seized_assets` or `repaid_shares`**; the
  other is derived from the LIF. `min_collateral_out` and `deadline` protect
  against price moves and stale simulations.

### Isolated market — read API

```
get_market_params()      -> { loan_asset, collateral_asset, oracle_adapter, rate_model, lltv }
get_market_state()       -> { total_supply_assets, total_supply_shares,
                              total_borrow_assets, total_borrow_shares,
                              total_collateral, fee_assets, last_update_timestamp }
get_user_position(user)  -> { supply_shares, borrow_shares, collateral } | null
get_health_factor(user)  -> i128            // WAD; i128::MAX when no debt
preview_liquidate(borrower, repay_assets)
                         -> { liquidatable, repaid_assets, repaid_shares,
                              seized_collateral, bad_debt_assets }
```

Convert shares → assets **off-chain** with the same virtual-share math the
contract uses (never divide `totalAssets / totalShares` by hand):

```ts
const VIRTUAL_SHARES = 1_000_000n;
const toAssetsDown = (shares, totalAssets, totalShares) =>
  (shares * (totalAssets + 1n)) / (totalShares + VIRTUAL_SHARES);   // supply balances / TVL
// use a round-up variant for debt so you never understate what a borrower owes
```

### Discovering markets & building a frontend

```ts
import { Contract, rpc, scValToNative } from "@stellar/stellar-sdk";

const server = new rpc.Server("https://soroban-testnet.stellar.org");
const factory = new Contract(MARKET_FACTORY_ID);

// 1. Enumerate markets deployed by the factory.
const markets = /* simulate factory.get_markets() -> Vec<Address> */;

// 2. Read each market's params + state (read-only simulate, no signing).
//    get_market_params(), get_market_state(), get_user_position(user)

// 3. Value legs independently: amount * price_wad / 10^decimals, per asset.
//    Loan and collateral are different tokens with different SEP-40 prices.
```

- Reads are `server.simulateTransaction(...)` on a view method — no wallet, no fee.
- Writes: build the invoke op, `simulate` to get auth entries + resource fees,
  have the wallet sign, `sendTransaction`, then poll `getTransaction`.

### Building a liquidation bot / keeper

1. Watch markets and call `get_health_factor(borrower)` (or track the `borrow`,
   `liq`, `baddebt` events).
2. For any `HF < 1`, call `accrue_interest()` then `preview_liquidate(borrower,
   repay_assets)` to size the trade and see any `bad_debt_assets`.
3. Send `liquidate(...)` with `min_collateral_out` and a near-future `deadline`.

Economics and event schema: [`docs/KEEPER_ECONOMICS.md`](docs/KEEPER_ECONOMICS.md),
[`docs/EVENT_SCHEMA.md`](docs/EVENT_SCHEMA.md), bot patterns in
[`dev_docs/bots.md`](dev_docs/bots.md).

### Building on the vault layer (ERC-4626)

```
deposit(caller, assets, receiver) -> shares      mint(caller, shares, receiver) -> assets
withdraw(caller, assets, receiver, owner) -> shares  redeem(caller, shares, receiver, owner) -> assets
preview_deposit/mint/withdraw/redeem, convert_to_shares, convert_to_assets
total_supply(), total_assets(), balance_of(user), accrue_interest_view() -> AccrualPreview
```

Always quote with `preview_*` / `accrue_interest_view()` — they fold in pending
interest and fee dilution. Curators enable markets and set caps; allocators route
liquidity via `allocate` / `deallocate` through a registered `market-adapter`.

### Wiring a market to an oracle

A market prices assets through the `oracle-adapter`, which speaks SEP-40. To make
a market usable with a real feed:

```bash
# Point the adapter's default feed at a Reflector (SEP-40) oracle:
stellar contract invoke --id $ORACLE_ADAPTER_ID -- \
  set_default_oracle --new_oracle <REFLECTOR_CONTRACT>

# Optionally override the feed per asset (e.g. a dedicated BTC feed):
stellar contract invoke --id $ORACLE_ADAPTER_ID -- \
  set_asset_oracle --asset '{"Stellar":"<TOKEN>"}' --source '{ ... }'
```

Any contract implementing SEP-40's `lastprice(asset)` + `decimals()` works. The
adapter handles staleness, optional bounds, and WAD normalisation for you.

### Integration checklist

- [ ] Point config at [`deployments/testnet/addresses.env`](deployments/testnet/addresses.env).
- [ ] Value loan and collateral legs separately (different SEP-40 prices/decimals).
- [ ] Use virtual-share math for balances; never divide totals directly.
- [ ] Prefer `shares` for max withdraw/repay; re-simulate before submit (quotes
      drift with interest).
- [ ] Treat lender balances as **non-monotonic** — bad-debt socialization can
      reduce a redeemable balance with no user action. Surface it.
- [ ] There is **no protocol-wide health factor** — HF is per market.

---

## Getting Started

### Prerequisites

| Tool | Version | Install |
|---|---|---|
| [Rust](https://rustup.rs) | stable | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| WASM target | — | `rustup target add wasm32v1-none` |
| [Stellar CLI](https://github.com/stellar/stellar-cli) | **≥ 26** | `cargo install --locked stellar-cli --features opt` |
| [jq](https://jqlang.github.io/jq/) | any | `apt install jq` / `brew install jq` |

```bash
rustup target list --installed | grep wasm32v1-none
stellar --version          # must be ≥ 26
jq --version
```

### Clone, build & test

```bash
git clone https://github.com/Astrion-Market/contracts.git
cd contracts

stellar contract build        # build all contracts to WASM
cargo test                    # run all unit tests

# Single contract:
stellar contract build --package market
cargo test -p market
```

---

## Project Structure

```
contracts/
├── Cargo.toml                   # Workspace root
├── Makefile                     # Top-level make entrypoint
│
├── contracts/                   # Soroban smart contracts
│   ├── oracle-adapter/          # SEP-40 oracle wrapper
│   ├── interest-rate-model/     # Kinked two-slope curve
│   ├── market/                  # Isolated Morpho market
│   ├── market-factory/          # Permissionless market creation
│   ├── vault/                   # ERC-4626 vault
│   ├── vault-factory/           # Vault deployer
│   ├── market-adapter/          # Vault → market liquidity router
│   ├── adapter-registry/        # Adapter allow-list
│   ├── liquidation-engine/      # Keeper helper (scaffold)
│   ├── core-pool/               # Legacy pool (deprecated)
│   ├── mock-oracle/             # Testnet-only SEP-40 oracle
│   └── test-token/              # Testnet-only SEP-41 tokens
│
├── libs/
│   ├── math/                    # WAD arithmetic + virtual-share conversion
│   └── market-types/            # Shared isolated-market types
│
├── ops/                         # Shell deployment scripts
├── mk/                          # Makefile include fragments
├── sim/                         # Testnet simulation harness
├── deployments/                 # Per-network addresses + state
├── dev_docs/                    # Port plan, whitepaper, integration notes
└── docs/                        # Contributor, security, keeper docs
```

Each contract follows the same internal layout: `lib.rs` (public `#[contract]`),
`types.rs`, `storage.rs`, `errors.rs`, `test.rs`.

---

## Deployment

> **Deploy once, upgrade forever.** Astrion contracts are deployed a single time
> per network. Re-deploying creates new addresses and silently breaks every
> integration pointed at the existing ones. The deployment system enforces this.

```
ops/deploy-all.sh [network] [source]
```

1. Builds all contracts via `stellar contract build`.
2. Pins SHA-256 checksums → `deployments/{network}/checksums.sha256`.
3. Checks deployment state — **blocks if core contracts are already live**.
4. Deploys each contract in dependency order, capturing the contract ID.
5. Records state → `deployments/{network}/state.json`.
6. Writes addresses → `deployments/{network}/addresses.env`.
7. Generates a timestamped report → `deployments/{network}/report-*.md`.

### The deployment guard

If the core protocol contracts already exist in `state.json` with status
`deployed`/`initialized`, `deploy-all.sh` exits and shows the upgrade commands
instead. Bypass for a genuinely fresh environment with `FORCE=1`:

```bash
FORCE=1 make deploy-all NETWORK=testnet SOURCE=deployer
```

`FORCE=1` bypasses only the global guard — individual contract idempotency still
applies.

### Testnet vs mainnet

| Behaviour | Testnet | Mainnet |
|---|---|---|
| mock-oracle + test tokens deployed | Yes | No |
| Demo market/vault seeded | Yes | No |
| Deployment guard | Yes | Yes |
| Manual confirmation prompt | No | Yes — interactive pause before deploy |
| CI/CD trigger | Push to `main` | Manual dispatch with approval |

```bash
make deploy-testnet   # protocol + test tokens + mock-oracle + demo market/vault
make deploy-mainnet   # confirmation prompt, then core only
```

### Testnet-only contracts

**`mock-oracle`** — a fixed-price SEP-40 oracle for simulation. Admin can update
prices at any time. Never mainnet.

```bash
# Set WBTC to $60,000 (7-decimal fixed-point):
stellar contract invoke --network testnet --source deployer \
  --id $MOCK_ORACLE_ID -- set_price \
  --asset '{"Stellar":"<WBTC_CONTRACT>"}' --price 600000000000
```

**`test-usdc` / `test-wbtc`** — admin-mintable SEP-41 tokens for testing.

```bash
stellar contract invoke --network testnet --source deployer \
  --id $TEST_USDC_ID -- mint \
  --account <RECIPIENT_ADDRESS> --amount 100000000000
```

### Initialize, verify, upgrade, rotate admin

```bash
make init-testnet                                   # idempotent init in dependency order
make verify NETWORK=testnet                         # WASM-hash + health-invocation checks
make status NETWORK=testnet                         # print deployment state table
make upgrade CONTRACT=market NETWORK=testnet        # in-place WASM upgrade (address unchanged)
make upgrade-all NETWORK=testnet                    # diff table, one confirmation
make rotate-admin NEW_ADMIN=G<MULTISIG> NETWORK=testnet
```

All commands accept `NETWORK=testnet` (default) or `NETWORK=mainnet` and
`SOURCE=deployer`. The upgrade policy and trust model are documented in
[`dev_docs/MORPHO_UPGRADE_POLICY.md`](dev_docs/MORPHO_UPGRADE_POLICY.md).

### Deployment quick reference

| Command | What it does |
|---|---|
| `make dry-run` | Preview all deploy commands without executing |
| `make build` | Compile all contracts to WASM |
| `make deploy-testnet` | Deploy everything to testnet |
| `make deploy-mainnet` | Deploy core contracts to mainnet (confirmation required) |
| `make init-testnet` | Initialize all contracts on testnet |
| `make verify` | WASM hash check + health invocations |
| `make status` | Print deployment state table |
| `make upgrade CONTRACT=X` | Upgrade a single contract in-place |
| `make upgrade-all` | Upgrade all contracts with diff preview |
| `make rotate-admin NEW_ADMIN=G…` | Transfer admin to a new address |

---

## Testnet Simulation

`sim/` contains a rotating-role harness that exercises every protocol operation in
a realistic sequence.

```bash
make sim-setup NETWORK=testnet                 # generate wallets, fund via friendbot, mint tokens
make sim-run   NETWORK=testnet                 # execute one round
make sim-loop  ROUNDS=10 DELAY=30 NETWORK=testnet
make sim-reset NETWORK=testnet                 # reset the round counter (does not undo on-chain state)
```

Five wallets rotate through supplier / borrower / liquidator roles each round;
for round `R`, wallet `test-N` takes role `(N-1 + R-1) % 5`. Each round prints
verification sections (oracle health, rate curve, balances, supply, collateral,
borrow, liquidation check, market state). See
[`dev_docs/TESTNET_FAUCET_AND_LIQUIDITY.md`](dev_docs/TESTNET_FAUCET_AND_LIQUIDITY.md)
for faucet + seeded-market details.

---

## Testing

All contracts use
[soroban-sdk testutils](https://docs.rs/soroban-sdk/latest/soroban_sdk/testutils/index.html):

```bash
cargo test                                  # all workspace tests
cargo test -p market -- --nocapture         # one contract, with output
cargo test -p market test_liquidate_bad_debt
```

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the contributor guide and
[`dev_docs/MORPHO_V2_SOROBAN_PORT_PLAN.md`](dev_docs/MORPHO_V2_SOROBAN_PORT_PLAN.md)
for the per-step port plan and testing order.

---

## Roadmap

- [x] Fixed-point math + Morpho virtual-share conversion
- [x] SEP-40 oracle adapter (production)
- [x] Kinked interest rate model (production)
- [x] Isolated Morpho market — supply/collateral/borrow/repay/liquidate + bad debt
- [x] Permissionless, LLTV/IRM-bounded market factory
- [x] ERC-4626 vault + roles/timelocks/gates + factory + market-adapter + registry
- [x] Account authorization (operator delegation)
- [x] Testnet deployment — all contracts live, demo market + vault seeded
- [x] Simulation harness (rotating roles)
- [ ] LiquidationEngine — full implementation + tests
- [ ] Interest accrual: Taylor-series compounding
- [ ] Audit
- [ ] Mainnet launch

---

## Security

This codebase is **pre-audit — do not use in production.**

Planned audit scope:
- Share-accounting rounding & virtual-share inflation defence
- Oracle manipulation resistance (staleness, bounds, decimals normalisation)
- Health-factor edge cases (zero debt, zero collateral)
- Bad-debt socialization correctness
- Liquidation incentive bounds
- Cross-contract invariants (Soroban's WASM model prevents classic reentrancy but
  not all cross-contract issues)

See [docs/SECURITY_CHECKLIST.md](docs/SECURITY_CHECKLIST.md),
[docs/PR_REVIEW_CHECKLIST.md](docs/PR_REVIEW_CHECKLIST.md), and
[docs/ADVANCED_SECURITY_PLAN.md](docs/ADVANCED_SECURITY_PLAN.md).

---

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for setup, branch conventions,
commit style, and the deployment/simulation systems.

---

## License

```
MIT License — Copyright (c) 2026 Astrion Labs
```

---

<p align="center">
  Built by <a href="https://astrion.market">Astrion Labs</a> · Stellar Soroban ·
  a Morpho Blue port
</p>
