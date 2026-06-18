use soroban_sdk::{contracttype, Address, Env};

use crate::types::RateModelConfig;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Initialized,
    Config,
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("rate-model: admin not set")
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

pub fn set_config(env: &Env, config: &RateModelConfig) {
    env.storage().instance().set(&DataKey::Config, config);
}

pub fn get_config(env: &Env) -> RateModelConfig {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .expect("rate-model: config not set")
}
