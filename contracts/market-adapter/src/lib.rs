#![no_std]
#![allow(deprecated)]

mod errors;
mod storage;

#[cfg(test)]
mod test;

use astrion_market_types::{IsolatedMarketConfig, IsolatedMarketState, MarketPosition};
use astrion_math::{to_assets_down, to_shares_up};
use errors::AdapterError;
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, vec,
    xdr::{FromXdr, ToXdr},
    Address, Bytes, BytesN, Env, IntoVal, Symbol, Val, Vec,
};
use storage::{
    config as read_config, is_initialized, markets as read_markets, set_config, set_initialized,
    set_markets as write_markets, set_supply_shares as write_supply_shares,
    supply_shares as read_supply_shares,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterConfig {
    pub parent_vault: Address,
    pub asset: Address,
    pub market_factory: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterChange {
    pub ids: Vec<BytesN<32>>,
    pub change: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MarketError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    Paused = 4,
    InvalidAmount = 10,
    InsufficientLiquidity = 11,
    InsufficientCollateral = 12,
    SupplyCapExceeded = 13,
    BorrowCapExceeded = 14,
    InsufficientSupply = 15,
    InconsistentInput = 16,
    HealthFactorTooLow = 20,
    HealthFactorOk = 21,
    SlippageExceeded = 22,
    DeadlineExpired = 23,
    OracleCallFailed = 30,
}

#[contractclient(name = "MarketClient")]
pub trait Market {
    fn get_market_config(env: Env) -> Option<IsolatedMarketConfig>;
    fn get_market_state(env: Env) -> Option<IsolatedMarketState>;
    fn get_user_position(env: Env, user: Address) -> Option<MarketPosition>;
    fn supply(
        env: Env,
        supplier: Address,
        assets: i128,
        on_behalf: Address,
    ) -> Result<i128, MarketError>;
    fn withdraw(
        env: Env,
        caller: Address,
        assets: i128,
        shares: i128,
        on_behalf: Address,
        receiver: Address,
    ) -> Result<(i128, i128), MarketError>;
}

#[contract]
pub struct MarketAdapterContract;

#[contractimpl]
impl MarketAdapterContract {
    pub fn initialize(env: Env, parent_vault: Address, asset: Address, market_factory: Address) {
        if is_initialized(&env) {
            panic_with_error!(&env, AdapterError::AlreadyInitialized);
        }
        set_config(
            &env,
            &AdapterConfig {
                parent_vault,
                asset,
                market_factory,
            },
        );
        set_initialized(&env);
    }

    pub fn get_config(env: Env) -> Option<AdapterConfig> {
        read_config(&env)
    }

    pub fn get_market_ids(env: Env) -> Vec<Address> {
        read_markets(&env)
    }

    pub fn get_supply_shares(env: Env, market: Address) -> i128 {
        read_supply_shares(&env, &market)
    }

    pub fn allocate(
        env: Env,
        data: Bytes,
        assets: i128,
        _selector: Symbol,
        sender: Address,
    ) -> AdapterChange {
        let config = Self::config_or_panic(&env);
        if sender != config.parent_vault {
            panic_with_error!(&env, AdapterError::Unauthorized);
        }
        if assets <= 0 {
            panic_with_error!(&env, AdapterError::InvalidAmount);
        }
        let market = Self::decode_market(&env, &data);
        let market_config = Self::market_config_or_panic(&env, &market);
        if market_config.loan_asset != config.asset {
            panic_with_error!(&env, AdapterError::InvalidMarket);
        }

        let adapter_address = env.current_contract_address();
        Self::authorize_market_call(
            &env,
            &market,
            symbol_short!("supply"),
            vec![
                &env,
                adapter_address.clone().into_val(&env),
                assets.into_val(&env),
                adapter_address.clone().into_val(&env),
            ],
            vec![&env],
        );
        Self::authorize_contract_call(
            &env,
            &market_config.loan_asset,
            symbol_short!("transfer"),
            vec![
                &env,
                adapter_address.clone().into_val(&env),
                market.clone().into_val(&env),
                assets.into_val(&env),
            ],
            vec![&env],
        );
        let shares =
            MarketClient::new(&env, &market).supply(&adapter_address, &assets, &adapter_address);
        if shares <= 0 {
            panic_with_error!(&env, AdapterError::InvalidAmount);
        }
        let current = read_supply_shares(&env, &market);
        write_supply_shares(&env, &market, current + shares);
        Self::remember_market(&env, &market);

        env.events().publish(
            (symbol_short!("allocate"), market.clone()),
            (assets, shares),
        );
        AdapterChange {
            ids: Self::ids(&env, &market, &market_config),
            change: assets,
        }
    }

    pub fn deallocate(
        env: Env,
        data: Bytes,
        assets: i128,
        _selector: Symbol,
        sender: Address,
    ) -> AdapterChange {
        let config = Self::config_or_panic(&env);
        if sender != config.parent_vault {
            panic_with_error!(&env, AdapterError::Unauthorized);
        }
        if assets <= 0 {
            panic_with_error!(&env, AdapterError::InvalidAmount);
        }
        let market = Self::decode_market(&env, &data);
        let market_config = Self::market_config_or_panic(&env, &market);
        let state = MarketClient::new(&env, &market)
            .get_market_state()
            .unwrap_or_else(|| panic_with_error!(&env, AdapterError::InvalidMarket));
        let stored_shares = read_supply_shares(&env, &market);
        let mut shares = to_shares_up(assets, state.total_supply_assets, state.total_supply_shares);
        if shares > stored_shares {
            shares = stored_shares;
        }
        if shares <= 0 {
            panic_with_error!(&env, AdapterError::InsufficientShares);
        }
        let adapter_address = env.current_contract_address();
        Self::authorize_market_call(
            &env,
            &market,
            symbol_short!("withdraw"),
            vec![
                &env,
                adapter_address.clone().into_val(&env),
                0i128.into_val(&env),
                shares.into_val(&env),
                adapter_address.clone().into_val(&env),
                sender.clone().into_val(&env),
            ],
            vec![&env],
        );
        let (withdrawn_assets, burned_shares) = MarketClient::new(&env, &market).withdraw(
            &adapter_address,
            &0,
            &shares,
            &adapter_address,
            &sender,
        );
        write_supply_shares(&env, &market, stored_shares - burned_shares);

        env.events().publish(
            (symbol_short!("dealloc"), market.clone()),
            (withdrawn_assets, burned_shares),
        );
        AdapterChange {
            ids: Self::ids(&env, &market, &market_config),
            change: -withdrawn_assets,
        }
    }

    pub fn real_assets(env: Env) -> i128 {
        let mut total = 0;
        for market in read_markets(&env).iter() {
            let shares = read_supply_shares(&env, &market);
            if shares == 0 {
                continue;
            }
            if let Some(state) = MarketClient::new(&env, &market).get_market_state() {
                total +=
                    to_assets_down(shares, state.total_supply_assets, state.total_supply_shares);
            }
        }
        total
    }

    fn config_or_panic(env: &Env) -> AdapterConfig {
        read_config(env).unwrap_or_else(|| panic_with_error!(env, AdapterError::NotInitialized))
    }

    fn decode_market(env: &Env, data: &Bytes) -> Address {
        Address::from_xdr(env, data)
            .unwrap_or_else(|_| panic_with_error!(env, AdapterError::InvalidData))
    }

    fn market_config_or_panic(env: &Env, market: &Address) -> IsolatedMarketConfig {
        MarketClient::new(env, market)
            .get_market_config()
            .unwrap_or_else(|| panic_with_error!(env, AdapterError::InvalidMarket))
    }

    fn remember_market(env: &Env, market: &Address) {
        let mut markets = read_markets(env);
        if !markets.iter().any(|current| current == *market) {
            markets.push_back(market.clone());
            write_markets(env, &markets);
        }
    }

    fn authorize_market_call(
        env: &Env,
        market: &Address,
        fn_name: Symbol,
        args: Vec<Val>,
        sub_invocations: Vec<InvokerContractAuthEntry>,
    ) {
        env.authorize_as_current_contract(vec![
            env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: market.clone(),
                    fn_name,
                    args,
                },
                sub_invocations,
            }),
        ]);
    }

    fn authorize_contract_call(
        env: &Env,
        contract: &Address,
        fn_name: Symbol,
        args: Vec<Val>,
        sub_invocations: Vec<InvokerContractAuthEntry>,
    ) {
        env.authorize_as_current_contract(vec![
            env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: contract.clone(),
                    fn_name,
                    args,
                },
                sub_invocations,
            }),
        ]);
    }

    fn ids(env: &Env, market: &Address, config: &IsolatedMarketConfig) -> Vec<BytesN<32>> {
        vec![
            env,
            id_for(
                env,
                symbol_short!("adapter"),
                &env.current_contract_address(),
            ),
            id_for(env, symbol_short!("collat"), &config.collateral_asset),
            id_for(env, symbol_short!("market"), market),
        ]
    }
}

fn id_for(env: &Env, tag: Symbol, address: &Address) -> BytesN<32> {
    env.crypto()
        .sha256(&(tag, address.clone()).to_xdr(env))
        .to_bytes()
}
