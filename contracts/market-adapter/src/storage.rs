use soroban_sdk::{contracttype, vec, Address, Env, Vec};

use crate::AdapterConfig;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    Initialized,
    Market(Address),
    Markets,
}

pub const PERSISTENT_TTL: u32 = 365 * 24 * 60 * 60 / 5;

pub fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Initialized)
        .unwrap_or(false)
}

pub fn set_initialized(env: &Env) {
    env.storage().instance().set(&DataKey::Initialized, &true);
}

pub fn config(env: &Env) -> Option<AdapterConfig> {
    env.storage().instance().get(&DataKey::Config)
}

pub fn set_config(env: &Env, config: &AdapterConfig) {
    env.storage().instance().set(&DataKey::Config, config);
}

pub fn supply_shares(env: &Env, market: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Market(market.clone()))
        .unwrap_or(0)
}

pub fn set_supply_shares(env: &Env, market: &Address, shares: i128) {
    let key = DataKey::Market(market.clone());
    env.storage().persistent().set(&key, &shares);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn markets(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Markets)
        .unwrap_or_else(|| vec![env])
}

pub fn set_markets(env: &Env, markets: &Vec<Address>) {
    env.storage().persistent().set(&DataKey::Markets, markets);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Markets, PERSISTENT_TTL, PERSISTENT_TTL);
}
