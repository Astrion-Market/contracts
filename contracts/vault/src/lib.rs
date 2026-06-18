#![no_std]
#![allow(deprecated)]

mod errors;
mod storage;
mod types;

#[cfg(test)]
mod test;

use astrion_math::{mul_div_down, mul_div_up};
use errors::VaultError;
use soroban_sdk::{
    contract, contractimpl, symbol_short, token, xdr::ToXdr, Address, BytesN, Env, String, Symbol,
};
use storage::{
    allowance as read_allowance, balance as read_balance, clear_pending,
    executable_at as read_executable_at, get_config as read_config, get_state as read_state,
    is_abdicated, is_allocator as read_is_allocator, is_initialized, is_locked,
    is_sentinel as read_is_sentinel, set_abdicated, set_allocator as write_allocator,
    set_allowance as write_allowance, set_balance as write_balance, set_config as write_config,
    set_initialized, set_locked, set_pending, set_sentinel as write_sentinel,
    set_state as write_state, set_timelock as write_timelock, timelock as read_timelock,
};
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
    ) -> Result<(), VaultError> {
        if is_initialized(&env) {
            return Err(VaultError::AlreadyInitialized);
        }

        let decimals = token::Client::new(&env, &asset).decimals();
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

    pub fn owner(env: Env) -> Option<Address> {
        read_config(&env).map(|c| c.owner)
    }

    pub fn curator(env: Env) -> Option<Address> {
        read_config(&env).map(|c| c.curator)
    }

    pub fn is_sentinel(env: Env, account: Address) -> bool {
        read_is_sentinel(&env, &account)
    }

    pub fn is_allocator(env: Env, account: Address) -> bool {
        read_is_allocator(&env, &account)
    }

    pub fn timelock(env: Env, action: Symbol) -> u64 {
        read_timelock(&env, &action)
    }

    pub fn executable_at(env: Env, action: Symbol, args_hash: BytesN<32>) -> Option<u64> {
        read_executable_at(&env, &action, &args_hash)
    }

    pub fn hash_allocator_args(env: Env, account: Address, enabled: bool) -> BytesN<32> {
        hash_allocator_args(&env, &account, enabled)
    }

    pub fn hash_performance_fee_args(env: Env, fee: i128, recipient: Address) -> BytesN<32> {
        hash_fee_args(&env, fee, &recipient)
    }

    pub fn hash_management_fee_args(env: Env, fee: i128, recipient: Address) -> BytesN<32> {
        hash_fee_args(&env, fee, &recipient)
    }

    pub fn hash_max_rate_args(env: Env, rate: i128) -> BytesN<32> {
        hash_max_rate_args(&env, rate)
    }

    pub fn hash_abdicate_args(env: Env, action: Symbol) -> BytesN<32> {
        hash_abdicate_args(&env, &action)
    }

    pub fn is_abdicated(env: Env, action: Symbol) -> bool {
        is_abdicated(&env, &action)
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

    pub fn set_curator(env: Env, owner: Address, new_curator: Address) -> Result<(), VaultError> {
        owner.require_auth();
        let mut config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        if owner != config.owner {
            return Err(VaultError::Unauthorized);
        }
        config.curator = new_curator.clone();
        write_config(&env, &config);
        env.events()
            .publish((symbol_short!("curator"),), new_curator);
        Ok(())
    }

    pub fn set_sentinel(
        env: Env,
        owner: Address,
        account: Address,
        enabled: bool,
    ) -> Result<(), VaultError> {
        owner.require_auth();
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        if owner != config.owner {
            return Err(VaultError::Unauthorized);
        }
        write_sentinel(&env, &account, enabled);
        env.events()
            .publish((symbol_short!("sentinel"), account), enabled);
        Ok(())
    }

    pub fn set_timelock(
        env: Env,
        owner: Address,
        action: Symbol,
        duration: u64,
    ) -> Result<(), VaultError> {
        owner.require_auth();
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        if owner != config.owner {
            return Err(VaultError::Unauthorized);
        }
        write_timelock(&env, &action, duration);
        env.events()
            .publish((symbol_short!("timelock"), action), duration);
        Ok(())
    }

    pub fn submit(
        env: Env,
        curator: Address,
        action: Symbol,
        args_hash: BytesN<32>,
    ) -> Result<u64, VaultError> {
        curator.require_auth();
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        if curator != config.curator {
            return Err(VaultError::Unauthorized);
        }
        if is_abdicated(&env, &action) {
            return Err(VaultError::Abdicated);
        }
        if read_executable_at(&env, &action, &args_hash).is_some() {
            return Err(VaultError::DataAlreadyPending);
        }
        let executable_at = env.ledger().timestamp() + read_timelock(&env, &action);
        set_pending(&env, &action, &args_hash, executable_at);
        env.events().publish(
            (symbol_short!("submit"), action),
            (args_hash.clone(), executable_at),
        );
        Ok(executable_at)
    }

    pub fn revoke(
        env: Env,
        caller: Address,
        action: Symbol,
        args_hash: BytesN<32>,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        if caller != config.curator && !read_is_sentinel(&env, &caller) {
            return Err(VaultError::Unauthorized);
        }
        if read_executable_at(&env, &action, &args_hash).is_none() {
            return Err(VaultError::DataNotTimelocked);
        }
        clear_pending(&env, &action, &args_hash);
        env.events()
            .publish((symbol_short!("revoke"), action), args_hash);
        Ok(())
    }

    pub fn set_allocator(env: Env, account: Address, enabled: bool) -> Result<(), VaultError> {
        let args_hash = hash_allocator_args(&env, &account, enabled);
        Self::accept(&env, &symbol_short!("alloc"), &args_hash)?;
        write_allocator(&env, &account, enabled);
        env.events()
            .publish((symbol_short!("alloc"), account), enabled);
        Ok(())
    }

    pub fn set_performance_fee(env: Env, fee: i128, recipient: Address) -> Result<(), VaultError> {
        if !(0..=MAX_PERFORMANCE_FEE).contains(&fee) {
            return Err(VaultError::FeeTooHigh);
        }
        let args_hash = hash_fee_args(&env, fee, &recipient);
        Self::accept(&env, &symbol_short!("perf_fee"), &args_hash)?;
        Self::accrue_interest_internal(&env)?;
        let mut config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        config.performance_fee = fee;
        config.performance_fee_recipient = recipient.clone();
        write_config(&env, &config);
        env.events()
            .publish((symbol_short!("perf_fee"), recipient), fee);
        Ok(())
    }

    pub fn set_management_fee(env: Env, fee: i128, recipient: Address) -> Result<(), VaultError> {
        if !(0..=MAX_MANAGEMENT_FEE).contains(&fee) {
            return Err(VaultError::FeeTooHigh);
        }
        let args_hash = hash_fee_args(&env, fee, &recipient);
        Self::accept(&env, &symbol_short!("mgmt_fee"), &args_hash)?;
        Self::accrue_interest_internal(&env)?;
        let mut config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        config.management_fee = fee;
        config.management_fee_recipient = recipient.clone();
        write_config(&env, &config);
        env.events()
            .publish((symbol_short!("mgmt_fee"), recipient), fee);
        Ok(())
    }

    pub fn set_max_rate(env: Env, rate: i128) -> Result<(), VaultError> {
        if !(0..=MAX_MAX_RATE).contains(&rate) {
            return Err(VaultError::RateTooHigh);
        }
        let args_hash = hash_max_rate_args(&env, rate);
        Self::accept(&env, &symbol_short!("max_rate"), &args_hash)?;
        Self::accrue_interest_internal(&env)?;
        let mut config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        config.max_rate = rate;
        write_config(&env, &config);
        env.events().publish((symbol_short!("max_rate"),), rate);
        Ok(())
    }

    pub fn abdicate(env: Env, action: Symbol) -> Result<(), VaultError> {
        let args_hash = hash_abdicate_args(&env, &action);
        Self::accept(&env, &symbol_short!("abdicate"), &args_hash)?;
        set_abdicated(&env, &action);
        env.events().publish((symbol_short!("abdicate"),), action);
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
        if assets < 0 {
            return Err(VaultError::InvalidAmount);
        }
        if assets == 0 {
            return Ok(0);
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
        if shares < 0 {
            return Err(VaultError::InvalidAmount);
        }
        if shares == 0 {
            return Ok(0);
        }
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        let projected = Self::projected_state(&env)?;
        Ok(Self::to_assets_up(
            shares,
            &projected,
            config.virtual_shares,
        ))
    }

    pub fn preview_withdraw(env: Env, assets: i128) -> Result<i128, VaultError> {
        if assets < 0 {
            return Err(VaultError::InvalidAmount);
        }
        if assets == 0 {
            return Ok(0);
        }
        let config = read_config(&env).ok_or(VaultError::NotInitialized)?;
        let projected = Self::projected_state(&env)?;
        Ok(Self::to_shares_up(
            assets,
            &projected,
            config.virtual_shares,
        ))
    }

    pub fn preview_redeem(env: Env, shares: i128) -> Result<i128, VaultError> {
        if shares < 0 {
            return Err(VaultError::InvalidAmount);
        }
        if shares == 0 {
            return Ok(0);
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
            env.current_contract_address(),
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
            env.current_contract_address(),
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
        if assets > token::Client::new(env, &config.asset).balance(&env.current_contract_address())
        {
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
        if assets > token::Client::new(env, &config.asset).balance(&env.current_contract_address())
        {
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
        let real_assets =
            token::Client::new(env, &config.asset).balance(&env.current_contract_address());
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

    fn accept(env: &Env, action: &Symbol, args_hash: &BytesN<32>) -> Result<(), VaultError> {
        if is_abdicated(env, action) {
            return Err(VaultError::Abdicated);
        }
        let executable_at =
            read_executable_at(env, action, args_hash).ok_or(VaultError::DataNotTimelocked)?;
        if env.ledger().timestamp() < executable_at {
            return Err(VaultError::TimelockNotExpired);
        }
        clear_pending(env, action, args_hash);
        env.events()
            .publish((symbol_short!("accept"), action.clone()), args_hash.clone());
        Ok(())
    }
}

const SECONDS_PER_YEAR: i128 = 31_536_000;
const DEFAULT_MAX_RATE: i128 = 2 * astrion_math::WAD / SECONDS_PER_YEAR;
const MAX_MAX_RATE: i128 = DEFAULT_MAX_RATE;
const MAX_PERFORMANCE_FEE: i128 = astrion_math::WAD / 2;
const MAX_MANAGEMENT_FEE: i128 = astrion_math::WAD / 20 / SECONDS_PER_YEAR;
const VIRTUAL_ASSETS: i128 = 1;

fn pow10(exp: u32) -> i128 {
    let mut out = 1i128;
    for _ in 0..exp {
        out *= 10;
    }
    out
}

fn hash_allocator_args(env: &Env, account: &Address, enabled: bool) -> BytesN<32> {
    env.crypto()
        .sha256(&(account.clone(), enabled).to_xdr(env))
        .to_bytes()
}

fn hash_fee_args(env: &Env, fee: i128, recipient: &Address) -> BytesN<32> {
    env.crypto()
        .sha256(&(fee, recipient.clone()).to_xdr(env))
        .to_bytes()
}

fn hash_max_rate_args(env: &Env, rate: i128) -> BytesN<32> {
    env.crypto().sha256(&rate.to_xdr(env)).to_bytes()
}

fn hash_abdicate_args(env: &Env, action: &Symbol) -> BytesN<32> {
    env.crypto().sha256(&action.clone().to_xdr(env)).to_bytes()
}
