# Contributing to Astrion Contracts

Thank you for your interest in contributing to Astrion — the hybrid lending protocol for Stellar. This guide covers everything you need: local setup, the deployment system, the simulation harness, branch and commit conventions, testing requirements, and the PR process.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Getting Started](#getting-started)
- [Repository Layout](#repository-layout)
- [Deployment System](#deployment-system)
- [Simulation Harness](#simulation-harness)
- [Development Workflow](#development-workflow)
- [Branch Naming](#branch-naming)
- [Commit Convention](#commit-convention)
- [Writing Contracts](#writing-contracts)
- [Testing Requirements](#testing-requirements)
- [Pull Request Process](#pull-request-process)
- [Reporting Issues](#reporting-issues)

---

## Prerequisites

| Tool | Version | Install |
|---|---|---|
| [Rust](https://rustup.rs) | stable | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| WASM target | — | `rustup target add wasm32v1-none` |
| [Stellar CLI](https://github.com/stellar/stellar-cli) | **≥ 26** | `cargo install --locked stellar-cli --features opt` |
| [jq](https://jqlang.github.io/jq/) | any | `apt install jq` / `brew install jq` |
| [Git](https://git-scm.com) | latest | platform-specific |

> **Note:** Stellar CLI v26 has breaking changes from earlier versions. The `--no-fund` flag was removed, the `balance` subcommand uses `--account` instead of `--id`, and the `--inclusion-fee` / `--resource-fee` flags replaced the old `--fee`. Always use v26 or later.

Verify your setup:

```bash
rustup target list --installed | grep wasm32v1-none
stellar --version      # must print v26+
jq --version
cargo test             # all tests should pass
```

---

## Getting Started

### Fork and clone

```bash
# Fork via GitHub UI, then:
git clone https://github.com/<your-username>/contracts.git
cd contracts
```

### Build all contracts

```bash
# Use stellar contract build — NOT cargo build directly
stellar contract build
```

> Always use `stellar contract build` (not `cargo build --target wasm32v1-none --release` directly). The Stellar CLI toolchain sets the correct `RUSTFLAGS` for the Soroban SDK's spec-shaking features.

### Build a single contract

```bash
stellar contract build --package oracle-adapter
stellar contract build --package core-pool
stellar contract build --package mock-oracle
```

### Run all tests

```bash
cargo test
```

### Run tests for a specific contract

```bash
cargo test -p oracle-adapter
cargo test -p interest-rate-model
cargo test -p astrion-math
cargo test -p core-pool
```

---

## Repository Layout

```
contracts/
├── Cargo.toml               # Workspace root — add new packages here
│
├── contracts/               # Soroban smart contracts
│   ├── oracle-adapter/      # SEP-0402 oracle wrapper (production)
│   ├── interest-rate-model/ # Kinked utilization rate curve (production)
│   ├── core-pool/           # Shared liquidity pool (production)
│   ├── market/              # Isolated two-asset pool (scaffold)
│   ├── market-factory/      # Isolated market deployer (production)
│   ├── liquidation-engine/  # Solvency enforcement (scaffold)
│   ├── mock-oracle/         # Fixed-price oracle — TESTNET ONLY
│   └── test-token/          # Mintable token — TESTNET ONLY
│
├── libs/
│   └── math/                # Shared no_std fixed-point arithmetic
│
├── ops/                     # Shell deployment scripts
│   ├── deploy-all.sh        # Deploy all contracts (deploy-once with guard)
│   ├── init-all.sh          # Initialize all contracts in dependency order
│   ├── upgrade-contract.sh  # In-place WASM upgrade for one contract
│   ├── upgrade-all.sh       # Batch upgrade with diff table
│   ├── verify-deploy.sh     # WASM hash + health-check verification
│   ├── rotate-admin.sh      # Transfer admin key across all contracts
│   └── state.sh             # Read/write deployment state.json
│
├── mk/                      # Makefile fragments (included by top-level Makefile)
│   ├── deploy.mk            # Deploy, upgrade, verify, rotate targets
│   ├── build.mk             # Build targets
│   └── sim.mk               # Simulation targets
│
├── sim/                     # Testnet simulation harness
│   ├── setup.sh             # Generate wallets, fund via friendbot, mint tokens
│   ├── run.sh               # Execute one simulation round (8 sections)
│   ├── wallets.env          # Test wallet public keys (committed, safe)
│   ├── tokens.env           # Test token contract IDs (committed)
│   └── state.env            # Round counter (committed, auto-updated)
│
└── deployments/
    ├── config.env.example   # Template — copy per network and fill in
    ├── testnet/
    │   ├── addresses.env    # Live contract IDs (committed)
    │   ├── state.json       # Deployment state + WASM hashes (committed)
    │   ├── checksums.sha256 # Build fingerprints (committed)
    │   └── config.env       # Active admin/treasury addresses (committed)
    └── mainnet/
        ├── config.env.example
        └── addresses.env    # (after launch)
```

### Adding a new contract

1. Create `contracts/<your-contract>/Cargo.toml` and `src/lib.rs`.
2. Add `"contracts/<your-contract>"` to `members` in the workspace `Cargo.toml`.
3. Follow the internal layout: `lib.rs`, `types.rs`, `storage.rs`, `errors.rs`, `test.rs`.
4. Add the contract to the inventory table in `README.md`.
5. If it needs deployment, add an alias to `ops/upgrade-contract.sh` and `ops/deploy-all.sh`.

---

## Deployment System

Understanding the deployment system prevents accidental re-deployment, data loss, and broken integrations. Read this section carefully before running any ops script.

### Deploy-once philosophy

Soroban contracts at a given address are permanent. Their addresses become the protocol's canonical identity — embedded in frontends, indexers, keeper bots, and the oracle adapter configuration. **Re-deploying means a new address, which immediately breaks everything pointed at the old one.**

Astrion's deployment system enforces this with a guard, an upgrade path, and an explicit bypass.

### The deployment guard

`ops/deploy-all.sh` checks `deployments/{network}/state.json` before deploying. If all five core contracts (`oracle-adapter`, `rate-model`, `core-pool`, `liquidation-engine`, `market-factory`) already have status `deployed` or `initialized`, the script exits with an error and shows upgrade instructions:

```
╔══════════════════════════════════════════════════════════════╗
║  CONTRACTS ALREADY DEPLOYED — USE UPGRADE INSTEAD           ║
╚══════════════════════════════════════════════════════════════╝

All core protocol contracts are already live on 'testnet'.
Re-deploying creates NEW addresses and breaks every integration.

  Upgrade all contracts (same addresses, new WASM only):
    make upgrade-all  NETWORK=testnet SOURCE=deployer

  Upgrade a single contract:
    make upgrade CONTRACT=oracle-adapter  NETWORK=testnet
    ...

  Force a fresh deployment (DESTRUCTIVE — new addresses, new environment):
    FORCE=1 ops/deploy-all.sh testnet deployer
```

### Per-contract idempotency

Each individual `deploy_contract()` and `deploy_token_ctor()` call in `deploy-all.sh` also checks state independently. If a contract has status `deployed` or `initialized`, it is **skipped** — regardless of `FORCE`. This means `FORCE=1` will only deploy contracts that are not yet in state, never re-deploy ones that are already live.

This makes it safe to add a new contract (e.g. `mock-oracle`) and run `FORCE=1 make deploy-testnet` — only the new contract is deployed; everything else is untouched.

### Deployment state file

`deployments/{network}/state.json` is the source of truth for every ops script. Each entry records:

```json
"oracle-adapter": {
  "contract_id": "CCVODOMSC3YBNTXDWRVKFFTSGLBYWZYGZ2N7WCL36WUGJVZVRH5Q5E2E",
  "wasm_sha256":  "7dfac9328d30438d…",
  "wasm_file":    "oracle_adapter.wasm",
  "status":       "initialized",
  "deployed_at":  "2026-05-24T19:00:33Z",
  "initialized_at": "2026-05-25T12:07:04Z"
}
```

Status lifecycle: `pending` → `deployed` → `initialized`.

**Commit `state.json` to git.** It is the shared record of what is actually on the network. Without it, the deployment guard cannot distinguish a fresh environment from an active one.

### Initialization

`ops/init-all.sh` initializes contracts in the correct dependency order. It is fully idempotent — if a contract's status is already `initialized` in state.json, it is skipped. Re-running after a partial failure is always safe.

**On testnet**, `init-all.sh` performs additional steps after the five core contracts:

1. Initializes `mock-oracle` with the admin address
2. Sets USDC price ($1.00) and WBTC price ($60,000) on mock-oracle
3. Points `oracle-adapter`'s default oracle at mock-oracle
4. Registers per-asset oracle overrides for test-usdc and test-wbtc
5. Adds USDC and WBTC markets to CorePool (if not already present)

The `DEFAULT_ORACLE_ID` config variable is only required on first initialization of `oracle-adapter`. Once oracle-adapter status is `initialized`, you can leave it blank — `init-all.sh` skips the check.

```bash
make init-testnet   # or:
make init-all NETWORK=testnet SOURCE=deployer
```

### Configuration

```bash
cp deployments/config.env.example deployments/testnet/config.env
```

| Variable | Required | Description |
|---|---|---|
| `ADMIN_ADDRESS` | Always | Stellar public key (`G…`) of the admin |
| `TREASURY_ADDRESS` | Always | Wallet that receives protocol reserve fees |
| `DEFAULT_ORACLE_ID` | First init only | SEP-0402 oracle contract ID (testnet: auto-set to mock-oracle) |
| `DEFAULT_MAX_STALENESS` | Optional | Max oracle age in seconds (default: 300) |
| `BASE_RATE` | Optional | Interest rate model base rate WAD (default: 1%) |
| `SLOPE1` | Optional | Rate slope below kink (default: 4%) |
| `SLOPE2` | Optional | Rate slope above kink (default: 75%) |
| `OPTIMAL_UTILIZATION` | Optional | Kink point WAD (default: 80%) |
| `RESERVE_FACTOR` | Optional | Protocol fee fraction WAD (default: 10%) |
| `CLOSE_FACTOR` | Optional | Max liquidation fraction WAD (default: 50%) |

All WAD values use 1e18 precision (e.g. `100000000000000000` = 10%).

### Upgrading contracts

Contracts are upgradeable in-place via the `upgrade(new_wasm_hash)` function each contract exposes. The deployed address never changes.

**Single contract:**

```bash
make upgrade CONTRACT=oracle-adapter NETWORK=testnet SOURCE=deployer
```

The script:
1. Builds the latest WASM
2. Compares its SHA-256 to the currently deployed hash in `state.json`
3. Skips if unchanged
4. Prompts for confirmation (unless `FORCE=1`)
5. Uploads the WASM to the network
6. Calls `upgrade(new_wasm_hash)` on the contract
7. Updates `state.json`

**All contracts:**

```bash
make upgrade-all NETWORK=testnet SOURCE=deployer
```

Shows a diff table before acting:

```
CONTRACT               OLD SHA-256       NEW SHA-256       CHANGE
─────────────────────  ──────────────    ──────────────    ──────
oracle-adapter         7dfac9328d…       7dfac9328d…       none
core-pool              ab10f2933b…       NEW_HASH…         upgrade
```

One confirmation prompt covers all upgrades. Unchanged contracts are skipped.

**Available aliases:** `oracle-adapter`, `rate-model`, `core-pool`, `liquidation-engine`, `market`, `market-factory`, `mock-oracle`, `test-usdc`, `test-wbtc`

### Verification

After any deployment or upgrade, verify the on-chain state:

```bash
make verify NETWORK=testnet
```

For each contract, this checks:

1. **WASM hash** — fetches on-chain bytecode and compares SHA-256 to the pinned fingerprint. Catches accidental upgrades, supply-chain tampering, or mismatched builds.
2. **Health invocations** — calls read-only functions (`admin`, `config`, etc.) to confirm the contract is responsive and correctly initialized.

Do not proceed to frontend integration or further testing if any check fails.

### Admin rotation

For mainnet preparation, transfer admin from the deployer key to a multisig:

```bash
make rotate-admin NEW_ADMIN=G<MULTISIG_ADDRESS> NETWORK=mainnet SOURCE=deployer
```

Calls `transfer_admin(new_admin)` on every initialized contract in dependency order. The old admin key must be available as `--source` for this transaction.

### Testnet-only contracts

**`mock-oracle`** (`contracts/mock-oracle/`) — A SEP-0402-compatible oracle that always returns the price set by the admin. Never stale (timestamp = current ledger). Allows fully scripted price manipulation for liquidation testing.

```bash
# Default prices after init:
# USDC = $1.00  → 10_000_000  (7-decimal fixed-point)
# WBTC = $60,000 → 600_000_000_000

# Update WBTC to $30,000 to test liquidation:
stellar contract invoke --network testnet --source deployer \
  --id $MOCK_ORACLE_ID -- set_price \
  --asset '{"Stellar":"<WBTC_CONTRACT>"}' --price 300000000000
```

**`test-usdc` / `test-wbtc`** (`contracts/test-token/`) — OZ `stellar-tokens` v0.7.1 mintable SEP-41 tokens. The deployer/admin can call `mint(account, amount)` to fund test wallets. Initialized via constructor args at deploy time (admin, decimals, name, symbol) — no separate `initialize` call needed.

```bash
stellar contract invoke --network testnet --source deployer \
  --id $TEST_USDC_ID -- mint \
  --account <ADDRESS> --amount 100000000000  # 10,000 USDC (7 decimals)
```

Neither of these contracts is deployed when `network == mainnet`. The `deploy-all.sh` and `init-all.sh` scripts check `$network != mainnet` before touching them.

### Deployment command reference

| Command | Effect |
|---|---|
| `make dry-run` | Print all deploy commands without executing |
| `make deploy-testnet` | Deploy core + test tokens + mock-oracle to testnet |
| `make deploy-mainnet` | Deploy core contracts to mainnet (confirmation prompt) |
| `make init-testnet` | Initialize all contracts on testnet (idempotent) |
| `make init-mainnet` | Initialize all contracts on mainnet |
| `make verify NETWORK=X` | WASM hash check + health invocations |
| `make status NETWORK=X` | Print state table |
| `make upgrade CONTRACT=X` | Upgrade one contract in-place |
| `make upgrade-all` | Upgrade all with diff preview |
| `make rotate-admin NEW_ADMIN=G…` | Transfer admin on all contracts |

---

## Simulation Harness

The simulation harness (`sim/`) exercises the full protocol flow on testnet using five rotating wallets. Use it to verify that a new implementation works end-to-end, not just in unit tests.

### Setup (first time only)

Contracts must be deployed and initialized before setup:

```bash
make deploy-testnet && make init-testnet
make sim-setup NETWORK=testnet
```

`sim/setup.sh` generates five Stellar identities (`test1`–`test5`), funds each via the testnet friendbot, then mints 10,000 USDC and 1 WBTC into each wallet. It writes `sim/wallets.env` and `sim/tokens.env` — both committed to git.

### Running a round

```bash
make sim-run NETWORK=testnet
```

Each round (`sim/run.sh`) executes 8 sections in sequence:

| Section | Action | Wallet role |
|---|---|---|
| 1 | Oracle health check (admin, max_staleness) | read-only |
| 2 | Interest rate curve (5 utilization points) | read-only |
| 3 | USDC balances for all 5 wallets | read-only |
| 4 | WBTC balances for all 5 wallets | read-only |
| 5 | Supply 100 USDC to CorePool | supplier-A, supplier-B |
| 5b | Supply 0.1 WBTC as collateral | borrower-A, borrower-B |
| 6 | Borrow 60 USDC from CorePool | borrower-A, borrower-B |
| 7 | Health factor check → liquidate if HF < 1.0 | liquidator |
| 8 | CorePool state (market list + USDC market state) | read-only |

Sections that rely on unimplemented scaffold functions print `─ skipped` and the round continues. A passing round shows all `✓` checkmarks.

### Running multiple rounds

```bash
make sim-loop ROUNDS=10 DELAY=30 NETWORK=testnet
```

### Role rotation

For round R, wallet `test-N` takes role `(N-1 + R-1) % 5`:

```
Round 1: test1=supplier-A, test2=supplier-B, test3=borrower-A, test4=borrower-B, test5=liquidator
Round 2: test5=supplier-A, test1=supplier-B, test2=borrower-A, test3=borrower-B, test4=liquidator
...
```

After 5 rounds the cycle repeats. Over time, all wallets accumulate supply, debt, and collateral in the pool — reflecting realistic multi-user state.

### Simulation invariants

The simulation is designed around known-safe parameters:

- **WBTC collateral**: 0.1 WBTC per round = $6,000 at $60k/BTC
- **WBTC LTV**: 70% → max $4,200 borrowable per deposit
- **USDC borrow**: 60 USDC per round — well within limit
- **Health factor**: ~81.5 at first borrow, grows as collateral accumulates

To test liquidation paths, reduce the WBTC price in mock-oracle to drive health factors below 1.0, then run `sim-run`.

### Known issues

- **`InsufficientRefundableFee`**: Soroban resource fee estimates use a snapshot ledger. When multiple transactions modify pool state in rapid succession, later transactions in the same round may land on a higher-cost ledger than the simulation anticipated. The sim script uses `--resource-fee 5000000` (0.5 XLM) on all `invoke` calls to prevent this.
- **Seq-num collisions**: `invoke_ro` (read-only queries) uses `deployer` as the source so test wallet sequence numbers are reserved for their actual supply/borrow operations.

---

## Development Workflow

```
main ← PR ← your-branch
```

1. **Sync** — pull the latest `main`
2. **Branch** — create a feature/fix branch off `main`
3. **Code** — implement following the TODO comments in scaffold contracts
4. **Test** — write tests; all new code requires coverage
5. **Verify** — run the checklist below
6. **Commit** — write a conventional commit message
7. **PR** — open a pull request against `main`

### Before every commit

```bash
# Build all contracts (must pass with zero warnings)
stellar contract build

# Run all tests
cargo test

# Lint (treat warnings as errors)
cargo clippy --target wasm32v1-none -- -D warnings
```

### Testnet verification

After implementing a function that CorePool or LiquidationEngine uses:

```bash
# Build + deploy any changed contract
make upgrade CONTRACT=core-pool NETWORK=testnet SOURCE=deployer

# Verify it's live and healthy
make verify NETWORK=testnet

# Run a simulation round to confirm end-to-end
make sim-run NETWORK=testnet
```

Do not open a PR for a core protocol function without first confirming it passes a simulation round.

---

## Branch Naming

| Prefix | Use case | Example |
|---|---|---|
| `feat/` | New feature or function implementation | `feat/core-pool-supply` |
| `fix/` | Bug fix | `fix/borrow-index-overflow` |
| `chore/` | Tooling, deps, workspace config | `chore/upgrade-soroban-sdk` |
| `docs/` | Documentation only | `docs/deployment-guide` |
| `refactor/` | Code restructuring without behaviour change | `refactor/storage-key-naming` |
| `test/` | Adding or fixing tests | `test/liquidation-edge-cases` |
| `security/` | Security fix or hardening | `security/oracle-staleness-bounds` |

```bash
git checkout -b feat/core-pool-supply
```

---

## Commit Convention

We follow [Conventional Commits](https://www.conventionalcommits.org/).

```
<type>(<scope>): <short description>
```

### Types

| Type | Description |
|---|---|
| `feat` | New feature or function implementation |
| `fix` | Bug fix |
| `docs` | Documentation changes |
| `refactor` | No behaviour change, code restructuring |
| `perf` | Performance improvement |
| `test` | Adding or correcting tests |
| `chore` | Workspace config, dependency updates |
| `ci` | CI/CD configuration |
| `security` | Security fix |

### Scopes (optional but recommended)

Use the contract or subsystem name: `oracle-adapter`, `core-pool`, `math`, `market`, `factory`, `liquidation`, `sim`, `ops`, `mock-oracle`.

### Examples

```
feat(core-pool): implement supply with index-based share minting
fix(oracle-adapter): reject zero price from oracle
test(interest-rate-model): add 100% utilization boundary test
chore: upgrade soroban-sdk to v26
docs(sim): document role rotation formula
security(liquidation-engine): cap collateral seizure to user balance
ops(deploy-all): add mock-oracle to testnet section
```

### Rules

- Lowercase type, scope, and description
- Subject line under 72 characters
- Imperative mood: "implement" not "implemented" or "implements"
- No trailing period on the subject line
- Separate body from subject with a blank line when a body is needed

---

## Writing Contracts

### General rules

| Rule | Detail |
|---|---|
| **No floats** | Use `astrion-math` WAD arithmetic exclusively |
| **No std** | All contracts are `#![no_std]` |
| **Overflow** | Use `checked_*` operations in critical accounting paths |
| **Storage** | `instance` for singleton data; `persistent` for per-user/asset data |
| **Comments** | Only for non-obvious intent — do not narrate the code |
| **Events** | Emit a `#[contractevent]` for every state-mutating operation |
| **Auth** | Call `address.require_auth()` before any state mutation; never skip |

### CLI encoding for i128

Soroban CLI encodes `i128` values as JSON strings in struct arguments. Always quote them:

```bash
# Correct — i128 inside a JSON struct must be a quoted string
--config '{"ltv":"700000000000000000","supply_cap":"0",...}'

# Correct — i128 as a standalone CLI flag can be a plain integer
--amount 600000000
--price 10000000
```

WAD arithmetic note: `1e18 = 1.0`, `700000000000000000 = 0.7 = 70%`.

### Implementing a scaffold function

Each scaffold function body has numbered TODO comments:

```rust
pub fn supply(env: Env, supplier: Address, asset: Address, amount: i128) -> Result<(), PoolError> {
    // TODO [1/5 — Auth + guards]: supplier.require_auth(); guard_user_live(&env)?;
    // TODO [2/5 — Validate]: ensure amount > 0, market exists and is active
    // TODO [3/5 — Accrue interest]: MUST accrue before reading/writing state
    // TODO [4/5 — Update state]: compute scaled_amount, update user account + market totals
    // TODO [5/5 — Transfer + event]: token.transfer(), emit supply event
}
```

Work through the TODOs in order. Never skip the interest accrual step — calling any market function without accruing first corrupts the index-based accounting.

### Cross-contract calls

Use `#[contractclient]` to define an interface:

```rust
#[contractclient(name = "OracleClient")]
pub trait OracleTrait {
    fn lastprice(env: Env, asset: Asset) -> Option<PriceData>;
    fn decimals(env: Env) -> u32;
}

let oracle = OracleClient::new(&env, &oracle_address);
let price = oracle.lastprice(&asset).ok_or(OracleError::NoPrice)?;
```

---

## Testing Requirements

Every function implementation must have tests before the PR is merged. Tests run in the Soroban sandbox — no network access, no XLM required.

### Required test patterns

| Scenario | Required |
|---|---|
| Happy path (golden path) | Yes |
| Already-initialized guard | Yes for lifecycle functions |
| Admin auth enforcement | Yes for admin-only functions |
| Invalid amounts (zero, negative) | Yes |
| Cap enforcement (supply cap, borrow cap) | Yes |
| Health factor boundary (HF at exactly 1.0 WAD) | Yes |
| Oracle staleness / zero price | Yes for oracle-dependent paths |
| Interest accrual (delta_t = 0, 1s, 1 year) | Yes for `accrue_interest` |

### Test setup pattern

```rust
#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Env,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        (env, admin)
    }

    fn set_ledger_time(env: &Env, timestamp: u64) {
        env.ledger().set(LedgerInfo {
            timestamp,
            protocol_version: 22,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1_000,
            min_persistent_entry_ttl: 1_000,
            max_entry_ttl: 10_000_000,
        });
    }
}
```

For tests that require an oracle, register a `MockOracle` instance using the `mock-oracle` contract or write a minimal inline mock with `env.register`.

See [docs/DEVELOPMENT.md](DEVELOPMENT.md) for the full list of test names required per contract to reach each phase milestone.

### Good first tasks (interns and new contributors)

- Add focused tests for admin-only paths in `contracts/core-pool/src/test.rs`.
- Add oracle edge-case tests in `contracts/oracle-adapter/src/test.rs`.
- Extend math edge cases in `libs/math/src/lib.rs`.
- Add event assertions for state-mutating functions.
- Pick a TODO from `ISSUES_FOR_INTERNS.md` and include the issue number in your PR.

Before requesting review, run through [PR_REVIEW_CHECKLIST.md](PR_REVIEW_CHECKLIST.md). For security-sensitive code, also use [SECURITY_CHECKLIST.md](SECURITY_CHECKLIST.md).

---

## Pull Request Process

### Before opening a PR

- [ ] Branch is up to date with `main`
- [ ] `stellar contract build` passes (zero warnings)
- [ ] `cargo test` passes with no failures
- [ ] `cargo clippy -- -D warnings` produces no warnings
- [ ] New functions have test coverage per the requirements above
- [ ] If the change affects CorePool or LiquidationEngine: run `make sim-run NETWORK=testnet` and confirm all sections pass

### PR template

```markdown
## What

Brief description of what changed and why.

## Contract(s) affected

- [ ] oracle-adapter
- [ ] interest-rate-model
- [ ] core-pool
- [ ] market
- [ ] market-factory
- [ ] liquidation-engine
- [ ] libs/math
- [ ] mock-oracle / test-token (testnet only)
- [ ] ops / sim / mk (tooling)

## Testing

Describe how you tested this. For core protocol functions, include
a sim-run result or a direct testnet invocation showing success.

## Checklist

- [ ] Build passes (`stellar contract build`)
- [ ] All tests pass (`cargo test`)
- [ ] Clippy clean (`cargo clippy -- -D warnings`)
- [ ] New tests added for changed functions
- [ ] Events emitted for state-mutating functions
- [ ] Storage TTLs bumped for persistent entries
- [ ] `README.md` / `CONTRIBUTING.md` updated if behaviour changed
```

### Review process

1. At least one maintainer approval is required.
2. All CI checks must pass.
3. Resolve all review comments before merging.
4. Squash and merge is preferred to keep `git log` readable.

---

## Reporting Issues

### Bug reports

Open a GitHub issue with:

- **Title** — clear, concise summary
- **Contract** — which contract is affected
- **Steps to reproduce** — numbered list (include test code or CLI invocation)
- **Expected behaviour**
- **Actual behaviour**
- **Stellar CLI version** — `stellar --version`
- **Soroban SDK version** — from `Cargo.lock`

### Security vulnerabilities

**Do not open a public issue for security vulnerabilities.**

Email [security@astrion.market](mailto:security@astrion.market) with:

- Description of the vulnerability
- Potential impact
- Steps to reproduce (proof of concept if possible)

We will respond within 48 hours.

### Feature requests

Open a GitHub issue with:

- **Title** — `[Feature] Brief description`
- **Problem** — what gap does this fill?
- **Proposed solution** — how would you implement it?
- **Alternatives considered**

---

## Questions?

- Open a [GitHub Discussion](https://github.com/Astrion-Market/contracts/discussions)
- Reach out on [Twitter](https://twitter.com/astrionmarket)
- Join our [Discord](https://discord.gg/astrionmarket)

---

<p align="center">
  Thank you for helping build the credit layer for Stellar.
</p>
