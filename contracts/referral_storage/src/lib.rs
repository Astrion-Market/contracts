//! Referral storage — on-chain referral code registry and tier management.
//! Mirrors GMX's ReferralStorage.sol.
#![no_std]
#![allow(dependency_on_unit_never_type_fallback)]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error,
    Address, BytesN, Env,
};

// ─── Storage key types ────────────────────────────────────────────────────────

#[contracttype]
pub enum ReferralKey {
    CodeOwner(BytesN<32>),
    TraderCode(Address),
    ReferrerTier(Address),
    TierConfig(u32),
}

#[contracttype]
enum InstanceKey {
    Initialized,
    Admin,
}

// ─── Config per tier ──────────────────────────────────────────────────────────

#[contracttype]
pub struct TierConfig {
    pub total_rebate_bps: u32,    // basis points of position fee paid back to referrer
    pub discount_share_bps: u32, // portion of that rebate forwarded to trader as discount
}

// ─── Events ───────────────────────────────────────────────────────────────────

#[contractevent(topics = ["ref_reg"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeRegistered {
    pub caller: Address,
    pub code:   BytesN<32>,
}

#[contractevent(topics = ["ref_set"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraderCodeSet {
    pub trader: Address,
    pub code:   BytesN<32>,
}

#[contractevent(topics = ["ref_xfr"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeOwnershipTransferred {
    pub code:      BytesN<32>,
    pub from:      Address,
    pub to:        Address,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized     = 1,
    AlreadyInitialized = 2,
    Unauthorized       = 3,
    CodeAlreadyTaken   = 4,
    CodeNotFound       = 5,
    InvalidTier        = 6,
    InvalidInput       = 7,
    NotCodeOwner       = 8,
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct ReferralStorage;

