#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

use astrion_math::WAD;

use crate::{
    errors::MarketError, IsolatedMarketConfig, IsolatedMarketContract, IsolatedMarketContractClient,
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
            let OracleAsset::Stellar(addr) = asset;
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
use test_token::{TestToken, TestTokenClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Setup {
    market_id: Address,
    oracle_id: Address,
    collateral: Address,
    loan: Address,
    treasury: Address,
}

/// Deploy an isolated market with collateral price $1 and loan price $10,
/// LLTV 80%, bonus 5%.
fn setup(env: &Env) -> Setup {
    let treasury = Address::generate(env);

    let oracle_id = env.register(MockOracle, ());
    let rate_model_id = env.register(MockRateModel, ());

    let collateral = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();
    let loan = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();

    let oracle = MockOracleClient::new(env, &oracle_id);
    oracle.set_price(&collateral, &WAD); // $1 per collateral unit
    oracle.set_price(&loan, &(10 * WAD)); // $10 per loan unit

    let market_id = env.register(IsolatedMarketContract, ());
    IsolatedMarketContractClient::new(env, &market_id).initialize(&IsolatedMarketConfig {
        collateral_asset: collateral.clone(),
        loan_asset: loan.clone(),
        oracle_adapter: oracle_id.clone(),
        lltv: WAD * 80 / 100,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 0,
        rate_model: rate_model_id,
        treasury: treasury.clone(),
    });

    Setup {
        market_id,
        oracle_id,
        collateral,
        loan,
        treasury,
    }
}

fn market<'a>(env: &'a Env, s: &'a Setup) -> IsolatedMarketContractClient<'a> {
    IsolatedMarketContractClient::new(env, &s.market_id)
}

fn mint_collateral(env: &Env, s: &Setup, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, &s.collateral).mint(to, &amount);
}

fn mint_loan(env: &Env, s: &Setup, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, &s.loan).mint(to, &amount);
}

fn register_test_token(env: &Env, admin: &Address, decimals: u32, symbol: &str) -> Address {
    env.register(
        TestToken,
        (
            admin.clone(),
            decimals,
            String::from_str(env, symbol),
            String::from_str(env, symbol),
        ),
    )
}

fn mint_test_token(env: &Env, token: &Address, to: &Address, amount: i128) {
    TestTokenClient::new(env, token).mint(to, &amount);
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
    let params = market(&env, &s).get_market_params().unwrap();
    assert_eq!(params.loan_asset, s.loan);
    assert_eq!(params.collateral_asset, s.collateral);
    assert_eq!(params.oracle_adapter, s.oracle_id);
    assert_eq!(params.lltv, WAD * 80 / 100);
    assert_eq!(market(&env, &s).app_version(), 1);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let rate_model_id = env.register(MockRateModel, ());
    let result = market(&env, &s).try_initialize(&IsolatedMarketConfig {
        collateral_asset: s.collateral.clone(),
        loan_asset: s.loan.clone(),
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
    let loan = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();
    let oracle_id = env.register(MockOracle, ());

    let market_id = env.register(IsolatedMarketContract, ());
    // lltv == WAD (100%) is out of range — invalid
    let result =
        IsolatedMarketContractClient::new(&env, &market_id).try_initialize(&IsolatedMarketConfig {
            collateral_asset: collateral,
            loan_asset: loan,
            oracle_adapter: oracle_id,
            lltv: WAD,
            liquidation_bonus: WAD * 5 / 100,
            reserve_factor: WAD / 10,
            supply_cap: 0,
            borrow_cap: 0,
            rate_model: rate_model_id,
            treasury,
        });
    assert_eq!(result, Err(Ok(MarketError::InvalidAmount)));
}

// ---------------------------------------------------------------------------
// Lender supply / withdraw (loan asset)
// ---------------------------------------------------------------------------

#[test]
fn test_supply_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let lender = Address::generate(&env);
    mint_loan(&env, &s, &lender, 1_000);

    let shares = m.supply(&lender, &1_000_i128, &lender);
    assert!(shares > 0);

    let pos = m.get_user_position(&lender).unwrap();
    assert_eq!(pos.supply_shares, shares);
    assert_eq!(token::Client::new(&env, &s.loan).balance(&lender), 0);

    let state = m.get_market_state().unwrap();
    assert_eq!(state.total_supply_assets, 1_000);
    assert_eq!(state.total_supply_shares, shares);
}

#[test]
fn test_supply_on_behalf_credits_receiver() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let payer = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    mint_loan(&env, &s, &payer, 500);

    m.supply(&payer, &500_i128, &beneficiary);

    assert!(m.get_user_position(&beneficiary).unwrap().supply_shares > 0);
    assert!(m.get_user_position(&payer).is_none());
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
    let loan = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();

    let market_id = env.register(IsolatedMarketContract, ());
    IsolatedMarketContractClient::new(&env, &market_id).initialize(&IsolatedMarketConfig {
        collateral_asset: collateral.clone(),
        loan_asset: loan.clone(),
        oracle_adapter: oracle_id.clone(),
        lltv: WAD * 80 / 100,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 500,
        borrow_cap: 0,
        rate_model: rate_model_id,
        treasury,
    });

    let lender = Address::generate(&env);
    token::StellarAssetClient::new(&env, &loan).mint(&lender, &1_000_i128);
    let m = IsolatedMarketContractClient::new(&env, &market_id);
    m.supply(&lender, &500_i128, &lender);

    let result = m.try_supply(&lender, &1_i128, &lender);
    assert_eq!(result, Err(Ok(MarketError::SupplyCapExceeded)));
}

