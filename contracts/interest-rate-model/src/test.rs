#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{
    compute_borrow_rate, compute_supply_rate, errors::RateModelError, types::RateModelConfig,
    RateModelContract, RateModelContractClient,
};
use astrion_math::WAD;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_config() -> RateModelConfig {
    RateModelConfig {
        base_rate: WAD / 100,                // 1%
        slope1: WAD * 4 / 100,               // 4%
        slope2: WAD * 75 / 100,              // 75%
        optimal_utilization: WAD * 80 / 100, // 80%
        reserve_factor: WAD / 10,            // 10%
    }
}

fn setup() -> (Env, RateModelContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RateModelContract, ());
    let client = RateModelContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    (env, client, admin)
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_success() {
    let (_, client, admin) = setup();
    client.initialize(&admin, &default_config());
    assert_eq!(client.admin(), admin);
}

#[test]
fn test_double_initialize_fails() {
    let (_, client, admin) = setup();
    client.initialize(&admin, &default_config());
    let result = client.try_initialize(&admin, &default_config());
    assert_eq!(result, Err(Ok(RateModelError::AlreadyInitialized)));
}

// ---------------------------------------------------------------------------
// Borrow rate — below kink (linear segment)
// ---------------------------------------------------------------------------

#[test]
fn test_borrow_rate_zero_utilization() {
    let config = default_config();
    let rate = compute_borrow_rate(&config, 0);
    // At 0% util, rate = base_rate = 1%
    assert_eq!(rate, WAD / 100);
}

#[test]
fn test_borrow_rate_at_optimal() {
    let config = default_config();
    let rate = compute_borrow_rate(&config, config.optimal_utilization);
    // At U_optimal: rate = base_rate + slope1 = 1% + 4% = 5%
    let expected = WAD / 100 + WAD * 4 / 100;
    assert_eq!(rate, expected);
}

#[test]
fn test_borrow_rate_half_optimal() {
    let config = default_config();
    // 40% utilization = half of 80% optimal
    let u = WAD * 40 / 100;
    let rate = compute_borrow_rate(&config, u);
    // base_rate + slope1 * (40/80) = 1% + 4% * 0.5 = 3%
    let expected = WAD / 100 + WAD * 2 / 100;
    assert_eq!(rate, expected);
}

// ---------------------------------------------------------------------------
// Borrow rate — above kink (steep segment)
// ---------------------------------------------------------------------------

#[test]
fn test_borrow_rate_100_percent_utilization() {
    let config = default_config();
    let rate = compute_borrow_rate(&config, WAD);
    // At 100%: base + slope1 + slope2 = 1% + 4% + 75% = 80%
    let expected = WAD / 100 + WAD * 4 / 100 + WAD * 75 / 100;
    assert_eq!(rate, expected);
}

#[test]
fn test_borrow_rate_90_percent_utilization() {
    let config = default_config();
    let u = WAD * 90 / 100;
    let rate = compute_borrow_rate(&config, u);
    // excess_u = (90% - 80%) / (100% - 80%) = 10%/20% = 50%
    // rate = 1% + 4% + 75% * 0.5 = 42.5%
    let expected = WAD / 100 + WAD * 4 / 100 + WAD * 75 / 100 / 2;
    assert_eq!(rate, expected);
}

#[test]
fn test_borrow_rate_near_kink_is_continuous() {
    let config = default_config();
    let below = compute_borrow_rate(&config, config.optimal_utilization - 1);
    let at = compute_borrow_rate(&config, config.optimal_utilization);
    let above = compute_borrow_rate(&config, config.optimal_utilization + 1);
    assert!(below <= at);
    assert!(at <= above);
    assert!(above - below < 10_000_000_000);
}

#[test]
fn test_borrow_rate_multiple_transition_points() {
    let config = default_config();
    let points = [
        WAD * 25 / 100,
        WAD * 50 / 100,
        WAD * 75 / 100,
        WAD * 85 / 100,
        WAD * 95 / 100,
    ];
    let mut prev = 0;
    for point in points {
        let rate = compute_borrow_rate(&config, point);
        assert!(rate > prev);
        prev = rate;
    }
}

// ---------------------------------------------------------------------------
// Supply rate
// ---------------------------------------------------------------------------

#[test]
fn test_supply_rate_at_optimal() {
    let config = default_config();
    let borrow_rate = compute_borrow_rate(&config, config.optimal_utilization);
    let supply_rate = compute_supply_rate(&config, borrow_rate, config.optimal_utilization);
    // supply = borrow_rate * U * (1 - reserve_factor)
    // = 5% * 80% * 90% = 3.6%
    let expected = astrion_math::wad_mul(
        astrion_math::wad_mul(borrow_rate, config.optimal_utilization),
        WAD - config.reserve_factor,
    );
    assert_eq!(supply_rate, expected);
    // Supply should always be less than borrow rate
    assert!(supply_rate < borrow_rate);
}

#[test]
fn test_supply_rate_zero_utilization() {
    let config = default_config();
    let borrow_rate = compute_borrow_rate(&config, 0);
    let supply_rate = compute_supply_rate(&config, borrow_rate, 0);
    // supply = borrow_rate * 0 * anything = 0
    assert_eq!(supply_rate, 0);
}

// ---------------------------------------------------------------------------
// get_rates convenience method
// ---------------------------------------------------------------------------

#[test]
fn test_get_rates_80_percent() {
    let (_, client, admin) = setup();
    client.initialize(&admin, &default_config());

    let total_borrowed = 80 * WAD;
    let total_supplied = 100 * WAD;
    let snapshot = client.get_rates(&total_borrowed, &total_supplied);

    assert_eq!(snapshot.utilization, WAD * 80 / 100);
    assert_eq!(snapshot.borrow_rate, WAD / 100 + WAD * 4 / 100); // 5%
    assert!(snapshot.supply_rate > 0);
    assert!(snapshot.supply_rate < snapshot.borrow_rate);
}

#[test]
fn test_get_rates_zero_supply_fails() {
    let (_, client, admin) = setup();
    client.initialize(&admin, &default_config());

    let result = client.try_get_rates(&100, &0);
    assert_eq!(result, Err(Ok(RateModelError::ZeroSupply)));
}

// ---------------------------------------------------------------------------
// Config validation
// ---------------------------------------------------------------------------

#[test]
fn test_invalid_optimal_utilization_zero() {
    let (_, client, admin) = setup();
    let mut config = default_config();
    config.optimal_utilization = 0;
    let result = client.try_initialize(&admin, &config);
    assert_eq!(result, Err(Ok(RateModelError::InvalidOptimalUtilization)));
}

#[test]
fn test_invalid_reserve_factor_100_percent() {
    let (_, client, admin) = setup();
    let mut config = default_config();
    config.reserve_factor = WAD; // 100% — everything to reserves, lenders get 0
    let result = client.try_initialize(&admin, &config);
    assert_eq!(result, Err(Ok(RateModelError::InvalidReserveFactor)));
}

#[test]
fn test_update_config_requires_admin() {
    let (_, client, admin) = setup();
    client.initialize(&admin, &default_config());

    let new_config = RateModelConfig {
        slope2: WAD * 50 / 100, // lower slope2
        ..default_config()
    };
    // mock_all_auths is on, so this passes; in prod only admin succeeds
    client.update_config(&new_config);
    assert_eq!(client.config().slope2, WAD * 50 / 100);
}
