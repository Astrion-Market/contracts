#![cfg(test)]

extern crate std;

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    testutils::{Address as _, Ledger},
    token, vec, Address, Bytes, BytesN, Env, String, Symbol,
};

use crate::types::AdapterChange;
use crate::{VaultContract, VaultContractClient};

#[contracttype]
#[derive(Clone)]
enum MockAdapterKey {
    Asset,
}

#[contracttype]
#[derive(Clone)]
enum MockGateKey {
    ReceiveShares(Address),
}

#[contract]
struct MockAdapter;

#[contractimpl]
impl MockAdapter {
    pub fn initialize(env: Env, asset: Address) {
        env.storage().instance().set(&MockAdapterKey::Asset, &asset);
    }

    pub fn allocate(
        env: Env,
        data: Bytes,
        assets: i128,
        _selector: Symbol,
        _sender: Address,
    ) -> AdapterChange {
        AdapterChange {
            ids: vec![&env, env.crypto().sha256(&data).to_bytes()],
            change: assets,
        }
    }

    pub fn deallocate(
        env: Env,
        data: Bytes,
        assets: i128,
        _selector: Symbol,
        sender: Address,
    ) -> AdapterChange {
        let asset: Address = env
            .storage()
            .instance()
            .get(&MockAdapterKey::Asset)
            .unwrap();
        token::Client::new(&env, &asset).transfer(
            &env.current_contract_address(),
            &sender,
            &assets,
        );
        AdapterChange {
            ids: vec![&env, env.crypto().sha256(&data).to_bytes()],
            change: -assets,
        }
    }

    pub fn real_assets(env: Env) -> i128 {
        let asset: Address = env
            .storage()
            .instance()
            .get(&MockAdapterKey::Asset)
            .unwrap();
        token::Client::new(&env, &asset).balance(&env.current_contract_address())
    }
}

#[contract]
struct MockGate;

#[contractimpl]
impl MockGate {
    pub fn set_receive_shares(env: Env, account: Address, allowed: bool) {
        env.storage()
            .persistent()
            .set(&MockGateKey::ReceiveShares(account), &allowed);
    }

    pub fn can_receive_shares(env: Env, account: Address) -> bool {
        env.storage()
            .persistent()
            .get(&MockGateKey::ReceiveShares(account))
            .unwrap_or(false)
    }

    pub fn can_send_shares(_env: Env, _account: Address) -> bool {
        true
    }

    pub fn can_receive_assets(_env: Env, _account: Address) -> bool {
        true
    }

    pub fn can_send_assets(_env: Env, _account: Address) -> bool {
        true
    }
}

struct Setup<'a> {
    env: Env,
    client: VaultContractClient<'a>,
    vault_id: Address,
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
    );
    Setup {
        env,
        client,
        vault_id,
        owner,
        asset,
    }
}

fn mint_asset(env: &Env, asset: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, asset).mint(to, &amount);
}

