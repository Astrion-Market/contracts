//! Fixed-point arithmetic for Astrion.
//!
//! All protocol values use WAD precision (1e18). Rates and indices are WAD-scaled;
//! oracle prices are normalised to WAD by the oracle adapter before entering any
//! calculation here.
//!
//! # Overflow safety
//! i128 max ≈ 1.7e38. A wad_mul of two WAD-scaled values (each ≤ 1e18) produces
//! an intermediate product ≤ 1e36, well within i128. Larger values (e.g. token
//! amounts scaled by a large index) may overflow — callers must ensure inputs are
//! within safe bounds. Use `checked_wad_mul` / `checked_wad_div` in critical paths.

#![no_std]

/// 1e18 — the base unit for all fixed-point values in Astrion.
pub const WAD: i128 = 1_000_000_000_000_000_000;

/// 1e18 alias for clarity in rate contexts.
pub const RAY: i128 = WAD;

/// Half a WAD — used for rounding.
const HALF_WAD: i128 = WAD / 2;

// ---------------------------------------------------------------------------
// Core WAD operations
// ---------------------------------------------------------------------------

/// Multiply two WAD-scaled values, rounding to nearest.
///
/// result = (a * b + WAD/2) / WAD
#[inline]
pub fn wad_mul(a: i128, b: i128) -> i128 {
    (a * b + HALF_WAD) / WAD
}

/// Divide two WAD-scaled values, rounding to nearest.
///
/// result = (a * WAD + b/2) / b
#[inline]
pub fn wad_div(a: i128, b: i128) -> i128 {
    assert!(b != 0, "wad_div: division by zero");
    (a * WAD + b / 2) / b
}

/// Checked multiply — returns None on overflow.
#[inline]
pub fn checked_wad_mul(a: i128, b: i128) -> Option<i128> {
    a.checked_mul(b).map(|product| (product + HALF_WAD) / WAD)
}

/// Checked divide — returns None on overflow or zero divisor.
#[inline]
pub fn checked_wad_div(a: i128, b: i128) -> Option<i128> {
    if b == 0 {
        return None;
    }
    a.checked_mul(WAD).map(|scaled| (scaled + b / 2) / b)
}

// ---------------------------------------------------------------------------
// Percentage helpers
// ---------------------------------------------------------------------------

/// Apply a WAD-scaled percentage: result = value * percent / WAD.
///
/// Example: `wad_percent(1000, 5 * WAD / 100)` → 50 (5% of 1000)
#[inline]
pub fn wad_percent(value: i128, percent_wad: i128) -> i128 {
    wad_mul(value, percent_wad)
}

/// Compute utilization: borrowed / supplied, as a WAD fraction in [0, WAD].
///
/// Returns 0 when supplied == 0 to avoid division by zero.
#[inline]
pub fn utilization(total_borrowed: i128, total_supplied: i128) -> i128 {
    if total_supplied == 0 {
        return 0;
    }
    wad_div(total_borrowed, total_supplied)
}

// ---------------------------------------------------------------------------
// Index-based share accounting
// ---------------------------------------------------------------------------

/// Convert a real token amount to a scaled share using the current index.
///
/// scaled = amount * WAD / index
/// (scaled shares shrink as the index grows, so real balance stays constant)
#[inline]
pub fn to_scaled(amount: i128, index: i128) -> i128 {
    wad_div(amount, index)
}

/// Convert scaled shares back to a real token amount using the current index.
///
/// amount = scaled * index / WAD
#[inline]
pub fn from_scaled(scaled: i128, index: i128) -> i128 {
    wad_mul(scaled, index)
}

// ---------------------------------------------------------------------------
// Morpho share accounting
// ---------------------------------------------------------------------------
//
// Pro-rata share/asset conversion with virtual shares and assets, as in Morpho
// Blue's `SharesMathLib`. The virtual offsets serve two purposes:
//   1. They make the first deposit's share price well-defined and resist the
//      first-depositor inflation/griefing attack.
//   2. They remove division-by-zero on an empty market.
// All operations use plain i128 arithmetic, which traps (and therefore reverts)
// on overflow under this workspace's `overflow-checks = true` profile, so the
// conversions are checked in both contract and test builds.

/// Virtual shares added to `total_shares` in every conversion (Morpho: 1e6).
pub const VIRTUAL_SHARES: i128 = 1_000_000;

/// Virtual assets added to `total_assets` in every conversion (Morpho: 1).
pub const VIRTUAL_ASSETS: i128 = 1;

/// `x * y / d`, rounded down (toward zero for non-negative inputs).
#[inline]
pub fn mul_div_down(x: i128, y: i128, d: i128) -> i128 {
    x * y / d
}

/// `x * y / d`, rounded up.
#[inline]
pub fn mul_div_up(x: i128, y: i128, d: i128) -> i128 {
    (x * y + (d - 1)) / d
}

