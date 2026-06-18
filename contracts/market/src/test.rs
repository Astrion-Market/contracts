#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, token, Address, Env};

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
    let result = IsolatedMarketContractClient::new(&env, &market_id).try_initialize(
        &IsolatedMarketConfig {
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
        },
    );
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
    assert_eq!(token::Client::new(&env, &s.collateral).balance(&borrower), 0);
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
