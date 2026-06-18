#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use crate::{FactoryError, MarketFactoryContract, MarketFactoryContractClient};

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
