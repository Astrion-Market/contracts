use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::types::{MarketConfig, MarketState, UserAccount};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Protocol admin.
    Admin,
    /// Initialisation guard.
    Initialized,
    /// Global pause flag.
    Paused,
    /// Oracle adapter contract address.
    OracleAdapter,
    /// Interest rate model contract address.
    RateModel,
    /// Protocol treasury address (receives reserve_factor cut).
    Treasury,
    /// Per-asset market configuration: Address → MarketConfig.
    MarketConfig(Address),
    /// Per-asset market live state: Address → MarketState.
    MarketState(Address),
    /// Per-user account: Address → UserAccount.
    UserAccount(Address),
    /// List of all registered market asset addresses (for iteration).
    MarketList,
}

const PERSISTENT_TTL: u32 = 365 * 24 * 60 * 60 / 5;

// ---------------------------------------------------------------------------
// Admin / lifecycle
// ---------------------------------------------------------------------------

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("core-pool: admin not set")
}

pub fn require_admin(env: &Env) {
    get_admin(env).require_auth();
}

pub fn set_initialized(env: &Env) {
    env.storage().instance().set(&DataKey::Initialized, &true);
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Initialized)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// External contracts
// ---------------------------------------------------------------------------

pub fn set_oracle(env: &Env, oracle: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::OracleAdapter, oracle);
}

pub fn get_oracle(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::OracleAdapter)
        .expect("core-pool: oracle not set")
}

pub fn set_rate_model(env: &Env, rate_model: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::RateModel, rate_model);
}

pub fn get_rate_model(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::RateModel)
        .expect("core-pool: rate model not set")
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}

#[allow(dead_code)]
pub fn get_treasury(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Treasury)
        .expect("core-pool: treasury not set")
}

// ---------------------------------------------------------------------------
// Markets
// ---------------------------------------------------------------------------

pub fn set_market_config(env: &Env, asset: &Address, config: &MarketConfig) {
    let key = DataKey::MarketConfig(asset.clone());
    env.storage().persistent().set(&key, config);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn get_market_config(env: &Env, asset: &Address) -> Option<MarketConfig> {
    let key = DataKey::MarketConfig(asset.clone());
    let val: Option<MarketConfig> = env.storage().persistent().get(&key);
    if val.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
    }
    val
}

pub fn set_market_state(env: &Env, asset: &Address, state: &MarketState) {
    let key = DataKey::MarketState(asset.clone());
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn get_market_state(env: &Env, asset: &Address) -> Option<MarketState> {
    let key = DataKey::MarketState(asset.clone());
    let val: Option<MarketState> = env.storage().persistent().get(&key);
    if val.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
    }
    val
}

pub fn get_market_list(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::MarketList)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn append_market(env: &Env, asset: &Address) {
    let mut markets = get_market_list(env);
    if !markets.iter().any(|item| item == *asset) {
        markets.push_back(asset.clone());
        env.storage()
            .persistent()
            .set(&DataKey::MarketList, &markets);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::MarketList, PERSISTENT_TTL, PERSISTENT_TTL);
    }
}

// ---------------------------------------------------------------------------
// User accounts
// ---------------------------------------------------------------------------

pub fn set_user_account(env: &Env, user: &Address, account: &UserAccount) {
    let key = DataKey::UserAccount(user.clone());
    env.storage().persistent().set(&key, account);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn get_user_account(env: &Env, user: &Address) -> Option<UserAccount> {
    let key = DataKey::UserAccount(user.clone());
    let val: Option<UserAccount> = env.storage().persistent().get(&key);
    if val.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
    }
    val
}
