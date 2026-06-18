//! # Liquidation Engine
//!
//! Standalone keeper-callable contract that enforces solvency in CorePool.
//!
//! ## Why a separate contract?
//!
//! Liquidation logic is complex, stateful across multiple contracts, and needs
//! independent upgrades (e.g. adjusting close factor, adding MEV protection).
//! Keeping it separate from CorePool reduces CorePool's attack surface and
//! allows the liquidation mechanism to evolve without touching the core accounting.
//!
//! ## Liquidation flow
//!
//! ```text
//! Liquidator → LiquidationEngine::liquidate(borrower, debt_asset, collateral_asset, repay_amount)
//!    │
//!    ├─ 1. Call CorePool::get_health_factor(borrower) → assert HF < WAD
//!    ├─ 2. Validate repay_amount ≤ outstanding_debt * close_factor
//!    ├─ 3. Compute collateral_to_seize:
//!    │       debt_value_repaid    = repay_amount   * oracle_price(debt_asset)
//!    │       collateral_to_seize  = debt_value_repaid * (WAD + liquidation_bonus)
//!    │                              / oracle_price(collateral_asset)
//!    ├─ 4. Call CorePool::repay(liquidator, borrower, debt_asset, repay_amount)
//!    ├─ 5. Call CorePool::seize_collateral(borrower, liquidator, collateral_asset, collateral_to_seize)
//!    │       (seize_collateral is an admin-only CorePool function, only callable by this engine)
//!    └─ 6. Emit ("liq", liquidator, borrower, repay_amount, collateral_to_seize, bonus)
//! ```
//!
//! ## Close factor
//!
//! The close factor limits how much of a position can be liquidated in a single
//! call. v1 default: 50%. This prevents full liquidations that punish borrowers
//! unfairly for small HF dips.
//!
//! ## Liquidation bonus
//!
//! Defined per-market in MarketConfig.liquidation_bonus. Liquidators receive
//! collateral_to_seize = repaid_debt_value * (1 + bonus) / collateral_price.
//!
//! ## Implementation status: SCAFFOLD
//!
//! Implement in this order:
//!   1. `check_liquidation` (read-only) — useful for keeper bots.
//!   2. `liquidate` for CorePool markets.
//!   3. Edge-case handling (dust, max seizure, bad oracle mid-tx).

#![no_std]
#![allow(deprecated)]
#![allow(clippy::too_many_arguments)]

mod errors;

#[cfg(test)]
mod test;

use astrion_math::{wad_div, wad_mul, WAD};
use errors::LiquidationError;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
};

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Initialized,
    CorePool,
    OracleAdapter,
    /// Protocol-wide close factor (WAD). Default 50% = WAD/2.
    CloseFactor,
    /// Replay protection for keeper-triggered liquidation operations.
    UsedNonce(Address, u64),
}

// ---------------------------------------------------------------------------
// View types
// ---------------------------------------------------------------------------

/// Result of a liquidation eligibility check (for keeper bots).
#[contracttype]
#[derive(Clone, Debug)]
pub struct LiquidationPreview {
    /// Whether this position can be liquidated right now.
    pub is_liquidatable: bool,
    /// Current health factor (WAD-scaled).
    pub health_factor: i128,
    /// Maximum repayable debt amount given the close factor.
    pub max_repay_amount: i128,
    /// Estimated collateral the liquidator would receive for max_repay_amount.
    pub estimated_collateral_seized: i128,
}

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
pub struct MarketConfig {
    pub asset: Address,
    pub ltv: i128,
    pub liquidation_threshold: i128,
    pub liquidation_bonus: i128,
    pub reserve_factor: i128,
    pub supply_cap: i128,
    pub borrow_cap: i128,
    pub is_active: bool,
    pub is_borrowable: bool,
}

#[contractclient(name = "CorePoolClient")]
pub trait CorePool {
    fn get_health_factor(env: Env, user: Address) -> Result<i128, LiquidationError>;
    fn get_borrow_balance(env: Env, user: Address, asset: Address) -> i128;
    fn get_supply_balance(env: Env, user: Address, asset: Address) -> i128;
    fn get_market_config(env: Env, asset: Address) -> Option<MarketConfig>;
    fn repay(
        env: Env,
        payer: Address,
        on_behalf_of: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), LiquidationError>;
}