#[contractimpl]
impl ReferralStorage {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        if env.storage().instance().has(&InstanceKey::Initialized) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&InstanceKey::Initialized, &true);
        env.storage().instance().set(&InstanceKey::Admin, &admin);
    }

    /// Upgrade the contract wasm. Only the stored admin may call this.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&InstanceKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Register a new referral code; caller becomes the owner.
    pub fn register_code(env: Env, caller: Address, code: BytesN<32>) {
        caller.require_auth();
        let key = ReferralKey::CodeOwner(code.clone());
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, Error::CodeAlreadyTaken);
        }
        env.storage().persistent().set(&key, &caller);
        env.events().publish_event(&CodeRegistered { caller, code });
    }

    /// Set the referral code for a trader (links them to a referrer).
    pub fn set_trader_referral_code(env: Env, trader: Address, code: BytesN<32>) {
        trader.require_auth();
        // Validate code exists
        if !env.storage().persistent().has(&ReferralKey::CodeOwner(code.clone())) {
            panic_with_error!(&env, Error::CodeNotFound);
        }
        env.storage().persistent().set(&ReferralKey::TraderCode(trader.clone()), &code);
        env.events().publish_event(&TraderCodeSet { trader, code });
    }

    /// Look up the referral code for a trader, and return the referrer's address.
    pub fn get_trader_referrer(env: Env, trader: Address) -> Option<Address> {
        let code: BytesN<32> = env.storage().persistent()
            .get(&ReferralKey::TraderCode(trader))?;
        env.storage().persistent().get(&ReferralKey::CodeOwner(code))
    }

    /// Return the referral code for a trader, or None.
    pub fn get_trader_referral_code(env: Env, trader: Address) -> Option<BytesN<32>> {
        env.storage().persistent().get(&ReferralKey::TraderCode(trader))
    }

    /// Set the tier for a referrer (admin only).
    pub fn set_referrer_tier(env: Env, admin: Address, referrer: Address, tier: u32) {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&InstanceKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));
        if admin != stored_admin {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if tier > 2 {
            panic_with_error!(&env, Error::InvalidTier);
        }
        env.storage().persistent().set(&ReferralKey::ReferrerTier(referrer), &tier);
    }

    /// Configure the rebate/discount parameters for a tier (admin only).
    pub fn set_tier_config(env: Env, admin: Address, tier: u32, config: TierConfig) {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&InstanceKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));
        if admin != stored_admin {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if tier > 2 {
            panic_with_error!(&env, Error::InvalidTier);
        }
        // Validate config parameters
        if config.total_rebate_bps > 10000 || config.discount_share_bps > 10000 {
            panic_with_error!(&env, Error::InvalidInput);
        }
        env.storage().persistent().set(&ReferralKey::TierConfig(tier), &config);
    }

    /// Transfer ownership of a registered referral code to a new address.
    ///
    /// Only the current code owner may call this. Requires auth from `from`.
    /// The new owner (`to`) immediately becomes the code's referrer for fee calculations.
    pub fn transfer_code_ownership(env: Env, from: Address, to: Address, code: BytesN<32>) {
        from.require_auth();
        let key = ReferralKey::CodeOwner(code.clone());
        let current_owner: Address = env.storage().persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, Error::CodeNotFound));
        if current_owner != from {
            panic_with_error!(&env, Error::NotCodeOwner);
        }
        env.storage().persistent().set(&key, &to);
        env.events().publish_event(&CodeOwnershipTransferred { code, from, to });
    }

    /// Return the owner address for a given referral code, or None if unregistered.
    pub fn get_code_owner(env: Env, code: BytesN<32>) -> Option<Address> {
        env.storage().persistent().get(&ReferralKey::CodeOwner(code))
    }

    /// Return the fee discount bps for a trader given their referral code, or 0 if none.
    pub fn get_trader_discount_bps(env: Env, trader: Address) -> u32 {
        let code: BytesN<32> = match env.storage().persistent()
            .get(&ReferralKey::TraderCode(trader))
        {
            Some(c) => c,
            None => return 0,
        };
        let referrer: Address = match env.storage().persistent()
            .get(&ReferralKey::CodeOwner(code))
        {
            Some(r) => r,
            None => return 0,
        };
        let tier: u32 = env.storage().persistent()
            .get(&ReferralKey::ReferrerTier(referrer))
            .unwrap_or(0);
        let config: TierConfig = match env.storage().persistent()
            .get(&ReferralKey::TierConfig(tier))
        {
            Some(c) => c,
            None => return 0,
        };
        // discount = total_rebate * discount_share / 10_000
        config.total_rebate_bps * config.discount_share_bps / 10_000
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, Address, ReferralStorageClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(ReferralStorage, ());
        let client = ReferralStorageClient::new(&env, &contract_id);
        client.initialize(&admin);
        (env, admin, client)
    }

    fn make_code(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    #[test]
    fn test_transfer_code_ownership_success() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        let code  = make_code(&env, 0x01);

        client.register_code(&alice, &code);
        assert_eq!(client.get_code_owner(&code), Some(alice.clone()));

        client.transfer_code_ownership(&alice, &bob, &code);
        assert_eq!(client.get_code_owner(&code), Some(bob));
    }

    #[test]
    fn test_transfer_code_ownership_non_owner_rejected() {
        let (env, _admin, client) = setup();
        let alice   = Address::generate(&env);
        let charlie = Address::generate(&env);
        let code    = make_code(&env, 0x02);

        client.register_code(&alice, &code);

        let result = client.try_transfer_code_ownership(&charlie, &alice, &code);
        assert_eq!(result, Err(Ok(Error::NotCodeOwner)));
    }

    #[test]
    fn test_transfer_code_ownership_missing_code_rejected() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        let code  = make_code(&env, 0x03);

        let result = client.try_transfer_code_ownership(&alice, &bob, &code);
        assert_eq!(result, Err(Ok(Error::CodeNotFound)));
    }

    #[test]
    fn test_get_code_owner_returns_none_for_unregistered() {
        let (env, _admin, client) = setup();
        let code = make_code(&env, 0x04);
        assert_eq!(client.get_code_owner(&code), None);
    }

    #[test]
    fn test_trader_discount_follows_new_owner_tier() {
        let (env, admin, client) = setup();
        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        let trader = Address::generate(&env);
        let code  = make_code(&env, 0x05);

        client.set_tier_config(&admin, &0, &TierConfig { total_rebate_bps: 1000, discount_share_bps: 5000 });
        client.set_tier_config(&admin, &1, &TierConfig { total_rebate_bps: 2000, discount_share_bps: 5000 });

        client.register_code(&alice, &code);
        client.set_trader_referral_code(&trader, &code);

        // After transfer, discount should reflect bob's tier (default 0)
        client.transfer_code_ownership(&alice, &bob, &code);
        let discount = client.get_trader_discount_bps(&trader);
        // tier 0 for bob: 1000 * 5000 / 10_000 = 500
        assert_eq!(discount, 500);
    }
}