#[test]
fn test_withdraw_round_trip() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let lender = Address::generate(&env);
    mint_loan(&env, &s, &lender, 1_000);

    m.supply(&lender, &1_000_i128, &lender);
    // Withdraw by asset amount (shares = 0).
    m.withdraw(&lender, &1_000_i128, &0_i128, &lender, &lender);

    assert_eq!(token::Client::new(&env, &s.loan).balance(&lender), 1_000);
    assert_eq!(m.get_user_position(&lender).unwrap().supply_shares, 0);
}

#[test]
fn test_withdraw_by_shares() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let lender = Address::generate(&env);
    mint_loan(&env, &s, &lender, 1_000);
    let shares = m.supply(&lender, &1_000_i128, &lender);

    // Withdraw by share amount (assets = 0).
    m.withdraw(&lender, &0_i128, &shares, &lender, &lender);
    assert_eq!(token::Client::new(&env, &s.loan).balance(&lender), 1_000);
}

#[test]
fn test_withdraw_both_inputs_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let lender = Address::generate(&env);
    mint_loan(&env, &s, &lender, 1_000);
    m.supply(&lender, &1_000_i128, &lender);

    let result = m.try_withdraw(&lender, &500_i128, &500_i128, &lender, &lender);
    assert_eq!(result, Err(Ok(MarketError::InconsistentInput)));
}

#[test]
fn test_withdraw_insufficient_supply_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let lender = Address::generate(&env);
    mint_loan(&env, &s, &lender, 500);
    m.supply(&lender, &500_i128, &lender);

    let result = m.try_withdraw(&lender, &600_i128, &0_i128, &lender, &lender);
    assert_eq!(result, Err(Ok(MarketError::InsufficientSupply)));
}

#[test]
fn test_withdraw_tiny_amount_burns_shares() {
    // Regression: once interest accrues, total_supply_assets > total share
    // value, so plain floor rounding let a small asset withdrawal burn 0 shares
    // and still pay out (a free-withdrawal leak). With shares rounded UP, any
    // positive withdrawal must burn at least one share.
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let lender = lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 10_000);
    m.borrow(&borrower, &500_i128, &borrower, &borrower);

    // Accrue a year of interest so the supply pool grows above its shares.
    env.ledger().with_mut(|li| li.timestamp = 31_536_000);
    m.accrue_interest();
    assert!(m.get_market_state().unwrap().total_supply_assets > 1_000);

    let before = m.get_user_position(&lender).unwrap().supply_shares;
    m.withdraw(&lender, &1_i128, &0_i128, &lender, &lender);
    let after = m.get_user_position(&lender).unwrap().supply_shares;
    assert!(
        after < before,
        "tiny withdrawal must burn shares, not be free"
    );
}

// ---------------------------------------------------------------------------
// Borrower collateral
// ---------------------------------------------------------------------------

#[test]
fn test_supply_collateral_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);

    m.supply_collateral(&borrower, &1_000_i128, &borrower);

    let pos = m.get_user_position(&borrower).unwrap();
    assert_eq!(pos.collateral, 1_000);
    assert_eq!(pos.supply_shares, 0);
    assert_eq!(
        token::Client::new(&env, &s.collateral).balance(&borrower),
        0
    );
    assert_eq!(m.get_market_state().unwrap().total_collateral, 1_000);
}

