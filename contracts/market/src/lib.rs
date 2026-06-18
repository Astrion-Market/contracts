//! # Isolated Market
//!
//! A self-contained two-asset lending pool deployed by `MarketFactory`.
//!
//! ## Design (Morpho Blue model)
//!
//! Each isolated market holds:
//! - `collateral_asset` — posted by borrowers, never lent out
//! - `loan_asset`       — supplied by lenders and borrowed by borrowers
//!
//! Lender supply and borrower collateral are tracked separately. Supplying the
//! loan asset earns interest; posting collateral does not. Risk is ring-fenced:
//! a bad oracle or insolvent position in this market cannot affect the CorePool
//! or other isolated markets.
//!
//! ## Accounting
//!
//! Morpho-style share accounting:
//! - Supply shares are a pro-rata claim on `total_supply_assets`.
//! - Borrow shares are a pro-rata obligation against `total_borrow_assets`.
//! - Interest accrual increases `total_borrow_assets`; lenders' claim grows by
//!   the borrower interest minus the protocol fee.
//!
//! ## Implementation status: STEP 2 (lender supply + borrower collateral)
//!
//! This crate is mid-port. Step 2 implements the lender pool (`supply`,
//! `withdraw`) and borrower collateral (`supply_collateral`,
//! `withdraw_collateral`) on the new share model. Borrowing, repayment, and
//! liquidation are rebuilt on the same model in the next step. Share-math
//! hardening (virtual shares, conservative rounding, checked arithmetic) is
//! Step 3.

#![no_std]
#![allow(deprecated)]

mod errors;
mod types;

#[cfg(test)]
mod test;

use astrion_math::{health_factor, wad_mul, WAD};
use errors::MarketError;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
};
use types::{IsolatedMarketConfig, IsolatedMarketState, MarketPosition};

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Config,
    State,
    Paused,
    Initialized,
    Position(Address),
}

