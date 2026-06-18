use soroban_sdk::{contracttype, Address};

/// Immutable configuration for an isolated market.
///
/// Each isolated market is a self-contained lending pool between exactly two
/// assets: a collateral asset and a debt asset.  A failure in one isolated
/// market (bad oracle, exploited asset) cannot propagate to other markets or
/// to the CorePool.
#[contracttype]
#[derive(Clone, Debug)]
pub struct IsolatedMarketConfig {
    /// The asset that borrowers deposit as collateral (e.g. MEMECOIN).
    pub collateral_asset: Address,

    /// The asset that borrowers receive (e.g. USDC).
    pub debt_asset: Address,

    /// Oracle adapter contract used for price feeds in this market.
    pub oracle_adapter: Address,

    /// Maximum collateral ratio (WAD).
    pub ltv: i128,

    /// Collateral percentage used in health factor (WAD). Must be > ltv.
    pub liquidation_threshold: i128,

    /// Bonus awarded to liquidators above seized collateral value (WAD).
    pub liquidation_bonus: i128,

    /// Fraction of borrow interest sent to protocol treasury (WAD).
    pub reserve_factor: i128,

    /// Maximum total supply of collateral_asset (raw units). 0 = no cap.
    pub supply_cap: i128,

    /// Maximum total borrow of debt_asset (raw units). 0 = no cap.
    pub borrow_cap: i128,

    /// Interest rate model contract address.
    pub rate_model: Address,

    /// Protocol treasury address.
    pub treasury: Address,
}

/// Live market state — updated on each user interaction.
///
/// Uses the same index-based accounting as CorePool.
/// See CorePool `MarketState` doc for the full accounting model.
#[contracttype]
#[derive(Clone, Debug)]
pub struct IsolatedMarketState {
    pub supply_index: i128,
    pub borrow_index: i128,
    pub total_scaled_supply: i128,
    pub total_scaled_borrow: i128,
    pub protocol_reserves: i128,
    pub last_update_timestamp: u64,
}

/// Per-user position in this isolated market.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserPosition {
    /// Scaled supply balance of the collateral asset.
    pub scaled_supply: i128,
    /// Scaled borrow balance of the debt asset.
    pub scaled_borrow: i128,
}
