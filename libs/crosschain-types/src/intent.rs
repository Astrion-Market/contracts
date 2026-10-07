//! Intent validation (`spec/schemas/intent.schema.json`).

use alloc::{collections::BTreeSet, string::String, vec::Vec};

use serde::Deserialize;
use serde_json::Value;
use sha3::{Digest, Keccak256};

use crate::{
    network::{network, Network, NetworkKind},
    util::{
        hex32, is_address_of_kind, is_bytes32, is_evm_address, is_uint256_decimal, parse_raw_amount,
    },
    ValidationError, SCHEMA_VERSION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Lend,
    Withdraw,
    SupplyCollateral,
    WithdrawCollateral,
    Borrow,
    Repay,
    Transfer,
}

impl Action {
    /// Actions that move funds INTO a protocol position on the destination.
    fn funds_destination(self) -> bool {
        matches!(self, Self::Lend | Self::SupplyCollateral | Self::Repay)
    }

    /// Actions that take funds OUT of a position on the source.
    fn drains_source(self) -> bool {
        matches!(
            self,
            Self::Withdraw | Self::WithdrawCollateral | Self::Borrow
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
pub enum Protocol {
    #[serde(rename = "aave-v3")]
    AaveV3,
    #[serde(rename = "morpho-blue")]
    MorphoBlue,
    #[serde(rename = "compound-v3")]
    CompoundV3,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct Amount {
    pub raw: String,
    pub decimals: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct Asset {
    pub symbol: String,
}

/// Typed v1 intent. Unknown fields are ignored for typing but still covered
/// by `intentId`, which hashes the raw object (additive versioning).
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub schema_version: u64,
    pub intent_id: String,
    pub action: Action,
    pub source_network: String,
    pub destination_network: String,
    pub asset: Asset,
    pub amount: Amount,
    #[serde(default)]
    pub protocol: Option<Protocol>,
    #[serde(default)]
    pub market_scope: Option<String>,
    pub position_owner: String,
    pub recipient: String,
    pub refund_recipient: String,
    pub max_fee: Amount,
    pub created_at: u64,
    pub action_deadline: u64,
    pub delivery_timeout_secs: u64,
    pub nonce: String,
}

/// `keccak256(canonical JSON of the object without "intentId")`, as 0x-hex.
///
/// Canonical JSON: keys sorted by code point (serde_json's map is a BTreeMap
/// without `preserve_order`), no insignificant whitespace, ASCII only.
pub fn intent_id(value: &Value) -> Result<String, ValidationError> {
    let mut object = value.as_object().ok_or(ValidationError::Malformed)?.clone();
    object.remove("intentId");
    let canonical =
        serde_json::to_string(&Value::Object(object)).map_err(|_| ValidationError::Malformed)?;
    if !canonical.is_ascii() {
        return Err(ValidationError::NonCanonicalEncoding);
    }
    let digest: [u8; 32] = Keccak256::digest(canonical.as_bytes()).into();
    Ok(hex32(&digest))
}

/// Validate one intent. Checks run in a fixed order so each fixture maps to
/// exactly one error code.
pub fn validate_intent(value: &Value) -> Result<Intent, ValidationError> {
    let version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(ValidationError::Malformed)?;
    if version != SCHEMA_VERSION {
        return Err(ValidationError::UnsupportedSchemaVersion);
    }

    let claimed = value
        .get("intentId")
        .and_then(Value::as_str)
        .ok_or(ValidationError::Malformed)?;
    if intent_id(value)? != claimed {
        return Err(ValidationError::IntentIdMismatch);
    }

    let intent: Intent =
        serde_json::from_value(value.clone()).map_err(|_| ValidationError::Malformed)?;
    if intent.asset.symbol != "USDC" {
        return Err(ValidationError::InvalidRoute);
    }

    let source = network(&intent.source_network).ok_or(ValidationError::UnknownNetwork)?;
    let destination =
        network(&intent.destination_network).ok_or(ValidationError::UnknownNetwork)?;
    if source.environment != destination.environment {
        return Err(ValidationError::EnvironmentMismatch);
    }
    check_route(&intent, source, destination)?;

    let amount = parse_raw_amount(&intent.amount.raw)?;
    if amount == 0 {
        return Err(ValidationError::InvalidAmount);
    }
    if intent.amount.decimals != source.usdc_decimals
        || intent.max_fee.decimals != source.usdc_decimals
    {
        return Err(ValidationError::DecimalsMismatch);
    }
    let max_fee = parse_raw_amount(&intent.max_fee.raw)?;
    if max_fee > amount {
        return Err(ValidationError::FeeExceedsAmount);
    }

    if !is_evm_address(&intent.position_owner)
        || !is_address_of_kind(&intent.recipient, destination.kind)
        || !is_address_of_kind(&intent.refund_recipient, source.kind)
        || intent
            .market_scope
            .as_deref()
            .is_some_and(|scope| !is_bytes32(scope))
    {
        return Err(ValidationError::InvalidAddress);
    }

    if intent.action_deadline <= intent.created_at || intent.delivery_timeout_secs == 0 {
        return Err(ValidationError::InvalidDeadline);
    }
    if !is_uint256_decimal(&intent.nonce) {
        return Err(ValidationError::InvalidNonce);
    }
    Ok(intent)
}

fn check_route(
    intent: &Intent,
    source: &Network,
    destination: &Network,
) -> Result<(), ValidationError> {
    let same_network = source.id == destination.id;
    if intent.action == Action::Transfer {
        if intent.protocol.is_some() || intent.market_scope.is_some() || same_network {
            return Err(ValidationError::InvalidRoute);
        }
        return Ok(());
    }

    // Protocol action: needs a protocol + market, and executes on an EVM chain.
    if intent.protocol.is_none() || intent.market_scope.is_none() {
        return Err(ValidationError::InvalidRoute);
    }
    // Cross-chain legs in scope are Stellar <-> EVM only.
    if !same_network && source.kind == destination.kind {
        return Err(ValidationError::InvalidRoute);
    }
    let execution_ok = if intent.action.funds_destination() {
        destination.kind == NetworkKind::Evm
    } else if intent.action.drains_source() {
        source.kind == NetworkKind::Evm
    } else {
        false
    };
    if !execution_ok {
        return Err(ValidationError::InvalidRoute);
    }
    Ok(())
}

/// Validate a batch: every intent individually, then duplicates by
/// `intentId` and by `(positionOwner, nonce)`.
pub fn validate_batch(values: &[Value]) -> Result<Vec<Intent>, ValidationError> {
    let mut ids = BTreeSet::new();
    let mut nonces = BTreeSet::new();
    let mut intents = Vec::with_capacity(values.len());
    for value in values {
        let intent = validate_intent(value)?;
        let owner_nonce = (
            intent.position_owner.to_ascii_lowercase(),
            intent.nonce.clone(),
        );
        if !ids.insert(intent.intent_id.clone()) || !nonces.insert(owner_nonce) {
            return Err(ValidationError::DuplicateIntent);
        }
        intents.push(intent);
    }
    Ok(intents)
}
