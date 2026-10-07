use core::fmt;

/// Mirrors `spec/schemas/failure-codes.json`; `as_str` is the wire code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    UnsupportedSchemaVersion,
    UnknownNetwork,
    EnvironmentMismatch,
    InvalidRoute,
    InvalidAmount,
    AmountTooLarge,
    DecimalsMismatch,
    FeeExceedsAmount,
    InvalidAddress,
    InvalidDeadline,
    InvalidNonce,
    NonCanonicalEncoding,
    IntentIdMismatch,
    DuplicateIntent,
    ReconciliationMismatch,
    /// The JSON did not have the required shape.
    Malformed,
}

impl ValidationError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedSchemaVersion => "UnsupportedSchemaVersion",
            Self::UnknownNetwork => "UnknownNetwork",
            Self::EnvironmentMismatch => "EnvironmentMismatch",
            Self::InvalidRoute => "InvalidRoute",
            Self::InvalidAmount => "InvalidAmount",
            Self::AmountTooLarge => "AmountTooLarge",
            Self::DecimalsMismatch => "DecimalsMismatch",
            Self::FeeExceedsAmount => "FeeExceedsAmount",
            Self::InvalidAddress => "InvalidAddress",
            Self::InvalidDeadline => "InvalidDeadline",
            Self::InvalidNonce => "InvalidNonce",
            Self::NonCanonicalEncoding => "NonCanonicalEncoding",
            Self::IntentIdMismatch => "IntentIdMismatch",
            Self::DuplicateIntent => "DuplicateIntent",
            Self::ReconciliationMismatch => "ReconciliationMismatch",
            Self::Malformed => "Malformed",
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
