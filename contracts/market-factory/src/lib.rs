//! # MarketFactory
//!
//! Deploys and registers new isolated market contracts.
//!
//! ## Design
//!
//! ```text
//! Admin → MarketFactory::create_market(config)
//!              │
//!              ├── deploys IsolatedMarket WASM via env.deployer()
//!              ├── calls IsolatedMarket::initialize(config)
//!              ├── registers market address in factory registry
//!              └── emits ("market_created", collateral, debt, address)
//! ```
//!
//! The factory is the single source of truth for which isolated markets are
//! legitimate Astrion markets. The frontend and liquidation bots should only
//! interact with markets registered here.
//!
//! ## Implementation status: SCAFFOLD

#![no_std]
#![allow(deprecated)]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Vec};

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Initialized,
    /// WASM hash of the IsolatedMarket contract to deploy.
    MarketWasmHash,
    /// Registry of all deployed market addresses.
    Markets,
    /// Lookup: (collateral_asset, debt_asset) → market address.
    MarketByPair(Address, Address),
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FactoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    MarketAlreadyExists = 4,
    MarketNotFound = 5,
    InvalidWasmHash = 6,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct MarketFactoryContract;

#[contractimpl]
impl MarketFactoryContract {
    /// Initialise the factory with an admin and the IsolatedMarket WASM hash.
    ///
    /// The WASM hash must match a contract already uploaded to the network:
    ///   `stellar contract upload --wasm target/wasm32-unknown-unknown/release/market.wasm`
    pub fn initialize(
        env: Env,
        admin: Address,
        market_wasm_hash: BytesN<32>,
    ) -> Result<(), FactoryError> {
        if is_initialized(&env) {
            return Err(FactoryError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::MarketWasmHash, &market_wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::Markets, &Vec::<Address>::new(&env));
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.events().publish((symbol_short!("init"),), admin);
        Ok(())
    }

    /// Deploy a new isolated market and register it in the factory.
    ///
    /// # Parameters
    /// `config` — the IsolatedMarketConfig forwarded verbatim to `IsolatedMarket::initialize`.
    ///
    /// # Returns
    /// The address of the newly deployed market contract.
    pub fn create_market(
        env: Env,
        collateral_asset: Address,
        debt_asset: Address,
        // Remaining config fields passed as-is to IsolatedMarket::initialize.
        // In practice this would be IsolatedMarketConfig from the market crate,
        // but to avoid a circular dependency the factory accepts primitives and
        // assembles the struct, OR the market crate exports its config type.
        // Decide on cross-crate dependency strategy before implementing.
    ) -> Result<Address, FactoryError> {
        require_live_admin(&env)?;
        if env.storage().persistent().has(&DataKey::MarketByPair(
            collateral_asset.clone(),
            debt_asset.clone(),
        )) || env.storage().persistent().has(&DataKey::MarketByPair(
            debt_asset.clone(),
            collateral_asset.clone(),
        )) {
            return Err(FactoryError::MarketAlreadyExists);
        }
        let wasm_hash = env
            .storage()
            .instance()
            .get::<DataKey, BytesN<32>>(&DataKey::MarketWasmHash)
            .ok_or(FactoryError::InvalidWasmHash)?;
        let markets = get_markets_internal(&env);
        let mut salt = [0u8; 32];
        salt[0..4].copy_from_slice(&markets.len().to_be_bytes());
        let market = env
            .deployer()
            .with_current_contract(salt)
            .deploy_v2(wasm_hash, ());
        let mut updated = markets;
        updated.push_back(market.clone());
        env.storage().persistent().set(&DataKey::Markets, &updated);
        env.storage().persistent().set(
            &DataKey::MarketByPair(collateral_asset.clone(), debt_asset.clone()),
            &market,
        );
        env.events().publish(
            (symbol_short!("market"), collateral_asset, debt_asset),
            market.clone(),
        );
        Ok(market)
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    /// Update the IsolatedMarket WASM hash used for future deployments.
    ///
    /// Does NOT upgrade existing deployed markets — call `IsolatedMarket::upgrade`
    /// on each market individually for that.
    pub fn update_market_wasm(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), FactoryError> {
        require_live_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::MarketWasmHash, &new_wasm_hash);
        Ok(())
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), FactoryError> {
        require_live_admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), FactoryError> {
        require_live_admin(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Return all deployed market addresses.
    pub fn get_markets(env: Env) -> Vec<Address> {
        get_markets_internal(&env)
    }

    /// Return the market address for a given asset pair, if it exists.
    pub fn get_market(env: Env, collateral: Address, debt: Address) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::MarketByPair(collateral, debt))
    }

    pub fn admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }
}

fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Initialized)
        .unwrap_or(false)
}

fn require_live_admin(env: &Env) -> Result<(), FactoryError> {
    if !is_initialized(env) {
        return Err(FactoryError::NotInitialized);
    }
    let admin = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Admin)
        .ok_or(FactoryError::NotInitialized)?;
    admin.require_auth();
    Ok(())
}

fn get_markets_internal(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Markets)
        .unwrap_or_else(|| Vec::new(env))
}
