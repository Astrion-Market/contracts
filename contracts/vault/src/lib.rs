#![no_std]
#![allow(deprecated)]

mod errors;
mod storage;
mod types;

#[cfg(test)]
mod test;

use errors::VaultError;
use astrion_math::{mul_div_down, mul_div_up};
use storage::{
    allowance as read_allowance, balance as read_balance, get_config as read_config,
    get_state as read_state, is_initialized, is_locked, set_allowance as write_allowance,
    set_balance as write_balance, set_config as write_config, set_initialized, set_locked,
    set_state as write_state,
};
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, String};
use types::{AccrualPreview, VaultConfig, VaultState};

#[contract]
pub struct VaultContract;

#[contractimpl]
impl VaultContract {
    pub fn initialize(
        env: Env,
        owner: Address,
        asset: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) -> Result<(), VaultError> {
        if is_initialized(&env) {
            return Err(VaultError::AlreadyInitialized);
        }

        let virtual_shares = pow10(18u32.saturating_sub(decimals));
        let config = VaultConfig {
            owner: owner.clone(),
            curator: owner,
            asset,
            name,
            symbol,
            decimals,
            virtual_shares,
            performance_fee: 0,
            performance_fee_recipient: env.current_contract_address(),
            management_fee: 0,
            management_fee_recipient: env.current_contract_address(),
            max_rate: DEFAULT_MAX_RATE,
        };
        let state = VaultState {
            total_assets: 0,
            total_shares: 0,
            last_update_timestamp: env.ledger().timestamp(),
        };

        write_config(&env, &config);
        write_state(&env, &state);
        set_initialized(&env);
        Ok(())
    }

    pub fn get_config(env: Env) -> Option<VaultConfig> {
        read_config(&env)
    }

    pub fn get_state(env: Env) -> Option<VaultState> {
        read_state(&env)
    }

    pub fn balance_of(env: Env, user: Address) -> i128 {
        read_balance(&env, &user)
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        read_allowance(&env, &owner, &spender)
    }

    pub fn approve(
        env: Env,
        owner: Address,
        spender: Address,
        shares: i128,
    ) -> Result<(), VaultError> {
        owner.require_auth();
        if shares < 0 {
            return Err(VaultError::InvalidAmount);
        }
        write_allowance(&env, &owner, &spender, shares);
        Ok(())
    }

    pub fn deposit(
        env: Env,
        caller: Address,
        assets: i128,
        receiver: Address,
    ) -> Result<i128, VaultError> {
        caller.require_auth();
        Self::enter(&env)?;
        let result = Self::deposit_internal(&env, &caller, assets, &receiver);
        Self::exit(&env);
        result
    }

    pub fn mint(
        env: Env,
        caller: Address,
        shares: i128,
        receiver: Address,
    ) -> Result<i128, VaultError> {
        caller.require_auth();
        Self::enter(&env)?;
        let result = Self::mint_internal(&env, &caller, shares, &receiver);
        Self::exit(&env);
        result
    }

    pub fn withdraw(
        env: Env,
        caller: Address,
        assets: i128,
        receiver: Address,
        owner: Address,
    ) -> Result<i128, VaultError> {
        caller.require_auth();
        Self::enter(&env)?;
        let result = Self::withdraw_internal(&env, &caller, assets, &receiver, &owner);
        Self::exit(&env);
        result
    }

    pub fn redeem(
        env: Env,
        caller: Address,
        shares: i128,
        receiver: Address,
        owner: Address,
    ) -> Result<i128, VaultError> {
        caller.require_auth();
        Self::enter(&env)?;
        let result = Self::redeem_internal(&env, &caller, shares, &receiver, &owner);
        Self::exit(&env);
        result
    }

    pub fn accrue_interest(env: Env) -> Result<(), VaultError> {
        Self::accrue_interest_internal(&env)
    }

    pub fn accrue_interest_view(env: Env) -> Result<AccrualPreview, VaultError> {
        Self::accrue_interest_preview(&env)
    }