#[test]
fn test_withdraw_collateral_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 1_000);
    m.supply_collateral(&borrower, &1_000_i128, &borrower);

    // No debt outstanding, so withdrawal stays healthy.
    m.withdraw_collateral(&borrower, &400_i128, &borrower, &borrower);

    assert_eq!(m.get_user_position(&borrower).unwrap().collateral, 600);
    assert_eq!(
        token::Client::new(&env, &s.collateral).balance(&borrower),
        400
    );
}

#[test]
fn test_withdraw_collateral_too_much_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let borrower = Address::generate(&env);
    mint_collateral(&env, &s, &borrower, 500);
    m.supply_collateral(&borrower, &500_i128, &borrower);

    let result = m.try_withdraw_collateral(&borrower, &600_i128, &borrower, &borrower);
    assert_eq!(result, Err(Ok(MarketError::InsufficientCollateral)));
}

#[test]
fn test_supply_and_collateral_are_separate() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let user = Address::generate(&env);
    mint_loan(&env, &s, &user, 1_000);
    mint_collateral(&env, &s, &user, 2_000);

    m.supply(&user, &1_000_i128, &user);
    m.supply_collateral(&user, &2_000_i128, &user);

    let pos = m.get_user_position(&user).unwrap();
    assert!(pos.supply_shares > 0);
    assert_eq!(pos.collateral, 2_000);

    let state = m.get_market_state().unwrap();
    assert_eq!(state.total_supply_assets, 1_000);
    assert_eq!(state.total_collateral, 2_000);
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

    let lender = Address::generate(&env);
    mint_loan(&env, &s, &lender, 1_000);
    let result = m.try_supply(&lender, &100_i128, &lender);
    assert_eq!(result, Err(Ok(MarketError::Paused)));

    m.unpause(&s.treasury);
    m.supply(&lender, &100_i128, &lender); // succeeds after unpause
}

// ---------------------------------------------------------------------------
// Borrow / repay helpers
// ---------------------------------------------------------------------------

/// Register a lender who supplies `amount` of the loan asset.
fn lender_supplies(env: &Env, s: &Setup, amount: i128) -> Address {
    let lender = Address::generate(env);
    mint_loan(env, s, &lender, amount);
    market(env, s).supply(&lender, &amount, &lender);
    lender
}

/// Register a borrower who posts `amount` of collateral.
fn borrower_with_collateral(env: &Env, s: &Setup, amount: i128) -> Address {
    let borrower = Address::generate(env);
    mint_collateral(env, s, &borrower, amount);
    market(env, s).supply_collateral(&borrower, &amount, &borrower);
    borrower
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

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);

    // Collateral $1000 × 80% = $800 power; borrow 70 × $10 = $700 → HF ≈ 1.14.
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    assert_eq!(token::Client::new(&env, &s.loan).balance(&borrower), 70);
    let state = m.get_market_state().unwrap();
    assert_eq!(state.total_borrow_assets, 70);
    assert!(m.get_user_position(&borrower).unwrap().borrow_shares > 0);
}

#[test]
fn test_borrow_exceeds_collateral_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);

    // Borrow 90 × $10 = $900 > $800 power → unhealthy.
    let result = m.try_borrow(&borrower, &90_i128, &borrower, &borrower);
    assert_eq!(result, Err(Ok(MarketError::HealthFactorTooLow)));
}

#[test]
fn test_borrow_insufficient_liquidity_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    // Only 50 loan supplied, but plenty of collateral.
    lender_supplies(&env, &s, 50);
    let borrower = borrower_with_collateral(&env, &s, 10_000);

    let result = m.try_borrow(&borrower, &70_i128, &borrower, &borrower);
    assert_eq!(result, Err(Ok(MarketError::InsufficientLiquidity)));
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
    let loan = env
        .register_stellar_asset_contract_v2(treasury.clone())
        .address();

    MockOracleClient::new(&env, &oracle_id).set_price(&collateral, &WAD);
    MockOracleClient::new(&env, &oracle_id).set_price(&loan, &(10 * WAD));

    let market_id = env.register(IsolatedMarketContract, ());
    IsolatedMarketContractClient::new(&env, &market_id).initialize(&IsolatedMarketConfig {
        collateral_asset: collateral.clone(),
        loan_asset: loan.clone(),
        oracle_adapter: oracle_id,
        lltv: WAD * 80 / 100,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 400,
        rate_model: rate_model_id,
        treasury,
    });
    let m = IsolatedMarketContractClient::new(&env, &market_id);

    // Lender supplies 500 loan; borrower posts 10000 collateral ($8000 power).
    let lender = Address::generate(&env);
    token::StellarAssetClient::new(&env, &loan).mint(&lender, &500_i128);
    m.supply(&lender, &500_i128, &lender);

    let borrower = Address::generate(&env);
    token::StellarAssetClient::new(&env, &collateral).mint(&borrower, &10_000_i128);
    m.supply_collateral(&borrower, &10_000_i128, &borrower);

    m.borrow(&borrower, &400_i128, &borrower, &borrower);
    let result = m.try_borrow(&borrower, &1_i128, &borrower, &borrower);
    assert_eq!(result, Err(Ok(MarketError::BorrowCapExceeded)));
}

