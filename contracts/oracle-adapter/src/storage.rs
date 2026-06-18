use soroban_sdk::{contracttype, Address, Env};

use crate::types::{Asset, PriceBounds, PriceSource};

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The protocol admin — the only account allowed to mutate configuration.
    Admin,
    /// Initialisation flag — prevents re-initialisation.
    Initialized,
    /// Default oracle used when no per-asset source is configured.
    DefaultOracle,
    /// Default max staleness (seconds) applied when no per-asset source is configured.
    DefaultMaxStaleness,
    /// Per-asset price source override: Asset → PriceSource.
    PriceSource(Asset),
    /// Per-asset WAD-normalized sanity bounds: Asset → PriceBounds.
    PriceBounds(Asset),
}

// ---------------------------------------------------------------------------
// TTL constants (Soroban persistent storage)
// ---------------------------------------------------------------------------

/// Persistent entries live for 365 days before requiring a bump.
const PERSISTENT_TTL_LEDGERS: u32 = 365 * 24 * 60 * 60 / 5; // ~6_307_200 ledgers

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("oracle-adapter: admin not set")
}

pub fn require_admin(env: &Env) {
    get_admin(env).require_auth();
}

// ---------------------------------------------------------------------------
// Initialisation guard
// ---------------------------------------------------------------------------

pub fn set_initialized(env: &Env) {
    env.storage().instance().set(&DataKey::Initialized, &true);
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Initialized)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Default oracle
// ---------------------------------------------------------------------------

pub fn set_default_oracle(env: &Env, oracle: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::DefaultOracle, oracle);
}

pub fn get_default_oracle(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::DefaultOracle)
}

pub fn set_default_max_staleness(env: &Env, secs: u64) {
    env.storage()
        .instance()
        .set(&DataKey::DefaultMaxStaleness, &secs);
}

pub fn get_default_max_staleness(env: &Env) -> Option<u64> {
    env.storage().instance().get(&DataKey::DefaultMaxStaleness)
}

// ---------------------------------------------------------------------------
// Per-asset price sources
// ---------------------------------------------------------------------------

pub fn set_price_source(env: &Env, asset: &Asset, source: &PriceSource) {
    env.storage()
        .persistent()
        .set(&DataKey::PriceSource(asset.clone()), source);
    env.storage().persistent().extend_ttl(
        &DataKey::PriceSource(asset.clone()),
        PERSISTENT_TTL_LEDGERS,
        PERSISTENT_TTL_LEDGERS,
    );
}

pub fn get_price_source(env: &Env, asset: &Asset) -> Option<PriceSource> {
    let key = DataKey::PriceSource(asset.clone());
    let source: Option<PriceSource> = env.storage().persistent().get(&key);
    if source.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_LEDGERS, PERSISTENT_TTL_LEDGERS);
    }
    source
}

pub fn remove_price_source(env: &Env, asset: &Asset) {
    env.storage()
        .persistent()
        .remove(&DataKey::PriceSource(asset.clone()));
}

pub fn set_price_bounds(env: &Env, asset: &Asset, bounds: &PriceBounds) {
    let key = DataKey::PriceBounds(asset.clone());
    env.storage().persistent().set(&key, bounds);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_LEDGERS, PERSISTENT_TTL_LEDGERS);
}

pub fn get_price_bounds(env: &Env, asset: &Asset) -> Option<PriceBounds> {
    let key = DataKey::PriceBounds(asset.clone());
    let bounds: Option<PriceBounds> = env.storage().persistent().get(&key);
    if bounds.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_LEDGERS, PERSISTENT_TTL_LEDGERS);
    }
    bounds
}

pub fn remove_price_bounds(env: &Env, asset: &Asset) {
    env.storage()
        .persistent()
        .remove(&DataKey::PriceBounds(asset.clone()));
}
