use soroban_sdk::{contracttype, Address};

/// Immutable configuration for an isolated market.
///
/// Each isolated market is a self-contained lending pool between exactly two
/// assets: a collateral asset and a loan asset (Morpho terminology).  A failure
/// in one isolated market (bad oracle, exploited asset) cannot propagate to
/// other markets or to the CorePool.
#[contracttype]
#[derive(Clone, Debug)]
pub struct IsolatedMarketConfig {
    /// The asset that borrowers deposit as collateral (e.g. MEMECOIN).
    pub collateral_asset: Address,

    /// The asset supplied by lenders and borrowed by borrowers (e.g. USDC).
    pub loan_asset: Address,

    /// Oracle adapter contract used for price feeds in this market.
    pub oracle_adapter: Address,

    /// Liquidation loan-to-value (WAD). Morpho's single risk parameter: a
    /// position is healthy while `borrow_value <= collateral_value * lltv`, and
    /// liquidatable once it crosses that line. Must be in (0, WAD).
    pub lltv: i128,

    /// Bonus awarded to liquidators above seized collateral value (WAD).
    pub liquidation_bonus: i128,

    /// Governance-set fee on borrower interest sent to the treasury (WAD).
    pub reserve_factor: i128,

    /// Maximum total supply of collateral_asset (raw units). 0 = no cap.
    /// Non-Morpho compatibility control; Morpho Blue has no market-level caps
    /// (caps belong in vaults/adapters). Retained for migration only.
    pub supply_cap: i128,

    /// Maximum total borrow of loan_asset (raw units). 0 = no cap.
    /// Non-Morpho compatibility control; see `supply_cap`.
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
    /// Scaled borrow balance of the loan asset.
    pub scaled_borrow: i128,
}
