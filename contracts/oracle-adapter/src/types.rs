use soroban_sdk::{contractevent, contracttype, Address, Symbol};

// ---------------------------------------------------------------------------
// SEP-40 standard types
// Matches the Reflector oracle interface exactly.
// https://github.com/reflector-network/reflector-contract
// ---------------------------------------------------------------------------

/// An asset that can be priced by a SEP-40 oracle.
///
/// - `Stellar(address)` — any Soroban token contract (USDC, XLM wrapper, etc.)
/// - `Other(symbol)`   — off-chain asset identified by ticker, e.g. Symbol::new("BTC")
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

/// A price observation returned by a SEP-40 oracle.
///
/// `price`     — raw price in the oracle's native decimal precision (see `decimals()`).
/// `timestamp` — Unix timestamp (seconds) of the price observation.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Internal Astrion types
// ---------------------------------------------------------------------------

/// Resolved, WAD-normalised price returned by the oracle adapter.
///
/// All consumers inside the protocol work with `ResolvedPrice`; they never
/// interact with raw oracle decimals.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPrice {
    /// Price in WAD (1e18) precision.
    pub price_wad: i128,
    /// Unix timestamp of the underlying oracle observation (seconds).
    pub timestamp: u64,
    /// Address of the oracle contract that sourced this price.
    pub source: Address,
}

/// Per-asset oracle configuration stored by the adapter.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceSource {
    /// The SEP-40 oracle contract address for this asset.
    pub oracle: Address,
    /// Maximum age of a price observation before it is considered stale (seconds).
    pub max_staleness: u64,
}

/// Optional WAD-scaled sanity bounds for an asset price.
///
/// Bounds are checked after raw oracle values are normalized to WAD. Set either
/// side to 0 to disable that side of the check.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceBounds {
    pub min_price_wad: i128,
    pub max_price_wad: i128,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[contractevent]
pub struct EventInitialized {
    pub admin: Address,
    pub oracle: Address,
    pub max_staleness: u64,
}

#[contractevent]
pub struct EventPriceQueried {
    pub asset: Asset,
    pub price_wad: i128,
    pub timestamp: u64,
    pub source: Address,
}

#[contractevent]
pub struct EventOracleUpdated {
    pub old_oracle: Option<Address>,
    pub new_oracle: Address,
}

#[contractevent]
pub struct EventAdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
}