fn hash(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn enable_allocator(s: &Setup, account: &Address) {
    let action = symbol_short!("alloc");
    let args_hash = s.client.hash_allocator_args(account, &true);
    s.client.submit(&s.owner, &action, &args_hash);
    s.client.set_allocator(account, &true);
}

fn enable_adapter(s: &Setup, adapter: &Address) {
    let action = symbol_short!("adapter");
    let args_hash = s.client.hash_adapter_args(adapter, &true);
    s.client.submit(&s.owner, &action, &args_hash);
    s.client.set_adapter(adapter, &true);
}

fn set_cap(s: &Setup, id: &BytesN<32>, absolute_cap: i128, relative_cap: i128) {
    let action = symbol_short!("caps");
    let args_hash = s.client.hash_cap_args(id, &absolute_cap, &relative_cap);
    s.client.submit(&s.owner, &action, &args_hash);
    s.client.set_caps(id, &absolute_cap, &relative_cap);
}

fn set_liquidity(s: &Setup, adapter: &Address, data: &Bytes) {
    let action = symbol_short!("liquid");
    let args_hash = s.client.hash_liquidity_args(&Some(adapter.clone()), data);
    s.client.submit(&s.owner, &action, &args_hash);
    s.client
        .set_liquidity_adapter_and_data(&Some(adapter.clone()), data);
}

fn set_gate(s: &Setup, gate: Symbol, address: Option<Address>) {
    let action = symbol_short!("gate");
    let args_hash = s.client.hash_gate_args(&gate, &address);
    s.client.submit(&s.owner, &action, &args_hash);
    s.client.set_gate(&gate, &address);
}

#[test]
fn test_initialize_success() {
    let env = Env::default();
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

#[test]
fn test_previews_match_deposit_and_withdraw() {
    let s = setup();
    let user = Address::generate(&s.env);
    mint_asset(&s.env, &s.asset, &user, 1_000);

    let preview_shares = s.client.preview_deposit(&1_000);
    let shares = s.client.deposit(&user, &1_000, &user);
    assert_eq!(shares, preview_shares);

    let preview_burn = s.client.preview_withdraw(&500);
    let burned = s.client.withdraw(&user, &500, &user, &user);
    assert_eq!(burned, preview_burn);
}

#[test]
fn test_accrual_caps_donation_gain_by_max_rate() {
    let s = setup();
    let user = Address::generate(&s.env);
    mint_asset(&s.env, &s.asset, &user, 100);
    s.client.deposit(&user, &100, &user);

    // Donation: real assets become 10_100, but max_rate is 200% APR.
    mint_asset(&s.env, &s.asset, &s.vault_id, 10_000);
    s.env.ledger().with_mut(|li| li.timestamp = 31_536_000);

    let preview = s.client.accrue_interest_view();
    assert_eq!(preview.new_total_assets, 299);
    assert_eq!(s.client.total_assets(), 299);

    s.client.accrue_interest();
    assert_eq!(s.client.get_state().unwrap().total_assets, 299);
}

#[test]
fn test_owner_sets_curator_sentinel_and_timelock() {
    let s = setup();
    let curator = Address::generate(&s.env);
    let sentinel = Address::generate(&s.env);
    let action = symbol_short!("alloc");

    s.client.set_curator(&s.owner, &curator);
    s.client.set_sentinel(&s.owner, &sentinel, &true);
    s.client.set_timelock(&s.owner, &action, &10);

    assert_eq!(s.client.curator().unwrap(), curator);
    assert!(s.client.is_sentinel(&sentinel));
    assert_eq!(s.client.timelock(&action), 10);
}

#[test]
fn test_timelocked_allocator_execution() {
    let s = setup();
    let curator = Address::generate(&s.env);
    let allocator = Address::generate(&s.env);
    let action = symbol_short!("alloc");
    let args_hash = s.client.hash_allocator_args(&allocator, &true);

    s.client.set_curator(&s.owner, &curator);
    s.client.set_timelock(&s.owner, &action, &10);
    assert_eq!(s.client.submit(&curator, &action, &args_hash), 10);

    let early = s.client.try_set_allocator(&allocator, &true);
    assert_eq!(
        early,
        Err(Ok(crate::errors::VaultError::TimelockNotExpired))
    );

    s.env.ledger().with_mut(|li| li.timestamp = 10);
    s.client.set_allocator(&allocator, &true);
    assert!(s.client.is_allocator(&allocator));
    assert!(s.client.executable_at(&action, &args_hash).is_none());
}

#[test]
fn test_timelock_rejects_allocator_arg_substitution() {
    let s = setup();
    let curator = Address::generate(&s.env);
    let intended = Address::generate(&s.env);
    let attacker = Address::generate(&s.env);
    let action = symbol_short!("alloc");
    let args_hash = s.client.hash_allocator_args(&intended, &true);

    s.client.set_curator(&s.owner, &curator);
    s.client.submit(&curator, &action, &args_hash);

    let substituted = s.client.try_set_allocator(&attacker, &true);
    assert_eq!(
        substituted,
        Err(Ok(crate::errors::VaultError::DataNotTimelocked))
    );
    assert!(!s.client.is_allocator(&attacker));
    assert!(s.client.executable_at(&action, &args_hash).is_some());

    s.client.set_allocator(&intended, &true);
    assert!(s.client.is_allocator(&intended));
    assert!(!s.client.is_allocator(&attacker));
    assert!(s.client.executable_at(&action, &args_hash).is_none());
}

#[test]
fn test_sentinel_can_revoke_pending_action() {
    let s = setup();
    let curator = Address::generate(&s.env);
    let sentinel = Address::generate(&s.env);
    let action = symbol_short!("alloc");
    let args_hash = hash(&s.env, 2);

    s.client.set_curator(&s.owner, &curator);
    s.client.set_sentinel(&s.owner, &sentinel, &true);
    s.client.submit(&curator, &action, &args_hash);
    s.client.revoke(&sentinel, &action, &args_hash);

    assert!(s.client.executable_at(&action, &args_hash).is_none());
}

#[test]
fn test_abdication_blocks_future_submit() {
    let s = setup();
    let curator = Address::generate(&s.env);
    let target = symbol_short!("alloc");
    let abdicate = symbol_short!("abdicate");
    let args_hash = s.client.hash_abdicate_args(&target);

    s.client.set_curator(&s.owner, &curator);
    s.client.submit(&curator, &abdicate, &args_hash);
    s.client.abdicate(&target);

    assert!(s.client.is_abdicated(&target));
    let blocked = s.client.try_submit(&curator, &target, &hash(&s.env, 4));
    assert_eq!(blocked, Err(Ok(crate::errors::VaultError::Abdicated)));
}

#[test]
fn test_timelocked_performance_fee_update() {
    let s = setup();
    let curator = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let action = symbol_short!("perf_fee");
    let args_hash = s
        .client
        .hash_performance_fee_args(&(astrion_math::WAD / 10), &recipient);

    s.client.set_curator(&s.owner, &curator);
    s.client.submit(&curator, &action, &args_hash);
    s.client
        .set_performance_fee(&(astrion_math::WAD / 10), &recipient);

    let config = s.client.get_config().unwrap();
    assert_eq!(config.performance_fee, astrion_math::WAD / 10);
    assert_eq!(config.performance_fee_recipient, recipient);
}

#[test]
fn test_convert_zero_returns_zero() {
    let s = setup();

    assert_eq!(s.client.convert_to_shares(&0), 0);
    assert_eq!(s.client.convert_to_assets(&0), 0);
}

#[test]
fn test_allocate_enforces_caps_and_deallocate_updates_allocation() {
    let s = setup();
    let allocator = Address::generate(&s.env);
    let user = Address::generate(&s.env);
    let adapter = s.env.register(MockAdapter, ());
    let adapter_client = MockAdapterClient::new(&s.env, &adapter);
    adapter_client.initialize(&s.asset);
    let data = Bytes::from_array(&s.env, &[1, 2, 3, 4]);
    let id = s.env.crypto().sha256(&data).to_bytes();

    enable_allocator(&s, &allocator);
    enable_adapter(&s, &adapter);
    set_cap(&s, &id, 500, 0);

    mint_asset(&s.env, &s.asset, &user, 1_000);
    s.client.deposit(&user, &1_000, &user);
    s.client
        .allocate(&allocator, &adapter, &data, &400, &symbol_short!("supply"));

    assert_eq!(s.client.allocation(&id), 400);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&adapter), 400);

    let too_much =
        s.client
            .try_allocate(&allocator, &adapter, &data, &200, &symbol_short!("supply"));
    assert_eq!(too_much, Err(Ok(crate::errors::VaultError::CapExceeded)));
    assert_eq!(s.client.allocation(&id), 400);

    s.client
        .deallocate(&allocator, &adapter, &data, &150, &symbol_short!("withdr"));
    assert_eq!(s.client.allocation(&id), 250);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&adapter), 250);
}

