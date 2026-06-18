use soroban_sdk::{contractevent, contracttype, Address};

/// Configuration for the kinked interest rate curve.
///
/// All rate values are WAD-scaled annualised rates (1e18 = 100% APR).
///
/// ## Kinked curve
///
/// ```text
///        rate
///          │                              /
/// base+s1  │                           /   ← slope2
///          │                         /
///    base  │──────────────────────/          ← slope1
///          │                   /
///          └──────────────────────────────── utilization
///          0              U_optimal         1.0
/// ```
///
/// Formula:
/// - if U ≤ U_opt: rate = base_rate + slope1 * (U / U_opt)
/// - if U > U_opt: rate = base_rate + slope1 + slope2 * ((U - U_opt) / (1 - U_opt))
#[contracttype]
#[derive(Clone, Debug)]
pub struct RateModelConfig {
    /// Annual base borrow rate when utilization is 0, in WAD.
    /// Typical: 1% = 0.01e18 = 10_000_000_000_000_000.
    pub base_rate: i128,

    /// Additional borrow rate added as utilization moves from 0 → U_optimal, in WAD.
    /// Typical: 4% = 0.04e18.
    pub slope1: i128,

    /// Steep additional borrow rate above U_optimal, in WAD.
    /// Typical: 75% = 0.75e18 — aggressively discourages near-full utilization.
    pub slope2: i128,

    /// Utilization point where the slope kinks, in WAD.
    /// Typical: 80% = 0.80e18.
    pub optimal_utilization: i128,

    /// Fraction of borrow interest that goes to protocol reserves, in WAD.
    /// Typical: 10% = 0.10e18.
    pub reserve_factor: i128,
}

/// Computed interest rates at a given utilization.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateSnapshot {
    /// Current borrow APR, WAD-scaled.
    pub borrow_rate: i128,
    /// Current supply APY (lender's effective yield), WAD-scaled.
    pub supply_rate: i128,
    /// Current utilization, WAD-scaled fraction in [0, WAD].
    pub utilization: i128,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[contractevent]
pub struct EventInitialized {
    pub admin: Address,
}

#[contractevent]
pub struct EventConfigUpdated {}
