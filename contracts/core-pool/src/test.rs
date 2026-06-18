#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

use astrion_math::WAD;

use crate::{errors::PoolError, types::MarketConfig, CorePoolContract, CorePoolContractClient};

// ---------------------------------------------------------------------------
// Mock oracle adapter — stores a price per asset address, returns it via
// get_price.  Uses OracleAsset / ResolvedPrice from the same crate so the
// XDR encoding is identical to what OracleAdapterClient expects.
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
// Mock rate model — constant 5 % borrow / 4 % supply rates
// ---------------------------------------------------------------------------

mod mock_rate_model {
    use soroban_sdk::{contract, contractimpl, Env};

    use crate::RateSnapshot;

    #[contract]
    pub struct MockRateModel;

    #[contractimpl]
    impl MockRateModel {
        pub fn get_rates(_env: Env, _total_borrowed: i128, _total_supplied: i128) -> RateSnapshot {
            RateSnapshot {
                borrow_rate: astrion_math::WAD * 5 / 100,
                supply_rate: astrion_math::WAD * 4 / 100,
                utilization: 0,
            }
        }
    }
}

use mock_oracle::{MockOracle, MockOracleClient};
use mock_rate_model::MockRateModel;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Addresses for the shared pool test environment.
struct Setup {
    pool_id: Address,
    oracle_id: Address,
    usdc: Address,
    wbtc: Address,
    admin: Address,
}

/// Deploy mocks, tokens, and the CorePool.
///
/// Prices:  USDC = 1 WAD,  WBTC = 10 WAD  (10× more valuable).
/// Markets: both at LTV 70%, liquidation_threshold 80%, bonus 5%.
fn setup(env: &Env) -> Setup {
    let admin = Address::generate(env);

    let oracle_id = env.register(MockOracle, ());
    let rate_model_id = env.register(MockRateModel, ());

    let usdc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let wbtc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let oracle = MockOracleClient::new(env, &oracle_id);
    oracle.set_price(&usdc, &WAD);
    oracle.set_price(&wbtc, &(10 * WAD));

    let pool_id = env.register(CorePoolContract, ());
    let pool = CorePoolContractClient::new(env, &pool_id);
    pool.initialize(&admin, &oracle_id, &rate_model_id, &admin);

    pool.add_market(&mkt(env, &usdc, WAD * 70 / 100, WAD * 80 / 100));
    pool.add_market(&mkt(env, &wbtc, WAD * 70 / 100, WAD * 80 / 100));

    Setup {
        pool_id,
        oracle_id,
        usdc,
        wbtc,
        admin,
    }
}

fn mkt(env: &Env, asset: &Address, ltv: i128, liq_threshold: i128) -> MarketConfig {
    let _ = env;
    MarketConfig {
        asset: asset.clone(),
        ltv,
        liquidation_threshold: liq_threshold,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 0,
        is_active: true,
        is_borrowable: true,
    }
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn pool<'a>(env: &'a Env, s: &'a Setup) -> CorePoolContractClient<'a> {
    CorePoolContractClient::new(env, &s.pool_id)
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let _ = CorePoolContractClient::new(&env, &s.pool_id);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);
    let result = pool.try_initialize(&s.admin, &s.oracle_id, &s.pool_id, &s.admin);
    assert_eq!(result, Err(Ok(PoolError::AlreadyInitialized)));
}

#[test]
fn test_pause_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);
    pool.pause();
    pool.unpause();
}

#[test]
fn test_pause_blocks_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);
    pool.pause();

    let user = Address::generate(&env);
    mint(&env, &s.usdc, &user, 1_000);
    let result = pool.try_supply(&user, &s.usdc, &100_i128);
    assert_eq!(result, Err(Ok(PoolError::Paused)));
}

// ---------------------------------------------------------------------------
// Market management
// ---------------------------------------------------------------------------

#[test]
fn test_add_market_already_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);
    // USDC market was already added in setup
    let result = pool.try_add_market(&mkt(&env, &s.usdc, WAD * 70 / 100, WAD * 80 / 100));
    assert_eq!(result, Err(Ok(PoolError::MarketAlreadyExists)));
}

#[test]
fn test_add_market_invalid_ltv_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);
    let new_asset = env
        .register_stellar_asset_contract_v2(s.admin.clone())
        .address();
    // ltv >= liquidation_threshold is invalid
    let bad = MarketConfig {
        asset: new_asset,
        ltv: WAD * 90 / 100,
        liquidation_threshold: WAD * 80 / 100, // liq_threshold < ltv — invalid
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 0,
        is_active: true,
        is_borrowable: true,
    };
    let result = pool.try_add_market(&bad);
    assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
}

#[test]
fn test_get_markets() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);
    let markets = pool.get_markets();
    assert_eq!(markets.len(), 2);
    assert!(markets.contains(&s.usdc));
    assert!(markets.contains(&s.wbtc));
}

// ---------------------------------------------------------------------------
// Supply
// ---------------------------------------------------------------------------