const PERSISTENT_TTL: u32 = 365 * 24 * 60 * 60 / 5;
const SECONDS_PER_YEAR: i128 = 31_536_000;
const MAX_LIQUIDATION_BONUS: i128 = WAD / 2;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OracleAsset {
    Stellar(Address),
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ResolvedPrice {
    pub price_wad: i128,
    pub timestamp: u64,
    pub source: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RateSnapshot {
    pub borrow_rate: i128,
    pub supply_rate: i128,
    pub utilization: i128,
}

#[contractclient(name = "OracleAdapterClient")]
pub trait OracleAdapter {
    fn get_price(env: Env, asset: OracleAsset) -> ResolvedPrice;
}

#[contractclient(name = "RateModelClient")]
pub trait RateModel {
    fn get_rates(env: Env, total_borrowed: i128, total_supplied: i128) -> RateSnapshot;
}

// ---------------------------------------------------------------------------
// Share math (Step 2: plain pro-rata; Step 3 hardens with virtual shares,
// conservative rounding, and checked arithmetic).
// ---------------------------------------------------------------------------

/// Convert `assets` to shares given the pool totals. Empty pool mints 1:1.
fn to_shares(assets: i128, total_assets: i128, total_shares: i128) -> i128 {
    if total_shares == 0 || total_assets == 0 {
        assets
    } else {
        assets * total_shares / total_assets
    }
}

/// Convert `shares` to assets given the pool totals.
fn to_assets(shares: i128, total_assets: i128, total_shares: i128) -> i128 {
    if total_shares == 0 {
        0
    } else {
        shares * total_assets / total_shares
    }
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct IsolatedMarketContract;

#[contractimpl]
impl IsolatedMarketContract {
    /// Initialise the isolated market.
    ///
    /// Called exactly once by `MarketFactory` immediately after deployment.
    pub fn initialize(env: Env, config: IsolatedMarketConfig) -> Result<(), MarketError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(MarketError::AlreadyInitialized);
        }
        Self::validate_config(&config)?;
        let state = IsolatedMarketState {
            total_supply_assets: 0,
            total_supply_shares: 0,
            total_borrow_assets: 0,
            total_borrow_shares: 0,
            total_collateral: 0,
            fee_assets: 0,
            last_update_timestamp: env.ledger().timestamp(),
        };
        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::State, &state);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.events().publish(
            (symbol_short!("init"), config.collateral_asset.clone()),
            config.loan_asset.clone(),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Lender operations (loan asset)
    // -----------------------------------------------------------------------

    /// Supply `assets` of the loan asset, crediting supply shares to `on_behalf`.
    ///
    /// The supplier pays the assets; shares can be credited to another account.
    /// Lenders earn interest as borrowers accrue it.
    pub fn supply(
        env: Env,
        supplier: Address,
        assets: i128,
        on_behalf: Address,
    ) -> Result<i128, MarketError> {
        supplier.require_auth();
        Self::guard_live(&env)?;
        if assets <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        if config.supply_cap > 0 && state.total_supply_assets + assets > config.supply_cap {
            return Err(MarketError::SupplyCapExceeded);
        }
        let shares = to_shares(assets, state.total_supply_assets, state.total_supply_shares);
        if shares <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        let mut position = Self::position_or_empty(&env, &on_behalf);
        position.supply_shares += shares;
        state.total_supply_shares += shares;
        state.total_supply_assets += assets;
        Self::set_position(&env, &on_behalf, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &supplier,
            env.current_contract_address(),
            &assets,
        );
        env.events()
            .publish((symbol_short!("supply"), on_behalf), (assets, shares));
        Ok(shares)
    }

    /// Withdraw supplied loan assets, burning supply shares from `on_behalf` and
    /// sending the assets to `receiver`.
    ///
    /// Caller specifies exactly one of `assets` or `shares` (the other is 0).
    /// No health check: suppliers are not borrowers. Bounded by available
    /// market liquidity (`total_supply_assets - total_borrow_assets`).
    pub fn withdraw(
        env: Env,
        caller: Address,
        assets: i128,
        shares: i128,
        on_behalf: Address,
        receiver: Address,
    ) -> Result<(i128, i128), MarketError> {
        caller.require_auth();
        Self::guard_live(&env)?;
        // Operator delegation (caller != on_behalf) arrives in Step 6.
        if caller != on_behalf {
            return Err(MarketError::Unauthorized);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;

        let (assets, shares) = Self::resolve_assets_shares(
            assets,
            shares,
            state.total_supply_assets,
            state.total_supply_shares,
        )?;

        let mut position = Self::position_or_empty(&env, &on_behalf);
        if position.supply_shares < shares {
            return Err(MarketError::InsufficientSupply);
        }
        let available = state.total_supply_assets - state.total_borrow_assets;
        if assets > available {
            return Err(MarketError::InsufficientLiquidity);
        }
        position.supply_shares -= shares;
        state.total_supply_shares -= shares;
        state.total_supply_assets -= assets;
        Self::set_position(&env, &on_behalf, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &env.current_contract_address(),
            &receiver,
            &assets,
        );
        env.events()
            .publish((symbol_short!("withdr"), on_behalf), (assets, shares));
        Ok((assets, shares))
    }

    // -----------------------------------------------------------------------
    // Borrower collateral operations
    // -----------------------------------------------------------------------

    /// Post `assets` of the collateral asset for `on_behalf`.
    ///
    /// Collateral does not earn interest and is never lent out.
    pub fn supply_collateral(
        env: Env,
        supplier: Address,
        assets: i128,
        on_behalf: Address,
    ) -> Result<(), MarketError> {
        supplier.require_auth();
        Self::guard_live(&env)?;
        if assets <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let mut position = Self::position_or_empty(&env, &on_behalf);
        position.collateral += assets;
        state.total_collateral += assets;
        Self::set_position(&env, &on_behalf, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.collateral_asset).transfer(
            &supplier,
            env.current_contract_address(),
            &assets,
        );
        env.events()
            .publish((symbol_short!("supcol"), on_behalf), assets);
        Ok(())
    }

    /// Withdraw `assets` of collateral from `on_behalf` to `receiver`.
    ///
    /// Fails if the position would become unhealthy (`borrow_value` exceeds
    /// `collateral_value * lltv`).
    pub fn withdraw_collateral(
        env: Env,
        caller: Address,
        assets: i128,
        on_behalf: Address,
        receiver: Address,
    ) -> Result<(), MarketError> {
        caller.require_auth();
        Self::guard_live(&env)?;
        if assets <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        // Operator delegation (caller != on_behalf) arrives in Step 6.
        if caller != on_behalf {
            return Err(MarketError::Unauthorized);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let mut position = Self::position_or_empty(&env, &on_behalf);
        if position.collateral < assets {
            return Err(MarketError::InsufficientCollateral);
        }
        position.collateral -= assets;
        if Self::health_factor_for_position(&env, &config, &state, &position)? < WAD {
            return Err(MarketError::HealthFactorTooLow);
        }
        state.total_collateral -= assets;
        Self::set_position(&env, &on_behalf, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.collateral_asset).transfer(
            &env.current_contract_address(),
            &receiver,
            &assets,
        );
        env.events()
            .publish((symbol_short!("wdcol"), on_behalf), assets);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Interest accrual
    // -----------------------------------------------------------------------

    /// Public entry-point for keeper-driven accrual.
    pub fn accrue_interest(env: Env) -> Result<(), MarketError> {
        Self::accrue_interest_internal(&env)
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Returns the current health factor for `user` (WAD-scaled).
    pub fn get_health_factor(env: Env, user: Address) -> Result<i128, MarketError> {
        let config = Self::config(&env)?;
        let state = Self::state(&env)?;
        let position = Self::position_or_empty(&env, &user);
        Self::health_factor_for_position(&env, &config, &state, &position)
    }

    pub fn get_user_position(env: Env, user: Address) -> Option<MarketPosition> {
        env.storage().persistent().get(&DataKey::Position(user))
    }

    pub fn get_market_state(env: Env) -> Option<IsolatedMarketState> {
        env.storage().instance().get(&DataKey::State)
    }

    pub fn get_market_config(env: Env) -> Option<IsolatedMarketConfig> {
        env.storage().instance().get(&DataKey::Config)
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    pub fn pause(env: Env, admin: Address) -> Result<(), MarketError> {
        admin.require_auth();
        let config = Self::config(&env)?;
        if admin != config.treasury {
            return Err(MarketError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), MarketError> {
        admin.require_auth();
        let config = Self::config(&env)?;
        if admin != config.treasury {
            return Err(MarketError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) -> Result<(), MarketError> {
        admin.require_auth();
        let config = Self::config(&env)?;
        if admin != config.treasury {
            return Err(MarketError::Unauthorized);
        }
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Accrue interest on the borrow side and credit lenders.
    ///
    /// Morpho model: borrower interest increases `total_borrow_assets`; lenders'
    /// claim (`total_supply_assets`) grows by that interest minus the protocol
    /// fee, which accrues to `fee_assets`. Multiply before dividing to avoid
    /// truncating the rate (Step 3 will add compounding and checked arithmetic).
    fn accrue_interest_internal(env: &Env) -> Result<(), MarketError> {
        let config = Self::config(env)?;
        let mut state = Self::state(env)?;
        let now = env.ledger().timestamp();
        if now <= state.last_update_timestamp {
            return Ok(());
        }
        let elapsed = (now - state.last_update_timestamp) as i128;
        if state.total_borrow_assets == 0 {
            state.last_update_timestamp = now;
            Self::set_state(env, &state);
            return Ok(());
        }
        let rates = RateModelClient::new(env, &config.rate_model)
            .get_rates(&state.total_borrow_assets, &state.total_supply_assets);
        let interest =
            state.total_borrow_assets * rates.borrow_rate * elapsed / WAD / SECONDS_PER_YEAR;
        if interest > 0 {
            let fee = wad_mul(interest, config.reserve_factor);
            state.total_borrow_assets += interest;
            state.total_supply_assets += interest - fee;
            state.fee_assets += fee;
        }
        state.last_update_timestamp = now;
        Self::set_state(env, &state);
        Ok(())
    }

    /// Resolve the (assets, shares) pair for a withdraw: exactly one input must
    /// be positive; the other is derived from the supply pool totals.
    fn resolve_assets_shares(
        assets: i128,
        shares: i128,
        total_assets: i128,
        total_shares: i128,
    ) -> Result<(i128, i128), MarketError> {
        if (assets > 0) == (shares > 0) {
            // Both set or neither set.
            return Err(MarketError::InconsistentInput);
        }
        if assets > 0 {
            Ok((assets, to_shares(assets, total_assets, total_shares)))
        } else {
            Ok((to_assets(shares, total_assets, total_shares), shares))
        }
    }

    fn validate_config(config: &IsolatedMarketConfig) -> Result<(), MarketError> {
        if config.lltv <= 0
            || config.lltv >= WAD
            || config.liquidation_bonus < 0
            || config.liquidation_bonus > MAX_LIQUIDATION_BONUS
            || config.reserve_factor < 0
            || config.reserve_factor >= WAD
            || config.supply_cap < 0
            || config.borrow_cap < 0
        {
            return Err(MarketError::InvalidAmount);
        }
        Ok(())
    }

    fn guard_live(env: &Env) -> Result<(), MarketError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            return Err(MarketError::NotInitialized);
        }
        if env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(MarketError::Paused);
        }
        Ok(())
    }

    fn config(env: &Env) -> Result<IsolatedMarketConfig, MarketError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(MarketError::NotInitialized)
    }

    fn state(env: &Env) -> Result<IsolatedMarketState, MarketError> {
        env.storage()
            .instance()
            .get(&DataKey::State)
            .ok_or(MarketError::NotInitialized)
    }

    fn set_state(env: &Env, state: &IsolatedMarketState) {
        env.storage().instance().set(&DataKey::State, state);
    }

    fn position_or_empty(env: &Env, user: &Address) -> MarketPosition {
        env.storage()
            .persistent()
            .get(&DataKey::Position(user.clone()))
            .unwrap_or(MarketPosition {
                supply_shares: 0,
                borrow_shares: 0,
                collateral: 0,
            })
    }

    fn set_position(env: &Env, user: &Address, position: &MarketPosition) {
        let key = DataKey::Position(user.clone());
        env.storage().persistent().set(&key, position);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
    }

    fn price(env: &Env, asset: &Address) -> Result<i128, MarketError> {
        Ok(
            OracleAdapterClient::new(env, &Self::config(env)?.oracle_adapter)
                .get_price(&OracleAsset::Stellar(asset.clone()))
                .price_wad,
        )
    }

    /// Health factor for a position: `collateral_value * lltv / borrow_value`,
    /// WAD-scaled. Returns `i128::MAX` when the account has no debt.
    fn health_factor_for_position(
        env: &Env,
        config: &IsolatedMarketConfig,
        state: &IsolatedMarketState,
        position: &MarketPosition,
    ) -> Result<i128, MarketError> {
        let debt = to_assets(
            position.borrow_shares,
            state.total_borrow_assets,
            state.total_borrow_shares,
        );
        if debt == 0 {
            return Ok(i128::MAX);
        }
        let collateral_value = wad_mul(position.collateral, Self::price(env, &config.collateral_asset)?);
        let debt_value = wad_mul(debt, Self::price(env, &config.loan_asset)?);
        Ok(health_factor(collateral_value, config.lltv, debt_value))
    }
}
