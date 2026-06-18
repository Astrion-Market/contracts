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
//! ## Implementation status: STEP 4 (Morpho liquidation + bad debt)
//!
//! This crate is mid-port. Steps 2–4 implement the lender pool, borrower
//! collateral, borrow/repay, and liquidation on Morpho share accounting with
//! virtual shares and conservative, solvency-favoring rounding (deposits down,
//! withdrawals/borrows up, debt valuation up). Liquidation uses Morpho's
//! incentive factor with no close factor, socializes bad debt at liquidation
//! time, and exposes `preview_liquidate` for bots. Arithmetic traps on overflow.
//! Operator authorization is Step 6.

#![no_std]
#![allow(deprecated)]

mod errors;

#[cfg(test)]
mod test;

use astrion_math::{
    health_factor, to_assets_down, to_assets_up, to_shares_down, to_shares_up, wad_div, wad_mul,
    zero_floor_sub, WAD,
};
use errors::MarketError;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
};
pub use astrion_market_types::{IsolatedMarketConfig, IsolatedMarketState, MarketPosition};

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

/// Morpho liquidation incentive parameters.
/// `lif = min(MAX_LIF, 1 / (1 - CURSOR * (1 - lltv)))`.
const MAX_LIF: i128 = 115 * WAD / 100; // 1.15
const LIQUIDATION_CURSOR: i128 = 3 * WAD / 10; // 0.30

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

/// Read-only projection of a liquidation, for off-chain liquidator bots.
/// Computed on current stored state — callers should `accrue_interest` first for
/// freshness. Treat `bad_debt_assets` as an estimate until confirmed on-chain.
#[contracttype]
#[derive(Clone, Debug)]
pub struct LiquidationPreview {
    /// Whether the position is currently liquidatable.
    pub liquidatable: bool,
    /// Loan assets the liquidator would pay for the previewed `repay_assets`.
    pub repaid_assets: i128,
    /// Borrow shares that would be repaid.
    pub repaid_shares: i128,
    /// Collateral the liquidator would receive (capped at the position).
    pub seized_collateral: i128,
    /// Residual debt that would be socialized as bad debt (0 if none).
    pub bad_debt_assets: i128,
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
        let shares = to_shares_down(assets, state.total_supply_assets, state.total_supply_shares);
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

        // Exactly one of assets/shares is specified. Round shares UP when assets
        // are given so the withdrawer can never burn fewer shares than the value
        // they remove (closes the free-withdrawal leak); round assets DOWN when
        // shares are given so the pool is favored.
        Self::require_one_input(assets, shares)?;
        let (assets, shares) = if assets > 0 {
            (
                assets,
                to_shares_up(assets, state.total_supply_assets, state.total_supply_shares),
            )
        } else {
            (
                to_assets_down(shares, state.total_supply_assets, state.total_supply_shares),
                shares,
            )
        };
        if assets <= 0 || shares <= 0 {
            return Err(MarketError::InvalidAmount);
        }

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
    // Borrow / repay (loan asset)
    // -----------------------------------------------------------------------

    /// Borrow `assets` of the loan asset against `on_behalf`'s collateral,
    /// sending the assets to `receiver` and minting borrow shares to `on_behalf`.
    pub fn borrow(
        env: Env,
        caller: Address,
        assets: i128,
        on_behalf: Address,
        receiver: Address,
    ) -> Result<i128, MarketError> {
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
        if config.borrow_cap > 0 && state.total_borrow_assets + assets > config.borrow_cap {
            return Err(MarketError::BorrowCapExceeded);
        }
        let available = state.total_supply_assets - state.total_borrow_assets;
        if assets > available {
            return Err(MarketError::InsufficientLiquidity);
        }
        // Mint debt shares rounded UP so a borrower never owes fewer shares than
        // the assets they take.
        let shares = to_shares_up(assets, state.total_borrow_assets, state.total_borrow_shares);
        if shares <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        let mut position = Self::position_or_empty(&env, &on_behalf);
        position.borrow_shares += shares;
        state.total_borrow_shares += shares;
        state.total_borrow_assets += assets;
        if Self::health_factor_for_position(&env, &config, &state, &position)? < WAD {
            return Err(MarketError::HealthFactorTooLow);
        }
        Self::set_position(&env, &on_behalf, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &env.current_contract_address(),
            &receiver,
            &assets,
        );
        env.events()
            .publish((symbol_short!("borrow"), on_behalf), (assets, shares));
        Ok(shares)
    }

