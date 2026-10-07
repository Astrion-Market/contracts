#![cfg(test)]

extern crate std;

use astrion_market_types::{IsolatedMarketConfig, IsolatedMarketState, MarketPosition};
use astrion_math::{to_assets_down, to_shares_down};
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    token,
    xdr::ToXdr,
    Address, Env, IntoVal,
};

use crate::{MarketAdapterContract, MarketAdapterContractClient, MarketError};

#[contracttype]
#[derive(Clone)]
enum MockMarketKey {
    Config,
    State,
    Position(Address),
}

#[contract]
struct MockMarket;

#[contractimpl]
impl MockMarket {
    pub fn initialize(env: Env, config: IsolatedMarketConfig) {
        env.storage()
            .instance()
            .set(&MockMarketKey::Config, &config);
        env.storage().instance().set(
            &MockMarketKey::State,
            &IsolatedMarketState {
                total_supply_assets: 0,
                total_supply_shares: 0,
                total_borrow_assets: 0,
                total_borrow_shares: 0,
                total_collateral: 0,
                fee_assets: 0,
                last_update_timestamp: 0,
            },
        );
    }

    pub fn get_market_config(env: Env) -> Option<IsolatedMarketConfig> {
        env.storage().instance().get(&MockMarketKey::Config)
    }

    pub fn get_market_state(env: Env) -> Option<IsolatedMarketState> {
        env.storage().instance().get(&MockMarketKey::State)
    }

    pub fn get_user_position(env: Env, user: Address) -> Option<MarketPosition> {
        env.storage()
            .persistent()
            .get(&MockMarketKey::Position(user))
    }

    pub fn supply(
        env: Env,
        supplier: Address,
        assets: i128,
        on_behalf: Address,
    ) -> Result<i128, MarketError> {
        supplier.require_auth();
        let config: IsolatedMarketConfig = env
            .storage()
            .instance()
            .get(&MockMarketKey::Config)
            .unwrap();
        let mut state: IsolatedMarketState =
            env.storage().instance().get(&MockMarketKey::State).unwrap();
        let mut position: MarketPosition = env
            .storage()
            .persistent()
            .get(&MockMarketKey::Position(on_behalf.clone()))
            .unwrap_or(MarketPosition {
                supply_shares: 0,
                borrow_shares: 0,
                collateral: 0,
            });
        let shares = to_shares_down(assets, state.total_supply_assets, state.total_supply_shares);
        position.supply_shares += shares;
        state.total_supply_assets += assets;
        state.total_supply_shares += shares;
        env.storage()
            .persistent()
            .set(&MockMarketKey::Position(on_behalf), &position);
        env.storage().instance().set(&MockMarketKey::State, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &supplier,
            &env.current_contract_address(),
            &assets,
        );
        Ok(shares)
    }

    pub fn withdraw(
        env: Env,
        caller: Address,
        assets: i128,
        shares: i128,
        on_behalf: Address,
        receiver: Address,
    ) -> Result<(i128, i128), MarketError> {
        caller.require_auth();
        let config: IsolatedMarketConfig = env
            .storage()
            .instance()
            .get(&MockMarketKey::Config)
            .unwrap();
        let mut state: IsolatedMarketState =
            env.storage().instance().get(&MockMarketKey::State).unwrap();
        let mut position: MarketPosition = env
            .storage()
            .persistent()
            .get(&MockMarketKey::Position(on_behalf.clone()))
            .unwrap();
        let assets = if assets > 0 {
            assets
        } else {
            to_assets_down(shares, state.total_supply_assets, state.total_supply_shares)
        };
        if caller != on_behalf || shares > position.supply_shares {
            return Err(MarketError::Unauthorized);
        }
        position.supply_shares -= shares;
        state.total_supply_assets -= assets;
        state.total_supply_shares -= shares;
        env.storage()
            .persistent()
            .set(&MockMarketKey::Position(on_behalf), &position);
        env.storage().instance().set(&MockMarketKey::State, &state);
        token::Client::new(&env, &config.loan_asset).transfer(
            &env.current_contract_address(),
            &receiver,
            &assets,
        );
        Ok((assets, shares))
    }
}

fn default_config(env: &Env, loan_asset: &Address) -> IsolatedMarketConfig {
    IsolatedMarketConfig {
        collateral_asset: Address::generate(env),
        loan_asset: loan_asset.clone(),
        oracle_adapter: Address::generate(env),
        lltv: astrion_math::WAD / 2,
        liquidation_bonus: 0,
        reserve_factor: 0,
        supply_cap: 0,
        borrow_cap: 0,
        rate_model: Address::generate(env),
        treasury: Address::generate(env),
    }
}

