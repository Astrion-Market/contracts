#![no_std]
#![allow(deprecated)]

mod errors;
mod types;

#[cfg(test)]
mod test;

use errors::VaultError;
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
        if env.storage().instance().has(&DataKey::Initialized) {
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

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::State, &state);
        env.storage().instance().set(&DataKey::Initialized, &true);
        Ok(())
    }

    pub fn get_config(env: Env) -> Option<VaultConfig> {
        env.storage().instance().get(&DataKey::Config)
    }

    pub fn get_state(env: Env) -> Option<VaultState> {
        env.storage().instance().get(&DataKey::State)
    }
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
enum DataKey {
    Config,
    State,
    Initialized,
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