    /// Repay borrowed loan assets on behalf of `on_behalf`.
    ///
    /// Caller specifies exactly one of `assets` or `shares` (the other is 0).
    /// Repayment is capped at the outstanding debt; `payer` provides the funds.
    pub fn repay(
        env: Env,
        payer: Address,
        assets: i128,
        shares: i128,
        on_behalf: Address,
    ) -> Result<(i128, i128), MarketError> {
        payer.require_auth();
        Self::guard_live(&env)?;
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let mut position = Self::position_or_empty(&env, &on_behalf);
        if position.borrow_shares == 0 {
            return Err(MarketError::InvalidAmount);
        }

        // Exactly one of assets/shares is specified. When assets are given,
        // burn shares rounded DOWN (the payer does not over-reduce the debt-share
        // pool); when shares are given, charge assets rounded UP. Both favor
        // solvency.
        Self::require_one_input(assets, shares)?;
        let (mut assets, mut shares) = if assets > 0 {
            (
                assets,
                to_shares_down(assets, state.total_borrow_assets, state.total_borrow_shares),
            )
        } else {
            (
                to_assets_up(shares, state.total_borrow_assets, state.total_borrow_shares),
                shares,
            )
        };
        // Never repay more than the position owes.
        if shares > position.borrow_shares {
            shares = position.borrow_shares;
            assets = to_assets_up(shares, state.total_borrow_assets, state.total_borrow_shares);
        }
        if assets <= 0 || shares <= 0 {
            return Err(MarketError::InvalidAmount);
        }

        position.borrow_shares -= shares;
        state.total_borrow_shares -= shares;
        state.total_borrow_assets -= assets;
        Self::set_position(&env, &on_behalf, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &payer,
            env.current_contract_address(),
            &assets,
        );
        env.events()
            .publish((symbol_short!("repay"), on_behalf), (assets, shares));
        Ok((assets, shares))
    }

    // -----------------------------------------------------------------------
    // Liquidation
    // -----------------------------------------------------------------------