/// `max(0, x - y)`.
#[inline]
pub fn zero_floor_sub(x: i128, y: i128) -> i128 {
    if x > y {
        x - y
    } else {
        0
    }
}

/// Assets → shares, rounded down. Use when minting shares for a deposit so the
/// depositor is never credited more than their pro-rata claim.
#[inline]
pub fn to_shares_down(assets: i128, total_assets: i128, total_shares: i128) -> i128 {
    mul_div_down(
        assets,
        total_shares + VIRTUAL_SHARES,
        total_assets + VIRTUAL_ASSETS,
    )
}

/// Assets → shares, rounded up. Use when burning shares for a specified asset
/// withdrawal, or minting debt shares for a borrow, so the protocol is favored.
#[inline]
pub fn to_shares_up(assets: i128, total_assets: i128, total_shares: i128) -> i128 {
    mul_div_up(
        assets,
        total_shares + VIRTUAL_SHARES,
        total_assets + VIRTUAL_ASSETS,
    )
}

/// Shares → assets, rounded down. Use when paying out assets for a specified
/// share redemption so the protocol is favored.
#[inline]
pub fn to_assets_down(shares: i128, total_assets: i128, total_shares: i128) -> i128 {
    mul_div_down(
        shares,
        total_assets + VIRTUAL_ASSETS,
        total_shares + VIRTUAL_SHARES,
    )
}

/// Shares → assets, rounded up. Use when valuing debt (overestimate) or when
/// charging assets for a specified debt-share repayment so solvency is favored.
#[inline]
pub fn to_assets_up(shares: i128, total_assets: i128, total_shares: i128) -> i128 {
    mul_div_up(
        shares,
        total_assets + VIRTUAL_ASSETS,
        total_shares + VIRTUAL_SHARES,
    )
}

// ---------------------------------------------------------------------------
// Oracle normalisation
// ---------------------------------------------------------------------------

/// Normalise a price that has `decimals` decimal places to WAD (1e18).
///
/// If decimals == 7 (Stellar native), multiplies by 1e11.
/// If decimals == 18, returns price unchanged.
pub fn normalise_to_wad(price: i128, decimals: u32) -> i128 {
    match decimals.cmp(&18) {
        core::cmp::Ordering::Less => {
            let factor = pow10(18 - decimals);
            price * factor
        }
        core::cmp::Ordering::Greater => {
            let factor = pow10(decimals - 18);
            price / factor
        }
        core::cmp::Ordering::Equal => price,
    }
}

/// Integer power of 10 — capped at 10^18 to stay within i128.
fn pow10(exp: u32) -> i128 {
    let mut result: i128 = 1;
    for _ in 0..exp {
        result *= 10;
    }
    result
}

// ---------------------------------------------------------------------------
// Health factor
// ---------------------------------------------------------------------------

/// Compute the health factor as a WAD-scaled value.
///
/// HF = (collateral_value * liquidation_threshold) / debt_value
///
/// Returns i128::MAX when debt == 0 (position is perfectly healthy).
#[inline]
pub fn health_factor(
    collateral_value: i128,
    liquidation_threshold: i128,
    debt_value: i128,
) -> i128 {
    if debt_value == 0 {
        return i128::MAX;
    }
    wad_div(wad_mul(collateral_value, liquidation_threshold), debt_value)
}

