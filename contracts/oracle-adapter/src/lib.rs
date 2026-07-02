//! # Oracle Adapter
//!
//! A SEP-40 compliant oracle adapter for the Astrion protocol.
//!
//! ## Design
//!
//! The adapter is a **thin, safe wrapper** around any Reflector-compatible
//! oracle contract. Consumers inside the protocol call `get_price(asset)` and
//! receive a WAD-normalised price with staleness already validated — they never
//! deal with raw oracle decimals or timestamp checking.
//!
//! ## SEP-40 reference
//! <https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0040.md>
//!
//! ## Price resolution flow
//!
//! ```text
//! CorePool / LiquidationEngine
//!        │  get_price(asset)
//!        ▼
//! OracleAdapter (this contract)
//!   1. Look up PriceSource for asset (per-asset override or default oracle)
//!   2. Cross-contract call → SEP-40 oracle: lastprice(asset)
//!   3. Validate: price > 0
//!   4. Validate: age ≤ max_staleness
//!   5. Normalise raw price to WAD using oracle's decimals()
//!   6. Return ResolvedPrice { price_wad, timestamp, source }
//! ```

#![no_std]

mod errors;
mod storage;
mod types;

#[cfg(test)]
mod test;

use astrion_math::normalise_to_wad;
use errors::OracleError;
use soroban_sdk::{contract, contractclient, contractimpl, Address, BytesN, Env};
use storage::{
    get_admin, get_default_max_staleness, get_default_oracle, get_price_bounds, get_price_source,
    is_initialized, remove_price_bounds, remove_price_source, require_admin, set_admin,
    set_default_max_staleness, set_default_oracle, set_initialized, set_price_bounds,
    set_price_source,
};
use types::{
    Asset, EventAdminTransferred, EventInitialized, EventOracleUpdated, EventPriceQueried,
    PriceBounds, PriceData, PriceSource, ResolvedPrice,
};

// ---------------------------------------------------------------------------
// SEP-40 oracle client
//
// Defines the cross-contract interface for calling any Reflector-compatible
// oracle. The `contractclient` macro generates `OracleClient` automatically.
// ---------------------------------------------------------------------------

/// Minimal SEP-40 interface — only the functions the adapter needs.
#[contractclient(name = "OracleClient")]
pub trait OracleTrait {
    /// Returns the most recent price observation for `asset`, or None if the
    /// oracle does not support this asset.
    fn lastprice(env: Env, asset: Asset) -> Option<PriceData>;

    /// The number of decimal places used in raw price values.
    fn decimals(env: Env) -> u32;
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct OracleAdapterContract;

#[contractimpl]
impl OracleAdapterContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialise the oracle adapter.
    ///
    /// Must be called exactly once after deployment.
    ///
    /// # Parameters
    /// - `admin`              — address that can update configuration.
    /// - `default_oracle`     — Reflector-compatible oracle contract to use when
    ///                          no per-asset source override exists.
    /// - `default_max_staleness` — maximum age (seconds) before a price is
    ///                             considered stale. Typical value: 300 (5 min).
    pub fn initialize(
        env: Env,
        admin: Address,
        default_oracle: Address,
        default_max_staleness: u64,
    ) -> Result<(), OracleError> {
        if is_initialized(&env) {
            return Err(OracleError::AlreadyInitialized);
        }
        if default_max_staleness == 0 {
            return Err(OracleError::InvalidStaleness);
        }

        set_admin(&env, &admin);
        set_default_oracle(&env, &default_oracle);
        set_default_max_staleness(&env, default_max_staleness);
        set_initialized(&env);

        EventInitialized {
            admin,
            oracle: default_oracle,
            max_staleness: default_max_staleness,
        }
        .publish(&env);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Price resolution (primary protocol interface)
    // -----------------------------------------------------------------------

    /// Fetch the current WAD-normalised price for `asset`.
    ///
    /// This is the **only function CorePool and LiquidationEngine should call**.
    ///
    /// # Errors
    /// - `NoPriceSource`  — no oracle configured for this asset.
    /// - `NoPrice`        — the oracle supports no price for this asset.
    /// - `StalePrice`     — the latest observation exceeds `max_staleness`.
    /// - `InvalidPrice`   — the oracle returned a zero or negative value.
    pub fn get_price(env: Env, asset: Asset) -> Result<ResolvedPrice, OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }

