use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RateModelError {
    /// Contract has already been initialised.
    AlreadyInitialized = 1,
    /// Contract has not been initialised.
    NotInitialized = 2,
    /// Caller is not the admin.
    Unauthorized = 3,
    /// Utilization must be in [0, WAD].
    InvalidUtilization = 4,
    /// optimal_utilization must be strictly between 0 and WAD.
    InvalidOptimalUtilization = 5,
    /// reserve_factor must be in [0, WAD).
    InvalidReserveFactor = 6,
    /// total_supplied is zero — utilization undefined.
    ZeroSupply = 7,
}
