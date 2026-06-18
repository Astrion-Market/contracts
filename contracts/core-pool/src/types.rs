use soroban_sdk::{contracttype, Address, Map};

/// Risk and cap parameters for a single market asset.
///
/// Set by the admin at market creation; updatable via `update_market_config`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MarketConfig {
    /// The Soroban token contract for this market.
    pub asset: Address,

    /// Maximum collateral ratio (WAD). At 80% LTV, $100 collateral → $80 borrowable.
    pub ltv: i128,

    /// Collateral value percentage used to compute health factor (WAD).
    /// Must be > LTV. Typical: 82.5% for stable assets.
    pub liquidation_threshold: i128,

    /// Extra collateral awarded to liquidators on top of seized value (WAD).
    /// Typical: 5% = 0.05e18. Incentivises liquidators without punishing borrowers excessively.
    pub liquidation_bonus: i128,

    /// Fraction of borrow interest that goes to the protocol treasury (WAD).
    pub reserve_factor: i128,

    /// Hard cap on total supply in this market (raw token units, NOT WAD).
    /// 0 = no cap.
    pub supply_cap: i128,

    /// Hard cap on total borrows in this market (raw token units, NOT WAD).
    /// 0 = no cap.
    pub borrow_cap: i128,

    /// Market is accepting supply and borrow.
    pub is_active: bool,

    /// Borrowing is enabled. Supply can still occur when false (useful for
    /// deprecating a market gracefully).
    pub is_borrowable: bool,
}

/// Live state of a market — updated on every supply/borrow/repay/withdraw.
///
/// ## Index-based accounting (Aave model)
///
/// Rather than tracking every user's accrued interest individually, we maintain
/// two monotonically increasing indexes:
///
/// ```text
/// supply_index  — starts at 1e18, grows as suppliers earn interest
/// borrow_index  — starts at 1e18, grows as borrowers accumulate debt
/// ```
///
/// Users store *scaled* balances:
///
/// ```text
/// scaled_supply = real_supply / supply_index   (shrinks as index grows)
/// scaled_borrow = real_borrow / borrow_index
/// ```
///
/// Real balance at any point in time:
///
/// ```text
/// real_supply = scaled_supply * supply_index
/// real_borrow = scaled_borrow * borrow_index
/// ```
///
/// This means interest accrual is O(1) — update the index, all users'
/// balances update automatically.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MarketState {
    /// Monotonically increasing supply interest index (WAD). Starts at 1e18.
    pub supply_index: i128,

    /// Monotonically increasing borrow interest index (WAD). Starts at 1e18.
    pub borrow_index: i128,

    /// Sum of all users' scaled supply balances.
    /// Real total supply = total_scaled_supply * supply_index / WAD
    pub total_scaled_supply: i128,

    /// Sum of all users' scaled borrow balances.
    /// Real total borrow = total_scaled_borrow * borrow_index / WAD
    pub total_scaled_borrow: i128,

    /// Protocol-owned interest reserves (raw token units).
    /// Accumulated from the reserve_factor fraction of borrow interest.
    pub protocol_reserves: i128,

    /// Unix timestamp of the last `accrue_interest` call (seconds).
    pub last_update_timestamp: u64,
}

/// Per-user accounting across all markets in the shared pool.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserAccount {
    /// Scaled supply balance per asset. real = scaled * supply_index / WAD.
    pub scaled_supply: Map<Address, i128>,

    /// Scaled borrow balance per asset. real = scaled * borrow_index / WAD.
    pub scaled_borrow: Map<Address, i128>,

    /// Which assets the user has enabled as collateral.
    /// Disabled assets still accrue supply interest but cannot be used for
    /// borrowing power calculations.
    pub collateral_enabled: Map<Address, bool>,
}
