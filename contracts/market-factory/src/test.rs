#![cfg(test)]

extern crate std;

use astrion_market_types::IsolatedMarketConfig;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use crate::{FactoryError, MarketFactoryContract, MarketFactoryContractClient};

const LLTV: i128 = 800_000_000_000_000_000; // 0.8 WAD

/// Build a config with distinct loan/collateral and the given irm/lltv.
fn cfg(
    env: &Env,
    loan: &Address,
    collateral: &Address,
    irm: &Address,
    lltv: i128,
) -> IsolatedMarketConfig {
    let any = Address::generate(env);
    IsolatedMarketConfig {
        collateral_asset: collateral.clone(),
        loan_asset: loan.clone(),
        oracle_adapter: any.clone(),
        lltv,
        liquidation_bonus: 0,
        reserve_factor: 0,
        supply_cap: 0,
        borrow_cap: 0,
        rate_model: irm.clone(),
        treasury: any,
    }
}

fn dummy_wasm_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

fn setup(env: &Env) -> (MarketFactoryContractClient<'_>, Address) {
    let admin = Address::generate(env);
    let factory_id = env.register(MarketFactoryContract, ());
    let client = MarketFactoryContractClient::new(env, &factory_id);
    client.initialize(&admin, &dummy_wasm_hash(env));
    (client, admin)
}

// ---------------------------------------------------------------------------
// IRM / LLTV registries
// ---------------------------------------------------------------------------

#[test]
fn test_irm_registry_roundtrip() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup(&env);

    let irm = Address::generate(&env);
    assert!(!factory.is_irm_enabled(&irm));

    factory.enable_irm(&irm);
    assert!(factory.is_irm_enabled(&irm));

    // An unrelated model is still disabled.
    assert!(!factory.is_irm_enabled(&Address::generate(&env)));
}

#[test]
fn test_lltv_registry_roundtrip() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup(&env);

    let lltv = 800_000_000_000_000_000_i128; // 0.8 WAD
    assert!(!factory.is_lltv_enabled(&lltv));

    factory.enable_lltv(&lltv);
    assert!(factory.is_lltv_enabled(&lltv));

    assert!(!factory.is_lltv_enabled(&(900_000_000_000_000_000_i128)));
}

#[test]
fn test_enable_irm_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register(MarketFactoryContract, ());
    let factory = MarketFactoryContractClient::new(&env, &factory_id);

    let result = factory.try_enable_irm(&Address::generate(&env));
    assert_eq!(result, Err(Ok(FactoryError::NotInitialized)));
}

// ---------------------------------------------------------------------------
// create_market validation (failure paths run before any deployment)
// ---------------------------------------------------------------------------

#[test]
fn test_create_market_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register(MarketFactoryContract, ());
    let factory = MarketFactoryContractClient::new(&env, &factory_id);

    let loan = Address::generate(&env);
    let collateral = Address::generate(&env);
    let irm = Address::generate(&env);
    let result = factory.try_create_market(&cfg(&env, &loan, &collateral, &irm, LLTV));
    assert_eq!(result, Err(Ok(FactoryError::NotInitialized)));
}

#[test]
fn test_create_market_rejects_same_asset() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup(&env);

    let asset = Address::generate(&env);
    let irm = Address::generate(&env);
    factory.enable_lltv(&LLTV);
    factory.enable_irm(&irm);

    let result = factory.try_create_market(&cfg(&env, &asset, &asset, &irm, LLTV));
    assert_eq!(result, Err(Ok(FactoryError::InvalidMarketParams)));
}

#[test]
fn test_create_market_rejects_disabled_lltv() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup(&env);

    let loan = Address::generate(&env);
    let collateral = Address::generate(&env);
    let irm = Address::generate(&env);
    factory.enable_irm(&irm); // lltv intentionally NOT enabled

    let result = factory.try_create_market(&cfg(&env, &loan, &collateral, &irm, LLTV));
    assert_eq!(result, Err(Ok(FactoryError::LltvNotEnabled)));
}

#[test]
fn test_create_market_rejects_disabled_irm() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup(&env);

    let loan = Address::generate(&env);
    let collateral = Address::generate(&env);
    let irm = Address::generate(&env);
    factory.enable_lltv(&LLTV); // irm intentionally NOT enabled

    let result = factory.try_create_market(&cfg(&env, &loan, &collateral, &irm, LLTV));
    assert_eq!(result, Err(Ok(FactoryError::IrmNotEnabled)));
}

// ---------------------------------------------------------------------------
// Market id derivation
// ---------------------------------------------------------------------------

#[test]
fn test_market_id_is_deterministic_and_param_sensitive() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup(&env);

    let loan = Address::generate(&env);
    let collateral = Address::generate(&env);
    let oracle = Address::generate(&env);
    let irm = Address::generate(&env);

    let id1 = factory.market_id(&loan, &collateral, &oracle, &irm, &LLTV);
    let id2 = factory.market_id(&loan, &collateral, &oracle, &irm, &LLTV);
    assert_eq!(id1, id2, "same params -> same id");

    // Different LLTV -> different market.
    let id3 = factory.market_id(&loan, &collateral, &oracle, &irm, &(LLTV + 1));
    assert_ne!(id1, id3);

    // Reversed loan/collateral direction -> different market.
    let id4 = factory.market_id(&collateral, &loan, &oracle, &irm, &LLTV);
    assert_ne!(id1, id4);
}