#[test]
fn test_health_factor_normalizes_mismatched_token_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    let treasury = Address::generate(&env);
    let rate_model_id = env.register(MockRateModel, ());
    let oracle_id = env.register(MockOracle, ());
    let collateral = register_test_token(&env, &treasury, 6, "COL6");
    let loan = register_test_token(&env, &treasury, 18, "LOAN18");

    MockOracleClient::new(&env, &oracle_id).set_price(&collateral, &1_000_000_000_i128);
    MockOracleClient::new(&env, &oracle_id).set_price(&loan, &WAD);

    let market_id = env.register(IsolatedMarketContract, ());
    IsolatedMarketContractClient::new(&env, &market_id).initialize(&IsolatedMarketConfig {
        collateral_asset: collateral.clone(),
        loan_asset: loan.clone(),
        oracle_adapter: oracle_id,
        lltv: WAD / 2,
        liquidation_bonus: WAD * 5 / 100,
        reserve_factor: WAD / 10,
        supply_cap: 0,
        borrow_cap: 0,
        rate_model: rate_model_id,
        treasury,
    });
    let m = IsolatedMarketContractClient::new(&env, &market_id);

    let lender = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint_test_token(&env, &loan, &lender, 2_000_i128);
    mint_test_token(&env, &collateral, &borrower, 2_i128);

    m.supply(&lender, &2_000_i128, &lender);
    m.supply_collateral(&borrower, &2_i128, &borrower);
    m.borrow(&borrower, &1_000_i128, &borrower, &borrower);

    assert_eq!(m.get_health_factor(&borrower), WAD);
    let result = m.try_borrow(&borrower, &1_i128, &borrower, &borrower);
    assert_eq!(result, Err(Ok(MarketError::HealthFactorTooLow)));
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

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    // Overpay by assets — capped at outstanding debt (70).
    m.repay(&borrower, &100_i128, &0_i128, &borrower);

    assert_eq!(m.get_user_position(&borrower).unwrap().borrow_shares, 0);
    assert_eq!(m.get_market_state().unwrap().total_borrow_assets, 0);
}

#[test]
fn test_repay_partial() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    m.repay(&borrower, &30_i128, &0_i128, &borrower);

    // Shares are virtual-offset scaled now; assert on the debt in asset terms.
    assert!(m.get_user_position(&borrower).unwrap().borrow_shares > 0);
    assert_eq!(m.get_market_state().unwrap().total_borrow_assets, 40);
}

#[test]
fn test_repay_no_debt_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let borrower = Address::generate(&env);
    mint_loan(&env, &s, &borrower, 100);
    let result = m.try_repay(&borrower, &10_i128, &0_i128, &borrower);
    assert_eq!(result, Err(Ok(MarketError::InvalidAmount)));
}

// ---------------------------------------------------------------------------
// Liquidate
// ---------------------------------------------------------------------------

const NO_DEADLINE: u64 = u64::MAX;

#[test]
fn test_liquidate_healthy_position_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    let liquidator = Address::generate(&env);
    mint_loan(&env, &s, &liquidator, 50);
    // seized=0, repay 1 share — gate should reject on health, not inputs.
    let result = m.try_liquidate(
        &liquidator,
        &borrower,
        &0_i128,
        &1_i128,
        &0_i128,
        &NO_DEADLINE,
    );
    assert_eq!(result, Err(Ok(MarketError::HealthFactorOk)));
}