#[test]
fn test_allocate_deallocate_and_real_assets() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let asset = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let vault = Address::generate(&env);
    let factory = Address::generate(&env);
    let adapter_id = env.register(MarketAdapterContract, ());
    let adapter = MarketAdapterContractClient::new(&env, &adapter_id);
    adapter.initialize(&vault, &asset, &factory);

    let market_id = env.register(MockMarket, ());
    let market = MockMarketClient::new(&env, &market_id);
    market.initialize(&default_config(&env, &asset));
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &asset,
            fn_name: "mint",
            args: (&adapter_id, 1_000i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    token::StellarAssetClient::new(&env, &asset).mint(&adapter_id, &1_000);

    let data = market_id.clone().to_xdr(&env);
    let change = adapter.allocate(&data, &500, &soroban_sdk::symbol_short!("supply"), &vault);
    assert_eq!(change.change, 500);
    assert_eq!(adapter.get_supply_shares(&market_id), 500_000_000);
    assert_eq!(adapter.real_assets(), 500);

    let change = adapter.deallocate(&data, &200, &soroban_sdk::symbol_short!("withdr"), &vault);
    assert_eq!(change.change, -200);
    assert_eq!(adapter.get_supply_shares(&market_id), 300_000_000);
    assert_eq!(adapter.real_assets(), 300);
    assert_eq!(token::Client::new(&env, &asset).balance(&vault), 200);
}

// ---------------------------------------------------------------------------
// Legacy review findings (docs/legacy/REVIEW_FINDINGS.md)
//
// Each test states the EXPECTED authorization behaviour. They are #[ignore]d
// because they fail against the current code; run them with
// `cargo test -p market-adapter -- --ignored` to reproduce. Remove the
// #[ignore] in the commit that remediates the finding.
// ---------------------------------------------------------------------------

fn finding_setup(env: &Env) -> (Address, Address, Address, MarketAdapterContractClient<'_>) {
    let admin = Address::generate(env);
    let asset = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let vault = Address::generate(env);
    let factory = Address::generate(env);
    let adapter_id = env.register(MarketAdapterContract, ());
    let adapter = MarketAdapterContractClient::new(env, &adapter_id);
    adapter.initialize(&vault, &asset, &factory);
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &asset,
            fn_name: "mint",
            args: (&adapter_id, 1_000i128).into_val(env),
            sub_invokes: &[],
        },
    }]);
    token::StellarAssetClient::new(env, &asset).mint(&adapter_id, &1_000);
    (asset, vault, adapter_id, adapter)
}

/// LEGACY-F1: `deallocate` compares the caller-supplied `sender` with
/// `parent_vault` but never calls `sender.require_auth()`. Anyone can pass the
/// vault's address and force liquidity out of markets.
#[test]
#[ignore = "LEGACY-F1: market-adapter deallocate lacks parent_vault.require_auth()"]
fn finding_f1_deallocate_requires_vault_auth() {
    let env = Env::default();
    let (asset, vault, _adapter_id, adapter) = finding_setup(&env);
    let market_id = env.register(MockMarket, ());
    MockMarketClient::new(&env, &market_id).initialize(&default_config(&env, &asset));
    let data = market_id.clone().to_xdr(&env);
    adapter.allocate(&data, &500, &soroban_sdk::symbol_short!("supply"), &vault);

    // No authorization from `vault` is provided for this call.
    let result =
        adapter.try_deallocate(&data, &200, &soroban_sdk::symbol_short!("withdr"), &vault);
    assert!(result.is_err(), "deallocate must require the vault's auth");
}

/// LEGACY-F1 (allocate side): same missing `require_auth()` on `allocate`.
#[test]
#[ignore = "LEGACY-F1: market-adapter allocate lacks parent_vault.require_auth()"]
fn finding_f1_allocate_requires_vault_auth() {
    let env = Env::default();
    let (asset, vault, _adapter_id, adapter) = finding_setup(&env);
    let market_id = env.register(MockMarket, ());
    MockMarketClient::new(&env, &market_id).initialize(&default_config(&env, &asset));
    let data = market_id.clone().to_xdr(&env);

    let result = adapter.try_allocate(&data, &500, &soroban_sdk::symbol_short!("supply"), &vault);
    assert!(result.is_err(), "allocate must require the vault's auth");
}

/// LEGACY-F2: `market_factory` is stored but never consulted. Any contract
/// that reports a matching loan asset is accepted as a market, so idle adapter
/// balance can be supplied into an attacker-controlled "market".
#[test]
#[ignore = "LEGACY-F2: market-adapter does not authenticate markets via market_factory"]
fn finding_f2_allocate_rejects_market_not_from_factory() {
    let env = Env::default();
    env.mock_all_auths();
    let (asset, vault, _adapter_id, adapter) = finding_setup(&env);
    // A market the configured factory never created.
    let rogue_market = env.register(MockMarket, ());
    MockMarketClient::new(&env, &rogue_market).initialize(&default_config(&env, &asset));
    let data = rogue_market.clone().to_xdr(&env);

    let result = adapter.try_allocate(&data, &500, &soroban_sdk::symbol_short!("supply"), &vault);
    assert!(result.is_err(), "allocate must reject markets not created by market_factory");
}
