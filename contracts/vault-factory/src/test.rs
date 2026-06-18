#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String};

use crate::{VaultFactoryContract, VaultFactoryContractClient, VaultFactoryError};

fn wasm_hash(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

#[test]
fn test_initialize_success() {
    let env = Env::default();
    let factory_id = env.register(VaultFactoryContract, ());
    let client = VaultFactoryContractClient::new(&env, &factory_id);
    let owner = Address::generate(&env);
    let hash = wasm_hash(&env, 7);

    client.initialize(&owner, &hash);

    assert_eq!(client.owner().unwrap(), owner);
    assert_eq!(client.vault_wasm_hash().unwrap(), hash);
}

#[test]
fn test_create_vault_before_init_fails() {
    let env = Env::default();
    let factory_id = env.register(VaultFactoryContract, ());
    let client = VaultFactoryContractClient::new(&env, &factory_id);
    let owner = Address::generate(&env);
    let asset = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();

    let result = client.try_create_vault(
        &owner,
        &asset,
        &wasm_hash(&env, 1),
        &String::from_str(&env, "Vault"),
        &String::from_str(&env, "vTKN"),
    );

    assert_eq!(result, Err(Ok(VaultFactoryError::NotInitialized)));
}

#[test]
fn test_owner_updates_wasm_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let factory_id = env.register(VaultFactoryContract, ());
    let client = VaultFactoryContractClient::new(&env, &factory_id);
    let owner = Address::generate(&env);

    client.initialize(&owner, &wasm_hash(&env, 1));
    client.set_vault_wasm_hash(&wasm_hash(&env, 2));

    assert_eq!(client.vault_wasm_hash().unwrap(), wasm_hash(&env, 2));
}

#[test]
fn test_deployment_salt_depends_on_full_key() {
    let env = Env::default();
    let factory_id = env.register(VaultFactoryContract, ());
    let client = VaultFactoryContractClient::new(&env, &factory_id);
    let owner = Address::generate(&env);
    let other_owner = Address::generate(&env);
    let asset = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    let salt = wasm_hash(&env, 3);

    let a = client.deployment_salt(&owner, &asset, &salt);
    let b = client.deployment_salt(&other_owner, &asset, &salt);

    assert_ne!(a, b);
    assert_eq!(token::Client::new(&env, &asset).decimals(), 7);
}