#[test]
fn test_liquidate_full_repay_by_shares() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    // Mild crash $1 → $0.85: power = 1000 × 0.85 × 0.8 = $680 < $700 debt.
    // Full repay seizes ~875 collateral (< 1000), so no bad debt.
    oracle.set_price(&s.collateral, &(WAD * 85 / 100));

    let shares = m.get_user_position(&borrower).unwrap().borrow_shares;
    let liquidator = Address::generate(&env);
    mint_loan(&env, &s, &liquidator, 100);

    let (seized, repaid) = m.liquidate(
        &liquidator,
        &borrower,
        &0_i128,
        &shares,
        &0_i128,
        &NO_DEADLINE,
    );

    assert!(seized > 0 && seized < 1_000);
    assert_eq!(repaid, 70);
    assert_eq!(
        token::Client::new(&env, &s.collateral).balance(&liquidator),
        seized
    );
    // Debt fully cleared, collateral remains (no bad debt).
    assert_eq!(m.get_user_position(&borrower).unwrap().borrow_shares, 0);
    assert_eq!(m.get_market_state().unwrap().total_borrow_assets, 0);
    assert!(m.get_user_position(&borrower).unwrap().collateral > 0);
}

#[test]
fn test_liquidate_by_seized_assets() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);
    oracle.set_price(&s.collateral, &(WAD * 85 / 100));

    let liquidator = Address::generate(&env);
    mint_loan(&env, &s, &liquidator, 100);

    // Seize exactly 400 collateral; repaid debt is derived.
    let (seized, repaid) = m.liquidate(
        &liquidator,
        &borrower,
        &400_i128,
        &0_i128,
        &0_i128,
        &NO_DEADLINE,
    );

    assert_eq!(seized, 400);
    assert!(repaid > 0 && repaid < 70);
    assert_eq!(
        token::Client::new(&env, &s.collateral).balance(&liquidator),
        400
    );
    assert!(m.get_market_state().unwrap().total_borrow_assets < 70);
}

#[test]
fn test_liquidate_deadline_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let liquidator = Address::generate(&env);
    let result = m.try_liquidate(&liquidator, &borrower, &0_i128, &1_i128, &0_i128, &500_u64);
    assert_eq!(result, Err(Ok(MarketError::DeadlineExpired)));
}

#[test]
fn test_liquidate_slippage_exceeded() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);
    oracle.set_price(&s.collateral, &(WAD * 85 / 100));

    let shares = m.get_user_position(&borrower).unwrap().borrow_shares;
    let liquidator = Address::generate(&env);
    mint_loan(&env, &s, &liquidator, 100);

    // Demand far more collateral than the incentive yields.
    let result = m.try_liquidate(
        &liquidator,
        &borrower,
        &0_i128,
        &shares,
        &10_000_i128,
        &NO_DEADLINE,
    );
    assert_eq!(result, Err(Ok(MarketError::SlippageExceeded)));
}

#[test]
fn test_liquidate_bad_debt_socialized() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    // Deep crash $1 → $0.5: seizing all 1000 collateral repays only part of the
    // $700 debt, leaving residual debt that must be written off.
    oracle.set_price(&s.collateral, &(WAD / 2));
    assert_eq!(m.get_market_state().unwrap().total_supply_assets, 1_000);

    let liquidator = Address::generate(&env);
    mint_loan(&env, &s, &liquidator, 100);

    // Seize all collateral.
    m.liquidate(
        &liquidator,
        &borrower,
        &1_000_i128,
        &0_i128,
        &0_i128,
        &NO_DEADLINE,
    );

    // Borrower wiped out, residual debt written off.
    let pos = m.get_user_position(&borrower).unwrap();
    assert_eq!(pos.collateral, 0);
    assert_eq!(pos.borrow_shares, 0);
    let state = m.get_market_state().unwrap();
    assert_eq!(state.total_borrow_assets, 0);
    // Lenders absorbed the loss: supply assets fell below the deposited 1000.
    assert!(state.total_supply_assets < 1_000);
}

#[test]
fn test_bad_debt_lowers_lender_share_price() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    let lender = lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    oracle.set_price(&s.collateral, &(WAD / 2));
    let liquidator = Address::generate(&env);
    mint_loan(&env, &s, &liquidator, 100);
    m.liquidate(
        &liquidator,
        &borrower,
        &1_000_i128,
        &0_i128,
        &0_i128,
        &NO_DEADLINE,
    );

    // The lender redeems all shares and recovers less than the 1000 deposited.
    let shares = m.get_user_position(&lender).unwrap().supply_shares;
    let (assets, _) = m.withdraw(&lender, &0_i128, &shares, &lender, &lender);
    assert!(
        assets < 1_000,
        "bad debt must reduce lender redemption value"
    );
}

