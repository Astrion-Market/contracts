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

    /// Deprecated: superseded by the Morpho liquidation incentive factor, which
    /// is derived from `lltv` at liquidation time. Still validated for range and
    /// retained for storage compatibility; no longer affects liquidation math.
    pub liquidation_bonus: i128,

    /// Governance-set fee on borrower interest sent to the treasury (WAD).
    pub reserve_factor: i128,

    /// Maximum total supply of loan_asset (raw units). 0 = no cap.
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
/// Morpho-style share accounting (replaces the old Aave-style supply/borrow
/// indexes). Supply shares are a pro-rata claim on `total_supply_assets`;
/// borrow shares are a pro-rata obligation against `total_borrow_assets`.
/// Interest accrual increases `total_borrow_assets`, and lenders' claim
/// (`total_supply_assets`) grows by the borrower interest minus the protocol
/// fee. Collateral is tracked separately and never lent out.
#[contracttype]
#[derive(Clone, Debug)]
pub struct IsolatedMarketState {
    /// Total loan assets owed to lenders (principal + accrued interest).
    pub total_supply_assets: i128,
    /// Total supply shares outstanding.
    pub total_supply_shares: i128,
    /// Total loan assets owed by borrowers (principal + accrued interest).
    pub total_borrow_assets: i128,
    /// Total borrow shares outstanding.
    pub total_borrow_shares: i128,
    /// Total collateral posted across all positions (raw collateral units).
    pub total_collateral: i128,
    /// Protocol fee accrued in loan-asset units, claimable by the treasury.
    pub fee_assets: i128,
    pub last_update_timestamp: u64,
}

/// Per-user position in this isolated market.
///
/// Lender supply (`supply_shares`) and borrower collateral (`collateral`) are
/// tracked separately: supplying the loan asset is distinct from posting
/// collateral, exactly as in Morpho Blue.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MarketPosition {
    /// Supply shares — pro-rata claim on the lender pool (`total_supply_assets`).
    pub supply_shares: i128,
    /// Borrow shares — pro-rata obligation against `total_borrow_assets`.
    pub borrow_shares: i128,
    /// Collateral posted by this account (raw collateral-asset units).
    pub collateral: i128,
}
