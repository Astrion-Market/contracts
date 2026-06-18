#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

use astrion_math::WAD;

use crate::{
    errors::MarketError, types::IsolatedMarketConfig, IsolatedMarketContract,
    IsolatedMarketContractClient,
};

// ---------------------------------------------------------------------------
// Mock Oracle Adapter
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
// Mock Rate Model — constant 5% borrow / 4% supply
// ---------------------------------------------------------------------------

mod mock_rate_model {
    use soroban_sdk::{contract, contractimpl, Env};

    use crate::RateSnapshot;

    #[contract]
    pub struct MockRateModel;

    #[contractimpl]
    impl MockRateModel {
        pub fn get_rates(
            _env: Env,
            _total_borrowed: i128,
            _total_supplied: i128,
        ) -> RateSnapshot {
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

struct Setup {
    market_id: Address,
    oracle_id: Address,
    collateral: Address,
    debt: Address,
    treasury: Address,
}

/// Deploy an isolated market with:
///   collateral price = 1 WAD,  debt price = 10 WAD.
///   LLTV 80%, bonus 5%.
///
/// With these prices:
///   Supply 1000 collateral → value $1000, weighted $800.
///   Borrow 70 debt   → value $700  → HF ≈ 1.14 (healthy).
///   Borrow 90 debt   → value $900  → HF ≈ 0.89 (liquidatable).
///   Liquidity check uses raw collateral count, so 70/90 ≤ 1000 always passes.
fn setup(env: &Env) -> Setup {
    let treasury = Address::generate(env);

    let oracle_id = env.register(MockOracle, ());
    let rate_model_id = env.register(MockRateModel, ());

    let collateral = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();
    let debt = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();

    let oracle = MockOracleClient::new(env, &oracle_id);
    oracle.set_price(&collateral, &WAD);       // $1 per collateral unit
    oracle.set_price(&debt, &(10 * WAD));      // $10 per debt unit

    let market_id = env.register(IsolatedMarketContract, ());
    IsolatedMarketContractClient::new(env, &market_id).initialize(
        &IsolatedMarketConfig {
            collateral_asset: collateral.clone(),
            loan_asset: debt.clone(),
            oracle_adapter: oracle_id.clone(),
            lltv: WAD * 80 / 100,
            liquidation_bonus: WAD * 5 / 100,
            reserve_factor: WAD / 10,
            supply_cap: 0,
            borrow_cap: 0,
            rate_model: rate_model_id,
            treasury: treasury.clone(),
        },
    );

    Setup {
        market_id,
        oracle_id,
        collateral,
        debt,
        treasury,
    }
}

fn market<'a>(env: &'a Env, s: &'a Setup) -> IsolatedMarketContractClient<'a> {
    IsolatedMarketContractClient::new(env, &s.market_id)
}

fn mint_collateral(env: &Env, s: &Setup, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, &s.collateral).mint(to, &amount);
}

fn mint_debt(env: &Env, s: &Setup, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, &s.debt).mint(to, &amount);
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let cfg = market(&env, &s).get_market_config().unwrap();
    assert_eq!(cfg.lltv, WAD * 80 / 100);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let rate_model_id = env.register(MockRateModel, ());
    let result = market(&env, &s).try_initialize(&IsolatedMarketConfig {
        collateral_asset: s.collateral.clone(),
        loan_asset: s.debt.clone(),
        oracle_adapter: s.oracle_id.clone(),
        lltv: WAD * 80 / 100,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 0,
        rate_model: rate_model_id,
        treasury: s.treasury.clone(),
    });
    assert_eq!(result, Err(Ok(MarketError::AlreadyInitialized)));
}

#[test]
fn test_initialize_invalid_config_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let treasury = Address::generate(&env);
    let rate_model_id = env.register(MockRateModel, ());
    let collateral = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();
    let debt = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();
    let oracle_id = env.register(MockOracle, ());

    let market_id = env.register(IsolatedMarketContract, ());
    // lltv == WAD (100%) is out of range — invalid
    let result = IsolatedMarketContractClient::new(&env, &market_id).try_initialize(
        &IsolatedMarketConfig {
            collateral_asset: collateral,
            loan_asset: debt,
            oracle_adapter: oracle_id,
            lltv: WAD,
            liquidation_bonus: WAD * 5 / 100,
            reserve_factor: WAD / 10,
            supply_cap: 0,
            borrow_cap: 0,
            rate_model: rate_model_id,
            treasury,
        },
    );
    assert_eq!(result, Err(Ok(MarketError::InvalidAmount)));
}

// ---------------------------------------------------------------------------
// Supply
// ---------------------------------------------------------------------------

