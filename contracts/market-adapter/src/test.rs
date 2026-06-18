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
