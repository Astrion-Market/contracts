//! # Isolated Market
//!
//! A self-contained two-asset lending pool deployed by `MarketFactory`.
//!
//! ## Design
//!
//! Each isolated market holds:
//! - `collateral_asset` — the asset deposited as collateral
//! - `loan_asset`       — the asset supplied by lenders and borrowed by borrowers
//!
//! Risk is ring-fenced: a bad oracle or insolvent position in this market
//! cannot affect the CorePool or other isolated markets.
//!
//! ## Architecture
//!
//! ```text
//! MarketFactory
//!   └── deploys → IsolatedMarket (this contract, one per market pair)
//!                   ├── OracleAdapter  (shared, passed at init)
//!                   └── RateModel      (shared or per-market, passed at init)
//! ```
//!
//! ## Implementation status: SCAFFOLD
//!
//! All public function signatures are final. Implement bodies in this order:
//!   1. `supply` — simplest, no HF check needed.
//!   2. `accrue_interest` — needed by borrow/repay/withdraw.
//!   3. `borrow` — needs oracle + HF check.
//!   4. `repay`  — straightforward share burn.
//!   5. `withdraw` — needs HF check.
//!   6. `liquidate` — most complex, do last.

#![no_std]
#![allow(deprecated)]

mod errors;
mod types;

#[cfg(test)]
mod test;

