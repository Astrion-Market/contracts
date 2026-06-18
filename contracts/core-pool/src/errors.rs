use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PoolError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    Paused = 4,

    // Market errors
    MarketNotFound = 10,
    MarketAlreadyExists = 11,
    MarketInactive = 12,
    BorrowingDisabled = 13,

    // Cap errors
    SupplyCapExceeded = 20,
    BorrowCapExceeded = 21,

    // Amount errors
    InvalidAmount = 30,
    InsufficientLiquidity = 31,
    InsufficientCollateral = 32,

    // Health errors
    HealthFactorTooLow = 40,
    HealthFactorOk = 41, // tried to liquidate a healthy position
    CollateralNotEnabled = 42,

    // Oracle errors
    OracleCallFailed = 50,

    // Invariant errors
    InvariantViolation = 60,
}