// ---------------------------------------------------------------------------
// Preview liquidate
// ---------------------------------------------------------------------------

#[test]
fn test_preview_liquidate_healthy_not_liquidatable() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    let preview = m.preview_liquidate(&borrower, &70_i128);
    assert!(!preview.liquidatable);
    assert_eq!(preview.seized_collateral, 0);
}

#[test]
fn test_preview_liquidate_matches_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);
    oracle.set_price(&s.collateral, &(WAD * 85 / 100));

    let preview = m.preview_liquidate(&borrower, &70_i128);
    assert!(preview.liquidatable);
    assert_eq!(preview.bad_debt_assets, 0);

    // Execute by the previewed repaid shares; seized should match within rounding.
    let liquidator = Address::generate(&env);
    mint_loan(&env, &s, &liquidator, 100);
    let (seized, _) = m.liquidate(
        &liquidator,
        &borrower,
        &0_i128,
        &preview.repaid_shares,
        &0_i128,
        &NO_DEADLINE,
    );
    assert!((seized - preview.seized_collateral).abs() <= 1);
}

// ---------------------------------------------------------------------------
// Account authorization
// ---------------------------------------------------------------------------

#[test]
fn test_authorization_roundtrip() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let owner = Address::generate(&env);
    let operator = Address::generate(&env);

    assert!(!m.is_authorized(&owner, &operator));
    m.set_authorization(&owner, &operator, &true);
    assert!(m.is_authorized(&owner, &operator));
    // Authorization is directional.
    assert!(!m.is_authorized(&operator, &owner));

    m.set_authorization(&owner, &operator, &false);
    assert!(!m.is_authorized(&owner, &operator));
}

#[test]
fn test_authorized_operator_can_borrow_on_behalf() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let owner = borrower_with_collateral(&env, &s, 1_000);
    let operator = Address::generate(&env);
    m.set_authorization(&owner, &operator, &true);

    // Operator borrows against owner's collateral, receiving the loan itself.
    m.borrow(&operator, &70_i128, &owner, &operator);

    assert_eq!(token::Client::new(&env, &s.loan).balance(&operator), 70);
    assert!(m.get_user_position(&owner).unwrap().borrow_shares > 0);
}

#[test]
fn test_unauthorized_operator_cannot_borrow() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let owner = borrower_with_collateral(&env, &s, 1_000);
    let operator = Address::generate(&env);

    let result = m.try_borrow(&operator, &70_i128, &owner, &operator);
    assert_eq!(result, Err(Ok(MarketError::Unauthorized)));
}

#[test]
fn test_unauthorized_operator_cannot_withdraw_collateral() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    let owner = borrower_with_collateral(&env, &s, 1_000);
    let operator = Address::generate(&env);

    let result = m.try_withdraw_collateral(&operator, &100_i128, &owner, &operator);
    assert_eq!(result, Err(Ok(MarketError::Unauthorized)));
}

#[test]
fn test_preview_liquidate_flags_bad_debt() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);
    let oracle = MockOracleClient::new(&env, &s.oracle_id);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    // Deep crash: repaying the full debt would need more collateral than exists.
    oracle.set_price(&s.collateral, &(WAD / 2));
    let preview = m.preview_liquidate(&borrower, &70_i128);

    assert!(preview.liquidatable);
    assert_eq!(preview.seized_collateral, 1_000); // capped at collateral
    assert!(preview.bad_debt_assets > 0);
}

// ---------------------------------------------------------------------------
// Interest accrual
// ---------------------------------------------------------------------------

#[test]
fn test_interest_accrual_grows_totals() {
    let env = Env::default();
    env.mock_all_auths();
    let s = setup(&env);
    let m = market(&env, &s);

    lender_supplies(&env, &s, 1_000);
    let borrower = borrower_with_collateral(&env, &s, 1_000);
    m.borrow(&borrower, &70_i128, &borrower, &borrower);

    let before = m.get_market_state().unwrap();
    assert_eq!(before.total_borrow_assets, 70);
    assert_eq!(before.total_supply_assets, 1_000);

    // Advance one year at 5% borrow APR.
    env.ledger().with_mut(|li| li.timestamp = 31_536_000);
    m.accrue_interest();

    let after = m.get_market_state().unwrap();
    assert!(after.total_borrow_assets > before.total_borrow_assets);
    assert!(after.total_supply_assets > before.total_supply_assets);
}
