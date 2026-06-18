#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, token, vec, Address, Env, String};

#[test]
fn test() {
    let env = Env::default();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let words = client.hello(&String::from_str(&env, "Dev"));
    assert_eq!(
        words,
        vec![
            &env,
            String::from_str(&env, "Hello"),
            String::from_str(&env, "Dev"),
        ]
    );
}

#[test]
fn token_transfer_demo() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(admin).address();
    let token = token::StellarAssetClient::new(&env, &token_id);
    token.mint(&alice, &100);

    let token_client = token::Client::new(&env, &token_id);
    token_client.transfer(&alice, &bob, &40);

    assert_eq!(token_client.balance(&alice), 60);
    assert_eq!(token_client.balance(&bob), 40);
}
