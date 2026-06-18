#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Symbol,
};

use crate::{
    errors::OracleError, types::Asset, OracleAdapterContract, OracleAdapterContractClient,
};

// ---------------------------------------------------------------------------
// Mock oracle that satisfies the OracleClient interface.
//
// In unit tests we can't call a real Reflector oracle, so we deploy a minimal
// mock contract that returns a configurable price and timestamp.
// ---------------------------------------------------------------------------

mod mock_oracle {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

    #[contracttype]
    #[derive(Clone)]
    pub enum Asset {
        Stellar(Address),
        Other(Symbol),
    }

    #[contracttype]
    #[derive(Clone)]
    pub struct PriceData {
        pub price: i128,
        pub timestamp: u64,
    }

    #[contracttype]
    enum DataKey {
        Price,
        Decimals,
    }

    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        pub fn set_price(env: Env, price: i128, timestamp: u64) {
            env.storage()
                .instance()
                .set(&DataKey::Price, &PriceData { price, timestamp });
        }

        pub fn set_decimals(env: Env, decimals: u32) {
            env.storage().instance().set(&DataKey::Decimals, &decimals);
        }

        pub fn lastprice(env: Env, _asset: Asset) -> Option<PriceData> {
            env.storage().instance().get(&DataKey::Price)
        }

        pub fn decimals(env: Env) -> u32 {
            env.storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::Decimals)
                .unwrap_or(7)
        }
    }
}

use mock_oracle::{MockOracle, MockOracleClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_adapter(env: &Env) -> (OracleAdapterContractClient<'_>, Address, Address) {
    let adapter_id = env.register(OracleAdapterContract, ());
    let oracle_id = env.register(MockOracle, ());
    let admin = Address::generate(env);
    let client = OracleAdapterContractClient::new(env, &adapter_id);
    (client, admin, oracle_id)
}

fn set_ledger_time(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| li.timestamp = timestamp);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, oracle_id) = create_adapter(&env);

    client.initialize(&admin, &oracle_id, &300);
    assert_eq!(client.admin(), admin);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, oracle_id) = create_adapter(&env);

    client.initialize(&admin, &oracle_id, &300);
    let result = client.try_initialize(&admin, &oracle_id, &300);
    assert_eq!(result, Err(Ok(OracleError::AlreadyInitialized)));
}

#[test]
fn test_get_price_success() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1_000);

    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &300); // 300s staleness

    // Configure the mock oracle: BTC at $50,000 with 7 decimals.
    let oracle_client = MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_decimals(&7);
    oracle_client.set_price(&(50_000 * 10_i128.pow(7)), &900); // timestamp 900, age = 100s

    let xlm_asset = Asset::Stellar(Address::generate(&env));
    let resolved = client.get_price(&xlm_asset);

    // 50_000 * 1e7 normalised to WAD (1e18) = 50_000 * 1e18
    let expected_wad = 50_000_i128 * 1_000_000_000_000_000_000;
    assert_eq!(resolved.price_wad, expected_wad);
    assert_eq!(resolved.timestamp, 900);
}

#[test]
fn test_normalizes_common_oracle_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1_000);

    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &300);
    let oracle_client = MockOracleClient::new(&env, &oracle_id);
    let asset = Asset::Stellar(Address::generate(&env));

    oracle_client.set_decimals(&6);
    oracle_client.set_price(&(123 * 10_i128.pow(6)), &990);
    assert_eq!(
        client.get_price(&asset).price_wad,
        123 * 1_000_000_000_000_000_000_i128
    );

    oracle_client.set_decimals(&8);
    oracle_client.set_price(&(123 * 10_i128.pow(8)), &990);
    assert_eq!(
        client.get_price(&asset).price_wad,
        123 * 1_000_000_000_000_000_000_i128
    );

    oracle_client.set_decimals(&18);
    oracle_client.set_price(&(123 * 1_000_000_000_000_000_000_i128), &990);
    assert_eq!(
        client.get_price(&asset).price_wad,
        123 * 1_000_000_000_000_000_000_i128
    );
}

#[test]
fn test_stale_price_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1_000);

    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &60); // 60s staleness

    let oracle_client = MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_price(&(100 * 10_i128.pow(7)), &800); // timestamp 800, age = 200s > 60s

    let asset = Asset::Stellar(Address::generate(&env));
    let result = client.try_get_price(&asset);
    assert_eq!(result, Err(Ok(OracleError::StalePrice)));
}

#[test]
fn test_invalid_price_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1_000);

    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &300);

    let oracle_client = MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_price(&0, &990); // price = 0, invalid

    let asset = Asset::Stellar(Address::generate(&env));
    let result = client.try_get_price(&asset);
    assert_eq!(result, Err(Ok(OracleError::InvalidPrice)));
}

#[test]
fn test_price_bounds_reject_outliers() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1_000);

    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &300);
    let oracle_client = MockOracleClient::new(&env, &oracle_id);
    oracle_client.set_decimals(&7);

    let asset = Asset::Stellar(Address::generate(&env));
    client.set_asset_bounds(
        &asset,
        &(90 * 1_000_000_000_000_000_000_i128),
        &(110 * 1_000_000_000_000_000_000_i128),
    );

    oracle_client.set_price(&(100 * 10_i128.pow(7)), &990);
    assert_eq!(
        client.get_price(&asset).price_wad,
        100 * 1_000_000_000_000_000_000_i128
    );

    oracle_client.set_price(&(120 * 10_i128.pow(7)), &990);
    assert_eq!(
        client.try_get_price(&asset),
        Err(Ok(OracleError::PriceOutOfBounds))
    );

    client.remove_asset_bounds(&asset);
    assert_eq!(
        client.get_price(&asset).price_wad,
        120 * 1_000_000_000_000_000_000_i128
    );
}

#[test]
fn test_invalid_price_bounds_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &300);

    let asset = Asset::Stellar(Address::generate(&env));
    let result = client.try_set_asset_bounds(&asset, &100, &99);
    assert_eq!(result, Err(Ok(OracleError::InvalidBounds)));
}

#[test]
fn test_per_asset_oracle_override() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1_000);

    let (client, admin, default_oracle_id) = create_adapter(&env);
    client.initialize(&admin, &default_oracle_id, &300);

    // Deploy a second oracle with a different price.
    let second_oracle_id = env.register(MockOracle, ());
    let second_oracle = MockOracleClient::new(&env, &second_oracle_id);
    second_oracle.set_decimals(&7);
    second_oracle.set_price(&(200 * 10_i128.pow(7)), &990);

    let asset = Asset::Other(Symbol::new(&env, "EURC"));
    client.set_asset_oracle(&asset, &second_oracle_id, &300);

    let resolved = client.get_price(&asset);
    assert_eq!(resolved.source, second_oracle_id);
    assert_eq!(resolved.price_wad, 200 * 1_000_000_000_000_000_000_i128);
}

#[test]
fn test_set_default_oracle_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &300);

    // Calling set_default_oracle as admin should succeed.
    let new_oracle = Address::generate(&env);
    client.set_default_oracle(&new_oracle);
    assert_eq!(client.default_oracle(), Some(new_oracle));
}

#[test]
fn test_transfer_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, oracle_id) = create_adapter(&env);
    client.initialize(&admin, &oracle_id, &300);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    assert_eq!(client.admin(), new_admin);
}

#[test]
fn test_zero_staleness_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, oracle_id) = create_adapter(&env);

    let result = client.try_initialize(&admin, &oracle_id, &0);
    assert_eq!(result, Err(Ok(OracleError::InvalidStaleness)));
}
