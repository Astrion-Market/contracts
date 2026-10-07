//! Per-leg receipt reconciliation (`spec/schemas/receipt.schema.json`).

use alloc::string::String;

use serde::Deserialize;
use serde_json::Value;

use crate::{
    intent::Amount,
    network::{network, NetworkKind},
    util::parse_raw_amount,
    ValidationError, SCHEMA_VERSION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegKind {
    CctpBurn,
    CctpMint,
    ProtocolAction,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CctpRef {
    pub source_domain: u32,
    pub destination_domain: u32,
    pub nonce: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub schema_version: u64,
    pub intent_id: String,
    pub kind: LegKind,
    pub network: String,
    pub tx_id: String,
    #[serde(default)]
    pub block_or_ledger: Option<u64>,
    pub finality: String,
    #[serde(default)]
    pub cctp: Option<CctpRef>,
    #[serde(default)]
    pub sent: Option<Amount>,
    #[serde(default)]
    pub burned: Option<Amount>,
    #[serde(default)]
    pub dust: Option<Amount>,
    #[serde(default)]
    pub fee: Option<Amount>,
    #[serde(default)]
    pub received: Option<Amount>,
}

/// CCTP message amounts always use 6 decimals.
const CCTP_DECIMALS: u8 = 6;

fn raw(amount: &Option<Amount>, decimals: u8) -> Result<u128, ValidationError> {
    let amount = amount
        .as_ref()
        .ok_or(ValidationError::ReconciliationMismatch)?;
    if amount.decimals != decimals {
        return Err(ValidationError::ReconciliationMismatch);
    }
    parse_raw_amount(&amount.raw)
}

/// Check a receipt's unit and amount reconciliation:
/// - burn: `burned * scale + dust == sent`, `dust < scale` (scale 10 on Stellar, 1 on EVM)
/// - mint: `received == (burned - fee) * scale`
/// - CCTP domains must match the leg's network.
pub fn validate_receipt(value: &Value) -> Result<Receipt, ValidationError> {
    let receipt: Receipt =
        serde_json::from_value(value.clone()).map_err(|_| ValidationError::Malformed)?;
    if receipt.schema_version != SCHEMA_VERSION {
        return Err(ValidationError::UnsupportedSchemaVersion);
    }
    let net = network(&receipt.network).ok_or(ValidationError::UnknownNetwork)?;
    let (local_decimals, scale): (u8, u128) = match net.kind {
        NetworkKind::Stellar => (7, 10),
        NetworkKind::Evm => (6, 1),
    };

    match receipt.kind {
        LegKind::CctpBurn => {
            let cctp = receipt.cctp.as_ref().ok_or(ValidationError::Malformed)?;
            if cctp.source_domain != net.cctp_domain {
                return Err(ValidationError::ReconciliationMismatch);
            }
            let sent = raw(&receipt.sent, local_decimals)?;
            let burned = raw(&receipt.burned, CCTP_DECIMALS)?;
            let dust = if scale == 1 && receipt.dust.is_none() {
                0
            } else {
                raw(&receipt.dust, local_decimals)?
            };
            if dust >= scale || burned * scale + dust != sent {
                return Err(ValidationError::ReconciliationMismatch);
            }
        }
        LegKind::CctpMint => {
            let cctp = receipt.cctp.as_ref().ok_or(ValidationError::Malformed)?;
            if cctp.destination_domain != net.cctp_domain {
                return Err(ValidationError::ReconciliationMismatch);
            }
            let burned = raw(&receipt.burned, CCTP_DECIMALS)?;
            let fee = raw(&receipt.fee, CCTP_DECIMALS)?;
            let received = raw(&receipt.received, local_decimals)?;
            let net_amount = burned
                .checked_sub(fee)
                .ok_or(ValidationError::ReconciliationMismatch)?;
            if received != net_amount * scale {
                return Err(ValidationError::ReconciliationMismatch);
            }
        }
        LegKind::ProtocolAction => {}
    }
    Ok(receipt)
}
