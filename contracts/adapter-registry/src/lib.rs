#![no_std]
#![allow(deprecated)]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec, Address, Env, Vec,
};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Owner,
    Initialized,
    Adapter(Address),
    Adapters,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AdapterRegistryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    AlreadyRegistered = 4,
}

#[contract]
pub struct AdapterRegistryContract;

#[contractimpl]
impl AdapterRegistryContract {
    pub fn initialize(env: Env, owner: Address) -> Result<(), AdapterRegistryError> {
        if is_initialized(&env) {
            return Err(AdapterRegistryError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage()
            .persistent()
            .set(&DataKey::Adapters, &Vec::<Address>::new(&env));
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.events().publish((symbol_short!("init"),), owner);
        Ok(())
    }

    pub fn add_adapter(
        env: Env,
        owner: Address,
        adapter: Address,
    ) -> Result<(), AdapterRegistryError> {
        owner.require_auth();
        let stored_owner = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Owner)
            .ok_or(AdapterRegistryError::NotInitialized)?;
        if owner != stored_owner {
            return Err(AdapterRegistryError::Unauthorized);
        }
        let key = DataKey::Adapter(adapter.clone());
        if env.storage().persistent().get(&key).unwrap_or(false) {
            return Err(AdapterRegistryError::AlreadyRegistered);
        }
        env.storage().persistent().set(&key, &true);
        let mut adapters = Self::adapters(env.clone());
        adapters.push_back(adapter.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Adapters, &adapters);
        env.events().publish((symbol_short!("adapter"),), adapter);
        Ok(())
    }

    pub fn is_in_registry(env: Env, adapter: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Adapter(adapter))
            .unwrap_or(false)
    }

    pub fn adapters(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::Adapters)
            .unwrap_or_else(|| vec![&env])
    }

    pub fn owner(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Owner)
    }

    pub fn transfer_owner(
        env: Env,
        owner: Address,
        new_owner: Address,
    ) -> Result<(), AdapterRegistryError> {
        owner.require_auth();
        let stored_owner = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Owner)
            .ok_or(AdapterRegistryError::NotInitialized)?;
        if owner != stored_owner {
            return Err(AdapterRegistryError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Owner, &new_owner);
        env.events().publish((symbol_short!("owner"),), new_owner);
        Ok(())
    }

    pub fn upgrade(
        env: Env,
        owner: Address,
        new_wasm_hash: soroban_sdk::BytesN<32>,
    ) -> Result<(), AdapterRegistryError> {
        owner.require_auth();
        let stored_owner = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Owner)
            .ok_or(AdapterRegistryError::NotInitialized)?;
        if owner != stored_owner {
            return Err(AdapterRegistryError::Unauthorized);
        }
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }
}

fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Initialized)
        .unwrap_or(false)
}
