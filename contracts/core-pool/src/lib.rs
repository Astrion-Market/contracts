//! # CorePool
//!
//! The shared liquidity pool — the heart of the Astrion protocol.
//!
//! ## Responsibilities
//! - Supply assets and mint scaled supply shares.
//! - Withdraw assets and burn scaled supply shares.
//! - Borrow assets against collateral.
//! - Repay borrowed assets.
//! - Accrue interest via index-based accounting.
//! - Enforce risk parameters (LTV, supply/borrow caps, health factor).
//!
//! ## Architecture relationship
//! ```text
//! User TX
//!   │
//!   ▼
//! CorePool  ──→  OracleAdapter  (get_price)
//!           ──→  InterestRateModel  (get_rates)
//!           ──→  Token contracts  (transfer)
//! ```
//!
//! ## Implementation status
//! This contract is **scaffolded** — all public functions exist with correct
//! signatures, storage keys, and type definitions. Each function body contains
//! a detailed implementation plan in TODO comments.
//!
//! The math library (`astrion-math`) and type layer are production-ready;
//! implement the function bodies incrementally following the TODO order.

#![no_std]
#![allow(deprecated)]

mod errors;
mod storage;
mod types;

#[cfg(test)]
mod test;

use astrion_math::{from_scaled, health_factor, to_scaled, wad_mul, WAD};
use errors::PoolError;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, token, Address, BytesN,
    Env, Map, Vec,
};
use storage::{
    append_market, get_market_config, get_market_list, get_market_state, get_oracle,
    get_rate_model, get_user_account, is_initialized, is_paused, require_admin, set_admin,
    set_initialized, set_market_config, set_market_state, set_oracle, set_paused, set_rate_model,
    set_treasury, set_user_account,
};
use types::{MarketConfig, MarketState, UserAccount};

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
    fn get_price(env: Env, asset: OracleAsset) -> Result<ResolvedPrice, PoolError>;
}

#[contractclient(name = "RateModelClient")]
pub trait RateModel {
    fn get_rates(
        env: Env,
        total_borrowed: i128,
        total_supplied: i128,
    ) -> Result<RateSnapshot, PoolError>;
}

#[contract]
pub struct CorePoolContract;