#[contractclient(name = "OracleAdapterClient")]
pub trait OracleAdapter {
    fn get_price(env: Env, asset: OracleAsset) -> ResolvedPrice;
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct LiquidationEngineContract;

#[contractimpl]
impl LiquidationEngineContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialise the liquidation engine.
    ///
    /// `close_factor` — WAD-scaled fraction of debt that can be liquidated per
    ///                  call. Typical: WAD / 2 (50%).
    pub fn initialize(
        env: Env,
        admin: Address,
        core_pool: Address,
        oracle_adapter: Address,
        close_factor: i128,
    ) -> Result<(), LiquidationError> {
        if is_initialized(&env) {
            return Err(LiquidationError::AlreadyInitialized);
        }
        validate_close_factor(close_factor)?;
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::CorePool, &core_pool);
        env.storage()
            .instance()
            .set(&DataKey::OracleAdapter, &oracle_adapter);
        env.storage()
            .instance()
            .set(&DataKey::CloseFactor, &close_factor);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.events()
            .publish((symbol_short!("init"), admin), core_pool);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Liquidation (CorePool market)
    // -----------------------------------------------------------------------

    /// Liquidate a borrower's position in the CorePool.
    ///
    /// # Parameters
    /// - `liquidator`        — caller; must pre-approve `repay_amount` of `debt_asset` to this contract.
    /// - `borrower`          — the undercollateralised account.
    /// - `debt_asset`        — the asset the liquidator will repay.
    /// - `collateral_asset`  — the asset the liquidator will receive.
    /// - `repay_amount`      — how much debt to repay (capped by close factor internally).
    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        debt_asset: Address,
        collateral_asset: Address,
        repay_amount: i128,
    ) -> Result<(), LiquidationError> {
        Self::execute_liquidation(
            &env,
            &liquidator,
            &borrower,
            &debt_asset,
            &collateral_asset,
            repay_amount,
            None,
        )
    }

    /// Liquidate with keeper safety limits.
    ///
    /// `max_collateral_seized` protects the keeper from unexpected oracle
    /// movement. `deadline` and `nonce` provide timeout and replay protection
    /// for off-chain keeper workers.
    pub fn liquidate_with_limits(
        env: Env,
        liquidator: Address,
        borrower: Address,
        debt_asset: Address,
        collateral_asset: Address,
        repay_amount: i128,
        max_collateral_seized: i128,
        deadline: u64,
        nonce: u64,
    ) -> Result<(), LiquidationError> {
        if env.ledger().timestamp() > deadline {
            return Err(LiquidationError::DeadlineExpired);
        }
        let nonce_key = DataKey::UsedNonce(liquidator.clone(), nonce);
        if env.storage().persistent().has(&nonce_key) {
            return Err(LiquidationError::DuplicateOperation);
        }
        Self::execute_liquidation(
            &env,
            &liquidator,
            &borrower,
            &debt_asset,
            &collateral_asset,
            repay_amount,
            Some(max_collateral_seized),
        )?;
        env.storage().persistent().set(&nonce_key, &true);
        Ok(())
    }

    fn execute_liquidation(
        env: &Env,
        liquidator: &Address,
        borrower: &Address,
        debt_asset: &Address,
        collateral_asset: &Address,
        repay_amount: i128,
        max_collateral_seized: Option<i128>,
    ) -> Result<(), LiquidationError> {
        liquidator.require_auth();
        if !is_initialized(env) {
            return Err(LiquidationError::NotInitialized);
        }
        if repay_amount <= 0 {
            return Err(LiquidationError::NoDebt);
        }
        let (core_pool, _, close_factor) = load_config(env)?;
        let core = CorePoolClient::new(env, &core_pool);
        let hf = core.get_health_factor(borrower);
        if hf >= WAD {
            return Err(LiquidationError::PositionHealthy);
        }
        let debt = core.get_borrow_balance(borrower, debt_asset);
        if debt == 0 {
            return Err(LiquidationError::NoDebt);
        }
        let max_repay = wad_mul(debt, close_factor);
        if repay_amount > max_repay {
            return Err(LiquidationError::RepayExceedsCloseFactor);
        }
        let collateral_seized = collateral_for_repay(
            env,
            &core,
            borrower,
            debt_asset,
            collateral_asset,
            repay_amount,
        )?;
        if max_collateral_seized.is_some_and(|max| collateral_seized > max) {
            return Err(LiquidationError::SlippageExceeded);
        }
        let collateral_balance = core.get_supply_balance(borrower, collateral_asset);
        if collateral_seized > collateral_balance {
            return Err(LiquidationError::InsufficientCollateral);
        }
        core.repay(liquidator, borrower, debt_asset, &repay_amount);
        env.events().publish(
            (symbol_short!("liq"), liquidator.clone(), borrower.clone()),
            (
                debt_asset.clone(),
                collateral_asset.clone(),
                repay_amount,
                collateral_seized,
            ),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Keeper utility
    // -----------------------------------------------------------------------

    /// Preview a potential liquidation without executing it.
    ///
    /// Keeper bots call this to determine if a position is worth liquidating
    /// and estimate their profit before committing gas.
    pub fn check_liquidation(
        env: Env,
        borrower: Address,
        debt_asset: Address,
        collateral_asset: Address,
    ) -> Result<LiquidationPreview, LiquidationError> {
        let (core_pool, _, close_factor) = load_config(&env)?;
        let core = CorePoolClient::new(&env, &core_pool);
        let hf = core.get_health_factor(&borrower);
        if hf >= WAD {
            return Ok(LiquidationPreview {
                is_liquidatable: false,
                health_factor: hf,
                max_repay_amount: 0,
                estimated_collateral_seized: 0,
            });
        }
        let debt = core.get_borrow_balance(&borrower, &debt_asset);
        let max_repay = wad_mul(debt, close_factor);
        let estimated_collateral_seized = collateral_for_repay(
            &env,
            &core,
            &borrower,
            &debt_asset,
            &collateral_asset,
            max_repay,
        )?;
        Ok(LiquidationPreview {
            is_liquidatable: true,
            health_factor: hf,
            max_repay_amount: max_repay,
            estimated_collateral_seized,
        })
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    /// Update the protocol-wide close factor.
    ///
    /// A lower close factor is more borrower-friendly but may leave dust
    /// positions that are too small to liquidate profitably.
    pub fn set_close_factor(env: Env, close_factor: i128) -> Result<(), LiquidationError> {
        require_admin(&env)?;
        validate_close_factor(close_factor)?;
        env.storage()
            .instance()
            .set(&DataKey::CloseFactor, &close_factor);
        Ok(())
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), LiquidationError> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), LiquidationError> {
        require_admin(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    pub fn close_factor(env: Env) -> Option<i128> {
        env.storage().instance().get(&DataKey::CloseFactor)
    }
}

fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Initialized)
        .unwrap_or(false)
}

fn validate_close_factor(close_factor: i128) -> Result<(), LiquidationError> {
    if close_factor <= 0 || close_factor > WAD {
        return Err(LiquidationError::RepayExceedsCloseFactor);
    }
    Ok(())
}

fn require_admin(env: &Env) -> Result<(), LiquidationError> {
    if !is_initialized(env) {
        return Err(LiquidationError::NotInitialized);
    }
    let admin = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Admin)
        .ok_or(LiquidationError::NotInitialized)?;
    admin.require_auth();
    Ok(())
}

