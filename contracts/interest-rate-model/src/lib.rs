//! # Interest Rate Model
//!
//! Implements the kinked (two-slope) utilization-based interest rate curve
//! used by Astrion's lending markets.
//!
//! ## Rate formula
//!
//! ```text
//! U = total_borrowed / total_supplied        (utilization, WAD)
//!
//! if U ≤ U_optimal:
//!     borrow_rate = base_rate + slope1 * (U / U_optimal)
//!
//! if U > U_optimal:
//!     excess  = (U - U_optimal) / (WAD - U_optimal)
//!     borrow_rate = base_rate + slope1 + slope2 * excess
//!
//! supply_rate = borrow_rate * U * (1 - reserve_factor)
//! ```
//!
//! All rates are annualised and WAD-scaled (1e18 = 100% APR).
//! CorePool converts them to per-second rates when accruing interest.

#![no_std]

mod errors;
mod storage;
mod types;

#[cfg(test)]
mod test;

use astrion_math::{utilization as calc_utilization, wad_div, wad_mul, WAD};
use errors::RateModelError;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};
use storage::{get_config, is_initialized, require_admin, set_admin, set_config, set_initialized};
use types::{EventConfigUpdated, EventInitialized, RateModelConfig, RateSnapshot};

#[contract]
pub struct RateModelContract;

#[contractimpl]
impl RateModelContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialise the rate model with an admin and initial curve parameters.
    ///
    /// # Default parameters (conservative v1 markets)
    /// - base_rate:           1%   (0.01e18)
    /// - slope1:              4%   (0.04e18)
    /// - slope2:              75%  (0.75e18)
    /// - optimal_utilization: 80%  (0.80e18)
    /// - reserve_factor:      10%  (0.10e18)
    pub fn initialize(
        env: Env,
        admin: Address,
        config: RateModelConfig,
    ) -> Result<(), RateModelError> {
        if is_initialized(&env) {
            return Err(RateModelError::AlreadyInitialized);
        }
        Self::validate_config(&config)?;

        set_admin(&env, &admin);
        set_config(&env, &config);
        set_initialized(&env);

        EventInitialized { admin }.publish(&env);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Rate computation (pure — no state mutation, can be called freely)
    // -----------------------------------------------------------------------

    /// Compute the current borrow APR for a given utilization level.
    ///
    /// `utilization_wad` — WAD-scaled fraction in [0, WAD].
    pub fn get_borrow_rate(env: Env, utilization_wad: i128) -> Result<i128, RateModelError> {
        if !is_initialized(&env) {
            return Err(RateModelError::NotInitialized);
        }
        if !(0..=WAD).contains(&utilization_wad) {
            return Err(RateModelError::InvalidUtilization);
        }
        let config = get_config(&env);
        Ok(compute_borrow_rate(&config, utilization_wad))
    }

    /// Compute the current supply APY for lenders.
    ///
    /// `utilization_wad` — WAD-scaled fraction in [0, WAD].
    pub fn get_supply_rate(env: Env, utilization_wad: i128) -> Result<i128, RateModelError> {
        if !is_initialized(&env) {
            return Err(RateModelError::NotInitialized);
        }
        if !(0..=WAD).contains(&utilization_wad) {
            return Err(RateModelError::InvalidUtilization);
        }
        let config = get_config(&env);
        let borrow_rate = compute_borrow_rate(&config, utilization_wad);
        Ok(compute_supply_rate(&config, borrow_rate, utilization_wad))
    }

    /// Compute utilization from raw market totals and return a full rate snapshot.
    ///
    /// Convenience method for CorePool — single call returns everything needed
    /// for interest accrual.
    pub fn get_rates(
        env: Env,
        total_borrowed: i128,
        total_supplied: i128,
    ) -> Result<RateSnapshot, RateModelError> {
        if !is_initialized(&env) {
            return Err(RateModelError::NotInitialized);
        }
        if total_supplied == 0 {
            return Err(RateModelError::ZeroSupply);
        }

        let utilization_wad = calc_utilization(total_borrowed, total_supplied);
        let config = get_config(&env);
        let borrow_rate = compute_borrow_rate(&config, utilization_wad);
        let supply_rate = compute_supply_rate(&config, borrow_rate, utilization_wad);

        Ok(RateSnapshot {
            borrow_rate,
            supply_rate,
            utilization: utilization_wad,
        })
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    /// Update the curve parameters.
    ///
    /// Takes effect on the next `accrue_interest` call in CorePool.
    /// Requires admin auth.
    pub fn update_config(env: Env, config: RateModelConfig) -> Result<(), RateModelError> {
        if !is_initialized(&env) {
            return Err(RateModelError::NotInitialized);
        }
        Self::validate_config(&config)?;
        require_admin(&env);

        set_config(&env, &config);

        EventConfigUpdated {}.publish(&env);

        Ok(())
    }

    /// Transfer admin rights.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), RateModelError> {
        if !is_initialized(&env) {
            return Err(RateModelError::NotInitialized);
        }
        require_admin(&env);
        set_admin(&env, &new_admin);
        Ok(())
    }

    /// Upgrade contract WASM.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), RateModelError> {
        if !is_initialized(&env) {
            return Err(RateModelError::NotInitialized);
        }
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn config(env: Env) -> RateModelConfig {
        get_config(&env)
    }

    pub fn admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn validate_config(config: &RateModelConfig) -> Result<(), RateModelError> {
        if config.optimal_utilization <= 0 || config.optimal_utilization >= WAD {
            return Err(RateModelError::InvalidOptimalUtilization);
        }
        if config.reserve_factor < 0 || config.reserve_factor >= WAD {
            return Err(RateModelError::InvalidReserveFactor);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pure rate math — separated so they can be unit-tested independently of Soroban.
// ---------------------------------------------------------------------------

/// Compute annualised borrow rate for a given utilization (both WAD-scaled).
pub(crate) fn compute_borrow_rate(config: &RateModelConfig, utilization: i128) -> i128 {
    if utilization <= config.optimal_utilization {
        // Linear segment below kink.
        // additional = slope1 * (U / U_optimal)
        let additional = wad_mul(
            config.slope1,
            wad_div(utilization, config.optimal_utilization),
        );
        config.base_rate + additional
    } else {
        // Steep segment above kink.
        // excess_u = (U - U_optimal) / (1 - U_optimal)
        let excess_u = wad_div(
            utilization - config.optimal_utilization,
            WAD - config.optimal_utilization,
        );
        let additional = wad_mul(config.slope2, excess_u);
        config.base_rate + config.slope1 + additional
    }
}

/// Compute annualised supply rate.
///
/// supply_rate = borrow_rate * utilization * (1 - reserve_factor)
pub(crate) fn compute_supply_rate(
    config: &RateModelConfig,
    borrow_rate: i128,
    utilization: i128,
) -> i128 {
    let gross = wad_mul(borrow_rate, utilization);
    wad_mul(gross, WAD - config.reserve_factor)
}
