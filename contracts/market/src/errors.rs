use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MarketError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    Paused = 4,

    // Amounts
    InvalidAmount = 10,
    InsufficientLiquidity = 11,
    InsufficientCollateral = 12,
    SupplyCapExceeded = 13,
    BorrowCapExceeded = 14,

    // Position health
    HealthFactorTooLow = 20,
    HealthFactorOk = 21,

    // Oracle
    OracleCallFailed = 30,
}
