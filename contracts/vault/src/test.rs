#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

use crate::{VaultContract, VaultContractClient};

struct Setup<'a> {
    env: Env,
    client: VaultContractClient<'a>,
    owner: Address,
    asset: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let vault_id = env.register(VaultContract, ());
    let client = VaultContractClient::new(&env, &vault_id);
    let owner = Address::generate(&env);
    let asset = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    client.initialize(
        &owner,
        &asset,
        &String::from_str(&env, "Astrion USDC Vault"),
        &String::from_str(&env, "asUSDC"),
        &7,
    );
    Setup {
        env,
        client,
        owner,
        asset,
    }
}

fn mint_asset(env: &Env, asset: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, asset).mint(to, &amount);
}

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

#[test]
fn test_approve_and_balance_storage() {
    let s = setup();
    let spender = Address::generate(&s.env);

    s.client.approve(&s.owner, &spender, &123);
    assert_eq!(s.client.allowance(&s.owner, &spender), 123);

    assert_eq!(s.client.balance_of(&s.owner), 0);
}

#[test]
fn test_deposit_and_withdraw_round_trip() {
    let s = setup();
    let user = Address::generate(&s.env);
    mint_asset(&s.env, &s.asset, &user, 1_000);

    let shares = s.client.deposit(&user, &1_000, &user);
    assert!(shares > 0);
    assert_eq!(s.client.balance_of(&user), shares);
    assert_eq!(s.client.total_assets(), 1_000);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&user), 0);

    let burned = s.client.withdraw(&user, &1_000, &user, &user);
    assert_eq!(burned, shares);
    assert_eq!(s.client.balance_of(&user), 0);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&user), 1_000);
}

#[test]
fn test_mint_and_redeem_round_trip() {
    let s = setup();
    let user = Address::generate(&s.env);
    mint_asset(&s.env, &s.asset, &user, 1_000);

    let shares = 100_000_000_000_i128;
    let assets = s.client.mint(&user, &shares, &user);
    assert!(assets > 0);
    assert_eq!(s.client.total_supply(), shares);

    let redeemed = s.client.redeem(&user, &shares, &user, &user);
    assert_eq!(redeemed, assets);
    assert_eq!(s.client.total_supply(), 0);
}

#[test]
fn test_withdraw_uses_allowance_for_operator() {
    let s = setup();
    let owner = Address::generate(&s.env);
    let operator = Address::generate(&s.env);
    let receiver = Address::generate(&s.env);
    mint_asset(&s.env, &s.asset, &owner, 1_000);

    let shares = s.client.deposit(&owner, &1_000, &owner);
    s.client.approve(&owner, &operator, &shares);
    s.client.withdraw(&operator, &500, &receiver, &owner);

    assert!(s.client.allowance(&owner, &operator) < shares);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&receiver), 500);
}
