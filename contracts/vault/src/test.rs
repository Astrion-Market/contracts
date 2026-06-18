#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env, String};

use crate::{VaultContract, VaultContractClient};

#[test]
fn test_initialize_success() {
    let env = Env::default();
    let vault_id = env.register(VaultContract, ());
    let client = VaultContractClient::new(&env, &vault_id);

    let owner = Address::generate(&env);
    let asset = Address::generate(&env);
    client.initialize(
        &owner,
        &asset,
        &String::from_str(&env, "Astrion USDC Vault"),
        &String::from_str(&env, "asUSDC"),
        &7,
    );

    let config = client.get_config().unwrap();
    assert_eq!(config.owner, owner);
    assert_eq!(config.asset, asset);
    assert_eq!(config.virtual_shares, 100_000_000_000);

    let state = client.get_state().unwrap();
    assert_eq!(state.total_assets, 0);
    assert_eq!(state.total_shares, 0);
}
