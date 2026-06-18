//! # TestToken
//!
//! A mintable ERC-20-style fungible token for testnet use only.
//! Deployed once per asset (e.g. USDC, WBTC) with a configurable
//! name, symbol, and decimals.  The deployer/admin can mint freely;
//! any holder can burn their own balance.
//!
//! Built on OpenZeppelin Stellar Contracts ^0.7.1.

#![no_std]

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, MuxedAddress, String};
use stellar_access::ownable::{self as ownable, Ownable};
use stellar_macros::only_owner;
use stellar_tokens::fungible::{burnable::FungibleBurnable, Base, FungibleToken};

#[contract]
pub struct TestToken;

#[contractimpl]
impl TestToken {
    /// Called once at deploy time (Soroban SDK constructor pattern).
    /// Sets token metadata and records the admin/owner.
    pub fn __constructor(e: &Env, admin: Address, decimals: u32, name: String, symbol: String) {
        Base::set_metadata(e, decimals, name, symbol);
        ownable::set_owner(e, &admin);
    }

    /// Mint `amount` tokens into `account`.  Only callable by the owner.
    #[only_owner]
    pub fn mint(e: &Env, account: Address, amount: i128) {
        Base::mint(e, &account, amount);
    }

    /// Upgrade the contract WASM in-place.  Address is preserved.
    #[only_owner]
    pub fn upgrade(e: &Env, new_wasm_hash: BytesN<32>) {
        e.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[contractimpl(contracttrait)]
impl FungibleToken for TestToken {
    type ContractType = Base;
}

#[contractimpl(contracttrait)]
impl FungibleBurnable for TestToken {}

#[contractimpl(contracttrait)]
impl Ownable for TestToken {}