#[test]
fn test_supply_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let user = Address::generate(&env);
    mint_collateral(&env, &s, &user, 1_000);

    m.supply(&user, &1_000_i128);

    let pos = m.get_user_position(&user).unwrap();
    assert!(pos.scaled_supply > 0);
    assert_eq!(token::Client::new(&env, &s.collateral).balance(&user), 0);
}

#[test]
fn test_supply_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let treasury = Address::generate(&env);
    let rate_model_id = env.register(MockRateModel, ());
    let oracle_id = env.register(MockOracle, ());
    let collateral = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();
    let debt = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();

    let market_id = env.register(IsolatedMarketContract, ());
    IsolatedMarketContractClient::new(&env, &market_id).initialize(
        &IsolatedMarketConfig {
            collateral_asset: collateral.clone(),
            loan_asset: debt.clone(),
            oracle_adapter: oracle_id.clone(),
            lltv: WAD * 80 / 100,
            liquidation_bonus: WAD * 5 / 100,
            reserve_factor: WAD / 10,
            supply_cap: 500,
            borrow_cap: 0,
            rate_model: rate_model_id,
            treasury,
        },
    );

    let user = Address::generate(&env);
    token::StellarAssetClient::new(&env, &collateral).mint(&user, &1_000_i128);
    let m = IsolatedMarketContractClient::new(&env, &market_id);
    m.supply(&user, &500_i128);

    let result = m.try_supply(&user, &1_i128);
    assert_eq!(result, Err(Ok(MarketError::SupplyCapExceeded)));
}

// ---------------------------------------------------------------------------
// Withdraw
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_round_trip() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let user = Address::generate(&env);
    mint_collateral(&env, &s, &user, 1_000);

    m.supply(&user, &1_000_i128);
    m.withdraw(&user, &1_000_i128);

    assert_eq!(
        token::Client::new(&env, &s.collateral).balance(&user),
        1_000
    );
}

#[test]
fn test_withdraw_insufficient_collateral_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let user = Address::generate(&env);
    mint_collateral(&env, &s, &user, 500);
    m.supply(&user, &500_i128);

    let result = m.try_withdraw(&user, &600_i128);
    assert_eq!(result, Err(Ok(MarketError::InsufficientCollateral)));
}

// ---------------------------------------------------------------------------
// Borrow
// ---------------------------------------------------------------------------

#[test]
fn test_borrow_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    // Seed pool with debt tokens (the market sends these out when borrowing).
    token::StellarAssetClient::new(&env, &s.debt).mint(&s.market_id, &200_i128);

    // Borrower supplies 1000 collateral (value $1000, weighted $800).
    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply(&borrower, &1_000_i128);

    // Borrow 70 debt units ($700 value) — HF = 800/700 ≈ 1.14 > 1.
    // Liquidity check: 70 ≤ 1000 total collateral supply ✓.
    m.borrow(&borrower, &70_i128);

    assert_eq!(token::Client::new(&env, &s.debt).balance(&borrower), 70);
}

#[test]
fn test_borrow_exceeds_collateral_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    token::StellarAssetClient::new(&env, &s.debt).mint(&s.market_id, &200_i128);

    // 1000 collateral × $1 = $1000 value, 80% threshold → weighted $800.
    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply(&borrower, &1_000_i128);

    // Borrow 90 ($900 value) — HF = 800/900 < 1.
    // Liquidity check: 90 ≤ 1000 ✓, so HF check fires.
    let result = m.try_borrow(&borrower, &90_i128);
    assert_eq!(result, Err(Ok(MarketError::HealthFactorTooLow)));
}

#[test]
fn test_borrow_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let treasury = Address::generate(&env);
    let rate_model_id = env.register(MockRateModel, ());
    let oracle_id = env.register(MockOracle, ());
    let collateral = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();
    let debt = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();

    MockOracleClient::new(&env, &oracle_id).set_price(&collateral, &WAD);
    MockOracleClient::new(&env, &oracle_id).set_price(&debt, &(10 * WAD));

    let market_id = env.register(IsolatedMarketContract, ());
    IsolatedMarketContractClient::new(&env, &market_id).initialize(
        &IsolatedMarketConfig {
            collateral_asset: collateral.clone(),
            loan_asset: debt.clone(),
            oracle_adapter: oracle_id,
            lltv: WAD * 80 / 100,
            liquidation_bonus: WAD * 5 / 100,
            reserve_factor: WAD / 10,
            supply_cap: 0,
            borrow_cap: 400,
            rate_model: rate_model_id,
            treasury,
        },
    );

    // Seed debt liquidity; borrow cap = 400.
    // Supply 10000 collateral × $1 = $10000, weighted $8000.
    // Borrow 400 debt × $10 = $4000. HF = 8000/4000 = 2 > 1 ✓.
    token::StellarAssetClient::new(&env, &debt).mint(&market_id, &500_i128);

    let borrower = Address::generate(&env);
    token::StellarAssetClient::new(&env, &collateral).mint(&borrower, &10_000_i128);
    let m = IsolatedMarketContractClient::new(&env, &market_id);
    m.supply(&borrower, &10_000_i128);
    m.borrow(&borrower, &400_i128);

    let result = m.try_borrow(&borrower, &1_i128);
    assert_eq!(result, Err(Ok(MarketError::BorrowCapExceeded)));
}