    /// Liquidate an unhealthy position (Morpho model — no close factor).
    ///
    /// The caller specifies exactly one of `seized_assets` (collateral to take)
    /// or `repaid_shares` (debt shares to repay); the other is derived from the
    /// liquidation incentive factor `lif = min(MAX_LIF, 1/(1 - CURSOR*(1-lltv)))`.
    /// `min_collateral_out` and `deadline` protect the liquidator from price
    /// moves and stale simulations. Returns `(seized_assets, repaid_assets)`.
    ///
    /// If a liquidation seizes all of the borrower's collateral while debt
    /// shares remain, the residual is written off as bad debt and socialized
    /// across lenders (see below).
    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        seized_assets: i128,
        repaid_shares: i128,
        min_collateral_out: i128,
        deadline: u64,
    ) -> Result<(i128, i128), MarketError> {
        liquidator.require_auth();
        Self::guard_live(&env)?;
        if env.ledger().timestamp() > deadline {
            return Err(MarketError::DeadlineExpired);
        }
        Self::require_one_input(seized_assets, repaid_shares)?;
        Self::accrue_interest_internal(&env)?;
        let config = Self::config(&env)?;
        let mut state = Self::state(&env)?;
        let mut position = Self::position_or_empty(&env, &borrower);
        if Self::health_factor_for_position(&env, &config, &state, &position)? >= WAD {
            return Err(MarketError::HealthFactorOk);
        }

        let lif = Self::liquidation_incentive_factor(config.lltv);
        let price_collateral = Self::price(&env, &config.collateral_asset)?;
        let price_loan = Self::price(&env, &config.loan_asset)?;

        // Derive the unspecified side from the incentive factor. Collateral and
        // loan are valued through their oracle prices in the common numeraire.
        let (seized_assets, repaid_shares) = if seized_assets > 0 {
            let seized_value = wad_mul(seized_assets, price_collateral);
            let repaid_value = wad_div(seized_value, lif);
            let repaid_assets = wad_div(repaid_value, price_loan);
            let shares =
                to_shares_up(repaid_assets, state.total_borrow_assets, state.total_borrow_shares);
            (seized_assets, shares)
        } else {
            let repaid_assets =
                to_assets_down(repaid_shares, state.total_borrow_assets, state.total_borrow_shares);
            let repaid_value = wad_mul(repaid_assets, price_loan);
            let seized_value = wad_mul(repaid_value, lif);
            let seized = wad_div(seized_value, price_collateral);
            (seized, repaid_shares)
        };
        // What the liquidator actually pays in loan assets (rounded up).
        let repaid_assets =
            to_assets_up(repaid_shares, state.total_borrow_assets, state.total_borrow_shares);

        if repaid_shares <= 0 || repaid_shares > position.borrow_shares {
            return Err(MarketError::InvalidAmount);
        }
        if seized_assets <= 0 || seized_assets > position.collateral {
            return Err(MarketError::InsufficientCollateral);
        }
        if seized_assets < min_collateral_out {
            return Err(MarketError::SlippageExceeded);
        }

        position.borrow_shares -= repaid_shares;
        position.collateral -= seized_assets;
        state.total_borrow_shares -= repaid_shares;
        state.total_borrow_assets = zero_floor_sub(state.total_borrow_assets, repaid_assets);
        state.total_collateral -= seized_assets;

        // Bad-debt socialization: if all collateral was seized but debt shares
        // remain, the residual debt is uncollectible. Write it off by reducing
        // both the borrow side and the lender claim (`total_supply_assets`),
        // which lowers the supply share price for all lenders. Recognized only
        // here, at liquidation time — never lazily.
        if position.collateral == 0 && position.borrow_shares > 0 {
            let bad_debt_shares = position.borrow_shares;
            let owed = to_assets_up(
                bad_debt_shares,
                state.total_borrow_assets,
                state.total_borrow_shares,
            );
            let bad_debt_assets = if owed < state.total_borrow_assets {
                owed
            } else {
                state.total_borrow_assets
            };
            state.total_borrow_assets = zero_floor_sub(state.total_borrow_assets, bad_debt_assets);
            state.total_supply_assets = zero_floor_sub(state.total_supply_assets, bad_debt_assets);
            state.total_borrow_shares -= bad_debt_shares;
            position.borrow_shares = 0;
            env.events().publish(
                (symbol_short!("baddebt"), borrower.clone()),
                (bad_debt_shares, bad_debt_assets),
            );
        }

        Self::set_position(&env, &borrower, &position);
        Self::set_state(&env, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &liquidator,
            env.current_contract_address(),
            &repaid_assets,
        );
        token::Client::new(&env, &config.collateral_asset).transfer(
            &env.current_contract_address(),
            &liquidator,
            &seized_assets,
        );
        env.events().publish(
            (symbol_short!("liq"), liquidator, borrower),
            (seized_assets, repaid_assets),
        );
        Ok((seized_assets, repaid_assets))
    }

    /// Liquidation incentive factor: `min(MAX_LIF, 1/(1 - CURSOR*(1 - lltv)))`.
    fn liquidation_incentive_factor(lltv: i128) -> i128 {
        let denom = WAD - wad_mul(LIQUIDATION_CURSOR, WAD - lltv);
        let lif = wad_div(WAD, denom);
        if lif < MAX_LIF {
            lif
        } else {
            MAX_LIF
        }
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

    /// Project the outcome of liquidating `borrower` while repaying up to
    /// `repay_assets` of loan assets. Read-only; computed on current stored
    /// state (call `accrue_interest` first for a fresh result).
    pub fn preview_liquidate(
        env: Env,
        borrower: Address,
        repay_assets: i128,
    ) -> Result<LiquidationPreview, MarketError> {
        let config = Self::config(&env)?;
        let state = Self::state(&env)?;
        let position = Self::position_or_empty(&env, &borrower);

        let liquidatable =
            Self::health_factor_for_position(&env, &config, &state, &position)? < WAD;
        let debt_assets = to_assets_up(
            position.borrow_shares,
            state.total_borrow_assets,
            state.total_borrow_shares,
        );
        if !liquidatable || debt_assets <= 0 || repay_assets <= 0 {
            return Ok(LiquidationPreview {
                liquidatable,
                repaid_assets: 0,
                repaid_shares: 0,
                seized_collateral: 0,
                bad_debt_assets: 0,
            });
        }

        let lif = Self::liquidation_incentive_factor(config.lltv);
        let price_collateral = Self::price(&env, &config.collateral_asset)?;
        let price_loan = Self::price(&env, &config.loan_asset)?;

        let want = if repay_assets < debt_assets {
            repay_assets
        } else {
            debt_assets
        };
        let mut repaid_assets = want;
        let mut seized = wad_div(wad_mul(wad_mul(want, price_loan), lif), price_collateral);
        let mut bad_debt_assets = 0;
        if seized >= position.collateral {
            // Collateral-limited: all collateral seized, residual is bad debt.
            seized = position.collateral;
            let supported_value = wad_div(wad_mul(seized, price_collateral), lif);
            repaid_assets = wad_div(supported_value, price_loan);
            bad_debt_assets = zero_floor_sub(debt_assets, repaid_assets);
        }
        let repaid_shares =
            to_shares_down(repaid_assets, state.total_borrow_assets, state.total_borrow_shares);

        Ok(LiquidationPreview {
            liquidatable: true,
            repaid_assets,
            repaid_shares,
            seized_collateral: seized,
            bad_debt_assets,
        })
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
    /// truncating the rate. Taylor-compounding is a possible later refinement.
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

    /// Require exactly one of `assets`/`shares` to be positive. The caller picks
    /// the rounding direction for the derived value, since withdraw and repay
    /// round oppositely to favor the protocol.
    fn require_one_input(assets: i128, shares: i128) -> Result<(), MarketError> {
        if (assets > 0) == (shares > 0) {
            // Both set or neither set.
            return Err(MarketError::InconsistentInput);
        }
        Ok(())
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
        // Value debt by rounding shares -> assets UP, so health is never
        // overstated.
        let debt = to_assets_up(
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