#[test]
fn test_liquidity_adapter_auto_allocates_and_deallocates() {
    let s = setup();
    let user = Address::generate(&s.env);
    let adapter = s.env.register(MockAdapter, ());
    let adapter_client = MockAdapterClient::new(&s.env, &adapter);
    adapter_client.initialize(&s.asset);
    let data = Bytes::from_array(&s.env, &[5, 6, 7, 8]);
    let id = s.env.crypto().sha256(&data).to_bytes();

    enable_adapter(&s, &adapter);
    set_cap(&s, &id, 2_000, 0);
    set_liquidity(&s, &adapter, &data);

    mint_asset(&s.env, &s.asset, &user, 1_000);
    s.client.deposit(&user, &1_000, &user);

    assert_eq!(s.client.allocation(&id), 1_000);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&s.vault_id), 0);
    assert_eq!(
        token::Client::new(&s.env, &s.asset).balance(&adapter),
        1_000
    );

    s.client.withdraw(&user, &250, &user, &user);
    assert_eq!(s.client.allocation(&id), 750);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&adapter), 750);
    assert_eq!(token::Client::new(&s.env, &s.asset).balance(&user), 250);
}

#[test]
fn test_receive_shares_gate_fails_closed() {
    let s = setup();
    let user = Address::generate(&s.env);
    let blocked = Address::generate(&s.env);
    let allowed = Address::generate(&s.env);
    let gate = s.env.register(MockGate, ());
    let gate_client = MockGateClient::new(&s.env, &gate);

    set_gate(&s, symbol_short!("recv_sh"), Some(gate));
    gate_client.set_receive_shares(&allowed, &true);
    mint_asset(&s.env, &s.asset, &user, 1_000);

    let rejected = s.client.try_deposit(&user, &100, &blocked);
    assert_eq!(rejected, Err(Ok(crate::errors::VaultError::GateRejected)));
    assert_eq!(s.client.balance_of(&blocked), 0);

    s.client.deposit(&user, &100, &allowed);
    assert!(s.client.balance_of(&allowed) > 0);
}
