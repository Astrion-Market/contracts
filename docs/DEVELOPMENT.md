# Astrion — Development Guide

This document is the single source of truth for contributors working on the
Astrion smart contracts.  It covers local setup, the current state of each
contract, and the work required to complete Phases 1–4.

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Prerequisites & Local Setup](#prerequisites--local-setup)
- [Contract Status](#contract-status)
- [Phase 1 — Full Testnet Simulation](#phase-1--full-testnet-simulation)
- [Phase 2 — Test Coverage](#phase-2--test-coverage)
- [Phase 3 — Isolated Markets](#phase-3--isolated-markets)
- [Phase 4 — Mainnet Launch](#phase-4--mainnet-launch)
- [Shared Conventions](#shared-conventions)

---

## Architecture Overview

```
User Transaction
       │
       ▼
 ┌─────────────────────────────────────────────────────────┐
 │  CorePool  (shared multi-asset pool)                    │
 │  supply / borrow / repay / withdraw / liquidate         │
 │         │                 │                             │
 │   OracleAdapter     InterestRateModel                   │
 │   get_price(asset)  get_rates(util)                     │
 │         │                                               │
 │  Reflector / MockOracle (testnet)                       │
 └─────────────────────────────────────────────────────────┘
       │
       ▼
 ┌──────────────────────────────────┐
 │  LiquidationEngine               │
 │  liquidate / liquidate_with_limits│
 │  (calls CorePool.repay           │
 │   + CorePool.seize_collateral)   │
 └──────────────────────────────────┘
       │
       ▼
 ┌──────────────────────────────────┐
 │  Market  (isolated pair)         │
 │  collateral_asset / debt_asset   │
 │  own oracle + rate-model ref     │
 └──────────────────────────────────┘
       │
       ▼
 ┌──────────────────────────────────┐
 │  MarketFactory                   │
 │  create_market(config)           │
 │  → deploys a new Market clone    │
 └──────────────────────────────────┘
```

### Key design decisions

| Decision | Rationale |
|----------|-----------|
| Index-based accounting | O(1) interest accrual — no loops over users |
| WAD fixed-point (1e18) | All rates and ratios are WAD-scaled i128 |
| OracleAdapter as indirection | Protocol never calls Reflector directly; swap oracle without touching core |
| Idempotent deploy scripts | Re-running ops scripts never re-deploys live contracts |
| Constructor-based test tokens | OZ pattern; `mint` is admin-only, standard SEP-41 interface |

---

## Prerequisites & Local Setup

### Required tools

| Tool | Version | Install |
|------|---------|---------|
| Rust (stable) | ≥ 1.85 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| WASM target | — | `rustup target add wasm32v1-none` |
| Stellar CLI | ≥ 26 | `cargo install --locked stellar-cli --features opt` |
| jq | any | `apt install jq` / `brew install jq` |

### Clone and build

```bash
git clone https://github.com/ibrahimijai/astrion-contracts
cd astrion-contracts
stellar contract build          # compiles all contracts to target/wasm32v1-none/release/
cargo test                      # runs all unit tests (no network needed)
```

### Configure a local testnet identity

```bash
stellar keys generate deployer --fund   # creates + funds with friendbot
stellar keys address deployer           # prints the public key
```

### Repository layout

```
contracts/           Soroban smart contracts (Rust)
  oracle-adapter/    SEP-0402 price oracle wrapper             ✅ complete
  interest-rate-model/ Kinked two-slope rate model             ✅ complete
  core-pool/         Shared multi-asset lending pool           ✅ complete
  liquidation-engine/ Liquidation logic with keeper support    ✅ complete
  market/            Isolated pair lending (Morpho-style)      ✅ complete
  market-factory/    Deploys Market clones                     🔧 needs test
  test-token/        Mintable ERC-20 for testnet               ✅ complete
  mock-oracle/       Fixed-price oracle for testnet sim        ✅ complete
libs/
  math/              WAD math, health factor, index helpers    ✅ complete
ops/                 Deployment and operations shell scripts
mk/                  GNU Make targets (build/deploy/sim)
sim/                 Testnet simulation scripts
deployments/
  testnet/           Live contract addresses + state
  mainnet/           Mainnet config template
docs/                Architecture, roadmap, security docs
```

---

## Contract Status

| Contract | Status | Functions implemented | Blocked on |
|----------|--------|-----------------------|------------|
| oracle-adapter | ✅ live, tested | all | — |
| interest-rate-model | ✅ live, tested | all | — |
| test-usdc | ✅ live | all | — |
| test-wbtc | ✅ live | all | — |
| mock-oracle | 🔧 Phase 1 deploy | all | deploy + init |
| core-pool | ✅ implemented, needs markets | all | mock oracle + add_market |
| liquidation-engine | ✅ implemented | all | CorePool markets |
| market | ✅ implemented, needs init | all | init with test assets |
| market-factory | ✅ implemented | all | integration test |

---

## Phase 1 — Full Testnet Simulation

**Goal:** make `make sim-run` execute all 8 sections including supply, borrow,
health-factor, and liquidation on real testnet contracts.

**Owner:** @ibrahimijai (admin keys required for oracle + market setup)

### What's needed

Three things block the protocol simulation:

1. **No price oracle for test tokens.**  
   The live Reflector oracle doesn't know about our custom test-usdc/test-wbtc.  
   Fix: deploy `mock-oracle`, set fixed prices ($1 USDC, $60k WBTC), register
   it in oracle-adapter for those assets.

2. **CorePool has no markets configured.**  
   `get_markets` returns `[]`.  Add USDC and WBTC markets via `add_market`.

3. **Sim script has wrong argument names.**  
   `supply --on_behalf_of` should be `--supplier`; `borrow` needs `--borrower`.

### Step-by-step execution

#### Step 1 — Deploy and wire the mock oracle

```bash
# Deploy (done automatically by deploy-all.sh on non-mainnet)
FORCE=1 make deploy-testnet    # skips already-deployed contracts, adds mock-oracle

# Verify it landed
make status
# Should show: mock-oracle ... initialized
```

#### Step 2 — Add test token markets to CorePool

```bash
source deployments/testnet/addresses.env

# Add USDC market (70% LTV, 80% liquidation threshold, 5% bonus, 10% reserve)
stellar contract invoke --id $CORE_POOL_ID --network testnet --source deployer \
  -- add_market --config \
  "{\"asset\":\"${TEST_USDC_ID}\",\"ltv\":\"700000000000000000\",\
\"liquidation_threshold\":\"800000000000000000\",\
\"liquidation_bonus\":\"50000000000000000\",\
\"reserve_factor\":\"100000000000000000\",\
\"supply_cap\":\"0\",\"borrow_cap\":\"0\",\
\"is_active\":true,\"is_borrowable\":true}"

# Add WBTC market (same params for testnet)
stellar contract invoke --id $CORE_POOL_ID --network testnet --source deployer \
  -- add_market --config \
  "{\"asset\":\"${TEST_WBTC_ID}\",\"ltv\":\"700000000000000000\",\
\"liquidation_threshold\":\"800000000000000000\",\
\"liquidation_bonus\":\"50000000000000000\",\
\"reserve_factor\":\"100000000000000000\",\
\"supply_cap\":\"0\",\"borrow_cap\":\"0\",\
\"is_active\":true,\"is_borrowable\":true}"

# Verify markets exist
stellar contract invoke --id $CORE_POOL_ID --network testnet --source deployer \
  -- get_markets
# Should return: ["<USDC_ID>", "<WBTC_ID>"]
```

#### Step 3 — Register mock oracle for test tokens

```bash
source deployments/testnet/addresses.env

# Set USDC price = $1.00 (10_000_000 with 7 decimals)
stellar contract invoke --id $MOCK_ORACLE_ID --network testnet --source deployer \
  -- set_price --asset "{Stellar:${TEST_USDC_ID}}" --price 10000000

# Set WBTC price = $60,000 (600_000_000_000 with 7 decimals)
stellar contract invoke --id $MOCK_ORACLE_ID --network testnet --source deployer \
  -- set_price --asset "{Stellar:${TEST_WBTC_ID}}" --price 600000000000

# Point oracle-adapter to use mock-oracle for test tokens
stellar contract invoke --id $ORACLE_ADAPTER_ID --network testnet --source deployer \
  -- set_asset_oracle \
  --asset "{Stellar:${TEST_USDC_ID}}" \
  --oracle "$MOCK_ORACLE_ID" --max_staleness 9999999

stellar contract invoke --id $ORACLE_ADAPTER_ID --network testnet --source deployer \
  -- set_asset_oracle \
  --asset "{Stellar:${TEST_WBTC_ID}}" \
  --oracle "$MOCK_ORACLE_ID" --max_staleness 9999999
```

#### Step 4 — Initialize the standalone isolated market

```bash
source deployments/testnet/addresses.env

# WBTC collateral → USDC debt (like a BTC-collateralised stablecoin loan)
stellar contract invoke --id $MARKET_ID --network testnet --source deployer \
  -- initialize --config \
  "{\"collateral_asset\":\"${TEST_WBTC_ID}\",\
\"debt_asset\":\"${TEST_USDC_ID}\",\
\"oracle_adapter\":\"${ORACLE_ADAPTER_ID}\",\
\"ltv\":\"700000000000000000\",\
\"liquidation_threshold\":\"800000000000000000\",\
\"liquidation_bonus\":\"50000000000000000\",\
\"reserve_factor\":\"100000000000000000\",\
\"supply_cap\":\"0\",\"borrow_cap\":\"0\",\
\"rate_model\":\"${RATE_MODEL_ID}\",\
\"treasury\":\"${TREASURY_ADDRESS}\"}"
```

#### Step 5 — Verify the full simulation runs

```bash
make sim-run
# Expected: all 8 sections show ✓ or execute
#   1. Oracle health    ✓ ✓
#   2. Rate curve       ✓ ✓ ✓ ✓ ✓
#   3. USDC balances    ✓ ✓ ✓ ✓ ✓
#   4. WBTC balances    ✓ ✓ ✓ ✓ ✓
#   5. Supply           ✓ supplied 100 USDC
#   6. Borrow           ✓ borrowed 60 USDC
#   7. Liquidation check ✓ health factor > 1.0
#   8. CorePool state   ✓ total_supplied non-zero
```

#### All-in-one after Phase 1 is wired in ops scripts

Once the ops scripts are updated (see PR), the entire Phase 1 setup is:

```bash
FORCE=1 make deploy-testnet
make init-testnet
make verify-testnet
make sim-run
```

---

## Phase 2 — Test Coverage

**Goal:** every public function in CorePool, LiquidationEngine, and Market has
at least one happy-path and one failure test.  CI blocks merges without tests.

**Owner:** any contributor — no admin keys needed, all tests run locally.

### How to run tests

```bash
cargo test                               # all contracts
cargo test -p core-pool                 # single contract
cargo test -p core-pool -- supply       # single test by name
cargo test -p core-pool -- --nocapture  # with println! output
```

### Test structure per contract

Tests live in `contracts/<name>/src/test.rs` alongside the contract.
The `Env::default()` test environment runs everything in-memory — no
network, no stellar-cli.

```rust
// Pattern for every test file
#[cfg(test)]
mod test {
    use soroban_sdk::{testutils::Address as _, Env};
    // register mock contracts in env.register(...)
    // call client methods
    // assert results
}
```

### CorePool — required test coverage

File: `contracts/core-pool/src/test.rs`

| Test | What to verify |
|------|---------------|
| `test_supply_basic` | supply 100 USDC → `get_supply_balance` returns 100 |
| `test_withdraw_basic` | supply then withdraw → balance = 0, tokens returned |
| `test_borrow_basic` | supply 100 → borrow 60 → health factor > 1.0 |
| `test_borrow_exceeds_collateral` | borrow 90 against 70 LTV → `HealthFactorTooLow` |
| `test_repay_full` | supply → borrow → repay full → debt = 0 |
| `test_repay_partial` | repay half → debt halved |
| `test_interest_accrual` | advance ledger timestamp → supply/borrow indexes increase |
| `test_supply_cap_enforced` | set supply_cap=100, supply 101 → `SupplyCapExceeded` |
| `test_borrow_cap_enforced` | set borrow_cap=50, borrow 51 → `BorrowCapExceeded` |
| `test_collateral_enable_disable` | disable collateral → health drops → re-enable |
| `test_add_market_duplicate` | add same asset twice → error |
| `test_pause_blocks_supply` | pause → supply → `Paused` error |

**Mock contracts needed in test.rs:**

```rust
// Mock oracle: always returns price = 1e18 (= $1.00 in WAD)
// Register with env.register(MockOracle, ()) 
// Must implement OracleAdapterClient interface: get_price(asset) -> ResolvedPrice

// Mock rate model: returns borrow_rate = 0.12e18, supply_rate = 0.10e18
// Must implement RateModelClient interface: get_rates(borrow, supply) -> Rates
```

### LiquidationEngine — required test coverage

File: `contracts/liquidation-engine/src/test.rs`

| Test | What to verify |
|------|---------------|
| `test_liquidate_healthy_position` | HF > 1.0 → `PositionHealthy` error |
| `test_liquidate_undercollateralised` | HF < 1.0 → liquidation succeeds, debt reduced |
| `test_close_factor_enforced` | repay > close_factor * debt → `RepayExceedsCloseFactor` |
| `test_liquidate_with_limits_deadline` | past deadline → `DeadlineExpired` |
| `test_liquidate_with_limits_slippage` | collateral_seized > max → `SlippageExceeded` |
| `test_nonce_replay_protection` | same nonce twice → `DuplicateOperation` |

### Market (isolated) — required test coverage

File: `contracts/market/src/test.rs`

| Test | What to verify |
|------|---------------|
| `test_supply_borrow_repay_roundtrip` | full cycle returns correct balances |
| `test_health_factor_enforced` | over-borrow → `HealthFactorTooLow` |
| `test_liquidate_isolatedmarket` | undercollateralised position seized |
| `test_interest_accrual` | borrow index advances with ledger time |

### How to add a new test

```rust
// In contracts/core-pool/src/test.rs

#[test]
fn test_supply_basic() {
    let env = Env::default();
    env.mock_all_auths();   // removes auth boilerplate in unit tests

    // 1. Register all contracts the test needs
    let pool_id = env.register(CorePoolContract, ());
    let pool    = CorePoolContractClient::new(&env, &pool_id);
    let oracle_id = env.register(MockOracleAdapterContract, ());
    let rate_id   = env.register(MockRateModelContract, ());
    let token_id  = env.register(TestTokenContract, ());  // or use soroban token mock

    // 2. Initialize
    let admin    = Address::generate(&env);
    let treasury = Address::generate(&env);
    pool.initialize(&admin, &oracle_id, &rate_id, &treasury);

    // 3. Add a market
    pool.add_market(&MarketConfig { asset: token_id.clone(), ltv: 700_000_000_000_000_000,
        liquidation_threshold: 800_000_000_000_000_000,
        liquidation_bonus: 50_000_000_000_000_000,
        reserve_factor: 100_000_000_000_000_000,
        supply_cap: 0, borrow_cap: 0,
        is_active: true, is_borrowable: true });

    // 4. Supply
    let user = Address::generate(&env);
    pool.supply(&user, &token_id, &1_000_000_000_i128);  // 100 USDC

    // 5. Assert
    let balance = pool.get_supply_balance(&user, &token_id);
    assert_eq!(balance, 1_000_000_000);
}
```

---

## Phase 3 — Isolated Markets

**Goal:** a contributor can deploy a new isolated lending pair (e.g. WBTC/USDC)
via `MarketFactory.create_market` without touching the shared CorePool.

**Owner:** any contributor.

### What's already built

- `contracts/market/` — fully implemented isolated pair lending with its own
  supply/borrow/repay/withdraw/liquidate and health factor.
- `contracts/market-factory/` — factory that holds a market WASM hash and
  deploys new market instances via `create_market(config)`.

### What needs to happen

#### 3a — Initialize the standalone Market on testnet (Phase 1 does this)

The `market` contract is already deployed (`MARKET_ID` in addresses.env) but
not yet initialized.  Phase 1's `make init-testnet` wires it to WBTC/USDC.

#### 3b — Test MarketFactory.create_market end-to-end

```bash
source deployments/testnet/addresses.env

# MarketFactory already holds the market WASM hash from init-all.sh.
# Create a new isolated market: ETH collateral / USDC debt
# (replace ETH_TOKEN_ID with whatever test asset you want)

stellar contract invoke --id $MARKET_FACTORY_ID --network testnet --source deployer \
  -- create_market --config \
  "{\"collateral_asset\":\"${TEST_WBTC_ID}\",\
\"debt_asset\":\"${TEST_USDC_ID}\",\
\"oracle_adapter\":\"${ORACLE_ADAPTER_ID}\",\
\"ltv\":\"700000000000000000\",\
\"liquidation_threshold\":\"800000000000000000\",\
\"liquidation_bonus\":\"50000000000000000\",\
\"reserve_factor\":\"100000000000000000\",\
\"supply_cap\":\"0\",\"borrow_cap\":\"0\",\
\"rate_model\":\"${RATE_MODEL_ID}\",\
\"treasury\":\"${TREASURY_ADDRESS}\"}"
```

`create_market` returns the new market's contract address.  Record it in
`deployments/testnet/addresses.env` as `ISOLATED_MARKET_1_ID=C...`.

#### 3c — Add sim support for isolated market

Update `sim/run.sh` to also run supply/borrow cycles against the isolated
Market contract.  The Market's function signatures are slightly different from
CorePool (simpler — no `--supplier` arg, just `--amount`):

```bash
# Isolated market supply (collateral_asset)
stellar contract invoke --id $MARKET_ID --source test1 -- supply \
  --supplier test1_address --amount 10000000  # 1.0 WBTC

# Isolated market borrow (debt_asset)  
stellar contract invoke --id $MARKET_ID --source test1 -- borrow \
  --borrower test1_address --amount 500000000  # 50 USDC
```

#### 3d — Write MarketFactory integration test

```rust
// contracts/market-factory/src/lib.rs (add test module)
#[test]
fn test_create_market_deploys_new_contract() {
    let env = Env::default();
    env.mock_all_auths();
    // register factory, upload market wasm, call create_market
    // assert the returned address is a valid contract
    // assert get_markets() contains the new address
}
```

---

## Phase 4 — Mainnet Launch

> **No contributor should execute Phase 4 without explicit sign-off from
> @ibrahimijai.  All steps require admin keys and coordination.**

### Checklist

- [ ] Security audit complete (see `docs/AUDIT_RFP.md`)
- [ ] All Phase 2 tests green on CI
- [ ] WASM sizes within budget (see `docs/GAS_GUIDELINES.md`)
- [ ] Admin rotated to multisig: `make rotate-admin NEW_ADMIN=G...`
- [ ] Oracle: Reflector mainnet contract verified, prices checked
- [ ] Risk params reviewed for mainnet (see `deployments/mainnet/config.env.example`)
- [ ] `make deploy-mainnet` executed — requires GitHub Environment approval
- [ ] `make init-mainnet` executed with real config
- [ ] `make verify-mainnet` all green
- [ ] Frontend `NEXT_PUBLIC_*` env vars updated with mainnet addresses
- [ ] Monitoring and alerting configured (see `docs/KEEPER_ECONOMICS.md`)

### Key differences from testnet

| Parameter | Testnet | Mainnet |
|-----------|---------|---------|
| Oracle | mock-oracle (fixed $1/$60k) | Reflector mainnet |
| Oracle staleness | 9,999,999s | 120s |
| Test tokens | test-usdc / test-wbtc | Real SAC tokens |
| Admin | deployer key | Multisig |
| LTV (stable) | 70% | 75% (after audit) |
| Supply caps | unlimited | set per-asset |

---

## Shared Conventions

### WAD math

All rates, prices, and ratios are stored as WAD-scaled integers:

```
1.0   = 1_000_000_000_000_000_000  (1e18)
0.5   =   500_000_000_000_000_000
0.05  =    50_000_000_000_000_000
0.001 =     1_000_000_000_000_000
```

### JSON i128 encoding

The Stellar CLI requires all `i128` values to be quoted strings in JSON:

```bash
# WRONG — CLI rejects this
--config '{"ltv": 700000000000000000}'

# CORRECT
--config '{"ltv": "700000000000000000"}'
```

### Token decimals

All test tokens use **7 decimal places** (Stellar convention):

```
1.00 USDC  =  10_000_000
10,000 USDC = 100_000_000_000
1.00 WBTC  =  10_000_000
```

### Make targets quick reference

```bash
make build                              # compile all contracts
make test                               # run all unit tests
make deploy-testnet                     # deploy to testnet (guarded if live)
make init-testnet                       # initialize all contracts
make verify-testnet                     # WASM hash + health checks
make upgrade-all NETWORK=testnet        # diff WASMs, single confirm, upgrade all
make upgrade CONTRACT=core-pool         # upgrade single contract
make sim-setup                          # fund wallets, mint tokens
make sim-run                            # one simulation round
make sim-loop ROUNDS=10 DELAY=30        # loop simulation
make status                             # print deployment state table
make rotate-admin NEW_ADMIN=G...        # transfer admin to multisig
```

### Branch naming

```
feat/<name>       new feature
fix/<name>        bug fix
test/<name>       test coverage
docs/<name>       documentation only
chore/<name>      tooling, deps, CI
```

### Commit convention

```
feat: add liquidation bonus cap check
fix: correct scaled_borrow underflow on full repay
test: supply→borrow→repay round-trip for CorePool
docs: update Phase 1 setup steps
```

### PR requirements

- All tests pass (`cargo test`)
- No new compiler warnings
- Any new public function has a doc comment explaining params + errors
- If touching CorePool or LiquidationEngine math: include a worked example
  in the commit message showing the before/after numbers

---

*For questions, open an issue or ping @ibrahimijai on the team channel.*
