use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    Paused = 4,
    Reentrant = 5,

    InvalidAmount = 10,
    InsufficientBalance = 11,
    InsufficientAllowance = 12,
    InsufficientLiquidity = 13,
    InconsistentInput = 14,

    FeeTooHigh = 20,
    RateTooHigh = 21,
    FeeInvariantBroken = 22,

    DataAlreadyPending = 30,
    DataNotTimelocked = 31,
    TimelockNotExpired = 32,
    Abdicated = 33,
}