#[contractimpl]
impl CorePoolContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Deploy and configure the pool.
    ///
    /// Must be called once immediately after deployment.
    pub fn initialize(
        env: Env,
        admin: Address,
        oracle_adapter: Address,
        rate_model: Address,
        treasury: Address,
    ) -> Result<(), PoolError> {
        if is_initialized(&env) {
            return Err(PoolError::AlreadyInitialized);
        }

        set_admin(&env, &admin);
        set_oracle(&env, &oracle_adapter);
        set_rate_model(&env, &rate_model);
        set_treasury(&env, &treasury);
        set_paused(&env, false);
        set_initialized(&env);

        env.events().publish((symbol_short!("init"),), admin);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Market management (admin)
    // -----------------------------------------------------------------------

    /// Register a new asset market in the shared pool.
    ///
    /// Must be called before any user can supply or borrow the asset.
    pub fn add_market(env: Env, config: MarketConfig) -> Result<(), PoolError> {
        Self::guard_admin_live(&env)?;
        if get_market_config(&env, &config.asset).is_some() {
            return Err(PoolError::MarketAlreadyExists);
        }
        Self::validate_market_config(&config)?;
        let state = MarketState {
            supply_index: WAD,
            borrow_index: WAD,
            total_scaled_supply: 0,
            total_scaled_borrow: 0,
            protocol_reserves: 0,
            last_update_timestamp: env.ledger().timestamp(),
        };
        set_market_state(&env, &config.asset, &state);
        set_market_config(&env, &config.asset, &config);
        append_market(&env, &config.asset);
        env.events().publish(
            (symbol_short!("mkt_add"), config.asset.clone()),
            (config.ltv, config.liquidation_threshold),
        );
        Ok(())
    }

    /// Update risk parameters for an existing market.
    ///
    /// Changes take effect on the next user interaction (supply/borrow/etc.).
    pub fn update_market_config(env: Env, config: MarketConfig) -> Result<(), PoolError> {
        Self::guard_admin_live(&env)?;
        get_market_config(&env, &config.asset).ok_or(PoolError::MarketNotFound)?;
        Self::validate_market_config(&config)?;
        set_market_config(&env, &config.asset, &config);
        env.events()
            .publish((symbol_short!("mkt_upd"),), config.asset.clone());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Core user operations
    // -----------------------------------------------------------------------

    /// Supply `amount` of `asset` to the pool and receive scaled supply shares.
    ///
    /// The supplier earns interest proportional to their share of total supply.
    /// Assets must be pre-approved (token.approve → CorePool).
    pub fn supply(
        env: Env,
        supplier: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        supplier.require_auth();
        Self::guard_user_live(&env)?;
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        let config = Self::require_active_market(&env, &asset)?;
        Self::accrue_interest_internal(&env, &asset)?;
        let mut state = get_market_state(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        let total_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        if config.supply_cap > 0 && total_supply + amount > config.supply_cap {
            return Err(PoolError::SupplyCapExceeded);
        }
        let scaled_amount = to_scaled(amount, state.supply_index);
        if scaled_amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        let mut account = Self::account_or_empty(&env, &supplier);
        let current = account.scaled_supply.get(asset.clone()).unwrap_or(0);
        account
            .scaled_supply
            .set(asset.clone(), current + scaled_amount);
        account.collateral_enabled.set(asset.clone(), true);
        state.total_scaled_supply += scaled_amount;
        set_user_account(&env, &supplier, &account);
        set_market_state(&env, &asset, &state);
        token::Client::new(&env, &asset).transfer(
            &supplier,
            env.current_contract_address(),
            &amount,
        );
        env.events().publish(
            (symbol_short!("supply"), supplier, asset),
            (amount, scaled_amount, state.supply_index),
        );
        Ok(())
    }

    /// Withdraw `amount` of `asset` from the pool, burning scaled supply shares.
    ///
    /// Fails if withdrawal would drop the user's health factor below 1.0.
    pub fn withdraw(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        user.require_auth();
        Self::guard_user_live(&env)?;
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        get_market_config(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        Self::accrue_interest_internal(&env, &asset)?;
        let mut state = get_market_state(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        let mut account = get_user_account(&env, &user).ok_or(PoolError::InvalidAmount)?;
        let scaled_to_burn = to_scaled(amount, state.supply_index);
        let user_scaled = account.scaled_supply.get(asset.clone()).unwrap_or(0);
        if scaled_to_burn <= 0 || user_scaled < scaled_to_burn {
            return Err(PoolError::InvalidAmount);
        }
        let total_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        let total_borrow = from_scaled(state.total_scaled_borrow, state.borrow_index);
        if amount > total_supply - total_borrow {
            return Err(PoolError::InsufficientLiquidity);
        }
        account
            .scaled_supply
            .set(asset.clone(), user_scaled - scaled_to_burn);
        state.total_scaled_supply -= scaled_to_burn;
        if account
            .collateral_enabled
            .get(asset.clone())
            .unwrap_or(false)
        {
            let projected_hf = Self::health_factor_for_account(&env, &account, None)?;
            if projected_hf < WAD {
                return Err(PoolError::HealthFactorTooLow);
            }
        }
        set_user_account(&env, &user, &account);
        set_market_state(&env, &asset, &state);
        token::Client::new(&env, &asset).transfer(&env.current_contract_address(), &user, &amount);
        env.events().publish(
            (symbol_short!("withdr"), user, asset),
            (amount, scaled_to_burn, state.supply_index),
        );
        Ok(())
    }

    /// Borrow `amount` of `asset` against the caller's existing collateral.
    ///
    /// Fails if the resulting health factor would be < 1.0.
    pub fn borrow(
        env: Env,
        borrower: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        borrower.require_auth();
        Self::guard_user_live(&env)?;
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        let config = Self::require_active_market(&env, &asset)?;
        if !config.is_borrowable {
            return Err(PoolError::BorrowingDisabled);
        }
        Self::accrue_interest_internal(&env, &asset)?;
        let mut state = get_market_state(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        let total_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        let total_borrow = from_scaled(state.total_scaled_borrow, state.borrow_index);
        if config.borrow_cap > 0 && total_borrow + amount > config.borrow_cap {
            return Err(PoolError::BorrowCapExceeded);
        }
        if amount > total_supply - total_borrow {
            return Err(PoolError::InsufficientLiquidity);
        }
        let scaled_debt = to_scaled(amount, state.borrow_index);
        if scaled_debt <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        let mut account = Self::account_or_empty(&env, &borrower);
        let current = account.scaled_borrow.get(asset.clone()).unwrap_or(0);
        account
            .scaled_borrow
            .set(asset.clone(), current + scaled_debt);
        let projected_hf = Self::health_factor_for_account(&env, &account, None)?;
        if projected_hf < WAD {
            return Err(PoolError::HealthFactorTooLow);
        }
        state.total_scaled_borrow += scaled_debt;
        set_user_account(&env, &borrower, &account);
        set_market_state(&env, &asset, &state);
        token::Client::new(&env, &asset).transfer(
            &env.current_contract_address(),
            &borrower,
            &amount,
        );
        env.events().publish(
            (symbol_short!("borrow"), borrower, asset),
            (amount, scaled_debt, state.borrow_index),
        );
        Ok(())
    }

    /// Repay `amount` of borrowed `asset` on behalf of `on_behalf_of`.
    ///
    /// Allows third-party repayment (e.g. liquidators, friends, keepers).
    pub fn repay(
        env: Env,
        payer: Address,
        on_behalf_of: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        payer.require_auth();
        Self::guard_user_live(&env)?;
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        get_market_config(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        Self::accrue_interest_internal(&env, &asset)?;
        let mut state = get_market_state(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        let mut account = get_user_account(&env, &on_behalf_of).ok_or(PoolError::InvalidAmount)?;
        let scaled_debt = account.scaled_borrow.get(asset.clone()).unwrap_or(0);
        let real_debt = from_scaled(scaled_debt, state.borrow_index);
        if real_debt == 0 {
            return Ok(());
        }
        let actual_repay = if amount > real_debt {
            real_debt
        } else {
            amount
        };
        let mut scaled_repay = to_scaled(actual_repay, state.borrow_index);
        if scaled_repay > scaled_debt {
            scaled_repay = scaled_debt;
        }
        account
            .scaled_borrow
            .set(asset.clone(), scaled_debt - scaled_repay);
        state.total_scaled_borrow -= scaled_repay;
        set_user_account(&env, &on_behalf_of, &account);
        set_market_state(&env, &asset, &state);
        token::Client::new(&env, &asset).transfer(
            &payer,
            env.current_contract_address(),
            &actual_repay,
        );
        env.events().publish(
            (symbol_short!("repay"), payer, on_behalf_of, asset),
            actual_repay,
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Collateral management
    // -----------------------------------------------------------------------

    /// Mark `asset` as collateral for the caller.
    ///
    /// Only collateral-enabled assets are counted toward borrowing power.
    pub fn enable_collateral(env: Env, user: Address, asset: Address) -> Result<(), PoolError> {
        user.require_auth();
        Self::guard_user_live(&env)?;
        Self::require_active_market(&env, &asset)?;
        let mut account = Self::account_or_empty(&env, &user);
        account.collateral_enabled.set(asset.clone(), true);
        set_user_account(&env, &user, &account);
        env.events().publish((symbol_short!("col_on"), user), asset);
        Ok(())
    }

    /// Disable `asset` as collateral for the caller.
    ///
    /// Fails if disabling would drop health factor below 1.0.
    pub fn disable_collateral(env: Env, user: Address, asset: Address) -> Result<(), PoolError> {
        user.require_auth();
        Self::guard_user_live(&env)?;
        let mut account = Self::account_or_empty(&env, &user);
        account.collateral_enabled.set(asset.clone(), false);
        let projected_hf = Self::health_factor_for_account(&env, &account, None)?;
        if projected_hf < WAD {
            return Err(PoolError::HealthFactorTooLow);
        }
        set_user_account(&env, &user, &account);
        env.events()
            .publish((symbol_short!("col_off"), user), asset);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Interest accrual (can be called externally as a keeper action)
    // -----------------------------------------------------------------------

    /// Accrue interest on a single market.
    ///
    /// Safe to call at any time — idempotent within the same ledger.
    /// Called internally before every supply/borrow/repay/withdraw.
    pub fn accrue_interest(env: Env, asset: Address) -> Result<(), PoolError> {
        Self::accrue_interest_internal(&env, &asset)
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Return the live health factor for `user` (WAD-scaled).
    ///
    /// Returns i128::MAX when the user has no debt.
    pub fn get_health_factor(env: Env, user: Address) -> Result<i128, PoolError> {
        Self::get_health_factor_internal(&env, &user)
    }

    /// Return the real (non-scaled) supply balance of `user` for `asset`.
    pub fn get_supply_balance(env: Env, user: Address, asset: Address) -> i128 {
        let Some(account) = get_user_account(&env, &user) else {
            return 0;
        };
        let Some(state) = get_market_state(&env, &asset) else {
            return 0;
        };
        from_scaled(
            account.scaled_supply.get(asset.clone()).unwrap_or(0),
            state.supply_index,
        )
    }

    /// Return the real (non-scaled) borrow balance of `user` for `asset`.
    pub fn get_borrow_balance(env: Env, user: Address, asset: Address) -> i128 {
        let Some(account) = get_user_account(&env, &user) else {
            return 0;
        };
        let Some(state) = get_market_state(&env, &asset) else {
            return 0;
        };
        from_scaled(
            account.scaled_borrow.get(asset.clone()).unwrap_or(0),
            state.borrow_index,
        )
    }

    /// Return the current MarketState for `asset`.
    pub fn get_market_state(env: Env, asset: Address) -> Option<MarketState> {
        get_market_state(&env, &asset)
    }

    /// Return the MarketConfig for `asset`.
    pub fn get_market_config(env: Env, asset: Address) -> Option<MarketConfig> {
        get_market_config(&env, &asset)
    }

    pub fn get_markets(env: Env) -> Vec<Address> {
        get_market_list(&env)
    }

    /// Assert basic per-market accounting invariants.
    ///
    /// This is intentionally cheap and view-only so tests, simulations, and
    /// off-chain monitors can call it after arbitrary operation sequences.
    pub fn assert_market_invariants(env: Env, asset: Address) -> Result<(), PoolError> {
        let config = get_market_config(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        Self::validate_market_config(&config)?;
        let state = get_market_state(&env, &asset).ok_or(PoolError::MarketNotFound)?;
        if state.supply_index < WAD
            || state.borrow_index < WAD
            || state.total_scaled_supply < 0
            || state.total_scaled_borrow < 0
            || state.protocol_reserves < 0
        {
            return Err(PoolError::InvariantViolation);
        }
        let total_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        let total_borrow = from_scaled(state.total_scaled_borrow, state.borrow_index);
        if total_borrow > total_supply {
            return Err(PoolError::InvariantViolation);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    /// Emergency pause — blocks all supply/borrow/repay/withdraw.
    pub fn pause(env: Env) -> Result<(), PoolError> {
        if !is_initialized(&env) {
            return Err(PoolError::NotInitialized);
        }
        require_admin(&env);
        set_paused(&env, true);
        env.events().publish((symbol_short!("pause"),), ());
        Ok(())
    }

    /// Lift pause.
    pub fn unpause(env: Env) -> Result<(), PoolError> {
        if !is_initialized(&env) {
            return Err(PoolError::NotInitialized);
        }
        require_admin(&env);
        set_paused(&env, false);
        env.events().publish((symbol_short!("unpause"),), ());
        Ok(())
    }

    /// Transfer admin rights.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), PoolError> {
        if !is_initialized(&env) {
            return Err(PoolError::NotInitialized);
        }
        require_admin(&env);
        set_admin(&env, &new_admin);
        Ok(())
    }

    /// Upgrade contract WASM.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), PoolError> {
        if !is_initialized(&env) {
            return Err(PoolError::NotInitialized);
        }
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers (private)
    // -----------------------------------------------------------------------

    /// Core interest accrual logic — must be called at the start of every
    /// state-mutating operation to keep indexes up to date.
    ///
    /// # Algorithm
    ///
    /// ```text
    /// delta_t = now - last_update_timestamp  (seconds)
    ///
    /// if delta_t == 0: return early (same ledger, no-op)
    ///
    /// utilization = total_scaled_borrow * borrow_index / (total_scaled_supply * supply_index)
    ///
    /// rates = RateModel::get_rates(total_borrow, total_supply)
    ///
    /// // Annualised rates → per-second (simple interest approximation for small dt)
    /// borrow_rate_per_sec  = rates.borrow_rate / SECONDS_PER_YEAR
    /// supply_rate_per_sec  = rates.supply_rate  / SECONDS_PER_YEAR
    ///
    /// // Compound interest approximation: (1 + r*t) — sufficient for short dt
    /// new_borrow_index = borrow_index * (WAD + borrow_rate_per_sec * delta_t) / WAD
    /// new_supply_index = supply_index * (WAD + supply_rate_per_sec * delta_t) / WAD
    ///
    /// // Protocol reserves accrue from the difference
    /// interest_accrued = total_scaled_borrow * (new_borrow_index - borrow_index) / WAD
    /// reserve_delta    = interest_accrued * reserve_factor / WAD
    /// state.protocol_reserves += reserve_delta
    ///
    /// state.borrow_index = new_borrow_index
    /// state.supply_index = new_supply_index
    /// state.last_update_timestamp = now
    ///
    /// persist state
    /// emit ("accrue", asset, new_supply_index, new_borrow_index, delta_t)
    /// ```
    ///
    /// # Constants
    /// SECONDS_PER_YEAR = 31_536_000
    ///
    /// # Precision note
    /// Simple interest (1 + r*t) diverges from compound interest for large dt.
    /// For daily accrual this error is negligible. For production, consider
    /// a higher-order Taylor expansion if accrual could be skipped for days.
    #[allow(dead_code)]
    fn accrue_interest_internal(env: &Env, asset: &Address) -> Result<(), PoolError> {
        let config = get_market_config(env, asset).ok_or(PoolError::MarketNotFound)?;
        let mut state = get_market_state(env, asset).ok_or(PoolError::MarketNotFound)?;
        let now = env.ledger().timestamp();
        if now <= state.last_update_timestamp {
            return Ok(());
        }

        let delta_t = (now - state.last_update_timestamp) as i128;
        let total_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        let total_borrow = from_scaled(state.total_scaled_borrow, state.borrow_index);
        if total_supply == 0 || total_borrow == 0 {
            state.last_update_timestamp = now;
            set_market_state(env, asset, &state);
            return Ok(());
        }

        let rates =
            RateModelClient::new(env, &get_rate_model(env)).get_rates(&total_borrow, &total_supply);
        let borrow_growth = WAD + (rates.borrow_rate / SECONDS_PER_YEAR) * delta_t;
        let supply_growth = WAD + (rates.supply_rate / SECONDS_PER_YEAR) * delta_t;
        let new_borrow_index = wad_mul(state.borrow_index, borrow_growth);
        let new_supply_index = wad_mul(state.supply_index, supply_growth);
        let interest_accrued = wad_mul(
            state.total_scaled_borrow,
            new_borrow_index - state.borrow_index,
        );
        state.protocol_reserves += wad_mul(interest_accrued, config.reserve_factor);
        state.borrow_index = new_borrow_index;
        state.supply_index = new_supply_index;
        state.last_update_timestamp = now;
        set_market_state(env, asset, &state);
        env.events().publish(
            (symbol_short!("accrue"), asset.clone()),
            (state.supply_index, state.borrow_index, delta_t),
        );
        Ok(())
    }

    /// Compute the health factor for `user` given current market state.
    ///
    /// Returns i128::MAX when the user has zero debt.
    ///
    /// # Note on oracle calls
    /// This function makes N cross-contract oracle calls (one per distinct asset
    /// in the user's portfolio). In v1 this is acceptable. In v2, consider
    /// batching via `OracleAdapter::get_prices_batch` if oracle supports it.
    #[allow(dead_code)]
    fn get_health_factor_internal(env: &Env, user: &Address) -> Result<i128, PoolError> {
        let Some(account) = get_user_account(env, user) else {
            return Ok(i128::MAX);
        };
        Self::health_factor_for_account(env, &account, None)
    }

    fn guard_admin_live(env: &Env) -> Result<(), PoolError> {
        if !is_initialized(env) {
            return Err(PoolError::NotInitialized);
        }
        if is_paused(env) {
            return Err(PoolError::Paused);
        }
        require_admin(env);
        Ok(())
    }

    fn guard_user_live(env: &Env) -> Result<(), PoolError> {
        if !is_initialized(env) {
            return Err(PoolError::NotInitialized);
        }
        if is_paused(env) {
            return Err(PoolError::Paused);
        }
        Ok(())
    }

    fn validate_market_config(config: &MarketConfig) -> Result<(), PoolError> {
        if config.ltv <= 0
            || config.ltv >= WAD
            || config.liquidation_threshold <= config.ltv
            || config.liquidation_threshold >= WAD
            || config.liquidation_bonus < 0
            || config.liquidation_bonus > MAX_LIQUIDATION_BONUS
            || config.reserve_factor < 0
            || config.reserve_factor >= WAD
            || config.supply_cap < 0
            || config.borrow_cap < 0
        {
            return Err(PoolError::InvalidAmount);
        }
        Ok(())
    }

    fn require_active_market(env: &Env, asset: &Address) -> Result<MarketConfig, PoolError> {
        let config = get_market_config(env, asset).ok_or(PoolError::MarketNotFound)?;
        if !config.is_active {
            return Err(PoolError::MarketInactive);
        }
        Ok(config)
    }

    fn account_or_empty(env: &Env, user: &Address) -> UserAccount {
        get_user_account(env, user).unwrap_or_else(|| UserAccount {
            scaled_supply: Map::new(env),
            scaled_borrow: Map::new(env),
            collateral_enabled: Map::new(env),
        })
    }

    fn price(env: &Env, asset: &Address) -> Result<i128, PoolError> {
        let resolved = OracleAdapterClient::new(env, &get_oracle(env))
            .get_price(&OracleAsset::Stellar(asset.clone()));
        Ok(resolved.price_wad)
    }

    fn health_factor_for_account(
        env: &Env,
        account: &UserAccount,
        ignored_collateral: Option<Address>,
    ) -> Result<i128, PoolError> {
        let mut weighted_collateral = 0;
        let mut debt_value = 0;
        for asset in get_market_list(env).iter() {
            let state = get_market_state(env, &asset).ok_or(PoolError::MarketNotFound)?;
            let price = Self::price(env, &asset)?;
            if account
                .collateral_enabled
                .get(asset.clone())
                .unwrap_or(false)
                && ignored_collateral.as_ref() != Some(&asset)
            {
                let scaled = account.scaled_supply.get(asset.clone()).unwrap_or(0);
                let amount = from_scaled(scaled, state.supply_index);
                let value = wad_mul(amount, price);
                let config = get_market_config(env, &asset).ok_or(PoolError::MarketNotFound)?;
                weighted_collateral += wad_mul(value, config.liquidation_threshold);
            }
            let scaled_debt = account.scaled_borrow.get(asset.clone()).unwrap_or(0);
            if scaled_debt > 0 {
                let amount = from_scaled(scaled_debt, state.borrow_index);
                debt_value += wad_mul(amount, price);
            }
        }
        Ok(health_factor(weighted_collateral, WAD, debt_value))
    }
}