        // Resolve which oracle + staleness limit to use for this asset.
        let (oracle_address, max_staleness) = Self::resolve_source(&env, &asset)?;

        // Cross-contract call to the SEP-40 oracle.
        let oracle = OracleClient::new(&env, &oracle_address);
        let price_data: PriceData = oracle.lastprice(&asset).ok_or(OracleError::NoPrice)?;

        // Validate price is positive.
        if price_data.price <= 0 {
            return Err(OracleError::InvalidPrice);
        }

        // Validate freshness.
        let now = env.ledger().timestamp();
        let age = now.saturating_sub(price_data.timestamp);
        if age > max_staleness {
            return Err(OracleError::StalePrice);
        }

        // Normalise raw price to WAD.
        let decimals = oracle.decimals();
        let price_wad = normalise_to_wad(price_data.price, decimals);
        Self::validate_price_bounds(&env, &asset, price_wad)?;

        let resolved = ResolvedPrice {
            price_wad,
            timestamp: price_data.timestamp,
            source: oracle_address.clone(),
        };

        EventPriceQueried {
            asset,
            price_wad,
            timestamp: price_data.timestamp,
            source: oracle_address,
        }
        .publish(&env);

        Ok(resolved)
    }

    /// Returns `true` when a live, non-stale price is available for `asset`.
    ///
    /// Useful for pre-flight checks without consuming the full resolution flow.
    pub fn has_price(env: Env, asset: Asset) -> bool {
        Self::get_price(env, asset).is_ok()
    }

    // -----------------------------------------------------------------------
    // Admin — oracle configuration
    // -----------------------------------------------------------------------

    /// Update the default oracle contract address.
    ///
    /// Requires admin auth.
    pub fn set_default_oracle(env: Env, new_oracle: Address) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        require_admin(&env);

        let old = get_default_oracle(&env);
        set_default_oracle(&env, &new_oracle);

        EventOracleUpdated {
            old_oracle: old,
            new_oracle,
        }
        .publish(&env);

        Ok(())
    }

    /// Update the default staleness limit (seconds).
    ///
    /// Requires admin auth.
    pub fn set_default_staleness(env: Env, max_staleness: u64) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        if max_staleness == 0 {
            return Err(OracleError::InvalidStaleness);
        }
        require_admin(&env);

        set_default_max_staleness(&env, max_staleness);

        Ok(())
    }

    /// Configure (or override) the price source for a specific asset.
    ///
    /// Allows pointing a single asset at a different oracle contract or using
    /// a tighter staleness window than the protocol default.
    ///
    /// Requires admin auth.
    pub fn set_asset_oracle(
        env: Env,
        asset: Asset,
        oracle: Address,
        max_staleness: u64,
    ) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        if max_staleness == 0 {
            return Err(OracleError::InvalidStaleness);
        }
        require_admin(&env);

        set_price_source(
            &env,
            &asset,
            &PriceSource {
                oracle,
                max_staleness,
            },
        );

        Ok(())
    }

    /// Remove a per-asset oracle override, reverting to the default oracle.
    ///
    /// Requires admin auth.
    pub fn remove_asset_oracle(env: Env, asset: Asset) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        require_admin(&env);

        remove_price_source(&env, &asset);

        Ok(())
    }

    /// Configure WAD-normalized sanity bounds for a specific asset.
    ///
    /// `0` disables one side of the check. If both are non-zero, `min` must be
    /// less than or equal to `max`.
    pub fn set_asset_bounds(
        env: Env,
        asset: Asset,
        min_price_wad: i128,
        max_price_wad: i128,
    ) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        Self::validate_bounds(min_price_wad, max_price_wad)?;
        require_admin(&env);

        set_price_bounds(
            &env,
            &asset,
            &PriceBounds {
                min_price_wad,
                max_price_wad,
            },
        );

        Ok(())
    }

    /// Remove configured sanity bounds for an asset.
    pub fn remove_asset_bounds(env: Env, asset: Asset) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        require_admin(&env);
        remove_price_bounds(&env, &asset);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin — admin transfer
    // -----------------------------------------------------------------------

    /// Transfer admin rights to `new_admin`.
    ///
    /// Requires current admin auth. Two-step transfer is not implemented in v1;
    /// ensure `new_admin` is correct before calling.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        require_admin(&env);

        let old_admin = get_admin(&env);
        set_admin(&env, &new_admin);

        EventAdminTransferred {
            old_admin,
            new_admin,
        }
        .publish(&env);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin — contract upgrade
    // -----------------------------------------------------------------------

    /// Upgrade the contract WASM.
    ///
    /// Requires admin auth. The new WASM must already be uploaded to the
    /// network via `stellar contract upload`.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), OracleError> {
        if !is_initialized(&env) {
            return Err(OracleError::NotInitialized);
        }
        require_admin(&env);

        env.deployer().update_current_contract_wasm(new_wasm_hash);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read-only view functions
    // -----------------------------------------------------------------------

    /// Return the current admin address.
    pub fn admin(env: Env) -> Address {
        get_admin(&env)
    }

    /// Return the default oracle address.
    pub fn default_oracle(env: Env) -> Option<Address> {
        get_default_oracle(&env)
    }

    /// Return the default staleness limit (seconds).
    pub fn default_max_staleness(env: Env) -> Option<u64> {
        get_default_max_staleness(&env)
    }

    /// Return the per-asset oracle configuration, if any.
    pub fn asset_oracle(env: Env, asset: Asset) -> Option<PriceSource> {
        get_price_source(&env, &asset)
    }

    /// Return the per-asset sanity bounds, if any.
    pub fn asset_bounds(env: Env, asset: Asset) -> Option<PriceBounds> {
        get_price_bounds(&env, &asset)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Determine which oracle contract and staleness limit to use for `asset`.
    ///
    /// Priority: per-asset override > default oracle.
    fn resolve_source(env: &Env, asset: &Asset) -> Result<(Address, u64), OracleError> {
        if let Some(source) = get_price_source(env, asset) {
            return Ok((source.oracle, source.max_staleness));
        }

        let oracle = get_default_oracle(env).ok_or(OracleError::NoPriceSource)?;
        let max_staleness = get_default_max_staleness(env).ok_or(OracleError::NoPriceSource)?;

        Ok((oracle, max_staleness))
    }

    fn validate_bounds(min_price_wad: i128, max_price_wad: i128) -> Result<(), OracleError> {
        if min_price_wad < 0 || max_price_wad < 0 {
            return Err(OracleError::InvalidBounds);
        }
        if min_price_wad > 0 && max_price_wad > 0 && min_price_wad > max_price_wad {
            return Err(OracleError::InvalidBounds);
        }
        Ok(())
    }

    fn validate_price_bounds(env: &Env, asset: &Asset, price_wad: i128) -> Result<(), OracleError> {
        if let Some(bounds) = get_price_bounds(env, asset) {
            if bounds.min_price_wad > 0 && price_wad < bounds.min_price_wad {
                return Err(OracleError::PriceOutOfBounds);
            }
            if bounds.max_price_wad > 0 && price_wad > bounds.max_price_wad {
                return Err(OracleError::PriceOutOfBounds);
            }
        }
        Ok(())
    }
}
