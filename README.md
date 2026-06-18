<p align="center">
  <img src="https://raw.githubusercontent.com/Astrion-Market/interface/main/apps/web/public/logo192.png" alt="Astrion" width="192" />
</p>

<h3 align="center">The credit layer for Stellar — smart contracts.</h3>

<p align="center">
  Rust · Soroban · WASM
</p>

<p align="center">
  <a href="https://astrion.market"><strong>astrion.market</strong></a> ·
  <a href="#getting-started">Get started</a> ·
  <a href="docs/CONTRIBUTING.md">Contribute</a> ·
  <a href="docs/ROADMAP.md">Roadmap</a> ·
  <a href="https://twitter.com/astrionmarket">Twitter</a>
</p>

---

## Overview

This repository contains the Soroban smart contracts for the Astrion hybrid lending protocol. Written in Rust and compiled to WASM, they run on Stellar's Soroban execution environment.

For the frontend application see the [interface repo](https://github.com/Astrion-Market/interface).

---

## Protocol Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                       ASTRION CONTRACTS                        │
│                                                                │
│  libs/math              Fixed-point WAD arithmetic (1e18)      │
│                                                                │
│  oracle-adapter         SEP-0402 price feed wrapper            │
│    ├── set_asset_oracle  per-asset override                    │
│    └── get_price         staleness-checked WAD price           │
│                                                                │
│  interest-rate-model    Kinked two-slope utilization curve     │
│                                                                │
│  core-pool              Shared liquidity pool                  │
│    ├── supply / withdraw                                       │
│    ├── borrow / repay                                          │
│    └── accrue_interest   index-based O(1) accounting           │
│                                                                │
│  market                 Isolated two-asset pool                │
│  market-factory         Deploys isolated markets               │
│                                                                │
│  liquidation-engine     Keeper-callable solvency enforcer      │
│                                                                │
│  ── testnet only ──────────────────────────────────────────── │
│  mock-oracle            Fixed-price SEP-0402 oracle (devnet)   │
│  test-usdc / test-wbtc  Admin-mintable test tokens             │
└────────────────────────────────────────────────────────────────┘
```

### Contract inventory

| Contract | Status | Description |
|---|---|---|
| `libs/math` | Production | Fixed-point WAD (1e18) arithmetic |
| `contracts/oracle-adapter` | Production | SEP-0402 oracle, staleness checks, WAD normalisation |
| `contracts/interest-rate-model` | Production | Kinked two-slope rate curve |
| `contracts/core-pool` | Production | Shared liquidity pool — supply/borrow/repay/withdraw |
| `contracts/market` | Scaffold | Isolated two-asset lending pool |
| `contracts/market-factory` | Production | Factory deployer for isolated markets |
| `contracts/liquidation-engine` | Scaffold | Keeper-callable liquidation enforcer |
| `contracts/mock-oracle` | Testnet only | Fixed-price oracle for simulation — never mainnet |
| `contracts/test-token` | Testnet only | Admin-mintable SEP-41 token (USDC, WBTC instances) |

> **Scaffold** — all types, storage keys, errors, and function signatures are final. Function bodies contain ordered TODO comments. Implement incrementally following the TODOs.

### Live testnet addresses

| Contract | Address |
|---|---|
| oracle-adapter | `CCVODOMSC3YBNTXDWRVKFFTSGLBYWZYGZ2N7WCL36WUGJVZVRH5Q5E2E` |
| rate-model | `CBHR3TTEVYHOTYCS2E3U2WOCLHEA25R2GOFCLMI6F5VT6A4OZ3NMRCF4` |
| core-pool | `CCOHNWEPMIBPNI2B43NQYVGHVN3344FZKOALBN77HSV2BRR3VYYY3TST` |
| liquidation-engine | `CCOBO7ABVY4XL2JNUXQH5EPQYIOOKAKQV656Z6URT5U3PD3AGKXIW7TB` |
| market-factory | `CBMMHD3EFPLI7PAQJGSR2S5ZM6ZTDTRVZMXBEBEWK3TJN5G5QHWUGWLN` |
| mock-oracle | `CCSGR2PW5LLTW6MHSPCFRFTNF5UUKGCXSN5DDK6UPF5PVJHPRIWVQHMW` |
| test-usdc | `CCMO7GBSI5NNSU4DGTW4X2G6EEVIECQABIGKIUR55YRJ4VVMYN2ODSYL` |
| test-wbtc | `CANNEKQWI5GAEVBDTWNSVSA3SZLI4QHVF3SD3KRU4QX7YKAIUBTVJ4IS` |

---

## Getting Started

### Prerequisites

| Tool | Version | Install |
|---|---|---|
| [Rust](https://rustup.rs) | stable | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| WASM target | — | `rustup target add wasm32v1-none` |
| [Stellar CLI](https://github.com/stellar/stellar-cli) | **≥ 26** | `cargo install --locked stellar-cli --features opt` |
| [jq](https://jqlang.github.io/jq/) | any | `apt install jq` / `brew install jq` |

Verify everything is installed:

```bash
rustup target list --installed | grep wasm32v1-none
stellar --version          # must be ≥ 26
jq --version
```

### Clone and build

```bash
git clone https://github.com/Astrion-Market/contracts.git
cd contracts

# Build all contracts (uses stellar-cli's build toolchain)
stellar contract build

# Run all unit tests
cargo test
```

### Build a single contract

```bash
stellar contract build --package oracle-adapter
stellar contract build --package core-pool
```

### Run tests for a single contract

```bash
cargo test -p oracle-adapter
cargo test -p interest-rate-model
cargo test -p astrion-math
cargo test -p core-pool
```

---

## Project Structure

```
contracts/
├── Cargo.toml                   # Workspace root
├── Makefile                     # Top-level make entrypoint
│
├── contracts/                   # Soroban smart contracts
│   ├── oracle-adapter/
│   ├── interest-rate-model/
│   ├── core-pool/
│   ├── market/
│   ├── market-factory/
│   ├── liquidation-engine/
│   ├── mock-oracle/             # Testnet only
│   └── test-token/              # Testnet only
│
├── libs/
│   └── math/                    # Shared fixed-point arithmetic
│
├── ops/                         # Shell deployment scripts
│   ├── deploy-all.sh            # Deploy all contracts (with guard)
│   ├── init-all.sh              # Initialize contracts in order
│   ├── upgrade-contract.sh      # In-place WASM upgrade
│   ├── upgrade-all.sh           # Batch upgrade with diff table
│   ├── verify-deploy.sh         # WASM hash + health checks
│   ├── rotate-admin.sh          # Transfer admin to multisig
│   └── state.sh                 # Deployment state management
│
├── mk/                          # Makefile include fragments
│   ├── deploy.mk
│   ├── build.mk
│   └── sim.mk
│
├── sim/                         # Testnet simulation harness
│   ├── setup.sh                 # Generate wallets, fund, mint tokens
│   ├── run.sh                   # Execute one simulation round
│   ├── wallets.env              # Test wallet public keys (committed)
│   ├── tokens.env               # Test token addresses (committed)
│   └── state.env                # Round counter (committed)
│
├── deployments/
│   ├── config.env.example       # Config template (fill + copy per network)
│   ├── testnet/
│   │   ├── addresses.env        # Live contract IDs
│   │   ├── state.json           # Deployment state + WASM hashes
│   │   ├── checksums.sha256     # Build fingerprints
│   │   └── config.env           # Active testnet config (not a secret)
│   └── mainnet/
│       ├── config.env.example   # Stricter mainnet template
│       └── addresses.env        # Mainnet contract IDs (after launch)
│
└── docs/
    ├── CONTRIBUTING.md
    ├── DEVELOPMENT.md           # Phase-by-phase contributor guide
    └── ROADMAP.md
```

Each contract follows the same internal layout:

```
contracts/<name>/
├── Cargo.toml
└── src/
    ├── lib.rs       # Public contract — #[contract] + #[contractimpl]
    ├── types.rs     # #[contracttype] structs and enums
    ├── storage.rs   # Typed storage helpers
    ├── errors.rs    # #[contracterror] enum
    └── test.rs      # Soroban testutils test suite
```

---

## Core Concepts

### Fixed-point math (WAD = 1e18)

All protocol values use 1e18 precision. Do not use floats. See `libs/math`.

```rust
// 5% expressed as WAD:
let five_pct: i128 = 5 * WAD / 100;  // 50_000_000_000_000_000

// Multiply two WAD values:
let result = wad_mul(a, b);  // (a * b + WAD/2) / WAD
```

### Index-based accounting

Inspired by Aave V3. Supply and borrow indexes grow monotonically:

```
supply_index:  1e18 → grows as lenders earn interest
borrow_index:  1e18 → grows as borrowers accumulate debt

User stores scaled balance:   scaled = real / index
Real balance at any time:     real   = scaled * index
```

Interest accrual is O(1): update the index, all balances update implicitly.

### Health Factor

```
HF = (Σ collateral_value × liquidation_threshold) / Σ debt_value

HF ≥ 1.0  →  safe
HF  < 1.0  →  liquidatable
```

### Oracle (SEP-0402)

The oracle adapter wraps any [Reflector](https://reflector.network)-compatible oracle:

- Fetches `lastprice(asset)` via cross-contract call
- Validates age ≤ `max_staleness_secs` (default: 300 s)
- Normalises raw price to WAD regardless of the oracle's decimal precision
- Per-asset overrides: different oracles for different assets via `set_asset_oracle`

---

## Deployment

> **Deploy once, upgrade forever.** Astrion contracts are designed to be deployed a single time per network. Re-deploying creates new addresses and silently breaks every integration pointed at the existing ones. The deployment system enforces this.

### How deployment works

```
ops/deploy-all.sh [network] [source]
```

1. Builds all contracts via `stellar contract build`
2. Pins SHA-256 checksums → `deployments/{network}/checksums.sha256`
3. Checks deployment state — **blocks if all core contracts are already live**
4. Deploys each contract in dependency order, capturing the contract ID
5. Records state → `deployments/{network}/state.json`
6. Writes all addresses → `deployments/{network}/addresses.env`
7. Generates a timestamped report → `deployments/{network}/report-*.md`

### The deployment guard

If all five core protocol contracts (`oracle-adapter`, `rate-model`, `core-pool`, `liquidation-engine`, `market-factory`) already exist in `state.json` with status `deployed` or `initialized`, `deploy-all.sh` exits with an explicit error and shows the upgrade commands instead:

```
╔══════════════════════════════════════════════════════════════╗
║  CONTRACTS ALREADY DEPLOYED — USE UPGRADE INSTEAD           ║
╚══════════════════════════════════════════════════════════════╝

All core protocol contracts are already live on 'testnet'.
Re-deploying creates NEW addresses and breaks every integration
pointed at the current ones.

  Upgrade all contracts (same addresses, new WASM only):
    make upgrade-all  NETWORK=testnet SOURCE=deployer

  Upgrade a single contract:
    make upgrade CONTRACT=oracle-adapter      NETWORK=testnet
    ...
```

To bypass the guard for a genuinely fresh environment (e.g. first-time setup or CI reset), use `FORCE=1`:

```bash
FORCE=1 make deploy-all NETWORK=testnet SOURCE=deployer
```

`FORCE=1` bypasses only the global guard — individual contract idempotency still applies. Already-deployed contracts are skipped automatically regardless of `FORCE`.

### Deployment state

`deployments/{network}/state.json` is the source of truth. Each contract entry tracks:

- `contract_id` — on-chain address
- `wasm_sha256` — SHA-256 of the deployed WASM
- `status` — `pending` → `deployed` → `initialized`
- `deployed_at`, `initialized_at` — UTC timestamps

```bash
make status NETWORK=testnet
```

```
Astrion Deployment State
Network: testnet       Deployer: deployer       Updated: 2026-05-25T23:36:46Z

CONTRACT                  ADDRESS                                                   STATUS
───────────────────────── ───────────────────────────────────────────────────────── ─────────────
core-pool                 CCOHNWEP…                                                  initialized
liquidation-engine        CCOBO7AB…                                                  initialized
market                    CBC5C6IY…                                                  deployed
market-factory            CBMMHD3E…                                                  initialized
mock-oracle               CCSGR2PW…                                                  initialized
oracle-adapter            CCVODOMS…                                                  initialized
rate-model                CBHR3TTE…                                                  initialized
test-usdc                 CCMO7GBS…                                                  initialized
test-wbtc                 CANNEKQW…                                                  initialized
```

### Testnet vs mainnet

The deployment system treats testnet and mainnet differently:

| Behaviour | Testnet | Mainnet |
|---|---|---|
| mock-oracle deployed | Yes | No |
| test-usdc / test-wbtc deployed | Yes | No |
| CorePool markets auto-configured | Yes (USDC + WBTC) | No |
| Deployment guard | Yes | Yes |
| Manual confirmation prompt | No | Yes — interactive pause before deploy |
| CI/CD trigger | Push to `main` | Manual workflow dispatch with approval |

```bash
make deploy-testnet   # deploys protocol + test tokens + mock-oracle
make deploy-mainnet   # shows confirmation prompt, then deploys core only
```

### Testnet-only contracts

**`mock-oracle`** — A fixed-price SEP-0402 oracle for testnet simulation. Admin can call `set_price(asset, price)` to update prices at any time. Never deploy to mainnet.

```bash
# Set WBTC to $60,000 (7-decimal fixed-point)
stellar contract invoke --network testnet --source deployer \
  --id $MOCK_ORACLE_ID -- set_price \
  --asset '{"Stellar":"<WBTC_CONTRACT>"}' --price 600000000000
```

**`test-usdc` / `test-wbtc`** — Admin-mintable SEP-41 tokens. The deployer account can mint arbitrary balances for testing. Never deploy to mainnet.

```bash
stellar contract invoke --network testnet --source deployer \
  --id $TEST_USDC_ID -- mint \
  --account <RECIPIENT_ADDRESS> --amount 100000000000
```

### Initializing contracts

After deployment, contracts must be initialized. `init-all.sh` does this in the correct dependency order and is fully idempotent — safe to re-run if interrupted:

```bash
make init-testnet   # or: make init-all NETWORK=testnet SOURCE=deployer
```

On testnet, `init-all.sh` additionally:

1. Initializes `mock-oracle` with the admin address
2. Sets USDC price to $1.00 and WBTC price to $60,000 in mock-oracle
3. Sets `mock-oracle` as the default oracle on `oracle-adapter`
4. Registers per-asset overrides for test-usdc and test-wbtc
5. Adds USDC market (LTV 85%, liq threshold 90%) to CorePool
6. Adds WBTC market (LTV 70%, liq threshold 80%) to CorePool

### Configuration

Copy the template and fill in your addresses:

```bash
cp deployments/config.env.example deployments/testnet/config.env
```

Required fields:

```bash
# Your admin key's Stellar public address (G...)
ADMIN_ADDRESS=G...

# Wallet that receives protocol reserve fees
TREASURY_ADDRESS=G...

# Only needed if oracle-adapter has NOT yet been initialized.
# Leave blank after first init — init-all.sh checks the state and skips.
DEFAULT_ORACLE_ID=
```

Everything else has safe testnet defaults.

### Verifying a deployment

```bash
make verify NETWORK=testnet
```

For each contract this checks:

1. **WASM hash** — fetches on-chain bytecode and compares SHA-256 against the pinned fingerprint in `checksums.sha256`. Catches supply-chain issues.
2. **Health invocations** — calls read-only methods (`admin`, `config`, `get_borrow_rate`, etc.) to confirm each contract is alive and initialized.

### Upgrading contracts

Contracts are upgradeable in-place. The deployed address never changes — only the WASM bytecode is replaced.

**Upgrade a single contract:**

```bash
make upgrade CONTRACT=oracle-adapter NETWORK=testnet SOURCE=deployer
```

This:
1. Builds the latest WASM
2. Compares SHA-256 — skips if unchanged
3. Uploads new WASM to the network
4. Calls `upgrade(new_wasm_hash)` on the live contract
5. Updates `state.json` with the new hash and timestamp

**Upgrade all contracts at once:**

```bash
make upgrade-all NETWORK=testnet SOURCE=deployer
```

Shows a diff table first:

```
CONTRACT               OLD SHA-256          NEW SHA-256          CHANGE
─────────────────────  ──────────────────── ──────────────────── ──────
oracle-adapter         7dfac9328d30438d…    7dfac9328d30438d…    none
rate-model             cb80e2f80a451e37…    NEW_HASH…            upgrade
core-pool              ab10f2933b037ae2…    NEW_HASH…            upgrade
```

One confirmation covers all contracts. Any contract where old = new is skipped automatically.

**Aliases for all upgradeable contracts:**

`oracle-adapter`, `rate-model`, `core-pool`, `liquidation-engine`, `market`, `market-factory`, `mock-oracle`, `test-usdc`, `test-wbtc`

### Admin rotation

After testnet validation, transfer admin from a single key to a multisig before mainnet:

```bash
make rotate-admin NEW_ADMIN=G<MULTISIG_ADDRESS> NETWORK=testnet SOURCE=deployer
```

This calls `transfer_admin(new_admin)` on every initialized contract in the correct order. The old key remains the `--source` for the rotation transaction — have it available.

### Deployment quick reference

| Command | What it does |
|---|---|
| `make dry-run` | Preview — shows all deploy commands without executing |
| `make build` | Compile all contracts to WASM |
| `make deploy-testnet` | Deploy all contracts + test tokens to testnet |
| `make deploy-mainnet` | Deploy core contracts to mainnet (confirmation required) |
| `make init-testnet` | Initialize all contracts on testnet |
| `make init-mainnet` | Initialize all contracts on mainnet |
| `make verify` | WASM hash check + health invocations |
| `make status` | Print deployment state table |
| `make upgrade CONTRACT=X` | Upgrade a single contract in-place |
| `make upgrade-all` | Upgrade all contracts with diff preview |
| `make rotate-admin NEW_ADMIN=G…` | Transfer admin to a new address |

All commands accept `NETWORK=testnet` (default) or `NETWORK=mainnet` and `SOURCE=deployer` (default).

### Starting fresh (destructive)

Only needed when the contracts themselves are broken and cannot be upgraded:

```bash
# Wipe local state — the old on-chain contracts remain forever but are abandoned
rm deployments/testnet/state.json deployments/testnet/addresses.env
FORCE=1 make deploy-testnet
make init-testnet
make verify NETWORK=testnet
```

The old contract instances remain on-chain permanently (Soroban contracts are immutable once deployed). You get new addresses that nobody is using yet.

---

## Testnet Simulation

The `sim/` directory contains a rotating-role simulation harness that exercises every protocol operation in a realistic sequence.

### How it works

Five test wallets (`test1`–`test5`) rotate through five roles each round:

```
Position 0: supplier-A    deposits USDC liquidity
Position 1: supplier-B    deposits USDC liquidity
Position 2: borrower-A    deposits WBTC collateral, borrows USDC
Position 3: borrower-B    deposits WBTC collateral, borrows USDC
Position 4: liquidator    checks health factor, liquidates if needed
```

For round R, wallet `test-N` takes role: `(N-1 + R-1) % 5`

Each round produces 8 verification sections:

```
1. Oracle health       — oracle-adapter admin + max_staleness
2. Interest rate curve — borrow rate at 0%, 20%, 50%, 80%, 95% utilization
3. Wallet USDC balances — all 5 wallets
4. Wallet WBTC balances — all 5 wallets
5. Supply              — suppliers deposit 100 USDC each
5b. Collateral         — borrowers deposit 0.1 WBTC each as collateral
6. Borrow              — borrowers draw 60 USDC each ($6k WBTC → max $4,200 USDC at 70% LTV)
7. Liquidation check   — health factor query + conditional liquidation attempt
8. CorePool state      — market list + USDC market state (indices, totals)
```

### Setup (first time)

```bash
# Generate test1–test5 wallets, fund via friendbot, mint test tokens
make sim-setup NETWORK=testnet
```

This requires contracts to be deployed and initialized first (`make deploy-testnet && make init-testnet`).

### Run one round

```bash
make sim-run NETWORK=testnet
```

### Run multiple rounds

```bash
make sim-loop ROUNDS=10 DELAY=30 NETWORK=testnet
# ROUNDS: number of rounds to execute (default: 5)
# DELAY:  seconds between rounds (default: 30)
```

### Reset the simulation

```bash
make sim-reset NETWORK=testnet
# Resets SIM_ROUND counter; does not undo on-chain state
```

### What a passing round looks like

```
╔══════════════════════════════════════════════════════════════╗
║  Astrion Simulation  Round 12                                ║
╚══════════════════════════════════════════════════════════════╝

  supplier-A → test5   supplier-B → test1
  borrower-A → test2   borrower-B → test3   liquidator → test4

1. Oracle health
  ✓  oracle-adapter admin: "GAAK4H…"
  ✓  oracle-adapter max_staleness: 300

2. Interest rate curve
  ✓  borrow rate @ 0%: 0.00%
  ✓  borrow rate @ 80% (kink): 12.00%
  ✓  borrow rate @ 95%: 68.25%

5. Supply — test5 + test1 (USDC liquidity)
  ✓  test5 supplied 100 USDC to CorePool
  ✓  test1 supplied 100 USDC to CorePool

5b. Collateral — test2 + test3 (WBTC → CorePool)
  ✓  test2 supplied 0.1 WBTC as collateral
  ✓  test3 supplied 0.1 WBTC as collateral

6. Borrow — test2 + test3 (60 USDC each)
  ✓  test2 borrowed 60 USDC from CorePool
  ✓  test3 borrowed 60 USDC from CorePool

7. Liquidation check — test4
  ✓  test2 health factor: 122.9997
  ✓    test2 is healthy — no liquidation needed

8. CorePool state
  ✓  core-pool markets: ["CCMO7GBS…", "CANNEKQW…"]
  ✓  core-pool USDC market state: { total_scaled_borrow: 7799971937, … }
```

Scaffold sections not yet implemented print `─ skipped` and do not break the round.

---

## Frontend Integration

All contract addresses live in `deployments/testnet/addresses.env`. Source it to configure your frontend:

```bash
source deployments/testnet/addresses.env

# Next.js .env.local
cat <<EOF > ../interface/.env.local
NEXT_PUBLIC_STELLAR_NETWORK=testnet
NEXT_PUBLIC_STELLAR_RPC_URL=https://soroban-testnet.stellar.org
NEXT_PUBLIC_ORACLE_ADAPTER_ID=${ORACLE_ADAPTER_ID}
NEXT_PUBLIC_RATE_MODEL_ID=${RATE_MODEL_ID}
NEXT_PUBLIC_CORE_POOL_ID=${CORE_POOL_ID}
NEXT_PUBLIC_LIQUIDATION_ENGINE_ID=${LIQUIDATION_ENGINE_ID}
NEXT_PUBLIC_MARKET_FACTORY_ID=${MARKET_FACTORY_ID}
EOF
```

---

## Testing

All contracts use [soroban-sdk testutils](https://docs.rs/soroban-sdk/latest/soroban_sdk/testutils/index.html):

```bash
# Run all workspace tests
cargo test

# Run with output
cargo test -p oracle-adapter -- --nocapture

# Run a single test
cargo test -p interest-rate-model test_borrow_rate_at_optimal
```

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the full Phase 1–4 implementation guide, including specific test names required before each phase is considered complete.

---

## Roadmap

- [x] Fixed-point math library
- [x] SEP-0402 oracle adapter (production)
- [x] Kinked interest rate model (production)
- [x] CorePool — supply / withdraw / borrow / repay / interest accrual
- [x] CorePool — health factor + oracle integration
- [x] Mock oracle + test tokens (testnet)
- [x] Full deployment automation (guard, upgrade, verify, rotate-admin)
- [x] Testnet deployment — all contracts live
- [x] Simulation harness (5-wallet rotating roles, 8 sections/round)
- [ ] CorePool — unit test coverage (Phase 2)
- [ ] LiquidationEngine — implementation + tests
- [ ] IsolatedMarket — full implementation + tests
- [ ] MarketFactory — end-to-end market creation tests
- [ ] Audit
- [ ] Mainnet launch

---

## Security

This codebase is pre-audit. Do not use in production.

Planned audit scope:
- Index overflow / underflow
- Oracle manipulation resistance
- Health factor edge cases (zero debt, zero collateral)
- Liquidation bonus bounds
- Cross-contract invariant violations (Soroban's WASM model prevents classic reentrancy but not all cross-contract issues)

See [docs/SECURITY_CHECKLIST.md](docs/SECURITY_CHECKLIST.md) during internal reviews, [docs/PR_REVIEW_CHECKLIST.md](docs/PR_REVIEW_CHECKLIST.md) before merging protocol changes, and [docs/ADVANCED_SECURITY_PLAN.md](docs/ADVANCED_SECURITY_PLAN.md) for the full pre-launch security plan.

---

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for setup, branch conventions, commit style, PR process, and a detailed guide to the deployment and simulation systems.

For a step-by-step Phase 1–4 implementation plan see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

---

## License

```
MIT License — Copyright (c) 2026 Astrion Labs
```

---

<p align="center">
  Built by <a href="https://astrion.market">Astrion Labs</a> · Stellar Soroban
</p>
