//! # MockOracle
//!
//! A SEP-0402 compatible price oracle for testnet simulation.
//! NEVER deploy to mainnet.
//!
//! ## Interface
//! Implements the same `lastprice` / `decimals` functions that the real
//! Reflector oracle exposes, so OracleAdapter can use it as a drop-in.
//!
//! ## Default prices (7-decimal fixed-point)
//! - Any asset without an override → $1.00  (10_000_000)
//! - Override via `set_price(asset, price)` — admin only.
//!
//! ## Typical setup
//! ```bash
//! # Set WBTC to $60,000
//! stellar contract invoke --id $MOCK_ORACLE_ID -- set_price \
//!   --asset '{"Stellar":"<WBTC_SAC>"}' --price 600000000000
//!
//! # Point OracleAdapter to use this oracle for test-wbtc
//! stellar contract invoke --id $ORACLE_ADAPTER_ID -- set_asset_oracle \
//!   --asset '{"Stellar":"<WBTC_SAC>"}' \
//!   --oracle "$MOCK_ORACLE_ID" --max_staleness 9999999
//! ```

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

// ---------------------------------------------------------------------------
// SEP-0402 types (must match oracle-adapter exactly)
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[contracttype]
enum DataKey {
    Admin,
    Price(Asset),
}

const DEFAULT_PRICE: i128 = 10_000_000; // $1.00 with 7 decimals

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

fn get_price(env: &Env, asset: &Asset) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Price(asset.clone()))
        .unwrap_or(DEFAULT_PRICE)
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    /// Deploy and set the admin.  Call once.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        set_admin(&env, &admin);
    }

    /// Set a fixed price for `asset` (7-decimal fixed-point, e.g. 10_000_000 = $1.00).
    pub fn set_price(env: Env, asset: Asset, price: i128) {
        get_admin(&env).require_auth();
        assert!(price > 0, "price must be positive");
        env.storage()
            .persistent()
            .set(&DataKey::Price(asset.clone()), &price);
        env.events()
            .publish((symbol_short!("setprice"), asset), price);
    }

    // -----------------------------------------------------------------------
    // SEP-0402 interface — called by OracleAdapter
    // -----------------------------------------------------------------------

    /// Returns the stored price for `asset`, or the $1.00 default.
    /// Timestamp is always the current ledger time so prices never go stale.
    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        let price = get_price(&env, &asset);
        Some(PriceData {
            price,
            timestamp: env.ledger().timestamp(),
        })
    }

    /// Raw price precision: 7 decimal places (matches test tokens).
    pub fn decimals(_env: Env) -> u32 {
        7
    }

    /// Admin address.
    pub fn admin(env: Env) -> Address {
        get_admin(&env)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_default_price() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(MockOracle, ());
        let client = MockOracleClient::new(&env, &id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset = Asset::Stellar(Address::generate(&env));
        let pd = client.lastprice(&asset).unwrap();
        assert_eq!(pd.price, 10_000_000);
        assert_eq!(client.decimals(), 7);
    }

    #[test]
    fn test_set_price() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(MockOracle, ());
        let client = MockOracleClient::new(&env, &id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset = Asset::Stellar(Address::generate(&env));
        client.set_price(&asset, &600_000_000_000_i128); // $60,000
        let pd = client.lastprice(&asset).unwrap();
        assert_eq!(pd.price, 600_000_000_000);
    }
}
