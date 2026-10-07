//! Canonical cross-chain types for Astrion, matching `spec/` (schema v1).
//!
//! JSON Schema covers shape; this crate enforces the semantic rules listed in
//! `spec/README.md` (network consistency, units, bounds, recipient binding,
//! intent-id integrity, duplicates). Fixtures in `spec/fixtures/` are the shared
//! vectors for every consumer.

#![no_std]

extern crate alloc;

pub mod error;
pub mod intent;
pub mod network;
pub mod receipt;
mod util;

pub use error::ValidationError;
pub use intent::{intent_id, validate_batch, validate_intent, Action, Amount, Intent, Protocol};
pub use network::{network, Environment, Network, NetworkKind, NETWORKS};
pub use receipt::{validate_receipt, LegKind, Receipt};

/// The only supported schema major.
pub const SCHEMA_VERSION: u64 = 1;

/// Largest raw amount accepted: the Stellar classic-asset limit (i64::MAX).
pub const MAX_RAW_AMOUNT: u128 = i64::MAX as u128;