/// Returns true when the health factor indicates a liquidatable position (HF < WAD).
#[inline]
pub fn is_liquidatable(hf: i128) -> bool {
    hf < WAD
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn test_wad_mul_identity() {
        assert_eq!(wad_mul(WAD, WAD), WAD);
    }

    #[test]
    fn test_wad_mul_half() {
        assert_eq!(wad_mul(WAD / 2, WAD), WAD / 2);
    }

    #[test]
    fn test_wad_div_identity() {
        assert_eq!(wad_div(WAD, WAD), WAD);
    }

    #[test]
    fn test_wad_div_half() {
        assert_eq!(wad_div(WAD / 2, WAD), WAD / 2);
    }

    #[test]
    fn test_utilization_zero_supply() {
        assert_eq!(utilization(100, 0), 0);
    }

    #[test]
    fn test_utilization_80_percent() {
        let u = utilization(80 * WAD, 100 * WAD);
        assert_eq!(u, 80 * WAD / 100);
    }

    #[test]
    fn test_scaled_roundtrip() {
        // Raw token units — 1000 XLM in stroops (7 decimals), NOT WAD-scaled.
        // to_scaled / from_scaled operate on raw amounts; the index is WAD-scaled.
        let amount = 1_000 * 10_000_000_i128; // 1_000 * 1e7 = 1e10
        let index = 11 * WAD / 10; // 1.1 index
        let scaled = to_scaled(amount, index);
        let recovered = from_scaled(scaled, index);
        assert!((recovered - amount).abs() <= 1);
    }

    #[test]
    fn test_normalise_up() {
        // price with 7 decimals → WAD (18 decimals)
        let price7 = 50_000 * 10_i128.pow(7);
        let wad_price = normalise_to_wad(price7, 7);
        assert_eq!(wad_price, 50_000 * WAD);
    }

    #[test]
    fn test_normalise_equal() {
        let price = 100 * WAD;
        assert_eq!(normalise_to_wad(price, 18), price);
    }

    #[test]
    fn test_health_factor_safe() {
        // 150 collateral, 80% threshold, 100 debt → HF = 1.2
        let hf = health_factor(150 * WAD, 8 * WAD / 10, 100 * WAD);
        assert_eq!(hf, 12 * WAD / 10);
        assert!(!is_liquidatable(hf));
    }

    #[test]
    fn test_health_factor_liquidatable() {
        // 90 collateral, 80% threshold, 100 debt → HF = 0.72
        let hf = health_factor(90 * WAD, 8 * WAD / 10, 100 * WAD);
        assert!(is_liquidatable(hf));
    }

    #[test]
    fn test_health_factor_zero_debt() {
        assert_eq!(health_factor(100 * WAD, WAD, 0), i128::MAX);
    }

    #[test]
    fn test_wad_mul_rounds_to_nearest() {
        assert_eq!(wad_mul(1, WAD / 2), 1);
        assert_eq!(wad_mul(1, WAD / 2 - 1), 0);
    }

    #[test]
    fn test_wad_div_zero_panics() {
        let result = std::panic::catch_unwind(|| wad_div(WAD, 0));
        assert!(result.is_err());
    }

    #[test]
    fn test_checked_helpers_reject_overflow_and_zero() {
        assert_eq!(checked_wad_div(WAD, 0), None);
        assert_eq!(checked_wad_mul(i128::MAX, WAD), None);
    }

    #[test]
    fn test_negative_values_are_consistent() {
        assert!((wad_mul(-2 * WAD, WAD / 2) + WAD).abs() <= 1);
        assert!((wad_div(-WAD, 2 * WAD) + WAD / 2).abs() <= 1);
    }

    #[test]
    fn test_mul_div_rounding_directions() {
        assert_eq!(mul_div_down(7, 1, 2), 3);
        assert_eq!(mul_div_up(7, 1, 2), 4);
        assert_eq!(mul_div_down(6, 1, 2), 3);
        assert_eq!(mul_div_up(6, 1, 2), 3);
    }

    #[test]
    fn test_zero_floor_sub() {
        assert_eq!(zero_floor_sub(10, 3), 7);
        assert_eq!(zero_floor_sub(3, 10), 0);
        assert_eq!(zero_floor_sub(5, 5), 0);
    }

    #[test]
    fn test_first_deposit_uses_virtual_offset() {
        // Empty market: shares = assets * VIRTUAL_SHARES / VIRTUAL_ASSETS.
        assert_eq!(to_shares_down(1_000, 0, 0), 1_000 * VIRTUAL_SHARES);
        // No division by zero, and a 1-unit deposit still mints shares.
        assert_eq!(to_shares_down(1, 0, 0), VIRTUAL_SHARES);
    }

    #[test]
    fn test_withdraw_rounds_up_against_user() {
        // After interest, total_assets > total_shares (per virtual-free view).
        // A tiny asset withdrawal must burn at least one share, never zero.
        let total_assets = 1_003;
        let total_shares = 1_000 * VIRTUAL_SHARES;
        let shares = to_shares_up(1, total_assets, total_shares);
        assert!(shares >= 1, "withdrawing 1 asset must burn >= 1 share");
    }

    #[test]
    fn test_redeem_all_shares_returns_principal() {
        // Deposit 1000 into an empty market, then redeem all shares.
        let shares = to_shares_down(1_000, 0, 0);
        let assets = to_assets_down(shares, 1_000, shares);
        assert_eq!(assets, 1_000);
    }

    #[test]
    fn test_shares_up_ge_shares_down() {
        let (ta, ts) = (1_234_i128, 567 * VIRTUAL_SHARES);
        for assets in [1_i128, 7, 100, 999] {
            assert!(to_shares_up(assets, ta, ts) >= to_shares_down(assets, ta, ts));
        }
    }

    #[test]
    fn test_scaled_roundtrip_property_sample() {
        let mut seed = 17_i128;
        for _ in 0..128 {
            seed = (seed * 1_103_515_245 + 12_345) % 1_000_000_000;
            let amount = seed + 1;
            let index = WAD + (seed % (WAD / 10));
            let scaled = to_scaled(amount, index);
            let recovered = from_scaled(scaled, index);
            assert!((recovered - amount).abs() <= 1);
        }
    }
}
