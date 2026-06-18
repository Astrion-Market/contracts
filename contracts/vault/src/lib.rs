#![no_std]
#![allow(deprecated)]

mod errors;
mod storage;
mod types;

#[cfg(test)]
mod test;

use errors::VaultError;
use storage::{
    allowance as read_allowance, balance as read_balance, get_config as read_config,
    get_state as read_state, is_initialized, set_allowance as write_allowance,
    set_config as write_config, set_initialized, set_state as write_state,
};
use soroban_sdk::{contract, contractimpl, Address, Env, String};
use types::{VaultConfig, VaultState};

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

    pub fn approve(env: Env, owner: Address, spender: Address, shares: i128) -> Result<(), VaultError> {
        owner.require_auth();
        if shares < 0 {
            return Err(VaultError::InvalidAmount);
        }
        write_allowance(&env, &owner, &spender, shares);
        Ok(())
    }

}

const SECONDS_PER_YEAR: i128 = 31_536_000;
const DEFAULT_MAX_RATE: i128 = 2 * astrion_math::WAD / SECONDS_PER_YEAR;

fn pow10(exp: u32) -> i128 {
    let mut out = 1i128;
    for _ in 0..exp {
        out *= 10;
    }
    out
}