#[test]
fn test_supply_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let user = Address::generate(&env);
    mint(&env, &s.usdc, &user, 1_000);

    pool.supply(&user, &s.usdc, &1_000_i128);

    assert_eq!(pool.get_supply_balance(&user, &s.usdc), 1_000);
    assert_eq!(token::Client::new(&env, &s.usdc).balance(&user), 0);
}

#[test]
fn test_supply_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let user = Address::generate(&env);
    let result = pool.try_supply(&user, &s.usdc, &0_i128);
    assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
}

#[test]
fn test_supply_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    // Deploy a fresh asset with a supply cap of 500.
    let asset = env
        .register_stellar_asset_contract_v2(s.admin.clone())
        .address();
    let oracle = MockOracleClient::new(&env, &s.oracle_id);
    oracle.set_price(&asset, &WAD);

    pool.add_market(&MarketConfig {
        asset: asset.clone(),
        ltv: WAD * 70 / 100,
        liquidation_threshold: WAD * 80 / 100,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 500,
        borrow_cap: 0,
        is_active: true,
        is_borrowable: true,
    });

    let user = Address::generate(&env);
    mint(&env, &asset, &user, 1_000);

    // First 500 units succeed.
    pool.supply(&user, &asset, &500_i128);
    // Second supply hits the cap.
    let result = pool.try_supply(&user, &asset, &1_i128);
    assert_eq!(result, Err(Ok(PoolError::SupplyCapExceeded)));
}

// ---------------------------------------------------------------------------
// Withdraw
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let user = Address::generate(&env);
    mint(&env, &s.usdc, &user, 1_000);

    pool.supply(&user, &s.usdc, &1_000_i128);
    pool.withdraw(&user, &s.usdc, &1_000_i128);

    assert_eq!(pool.get_supply_balance(&user, &s.usdc), 0);
    assert_eq!(token::Client::new(&env, &s.usdc).balance(&user), 1_000);
}

#[test]
fn test_withdraw_more_than_supplied_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let user = Address::generate(&env);
    mint(&env, &s.usdc, &user, 1_000);
    pool.supply(&user, &s.usdc, &500_i128);

    let result = pool.try_withdraw(&user, &s.usdc, &600_i128);
    assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
}

#[test]
fn test_withdraw_insufficient_liquidity_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    // Supplier A provides 1000 USDC.
    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    // Borrower supplies WBTC collateral (100 units × $10 = $1000 value).
    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);

    // Borrower draws 900 USDC (HF = 800/900 < 1 — this should fail).
    // Use a safe borrow (700) so the pool state is set up for the next check.
    pool.borrow(&borrower, &s.usdc, &700_i128);

    // Supplier cannot withdraw more than available liquidity (1000 - 700 = 300).
    let result = pool.try_withdraw(&supplier, &s.usdc, &400_i128);
    assert_eq!(result, Err(Ok(PoolError::InsufficientLiquidity)));
}

// ---------------------------------------------------------------------------
// Borrow
// ---------------------------------------------------------------------------

#[test]
fn test_borrow_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    // Supply USDC liquidity.
    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    // Borrower supplies 100 WBTC collateral (value = 1000, weighted = 800).
    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);

    // Borrow 700 USDC — HF = 800/700 ≈ 1.14 > 1.
    pool.borrow(&borrower, &s.usdc, &700_i128);

    assert_eq!(pool.get_borrow_balance(&borrower, &s.usdc), 700);
    assert_eq!(token::Client::new(&env, &s.usdc).balance(&borrower), 700);
}

#[test]
fn test_borrow_no_collateral_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    let borrower = Address::generate(&env);
    let result = pool.try_borrow(&borrower, &s.usdc, &100_i128);
    assert_eq!(result, Err(Ok(PoolError::HealthFactorTooLow)));
}

#[test]
fn test_borrow_exceeds_collateral_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 2_000);
    pool.supply(&supplier, &s.usdc, &2_000_i128);

    // 100 WBTC × $10 = $1000 value, 80% threshold → max borrow = 800.
    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);

    // Borrow 900 — HF = 800/900 < 1.
    let result = pool.try_borrow(&borrower, &s.usdc, &900_i128);
    assert_eq!(result, Err(Ok(PoolError::HealthFactorTooLow)));
}

#[test]
fn test_borrow_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    // New asset with borrow cap 400.
    let asset = env
        .register_stellar_asset_contract_v2(s.admin.clone())
        .address();
    let oracle = MockOracleClient::new(&env, &s.oracle_id);
    oracle.set_price(&asset, &WAD);

    pool.add_market(&MarketConfig {
        asset: asset.clone(),
        ltv: WAD * 70 / 100,
        liquidation_threshold: WAD * 80 / 100,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 400,
        is_active: true,
        is_borrowable: true,
    });

    // Supplier provides liquidity.
    let supplier = Address::generate(&env);
    mint(&env, &asset, &supplier, 2_000);
    pool.supply(&supplier, &asset, &2_000_i128);

    // Borrower provides ample WBTC collateral.
    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 1_000);
    pool.supply(&borrower, &s.wbtc, &1_000_i128);

    // Borrow 400 succeeds.
    pool.borrow(&borrower, &asset, &400_i128);
    // Borrow 1 more hits the cap.
    let result = pool.try_borrow(&borrower, &asset, &1_i128);
    assert_eq!(result, Err(Ok(PoolError::BorrowCapExceeded)));
}

