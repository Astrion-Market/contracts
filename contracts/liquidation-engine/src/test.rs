#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

use astrion_math::WAD;

use crate::{
    errors::LiquidationError, LiquidationEngineContract, LiquidationEngineContractClient,
    MarketConfig,
};

// ---------------------------------------------------------------------------
// Mock Oracle Adapter — configurable per-asset price_wad.
// Uses OracleAsset / ResolvedPrice from the same crate.
// ---------------------------------------------------------------------------

mod mock_oracle {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map};

    use crate::{OracleAsset, ResolvedPrice};

    #[contracttype]
    enum Key {
        Prices,
    }

    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        pub fn set_price(env: Env, asset: Address, price_wad: i128) {
            let mut m: Map<Address, i128> = env
                .storage()
                .instance()
                .get(&Key::Prices)
                .unwrap_or_else(|| Map::new(&env));
            m.set(asset, price_wad);
            env.storage().instance().set(&Key::Prices, &m);
        }

        pub fn get_price(env: Env, asset: OracleAsset) -> ResolvedPrice {
            let addr = match asset {
                OracleAsset::Stellar(a) => a,
            };
            let m: Map<Address, i128> = env
                .storage()
                .instance()
                .get(&Key::Prices)
                .unwrap_or_else(|| Map::new(&env));
            let price_wad = m.get(addr.clone()).unwrap_or(astrion_math::WAD);
            ResolvedPrice {
                price_wad,
                timestamp: env.ledger().timestamp(),
                source: env.current_contract_address(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mock CorePool — stores configurable state; repay is a no-op.
// Satisfies the CorePool trait that LiquidationEngine calls into.
// ---------------------------------------------------------------------------

mod mock_core_pool {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

    use crate::MarketConfig;

    #[contracttype]
    #[derive(Clone)]
    struct BalKey {
        user: Address,
        asset: Address,
    }

    #[contracttype]
    enum DataKey {
        HF(Address),
        Borrow(BalKey),
        Supply(BalKey),
        Market(Address),
    }

    #[contract]
    pub struct MockCorePool;

    #[contractimpl]
    impl MockCorePool {
        // ---- test setup helpers ----

        pub fn mock_hf(env: Env, user: Address, hf: i128) {
            env.storage().persistent().set(&DataKey::HF(user), &hf);
        }

        pub fn mock_borrow(env: Env, user: Address, asset: Address, bal: i128) {
            env.storage()
                .persistent()
                .set(&DataKey::Borrow(BalKey { user, asset }), &bal);
        }

        pub fn mock_supply(env: Env, user: Address, asset: Address, bal: i128) {
            env.storage()
                .persistent()
                .set(&DataKey::Supply(BalKey { user, asset }), &bal);
        }

        pub fn mock_market(env: Env, config: MarketConfig) {
            env.storage()
                .persistent()
                .set(&DataKey::Market(config.asset.clone()), &config);
        }

        // ---- CorePool trait surface ----

        pub fn get_health_factor(env: Env, user: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::HF(user))
                .unwrap_or(i128::MAX)
        }

        pub fn get_borrow_balance(env: Env, user: Address, asset: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::Borrow(BalKey { user, asset }))
                .unwrap_or(0)
        }

        pub fn get_supply_balance(env: Env, user: Address, asset: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::Supply(BalKey { user, asset }))
                .unwrap_or(0)
        }

        pub fn get_market_config(env: Env, asset: Address) -> Option<MarketConfig> {
            env.storage()
                .persistent()
                .get(&DataKey::Market(asset))
        }

        pub fn repay(
            _env: Env,
            _payer: Address,
            _on_behalf_of: Address,
            _asset: Address,
            _amount: i128,
        ) {
            // no-op for tests
        }
    }
}

