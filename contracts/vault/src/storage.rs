use soroban_sdk::{contracttype, Address, Env};

use crate::types::{VaultConfig, VaultState};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    State,
    Initialized,
    Balance(Address),
    Allowance(Address, Address),
    Locked,
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

pub fn get_config(env: &Env) -> Option<VaultConfig> {
    env.storage().instance().get(&DataKey::Config)
}

pub fn set_config(env: &Env, config: &VaultConfig) {
    env.storage().instance().set(&DataKey::Config, config);
}

pub fn get_state(env: &Env) -> Option<VaultState> {
    env.storage().instance().get(&DataKey::State)
}

pub fn set_state(env: &Env, state: &VaultState) {
    env.storage().instance().set(&DataKey::State, state);
}

pub fn balance(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(user.clone()))
        .unwrap_or(0)
}

pub fn set_balance(env: &Env, user: &Address, amount: i128) {
    let key = DataKey::Balance(user.clone());
    env.storage().persistent().set(&key, &amount);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn allowance(env: &Env, owner: &Address, spender: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Allowance(owner.clone(), spender.clone()))
        .unwrap_or(0)
}

pub fn set_allowance(env: &Env, owner: &Address, spender: &Address, amount: i128) {
    let key = DataKey::Allowance(owner.clone(), spender.clone());
    env.storage().persistent().set(&key, &amount);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn is_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Locked)
        .unwrap_or(false)
}

pub fn set_locked(env: &Env, locked: bool) {
    env.storage().instance().set(&DataKey::Locked, &locked);
}