// ---------------------------------------------------------------------------
// Repay
// ---------------------------------------------------------------------------

#[test]
fn test_repay_full() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);
    pool.borrow(&borrower, &s.usdc, &700_i128);

    // Give borrower more USDC than needed (to test overpayment capping).
    mint(&env, &s.usdc, &borrower, 200);

    // Repay way more than owed — should cap at actual debt.
    pool.repay(&borrower, &borrower, &s.usdc, &2_000_i128);

    assert_eq!(pool.get_borrow_balance(&borrower, &s.usdc), 0);
}

#[test]
fn test_repay_partial() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);
    pool.borrow(&borrower, &s.usdc, &700_i128);

    mint(&env, &s.usdc, &borrower, 0); // borrower already has 700 from borrow
    pool.repay(&borrower, &borrower, &s.usdc, &300_i128);

    let remaining = pool.get_borrow_balance(&borrower, &s.usdc);
    // Slight rounding is acceptable; should be close to 400.
    assert!(remaining >= 399 && remaining <= 401);
}

// ---------------------------------------------------------------------------
// Collateral enable / disable
// ---------------------------------------------------------------------------

#[test]
fn test_disable_collateral_with_debt_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);
    pool.borrow(&borrower, &s.usdc, &700_i128);

    // Disabling WBTC would bring HF below 1 — should fail.
    let result = pool.try_disable_collateral(&borrower, &s.wbtc);
    assert_eq!(result, Err(Ok(PoolError::HealthFactorTooLow)));
}

#[test]
fn test_enable_disable_collateral_no_debt() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let user = Address::generate(&env);
    mint(&env, &s.usdc, &user, 500);
    pool.supply(&user, &s.usdc, &500_i128);

    // Disable collateral — no debt, so HF check succeeds.
    pool.disable_collateral(&user, &s.usdc);
    // Re-enable.
    pool.enable_collateral(&user, &s.usdc);
}

// ---------------------------------------------------------------------------
// Interest accrual
// ---------------------------------------------------------------------------

#[test]
fn test_interest_accrual_advances_indexes() {
    let env = Env::default();
    env.mock_all_auths();

    // Time starts at 0 (ledger default).
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 2_000);
    pool.supply(&supplier, &s.usdc, &2_000_i128);

    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);
    pool.borrow(&borrower, &s.usdc, &700_i128);

    let state_before = pool.get_market_state(&s.usdc).unwrap();
    assert_eq!(state_before.borrow_index, WAD);
    assert_eq!(state_before.supply_index, WAD);

    // Advance one year.
    env.ledger().with_mut(|li| li.timestamp = 31_536_000);
    pool.accrue_interest(&s.usdc);

    let state_after = pool.get_market_state(&s.usdc).unwrap();
    assert!(state_after.borrow_index > WAD);
    assert!(state_after.supply_index > WAD);
    assert!(state_after.borrow_index > state_after.supply_index);
}

#[test]
fn test_interest_accrual_same_block_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 2_000);
    pool.supply(&supplier, &s.usdc, &2_000_i128);

    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);
    pool.borrow(&borrower, &s.usdc, &700_i128);

    let before = pool.get_market_state(&s.usdc).unwrap();
    // No time passes — accrual should be idempotent.
    pool.accrue_interest(&s.usdc);
    let after = pool.get_market_state(&s.usdc).unwrap();

    assert_eq!(before.borrow_index, after.borrow_index);
    assert_eq!(before.supply_index, after.supply_index);
}

// ---------------------------------------------------------------------------
// Health factor
// ---------------------------------------------------------------------------

#[test]
fn test_get_health_factor_no_debt_returns_max() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let user = Address::generate(&env);
    // No positions at all — health factor is i128::MAX.
    assert_eq!(pool.get_health_factor(&user), i128::MAX);
}

#[test]
fn test_get_health_factor_with_debt() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    // 100 WBTC × $10 = $1000 collateral, 80% threshold → $800 weighted.
    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);
    pool.borrow(&borrower, &s.usdc, &700_i128);

    let hf = pool.get_health_factor(&borrower);
    // HF = 800 / 700 ≈ 1.142 WAD
    assert!(hf > WAD);
    assert!(hf < WAD * 2);
}

// ---------------------------------------------------------------------------
// Market invariants
// ---------------------------------------------------------------------------

#[test]
fn test_assert_market_invariants_after_operations() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let pool = pool(&env, &s);

    let supplier = Address::generate(&env);
    mint(&env, &s.usdc, &supplier, 1_000);
    pool.supply(&supplier, &s.usdc, &1_000_i128);

    let borrower = Address::generate(&env);
    mint(&env, &s.wbtc, &borrower, 100);
    pool.supply(&borrower, &s.wbtc, &100_i128);
    pool.borrow(&borrower, &s.usdc, &700_i128);

    pool.assert_market_invariants(&s.usdc);
    pool.assert_market_invariants(&s.wbtc);
}