use mock_core_pool::{MockCorePool, MockCorePoolClient};
use mock_oracle::{MockOracle, MockOracleClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Setup {
    engine_id: Address,
    core_id: Address,
    oracle_id: Address,
    debt_asset: Address,
    collateral_asset: Address,
    admin: Address,
}

fn default_market_config(asset: &Address) -> MarketConfig {
    MarketConfig {
        asset: asset.clone(),
        ltv: WAD * 70 / 100,
        liquidation_threshold: WAD * 80 / 100,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 0,
        is_active: true,
        is_borrowable: true,
    }
}

/// Deploys the engine plus mocks.
/// Default prices: debt_asset = 1 WAD, collateral_asset = 10 WAD.
fn setup(env: &Env) -> Setup {
    let admin = Address::generate(env);
    let debt_asset = Address::generate(env);
    let collateral_asset = Address::generate(env);

    let oracle_id = env.register(MockOracle, ());
    let core_id = env.register(MockCorePool, ());
    let engine_id = env.register(LiquidationEngineContract, ());

    let oracle = MockOracleClient::new(env, &oracle_id);
    oracle.set_price(&debt_asset, &WAD);
    oracle.set_price(&collateral_asset, &(10 * WAD));

    let core = MockCorePoolClient::new(env, &core_id);
    core.mock_market(&default_market_config(&collateral_asset));

    LiquidationEngineContractClient::new(env, &engine_id).initialize(
        &admin,
        &core_id,
        &oracle_id,
        &(WAD / 2), // 50% close factor
    );

    Setup {
        engine_id,
        core_id,
        oracle_id,
        debt_asset,
        collateral_asset,
        admin,
    }
}

fn engine<'a>(env: &'a Env, s: &'a Setup) -> LiquidationEngineContractClient<'a> {
    LiquidationEngineContractClient::new(env, &s.engine_id)
}

fn core<'a>(env: &'a Env, s: &'a Setup) -> MockCorePoolClient<'a> {
    MockCorePoolClient::new(env, &s.core_id)
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    assert_eq!(engine(&env, &s).close_factor(), Some(WAD / 2));
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let eng = engine(&env, &s);
    let result = eng.try_initialize(
        &s.admin,
        &s.core_id,
        &s.oracle_id,
        &(WAD / 2),
    );
    assert_eq!(result, Err(Ok(LiquidationError::AlreadyInitialized)));
}

// ---------------------------------------------------------------------------
// check_liquidation
// ---------------------------------------------------------------------------

#[test]
fn test_check_liquidation_healthy_position() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let borrower = Address::generate(&env);
    // Health factor > WAD — healthy.
    core(&env, &s).mock_hf(&borrower, &(WAD * 2));
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);

    let preview = engine(&env, &s)
        .check_liquidation(&borrower, &s.debt_asset, &s.collateral_asset);
    assert!(!preview.is_liquidatable);
    assert_eq!(preview.max_repay_amount, 0);
}

#[test]
fn test_check_liquidation_undercollateralized() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2)); // HF = 0.5 < 1
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    core(&env, &s).mock_supply(&borrower, &s.collateral_asset, &10_000_i128);

    let preview = engine(&env, &s)
        .check_liquidation(&borrower, &s.debt_asset, &s.collateral_asset);
    assert!(preview.is_liquidatable);
    assert_eq!(preview.max_repay_amount, 500); // 50% close factor × 1000 debt
}

// ---------------------------------------------------------------------------
// liquidate — error paths
// ---------------------------------------------------------------------------

#[test]
fn test_liquidate_healthy_position_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD * 3)); // healthy

    let result = engine(&env, &s).try_liquidate(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &100_i128,
    );
    assert_eq!(result, Err(Ok(LiquidationError::PositionHealthy)));
}

#[test]
fn test_liquidate_no_debt_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2)); // unhealthy
    // No borrow balance set → defaults to 0.

    let result = engine(&env, &s).try_liquidate(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &1_i128,
    );
    assert_eq!(result, Err(Ok(LiquidationError::NoDebt)));
}

