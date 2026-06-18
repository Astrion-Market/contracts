use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LiquidationError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,

    /// The target position's health factor is >= 1.0 — not liquidatable.
    PositionHealthy = 10,
    /// The liquidator tried to repay more than the close factor allows.
    RepayExceedsCloseFactor = 11,
    /// The position has no outstanding debt.
    NoDebt = 12,
    /// Collateral asset not enabled for this borrower.
    CollateralNotEnabled = 13,
    /// Collateral value would be insufficient to cover repayment + bonus.
    InsufficientCollateral = 14,
    /// Keeper supplied a max collateral seizure below computed seizure.
    SlippageExceeded = 15,
    /// Keeper operation expired before execution.
    DeadlineExpired = 16,
    /// Keeper nonce has already been used.
    DuplicateOperation = 17,

    /// Cross-contract call to CorePool failed.
    PoolCallFailed = 20,
    /// Cross-contract call to OracleAdapter failed.
    OracleCallFailed = 21,
}