    pub fn preview_deposit(env: Env, assets: i128) -> Result<i128, VaultError> {
        if assets <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        let projected = Self::projected_state(&env)?;
        Ok(Self::to_shares_down(
            assets,
            &projected,
            config.virtual_shares,
        ))
    }

    pub fn preview_mint(env: Env, shares: i128) -> Result<i128, VaultError> {
        if shares <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        let projected = Self::projected_state(&env)?;
        Ok(Self::to_assets_up(shares, &projected, config.virtual_shares))
    }

    pub fn preview_withdraw(env: Env, assets: i128) -> Result<i128, VaultError> {
        if assets <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        let projected = Self::projected_state(&env)?;
        Ok(Self::to_shares_up(assets, &projected, config.virtual_shares))
    }

    pub fn preview_redeem(env: Env, shares: i128) -> Result<i128, VaultError> {
        if shares <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        let projected = Self::projected_state(&env)?;
        Ok(Self::to_assets_down(
            shares,
            &projected,
            config.virtual_shares,
        ))
    }

    pub fn convert_to_shares(env: Env, assets: i128) -> Result<i128, VaultError> {
        Self::preview_deposit(env, assets)
    }

    pub fn convert_to_assets(env: Env, shares: i128) -> Result<i128, VaultError> {
        Self::preview_redeem(env, shares)
    }

    pub fn total_supply(env: Env) -> i128 {
        Self::projected_state(&env)
            .map(|s| s.total_shares)
            .unwrap_or(0)
    }

    pub fn total_assets(env: Env) -> i128 {
        Self::projected_state(&env)
            .map(|s| s.total_assets)
            .unwrap_or(0)
    }

    fn deposit_internal(
        env: &Env,
        caller: &Address,
        assets: i128,
        receiver: &Address,
    ) -> Result<i128, VaultError> {
        if assets <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::accrue_interest_internal(env)?;
        let config = read_config(env).ok_or(VaultError::NotInitialized)?;
        let mut state = read_state(env).ok_or(VaultError::NotInitialized)?;
        let shares = Self::to_shares_down(assets, &state, config.virtual_shares);
        if shares <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::mint_shares(env, receiver, shares, &mut state);
        state.total_assets += assets;
        write_state(env, &state);
        token::Client::new(env, &config.asset).transfer(
            caller,
            &env.current_contract_address(),
            &assets,
        );
        env.events().publish(
            (symbol_short!("deposit"), caller.clone()),
            (receiver.clone(), assets, shares),
        );
        Ok(shares)
    }

    fn mint_internal(
        env: &Env,
        caller: &Address,
        shares: i128,
        receiver: &Address,
    ) -> Result<i128, VaultError> {
        if shares <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::accrue_interest_internal(env)?;
        let config = read_config(env).ok_or(VaultError::NotInitialized)?;
        let mut state = read_state(env).ok_or(VaultError::NotInitialized)?;
        let assets = Self::to_assets_up(shares, &state, config.virtual_shares);
        if assets <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::mint_shares(env, receiver, shares, &mut state);
        state.total_assets += assets;
        write_state(env, &state);
        token::Client::new(env, &config.asset).transfer(
            caller,
            &env.current_contract_address(),
            &assets,
        );
        env.events().publish(
            (symbol_short!("mint"), caller.clone()),
            (receiver.clone(), assets, shares),
        );
        Ok(assets)
    }