use astrion_math::{from_scaled, health_factor, to_scaled, wad_div, wad_mul, WAD};
use errors::MarketError;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
};
use types::{IsolatedMarketConfig, IsolatedMarketState, UserPosition};

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
            supply_index: WAD,
            borrow_index: WAD,
            total_scaled_supply: 0,
            total_scaled_borrow: 0,
            protocol_reserves: 0,
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
    // User operations
    // -----------------------------------------------------------------------

    /// Supply `amount` of the collateral asset.
    ///
    /// Caller earns interest on their deposited collateral and can use it to
    /// borrow the loan asset.
    pub fn supply(env: Env, supplier: Address, amount: i128) -> Result<(), MarketError> {
        supplier.require_auth();
        Self::guard_live(&env)?;
        if amount <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let real_total_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        if config.supply_cap > 0 && real_total_supply + amount > config.supply_cap {
            return Err(MarketError::SupplyCapExceeded);
        }
        let scaled = to_scaled(amount, state.supply_index);
        if scaled <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        let mut position = Self::position_or_empty(&env, &supplier);
        position.scaled_supply += scaled;
        state.total_scaled_supply += scaled;
        Self::set_position(&env, &supplier, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.collateral_asset).transfer(
            &supplier,
            env.current_contract_address(),
            &amount,
        );
        env.events()
            .publish((symbol_short!("supply"), supplier), (amount, scaled));
        Ok(())
    }

    /// Withdraw `amount` of collateral.
    ///
    /// Fails if the resulting health factor would drop below 1.0.
    pub fn withdraw(env: Env, user: Address, amount: i128) -> Result<(), MarketError> {
        user.require_auth();
        Self::guard_live(&env)?;
        if amount <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let mut position = Self::position_or_empty(&env, &user);
        let scaled = to_scaled(amount, state.supply_index);
        if scaled <= 0 || position.scaled_supply < scaled {
            return Err(MarketError::InsufficientCollateral);
        }
        let real_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        let real_borrow = from_scaled(state.total_scaled_borrow, state.borrow_index);
        if amount > real_supply - real_borrow {
            return Err(MarketError::InsufficientLiquidity);
        }
        position.scaled_supply -= scaled;
        if Self::health_factor_for_position(&env, &config, &state, &position)? < WAD {
            return Err(MarketError::HealthFactorTooLow);
        }
        state.total_scaled_supply -= scaled;
        Self::set_position(&env, &user, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.collateral_asset).transfer(
            &env.current_contract_address(),
            &user,
            &amount,
        );
        env.events()
            .publish((symbol_short!("withdr"), user), (amount, scaled));
        Ok(())
    }

    /// Borrow `amount` of the loan asset against supplied collateral.
    pub fn borrow(env: Env, borrower: Address, amount: i128) -> Result<(), MarketError> {
        borrower.require_auth();
        Self::guard_live(&env)?;
        if amount <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let real_borrow = from_scaled(state.total_scaled_borrow, state.borrow_index);
        if config.borrow_cap > 0 && real_borrow + amount > config.borrow_cap {
            return Err(MarketError::BorrowCapExceeded);
        }
        let real_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        if amount > real_supply - real_borrow {
            return Err(MarketError::InsufficientLiquidity);
        }
        let scaled = to_scaled(amount, state.borrow_index);
        let mut position = Self::position_or_empty(&env, &borrower);
        position.scaled_borrow += scaled;
        if Self::health_factor_for_position(&env, &config, &state, &position)? < WAD {
            return Err(MarketError::HealthFactorTooLow);
        }
        state.total_scaled_borrow += scaled;
        Self::set_position(&env, &borrower, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &env.current_contract_address(),
            &borrower,
            &amount,
        );
        env.events()
            .publish((symbol_short!("borrow"), borrower), (amount, scaled));
        Ok(())
    }

    /// Repay `amount` of borrowed debt on behalf of `on_behalf_of`.
    pub fn repay(
        env: Env,
        payer: Address,
        on_behalf_of: Address,
        amount: i128,
    ) -> Result<(), MarketError> {
        payer.require_auth();
        Self::guard_live(&env)?;
        if amount <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let mut position = Self::position_or_empty(&env, &on_behalf_of);
        let debt = from_scaled(position.scaled_borrow, state.borrow_index);
        if debt == 0 {
            return Ok(());
        }
        let actual = if amount > debt { debt } else { amount };
        let mut scaled = to_scaled(actual, state.borrow_index);
        if scaled > position.scaled_borrow {
            scaled = position.scaled_borrow;
        }
        position.scaled_borrow -= scaled;
        state.total_scaled_borrow -= scaled;
        Self::set_position(&env, &on_behalf_of, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &payer,
            env.current_contract_address(),
            &actual,
        );
        env.events()
            .publish((symbol_short!("repay"), payer, on_behalf_of), actual);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Liquidation
    // -----------------------------------------------------------------------

    /// Liquidate an undercollateralised position.
    ///
    /// The liquidator repays up to `close_factor * debt` and receives the
    /// equivalent collateral value plus `liquidation_bonus`.
    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        repay_amount: i128,
    ) -> Result<(), MarketError> {
        liquidator.require_auth();
        Self::guard_live(&env)?;
        if repay_amount <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let mut position = Self::position_or_empty(&env, &borrower);
        if Self::health_factor_for_position(&env, &config, &state, &position)? >= WAD {
            return Err(MarketError::HealthFactorOk);
        }
        let debt = from_scaled(position.scaled_borrow, state.borrow_index);
        let max_repay = wad_mul(debt, WAD / 2);
        let actual = if repay_amount > max_repay {
            max_repay
        } else {
            repay_amount
        };
        let debt_value = wad_mul(actual, Self::price(&env, &config.loan_asset)?);
        let with_bonus = wad_mul(debt_value, WAD + config.liquidation_bonus);
        let collateral = wad_div(with_bonus, Self::price(&env, &config.collateral_asset)?);
        let user_collateral = from_scaled(position.scaled_supply, state.supply_index);
        if collateral > user_collateral {
            return Err(MarketError::InsufficientCollateral);
        }
        let mut debt_scaled = to_scaled(actual, state.borrow_index);
        if debt_scaled > position.scaled_borrow {
            debt_scaled = position.scaled_borrow;
        }
        let collateral_scaled = to_scaled(collateral, state.supply_index);
        position.scaled_borrow -= debt_scaled;
        position.scaled_supply -= collateral_scaled;
        state.total_scaled_borrow -= debt_scaled;
        state.total_scaled_supply -= collateral_scaled;
        Self::set_position(&env, &borrower, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &liquidator,
            env.current_contract_address(),
            &actual,
        );
        token::Client::new(&env, &config.collateral_asset).transfer(
            &env.current_contract_address(),
            &liquidator,
            &collateral,
        );
        env.events().publish(
            (symbol_short!("liq"), liquidator, borrower),
            (actual, collateral),
        );
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

    pub fn get_user_position(env: Env, user: Address) -> Option<UserPosition> {
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

    /// Accrue interest on both supply and borrow indexes.
    ///
    /// Same algorithm as CorePool::accrue_interest_internal.
    /// See that function's doc comment for the full formula.
    #[allow(dead_code)]
    fn accrue_interest_internal(env: &Env) -> Result<(), MarketError> {
        let config = Self::config(env)?;
        let mut state = Self::state(env)?;
        let now = env.ledger().timestamp();
        if now <= state.last_update_timestamp {
            return Ok(());
        }
        let delta_t = (now - state.last_update_timestamp) as i128;
        let total_supply = from_scaled(state.total_scaled_supply, state.supply_index);
        let total_borrow = from_scaled(state.total_scaled_borrow, state.borrow_index);
        if total_supply == 0 || total_borrow == 0 {
            state.last_update_timestamp = now;
            Self::set_state(env, &state);
            return Ok(());
        }
        let rates =
            RateModelClient::new(env, &config.rate_model).get_rates(&total_borrow, &total_supply);
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
        Self::set_state(env, &state);
        Ok(())
    }

    fn validate_config(config: &IsolatedMarketConfig) -> Result<(), MarketError> {
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

    fn position_or_empty(env: &Env, user: &Address) -> UserPosition {
        env.storage()
            .persistent()
            .get(&DataKey::Position(user.clone()))
            .unwrap_or(UserPosition {
                scaled_supply: 0,
                scaled_borrow: 0,
            })
    }

    fn set_position(env: &Env, user: &Address, position: &UserPosition) {
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

    fn health_factor_for_position(
        env: &Env,
        config: &IsolatedMarketConfig,
        state: &IsolatedMarketState,
        position: &UserPosition,
    ) -> Result<i128, MarketError> {
        let debt = from_scaled(position.scaled_borrow, state.borrow_index);
        if debt == 0 {
            return Ok(i128::MAX);
        }
        let collateral = from_scaled(position.scaled_supply, state.supply_index);
        let collateral_value = wad_mul(collateral, Self::price(env, &config.collateral_asset)?);
        let debt_value = wad_mul(debt, Self::price(env, &config.loan_asset)?);
        Ok(health_factor(
            collateral_value,
            config.liquidation_threshold,
            debt_value,
        ))
    }
}
