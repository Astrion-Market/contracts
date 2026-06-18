use soroban_sdk::{contracttype, vec, Address, Bytes, BytesN, Env, Symbol, Vec};

use crate::types::{Caps, VaultConfig, VaultState};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    State,
    Initialized,
    Balance(Address),
    Allowance(Address, Address),
    Locked,
    IsSentinel(Address),
    IsAllocator(Address),
    Adapter(Address),
    Adapters,
    Caps(BytesN<32>),
    LiquidityAdapter,
    LiquidityData,
    Timelock(Symbol),
    Abdicated(Symbol),
    Pending(Symbol, BytesN<32>),
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

pub fn is_sentinel(env: &Env, user: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::IsSentinel(user.clone()))
        .unwrap_or(false)
}

pub fn set_sentinel(env: &Env, user: &Address, enabled: bool) {
    let key = DataKey::IsSentinel(user.clone());
    env.storage().persistent().set(&key, &enabled);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn is_allocator(env: &Env, user: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::IsAllocator(user.clone()))
        .unwrap_or(false)
}

pub fn set_allocator(env: &Env, user: &Address, enabled: bool) {
    let key = DataKey::IsAllocator(user.clone());
    env.storage().persistent().set(&key, &enabled);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn is_adapter(env: &Env, adapter: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Adapter(adapter.clone()))
        .unwrap_or(false)
}

pub fn set_adapter(env: &Env, adapter: &Address, enabled: bool) {
    let key = DataKey::Adapter(adapter.clone());
    if enabled {
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
    } else {
        env.storage().persistent().remove(&key);
    }
}

pub fn adapters(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Adapters)
        .unwrap_or_else(|| vec![env])
}

pub fn set_adapters(env: &Env, adapters: &Vec<Address>) {
    env.storage().persistent().set(&DataKey::Adapters, adapters);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Adapters, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn caps(env: &Env, id: &BytesN<32>) -> Caps {
    env.storage()
        .persistent()
        .get(&DataKey::Caps(id.clone()))
        .unwrap_or(Caps {
            allocation: 0,
            absolute_cap: 0,
            relative_cap: 0,
        })
}

pub fn set_caps(env: &Env, id: &BytesN<32>, caps: &Caps) {
    let key = DataKey::Caps(id.clone());
    env.storage().persistent().set(&key, caps);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn liquidity_adapter(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&DataKey::LiquidityAdapter)
}

pub fn liquidity_data(env: &Env) -> Bytes {
    env.storage()
        .persistent()
        .get(&DataKey::LiquidityData)
        .unwrap_or_else(|| Bytes::new(env))
}

pub fn set_liquidity(env: &Env, adapter: &Option<Address>, data: &Bytes) {
    match adapter {
        Some(address) => env
            .storage()
            .persistent()
            .set(&DataKey::LiquidityAdapter, address),
        None => env
            .storage()
            .persistent()
            .remove(&DataKey::LiquidityAdapter),
    }
    env.storage()
        .persistent()
        .set(&DataKey::LiquidityData, data);
    env.storage().persistent().extend_ttl(
        &DataKey::LiquidityAdapter,
        PERSISTENT_TTL,
        PERSISTENT_TTL,
    );
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::LiquidityData, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn timelock(env: &Env, action: &Symbol) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::Timelock(action.clone()))
        .unwrap_or(0)
}

pub fn set_timelock(env: &Env, action: &Symbol, duration: u64) {
    let key = DataKey::Timelock(action.clone());
    env.storage().persistent().set(&key, &duration);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn is_abdicated(env: &Env, action: &Symbol) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Abdicated(action.clone()))
        .unwrap_or(false)
}

pub fn set_abdicated(env: &Env, action: &Symbol) {
    let key = DataKey::Abdicated(action.clone());
    env.storage().persistent().set(&key, &true);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn executable_at(env: &Env, action: &Symbol, args_hash: &BytesN<32>) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::Pending(action.clone(), args_hash.clone()))
}

pub fn set_pending(env: &Env, action: &Symbol, args_hash: &BytesN<32>, executable_at: u64) {
    let key = DataKey::Pending(action.clone(), args_hash.clone());
    env.storage().persistent().set(&key, &executable_at);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
}

pub fn clear_pending(env: &Env, action: &Symbol, args_hash: &BytesN<32>) {
    env.storage()
        .persistent()
        .remove(&DataKey::Pending(action.clone(), args_hash.clone()));
}
