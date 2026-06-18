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
//!              └── emits ("market_created", collateral, loan, address)
//! ```
//!
//! The factory is the single source of truth for which isolated markets are
//! legitimate Astrion markets. The frontend and liquidation bots should only
//! interact with markets registered here.
//!
//! ## Implementation status: STEP 5 (permissionless, parameter-bounded)
//!
//! Market creation is permissionless; governance only approves the IRM and LLTV
//! parameter space. Markets are keyed by the hash of their full parameters
//! `(loan_asset, collateral_asset, oracle, irm, lltv)`, so the loan/collateral
//! direction matters and the same pair can have multiple markets.

#![no_std]
#![allow(deprecated)]

#[cfg(test)]
mod test;

use astrion_market_types::IsolatedMarketConfig;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, xdr::ToXdr, Address, BytesN,
    Env, Vec,
};

/// Client trait for calling the deployed market's `initialize`. Declared here
/// (rather than depending on the `market` crate) so the factory does not link —
/// and re-export — the market's contract entrypoints.
#[contractclient(name = "MarketClient")]
pub trait IsolatedMarket {
    fn initialize(env: Env, config: IsolatedMarketConfig);
}

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
    /// Lookup: market id (hash of full params) → market address.
    MarketById(BytesN<32>),
    /// Governance-approved interest rate models.
    EnabledIrm(Address),
    /// Governance-approved liquidation LTVs (WAD).
    EnabledLltv(i128),
}

/// The identity-defining parameters of a market. Two markets with the same
/// asset pair but different oracle, IRM, or LLTV are distinct markets (and the
/// loan/collateral direction matters), exactly as in Morpho Blue.
#[contracttype]
#[derive(Clone)]
pub struct MarketParamsKey {
    pub loan_asset: Address,
    pub collateral_asset: Address,
    pub oracle: Address,
    pub irm: Address,
    pub lltv: i128,
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
    /// `rate_model` is not in the enabled-IRM registry.
    IrmNotEnabled = 7,
    /// `lltv` is not in the enabled-LLTV registry.
    LltvNotEnabled = 8,
    /// loan_asset == collateral_asset, or other invalid market parameters.
    InvalidMarketParams = 9,
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

    /// Permissionlessly deploy a new isolated market, initialise it, and register
    /// it. Anyone may call this; governance only bounds the parameter space.
    ///
    /// Validates that `loan_asset != collateral_asset` and that the config's IRM
    /// and LLTV are in the enabled registries. The market id is the hash of
    /// `(loan_asset, collateral_asset, oracle, irm, lltv)`, so markets with the
    /// same pair but different oracle/IRM/LLTV — and the reverse direction — are
    /// distinct. Returns the market address.
    pub fn create_market(
        env: Env,
        config: IsolatedMarketConfig,
    ) -> Result<Address, FactoryError> {
        require_initialized(&env)?;
        if config.loan_asset == config.collateral_asset {
            return Err(FactoryError::InvalidMarketParams);
        }
        if !Self::is_lltv_enabled(env.clone(), config.lltv) {
            return Err(FactoryError::LltvNotEnabled);
        }
        if !Self::is_irm_enabled(env.clone(), config.rate_model.clone()) {
            return Err(FactoryError::IrmNotEnabled);
        }
        let id = compute_id(&env, &config);
        if env
            .storage()
            .persistent()
            .has(&DataKey::MarketById(id.clone()))
        {
            return Err(FactoryError::MarketAlreadyExists);
        }
        let wasm_hash = env
            .storage()
            .instance()
            .get::<DataKey, BytesN<32>>(&DataKey::MarketWasmHash)
            .ok_or(FactoryError::InvalidWasmHash)?;
        // Deterministic address from the market id.
        let market = env
            .deployer()
            .with_current_contract(id.clone())
            .deploy_v2(wasm_hash, ());
        MarketClient::new(&env, &market).initialize(&config);
        let mut markets = get_markets_internal(&env);
        markets.push_back(market.clone());
        env.storage().persistent().set(&DataKey::Markets, &markets);
        env.storage()
            .persistent()
            .set(&DataKey::MarketById(id.clone()), &market);
        env.events()
            .publish((symbol_short!("market"), id), market.clone());
        Ok(market)
    }

    /// Compute the market id for a parameter set, without creating anything.
    pub fn market_id(
        env: Env,
        loan_asset: Address,
        collateral_asset: Address,
        oracle: Address,
        irm: Address,
        lltv: i128,
    ) -> BytesN<32> {
        let key = MarketParamsKey {
            loan_asset,
            collateral_asset,
            oracle,
            irm,
            lltv,
        };
        env.crypto().sha256(&key.to_xdr(&env)).to_bytes()
    }

    // -----------------------------------------------------------------------
    // Governance: enabled IRM / LLTV registries
    // -----------------------------------------------------------------------
    //
    // Morpho externalizes risk: anyone may create a market, but governance bounds
    // the parameter space by approving which interest rate models and which
    // liquidation LTVs are allowed.

    /// Approve an interest rate model for use in new markets (admin only).
    pub fn enable_irm(env: Env, rate_model: Address) -> Result<(), FactoryError> {
        require_live_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::EnabledIrm(rate_model.clone()), &true);
        env.events().publish((symbol_short!("irm_on"),), rate_model);
        Ok(())
    }

    /// Approve a liquidation LTV (WAD) for use in new markets (admin only).
    pub fn enable_lltv(env: Env, lltv: i128) -> Result<(), FactoryError> {
        require_live_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::EnabledLltv(lltv), &true);
        env.events().publish((symbol_short!("lltv_on"),), lltv);
        Ok(())
    }

    pub fn is_irm_enabled(env: Env, rate_model: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::EnabledIrm(rate_model))
            .unwrap_or(false)
    }

    pub fn is_lltv_enabled(env: Env, lltv: i128) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::EnabledLltv(lltv))
            .unwrap_or(false)
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

    /// Return the market address for a given market id, if it exists.
    pub fn get_market_by_id(env: Env, id: BytesN<32>) -> Option<Address> {
        env.storage().persistent().get(&DataKey::MarketById(id))
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

fn require_initialized(env: &Env) -> Result<(), FactoryError> {
    if !is_initialized(env) {
        return Err(FactoryError::NotInitialized);
    }
    Ok(())
}

/// Market id = sha256 of the identity-defining parameters.
fn compute_id(env: &Env, config: &IsolatedMarketConfig) -> BytesN<32> {
    let key = MarketParamsKey {
        loan_asset: config.loan_asset.clone(),
        collateral_asset: config.collateral_asset.clone(),
        oracle: config.oracle_adapter.clone(),
        irm: config.rate_model.clone(),
        lltv: config.lltv,
    };
    env.crypto().sha256(&key.to_xdr(env)).to_bytes()
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