    fn withdraw_internal(
        env: &Env,
        caller: &Address,
        assets: i128,
        receiver: &Address,
        owner: &Address,
    ) -> Result<i128, VaultError> {
        if assets <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::accrue_interest_internal(env)?;
        let config = read_config(env).ok_or(VaultError::NotInitialized)?;
        let mut state = read_state(env).ok_or(VaultError::NotInitialized)?;
        let shares = Self::to_shares_up(assets, &state, config.virtual_shares);
        if shares <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::spend_allowance_if_needed(env, caller, owner, shares)?;
        Self::burn_shares(env, owner, shares, &mut state)?;
        if assets > token::Client::new(env, &config.asset).balance(&env.current_contract_address()) {
            return Err(VaultError::InsufficientLiquidity);
        }
        state.total_assets -= assets;
        write_state(env, &state);
        token::Client::new(env, &config.asset).transfer(
            &env.current_contract_address(),
            receiver,
            &assets,
        );
        env.events().publish(
            (symbol_short!("withdraw"), caller.clone()),
            (receiver.clone(), owner.clone(), assets, shares),
        );
        Ok(shares)
    }

    fn redeem_internal(
        env: &Env,
        caller: &Address,
        shares: i128,
        receiver: &Address,
        owner: &Address,
    ) -> Result<i128, VaultError> {
        if shares <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::accrue_interest_internal(env)?;
        let config = read_config(env).ok_or(VaultError::NotInitialized)?;
        let mut state = read_state(env).ok_or(VaultError::NotInitialized)?;
        let assets = Self::to_assets_down(shares, &state, config.virtual_shares);
        if assets <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::spend_allowance_if_needed(env, caller, owner, shares)?;
        Self::burn_shares(env, owner, shares, &mut state)?;
        if assets > token::Client::new(env, &config.asset).balance(&env.current_contract_address()) {
            return Err(VaultError::InsufficientLiquidity);
        }
        state.total_assets -= assets;
        write_state(env, &state);
        token::Client::new(env, &config.asset).transfer(
            &env.current_contract_address(),
            receiver,
            &assets,
        );
        env.events().publish(
            (symbol_short!("redeem"), caller.clone()),
            (receiver.clone(), owner.clone(), assets, shares),
        );
        Ok(assets)
    }

    fn mint_shares(env: &Env, receiver: &Address, shares: i128, state: &mut VaultState) {
        let balance = read_balance(env, receiver);
        write_balance(env, receiver, balance + shares);
        state.total_shares += shares;
    }

    fn burn_shares(
        env: &Env,
        owner: &Address,
        shares: i128,
        state: &mut VaultState,
    ) -> Result<(), VaultError> {
        let balance = read_balance(env, owner);
        if balance < shares {
            return Err(VaultError::InsufficientBalance);
        }
        write_balance(env, owner, balance - shares);
        state.total_shares -= shares;
        Ok(())
    }

    fn spend_allowance_if_needed(
        env: &Env,
        caller: &Address,
        owner: &Address,
        shares: i128,
    ) -> Result<(), VaultError> {
        if caller == owner {
            return Ok(());
        }
        let allowance = read_allowance(env, owner, caller);
        if allowance < shares {
            return Err(VaultError::InsufficientAllowance);
        }
        write_allowance(env, owner, caller, allowance - shares);
        Ok(())
    }

    fn accrue_interest_internal(env: &Env) -> Result<(), VaultError> {
        let preview = Self::accrue_interest_preview(env)?;
        let mut state = read_state(env).ok_or(VaultError::NotInitialized)?;
        state.total_assets = preview.new_total_assets;
        state.total_shares += preview.performance_fee_shares + preview.management_fee_shares;
        state.last_update_timestamp = env.ledger().timestamp();
        write_state(env, &state);

        let config = read_config(env).ok_or(VaultError::NotInitialized)?;
        if preview.performance_fee_shares > 0 {
            let balance = read_balance(env, &config.performance_fee_recipient);
            write_balance(
                env,
                &config.performance_fee_recipient,
                balance + preview.performance_fee_shares,
            );
        }
        if preview.management_fee_shares > 0 {
            let balance = read_balance(env, &config.management_fee_recipient);
            write_balance(
                env,
                &config.management_fee_recipient,
                balance + preview.management_fee_shares,
            );
        }
        env.events().publish(
            (symbol_short!("accrue"),),
            (
                preview.new_total_assets,
                preview.performance_fee_shares,
                preview.management_fee_shares,
            ),
        );
        Ok(())
    }