#[test]
fn test_liquidate_repay_exceeds_close_factor_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2));
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    // max_repay = 50% × 1000 = 500; repay 600 → fails
    let result = engine(&env, &s).try_liquidate(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &600_i128,
    );
    assert_eq!(result, Err(Ok(LiquidationError::RepayExceedsCloseFactor)));
}

#[test]
fn test_liquidate_insufficient_collateral_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2));
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    // Repay 500 (50% close factor).
    // debt_value = 500 × WAD = 500, with_bonus = 500 × 1.05 = 525
    // collateral_seized = wad_div(525, 10*WAD) ≈ 52.
    // Borrower only has 10 collateral → insufficient.
    core(&env, &s).mock_supply(&borrower, &s.collateral_asset, &10_i128);

    let result = engine(&env, &s).try_liquidate(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &500_i128,
    );
    assert_eq!(result, Err(Ok(LiquidationError::InsufficientCollateral)));
}

// ---------------------------------------------------------------------------
// liquidate — success path
// ---------------------------------------------------------------------------

#[test]
fn test_liquidate_undercollateralized_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2));
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    // Supply enough collateral to cover seizure (≥ 52 units).
    core(&env, &s).mock_supply(&borrower, &s.collateral_asset, &1_000_i128);

    engine(&env, &s).liquidate(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &500_i128,
    );
}

// ---------------------------------------------------------------------------
// liquidate_with_limits
// ---------------------------------------------------------------------------

#[test]
fn test_liquidate_with_limits_deadline_expired_fails() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().with_mut(|li| li.timestamp = 2_000);
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2));
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    core(&env, &s).mock_supply(&borrower, &s.collateral_asset, &1_000_i128);

    // Deadline 1000 < current timestamp 2000 → expired.
    let result = engine(&env, &s).try_liquidate_with_limits(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &500_i128,
        &5_000_i128,
        &1_000_u64,
        &42_u64,
    );
    assert_eq!(result, Err(Ok(LiquidationError::DeadlineExpired)));
}

#[test]
fn test_liquidate_with_limits_duplicate_nonce_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2));
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    core(&env, &s).mock_supply(&borrower, &s.collateral_asset, &1_000_i128);

    let deadline = u64::MAX;
    let nonce = 99_u64;

    // First call succeeds.
    engine(&env, &s).liquidate_with_limits(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &500_i128,
        &5_000_i128,
        &deadline,
        &nonce,
    );

    // Reset borrow balance for second attempt.
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    core(&env, &s).mock_supply(&borrower, &s.collateral_asset, &1_000_i128);

    // Second call with the same nonce → duplicate.
    let result = engine(&env, &s).try_liquidate_with_limits(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &500_i128,
        &5_000_i128,
        &deadline,
        &nonce,
    );
    assert_eq!(result, Err(Ok(LiquidationError::DuplicateOperation)));
}

#[test]
fn test_liquidate_with_limits_slippage_exceeded_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);

    let liquidator = Address::generate(&env);
    let borrower = Address::generate(&env);
    core(&env, &s).mock_hf(&borrower, &(WAD / 2));
    core(&env, &s).mock_borrow(&borrower, &s.debt_asset, &1_000_i128);
    core(&env, &s).mock_supply(&borrower, &s.collateral_asset, &1_000_i128);

    // collateral_seized ≈ 52; max_collateral_seized = 10 → slippage exceeded.
    let result = engine(&env, &s).try_liquidate_with_limits(
        &liquidator,
        &borrower,
        &s.debt_asset,
        &s.collateral_asset,
        &500_i128,
        &10_i128,  // max_collateral_seized too low
        &u64::MAX,
        &1_u64,
    );
    assert_eq!(result, Err(Ok(LiquidationError::SlippageExceeded)));
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

#[test]
fn test_set_close_factor() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let eng = engine(&env, &s);

    eng.set_close_factor(&(WAD * 30 / 100));
    assert_eq!(eng.close_factor(), Some(WAD * 30 / 100));
}