fn load_config(env: &Env) -> Result<(Address, Address, i128), LiquidationError> {
    if !is_initialized(env) {
        return Err(LiquidationError::NotInitialized);
    }
    let core_pool = env
        .storage()
        .instance()
        .get(&DataKey::CorePool)
        .ok_or(LiquidationError::NotInitialized)?;
    let oracle = env
        .storage()
        .instance()
        .get(&DataKey::OracleAdapter)
        .ok_or(LiquidationError::NotInitialized)?;
    let close_factor = env
        .storage()
        .instance()
        .get(&DataKey::CloseFactor)
        .ok_or(LiquidationError::NotInitialized)?;
    Ok((core_pool, oracle, close_factor))
}

fn price(env: &Env, asset: &Address) -> Result<i128, LiquidationError> {
    let (_, oracle, _) = load_config(env)?;
    Ok(OracleAdapterClient::new(env, &oracle)
        .get_price(&OracleAsset::Stellar(asset.clone()))
        .price_wad)
}

fn collateral_for_repay(
    env: &Env,
    core: &CorePoolClient,
    _borrower: &Address,
    debt_asset: &Address,
    collateral_asset: &Address,
    repay_amount: i128,
) -> Result<i128, LiquidationError> {
    let debt_price = price(env, debt_asset)?;
    let collateral_price = price(env, collateral_asset)?;
    let config = core
        .get_market_config(collateral_asset)
        .ok_or(LiquidationError::CollateralNotEnabled)?;
    let debt_value = wad_mul(repay_amount, debt_price);
    let with_bonus = wad_mul(debt_value, WAD + config.liquidation_bonus);
    Ok(wad_div(with_bonus, collateral_price))
}