// ---------------------------------------------------------------------------
// Repay
// ---------------------------------------------------------------------------

#[test]
fn test_repay_full() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    token::StellarAssetClient::new(&env, &s.debt).mint(&s.market_id, &200_i128);

    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply(&borrower, &1_000_i128);
    m.borrow(&borrower, &70_i128);

    // Borrower now has 70 debt tokens; add 10 more to overpay (capped at actual debt).
    mint_debt(&env, &s, &borrower, 10);
    m.repay(&borrower, &borrower, &500_i128);

    let pos = m.get_user_position(&borrower).unwrap();
    assert_eq!(pos.scaled_borrow, 0);
}

#[test]
fn test_repay_partial() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    token::StellarAssetClient::new(&env, &s.debt).mint(&s.market_id, &200_i128);

    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply(&borrower, &1_000_i128);
    m.borrow(&borrower, &70_i128);

    m.repay(&borrower, &borrower, &30_i128);

    let pos = m.get_user_position(&borrower).unwrap();
    assert!(pos.scaled_borrow > 0);
    assert!(pos.scaled_borrow < 70);
}

// ---------------------------------------------------------------------------
// Liquidate
// ---------------------------------------------------------------------------

#[test]
fn test_liquidate_healthy_position_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    token::StellarAssetClient::new(&env, &s.debt).mint(&s.market_id, &200_i128);

    // 1000 collateral × $1 = $1000, weighted $800; borrow 70 × $10 = $700 → HF ≈ 1.14.
    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply(&borrower, &1_000_i128);
    m.borrow(&borrower, &70_i128);

    let liquidator = Address::generate(&env);
    mint_debt(&env, &s, &liquidator, 50);
    let result = m.try_liquidate(&liquidator, &borrower, &35_i128);
    assert_eq!(result, Err(Ok(MarketError::HealthFactorOk)));
}

#[test]
fn test_liquidate_undercollateralized_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    token::StellarAssetClient::new(&env, &s.debt).mint(&s.market_id, &200_i128);

    // Start healthy: 1000 collateral × $1 = $1000, weighted $800; borrow 70 debt × $10.
    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply(&borrower, &1_000_i128);
    m.borrow(&borrower, &70_i128);

    // Crash collateral price from $1 to $0.5.
    // New: weighted collateral = 1000 × $0.5 × 0.8 = $400; debt = 70 × $10 = $700.
    // HF = 400/700 ≈ 0.57 < 1 — liquidatable.
    oracle.set_price(&s.collateral, &(WAD / 2));

    let liquidator = Address::generate(&env);
    // Repay 35 (≤ 50% × 70 = 35). Liquidator needs 35 debt tokens.
    mint_debt(&env, &s, &liquidator, 35);

    // collateral_seized = wad_div(35 × $10 × 1.05, $0.5) = wad_div(367, WAD/2) = 734 units.
    // Market has 1000 collateral → sufficient.
    m.liquidate(&liquidator, &borrower, &35_i128);

    assert!(token::Client::new(&env, &s.collateral).balance(&liquidator) > 0);
}

// ---------------------------------------------------------------------------
// Interest accrual
// ---------------------------------------------------------------------------

#[test]
fn test_interest_accrual_advances_indexes() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    token::StellarAssetClient::new(&env, &s.debt).mint(&s.market_id, &200_i128);

    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply(&borrower, &1_000_i128);
    m.borrow(&borrower, &70_i128);

    let state_before = m.get_market_state().unwrap();
    assert_eq!(state_before.borrow_index, WAD);
    assert_eq!(state_before.supply_index, WAD);

    env.ledger().with_mut(|li| li.timestamp = 31_536_000);
    m.accrue_interest();

    let state_after = m.get_market_state().unwrap();
    assert!(state_after.borrow_index > WAD);
    assert!(state_after.supply_index > WAD);
}

// ---------------------------------------------------------------------------
// Pause
// ---------------------------------------------------------------------------

#[test]
fn test_pause_blocks_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    m.pause(&s.treasury);

    let user = Address::generate(&env);
    mint_collateral(&env, &s, &user, 1_000);
    let result = m.try_supply(&user, &100_i128);
    assert_eq!(result, Err(Ok(MarketError::Paused)));

    m.unpause(&s.treasury);
    m.supply(&user, &100_i128); // succeeds after unpause
}