    fn accrue_interest_preview(env: &Env) -> Result<AccrualPreview, VaultError> {
        let config = read_config(env).ok_or(VaultError::NotInitialized)?;
        let state = read_state(env).ok_or(VaultError::NotInitialized)?;
        let now = env.ledger().timestamp();
        if now <= state.last_update_timestamp {
            return Ok(AccrualPreview {
                new_total_assets: state.total_assets,
                performance_fee_shares: 0,
                management_fee_shares: 0,
            });
        }
        let elapsed = (now - state.last_update_timestamp) as i128;
        let real_assets = token::Client::new(env, &config.asset).balance(&env.current_contract_address());
        let max_gain = state.total_assets * config.max_rate * elapsed / astrion_math::WAD;
        let max_total = state.total_assets + max_gain;
        let new_total_assets = if real_assets < max_total {
            real_assets
        } else {
            max_total
        };
        let interest = if new_total_assets > state.total_assets {
            new_total_assets - state.total_assets
        } else {
            0
        };
        let performance_fee_assets = interest * config.performance_fee / astrion_math::WAD;
        let management_fee_assets =
            new_total_assets * elapsed * config.management_fee / astrion_math::WAD;
        let fee_base = new_total_assets - performance_fee_assets - management_fee_assets;
        let performance_fee_shares = if performance_fee_assets > 0 {
            mul_div_down(
                performance_fee_assets,
                state.total_shares + config.virtual_shares,
                fee_base + VIRTUAL_ASSETS,
            )
        } else {
            0
        };
        let management_fee_shares = if management_fee_assets > 0 {
            mul_div_down(
                management_fee_assets,
                state.total_shares + config.virtual_shares,
                fee_base + VIRTUAL_ASSETS,
            )
        } else {
            0
        };
        Ok(AccrualPreview {
            new_total_assets,
            performance_fee_shares,
            management_fee_shares,
        })
    }

    fn projected_state(env: &Env) -> Result<VaultState, VaultError> {
        let preview = Self::accrue_interest_preview(env)?;
        let mut state = read_state(env).ok_or(VaultError::NotInitialized)?;
        state.total_assets = preview.new_total_assets;
        state.total_shares += preview.performance_fee_shares + preview.management_fee_shares;
        Ok(state)
    }

    fn to_shares_down(assets: i128, state: &VaultState, virtual_shares: i128) -> i128 {
        mul_div_down(
            assets,
            state.total_shares + virtual_shares,
            state.total_assets + VIRTUAL_ASSETS,
        )
    }

    fn to_shares_up(assets: i128, state: &VaultState, virtual_shares: i128) -> i128 {
        mul_div_up(
            assets,
            state.total_shares + virtual_shares,
            state.total_assets + VIRTUAL_ASSETS,
        )
    }

    fn to_assets_down(shares: i128, state: &VaultState, virtual_shares: i128) -> i128 {
        mul_div_down(
            shares,
            state.total_assets + VIRTUAL_ASSETS,
            state.total_shares + virtual_shares,
        )
    }

    fn to_assets_up(shares: i128, state: &VaultState, virtual_shares: i128) -> i128 {
        mul_div_up(
            shares,
            state.total_assets + VIRTUAL_ASSETS,
            state.total_shares + virtual_shares,
        )
    }

    fn enter(env: &Env) -> Result<(), VaultError> {
        if is_locked(env) {
            return Err(VaultError::Reentrant);
        }
        set_locked(env, true);
        Ok(())
    }

    fn exit(env: &Env) {
        set_locked(env, false);
    }
}

const SECONDS_PER_YEAR: i128 = 31_536_000;
const DEFAULT_MAX_RATE: i128 = 2 * astrion_math::WAD / SECONDS_PER_YEAR;
const VIRTUAL_ASSETS: i128 = 1;

fn pow10(exp: u32) -> i128 {
    let mut out = 1i128;
    for _ in 0..exp {
        out *= 10;
    }
    out
}
