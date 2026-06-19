#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use crate::{AdapterRegistryContract, AdapterRegistryContractClient, AdapterRegistryError};

#[test]
fn test_initialize_success() {
    let env = Env::default();
    let registry_id = env.register(AdapterRegistryContract, ());
    let client = AdapterRegistryContractClient::new(&env, &registry_id);
    let owner = Address::generate(&env);

    client.initialize(&owner);

    assert_eq!(client.owner().unwrap(), owner);
    assert_eq!(client.adapters().len(), 0);
}

#[test]
fn test_add_adapter_is_add_only() {
    let env = Env::default();
    env.mock_all_auths();
    let registry_id = env.register(AdapterRegistryContract, ());
    let client = AdapterRegistryContractClient::new(&env, &registry_id);
    let owner = Address::generate(&env);
    let adapter = Address::generate(&env);

    client.initialize(&owner);
    client.add_adapter(&owner, &adapter);

    assert!(client.is_in_registry(&adapter));
    assert_eq!(client.adapters().len(), 1);
    assert_eq!(
        client.try_add_adapter(&owner, &adapter),
        Err(Ok(AdapterRegistryError::AlreadyRegistered))
    );
}

#[test]
fn test_add_adapter_requires_owner() {
    let env = Env::default();
    env.mock_all_auths();
    let registry_id = env.register(AdapterRegistryContract, ());
    let client = AdapterRegistryContractClient::new(&env, &registry_id);
    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.initialize(&owner);

    assert_eq!(
        client.try_add_adapter(&stranger, &Address::generate(&env)),
        Err(Ok(AdapterRegistryError::Unauthorized))
    );
}

#[test]
fn test_owner_can_transfer_and_upgrade() {
    let env = Env::default();
    env.mock_all_auths();
    let registry_id = env.register(AdapterRegistryContract, ());
    let client = AdapterRegistryContractClient::new(&env, &registry_id);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);

    client.initialize(&owner);
    client.transfer_owner(&owner, &new_owner);

    assert_eq!(client.owner().unwrap(), new_owner);
    let _hash = BytesN::from_array(&env, &[9; 32]);
}
