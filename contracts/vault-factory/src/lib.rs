#![no_std]
#![allow(deprecated)]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, xdr::ToXdr,
    Address, BytesN, Env, String,
};

#[contractclient(name = "VaultClient")]
pub trait Vault {
    fn initialize(env: Env, owner: Address, asset: Address, name: String, symbol: String);
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Owner,
    Initialized,
    VaultWasmHash,
    IsVault(Address),
    VaultByOwnerAssetSalt(Address, Address, BytesN<32>),
}

#[contracttype]
#[derive(Clone)]
pub struct VaultSaltKey {
    pub owner: Address,
    pub asset: Address,
    pub salt: BytesN<32>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultFactoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    VaultAlreadyExists = 4,
    InvalidWasmHash = 5,
}

#[contract]
pub struct VaultFactoryContract;

#[contractimpl]
impl VaultFactoryContract {
    pub fn initialize(
        env: Env,
        owner: Address,
        vault_wasm_hash: BytesN<32>,
    ) -> Result<(), VaultFactoryError> {
        if is_initialized(&env) {
            return Err(VaultFactoryError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage()
            .instance()
            .set(&DataKey::VaultWasmHash, &vault_wasm_hash);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.events().publish((symbol_short!("init"),), owner);
        Ok(())
    }

    pub fn create_vault(
        env: Env,
        owner: Address,
        asset: Address,
        salt: BytesN<32>,
        name: String,
        symbol: String,
    ) -> Result<Address, VaultFactoryError> {
        require_initialized(&env)?;
        let key = DataKey::VaultByOwnerAssetSalt(owner.clone(), asset.clone(), salt.clone());
        if env.storage().persistent().has(&key) {
            return Err(VaultFactoryError::VaultAlreadyExists);
        }
        let wasm_hash = env
            .storage()
            .instance()
            .get::<DataKey, BytesN<32>>(&DataKey::VaultWasmHash)
            .ok_or(VaultFactoryError::InvalidWasmHash)?;
        let deploy_salt =
            Self::deployment_salt(env.clone(), owner.clone(), asset.clone(), salt.clone());
        let vault = env
            .deployer()
            .with_current_contract(deploy_salt)
            .deploy_v2(wasm_hash, ());
        VaultClient::new(&env, &vault).initialize(&owner, &asset, &name, &symbol);
        env.storage()
            .persistent()
            .set(&DataKey::IsVault(vault.clone()), &true);
        env.storage().persistent().set(&key, &vault);
        env.events()
            .publish((symbol_short!("vault"), owner, asset, salt), vault.clone());
        Ok(vault)
    }

    pub fn set_vault_wasm_hash(
        env: Env,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), VaultFactoryError> {
        require_owner(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::VaultWasmHash, &new_wasm_hash);
        env.events()
            .publish((symbol_short!("wasm"),), new_wasm_hash);
        Ok(())
    }

    pub fn transfer_owner(env: Env, new_owner: Address) -> Result<(), VaultFactoryError> {
        require_owner(&env)?;
        env.storage().instance().set(&DataKey::Owner, &new_owner);
        env.events().publish((symbol_short!("owner"),), new_owner);
        Ok(())
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), VaultFactoryError> {
        require_owner(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    pub fn owner(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Owner)
    }

    pub fn vault_wasm_hash(env: Env) -> Option<BytesN<32>> {
        env.storage().instance().get(&DataKey::VaultWasmHash)
    }

    pub fn is_vault(env: Env, vault: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::IsVault(vault))
            .unwrap_or(false)
    }

    pub fn vault_by_owner_asset_salt(
        env: Env,
        owner: Address,
        asset: Address,
        salt: BytesN<32>,
    ) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::VaultByOwnerAssetSalt(owner, asset, salt))
    }

    pub fn deployment_salt(
        env: Env,
        owner: Address,
        asset: Address,
        salt: BytesN<32>,
    ) -> BytesN<32> {
        let key = VaultSaltKey { owner, asset, salt };
        env.crypto().sha256(&key.to_xdr(&env)).to_bytes()
    }
}

fn is_initialized(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Initialized)
        .unwrap_or(false)
}

fn require_initialized(env: &Env) -> Result<(), VaultFactoryError> {
    if is_initialized(env) {
        Ok(())
    } else {
        Err(VaultFactoryError::NotInitialized)
    }
}

fn require_owner(env: &Env) -> Result<(), VaultFactoryError> {
    require_initialized(env)?;
    let owner = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Owner)
        .ok_or(VaultFactoryError::NotInitialized)?;
    owner.require_auth();
    Ok(())
}
