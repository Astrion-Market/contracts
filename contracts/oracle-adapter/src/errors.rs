use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OracleError {
    /// Contract has already been initialised.
    AlreadyInitialized = 1,
    /// Contract has not been initialised.
    NotInitialized = 2,
    /// Caller is not the admin.
    Unauthorized = 3,
    /// No price source configured for this asset.
    NoPriceSource = 4,
    /// The oracle returned no price data (asset not supported by that oracle).
    NoPrice = 5,
    /// The latest oracle observation is older than `max_staleness`.
    StalePrice = 6,
    /// Oracle returned a non-positive price, which is invalid.
    InvalidPrice = 7,
    /// `max_staleness` of 0 is not allowed.
    InvalidStaleness = 8,
    /// Oracle price is outside configured sanity bounds for the asset.
    PriceOutOfBounds = 9,
    /// Price sanity bounds were malformed.
    InvalidBounds = 10,
}
