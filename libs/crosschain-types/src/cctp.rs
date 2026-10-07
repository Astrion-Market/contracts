//! Circle CCTP V2 codec for Stellar <-> EVM USDC routes.
//!
//! Sources (checked 2026-10-07):
//! - https://developers.circle.com/cctp/references/stellar
//! - https://developers.circle.com/cctp/references/stellar-contracts
//! - https://developers.circle.com/cctp/quickstarts/transfer-usdc-stellar-arc
//!
//! Facts this module encodes:
//! - Stellar USDC has 7 decimals; CCTP message amounts have 6. On Stellar,
//!   `deposit_for_burn` takes `amount` and `max_fee` in 7-decimal subunits and
//!   burns only through the 6th decimal; the 7th-decimal remainder stays.
//!   Minting on Stellar scales the 6-decimal amount by 10.
//! - Inbound to Stellar MUST set both `mintRecipient` and `destinationCaller`
//!   to the `CctpForwarder` contract, with the final strkey in hook data.
//!   Otherwise funds are permanently stuck.
//! - Forwarder hook data: bytes24 zero magic || u32 BE version (0) ||
//!   u32 BE length L || L bytes of strkey (UTF-8) || optional payload.
//! - Stellar addresses in bytes32 fields are raw 32-byte contract ids; the
//!   type is not encoded, so only contracts (C...) may appear there.

use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::strkey::{self, StrkeyKind};

/// Stellar's CCTP domain.
pub const STELLAR_DOMAIN: u32 = 27;
/// MessageV2 and BurnMessageV2 versions this codec accepts.
pub const MESSAGE_VERSION: u32 = 1;
pub const BURN_MESSAGE_VERSION: u32 = 1;
/// Forwarder hook version.
pub const HOOK_VERSION: u32 = 0;
/// Stellar classic-asset limit (i64::MAX) bounds every amount.
pub const MAX_STELLAR_AMOUNT: u128 = i64::MAX as u128;

const HEADER_LEN: usize = 148;
const BURN_BODY_LEN: usize = 228;
const HOOK_HEADER_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    InvalidStrkey,
    UnsupportedStrkeyKind,
    NotAContract,
    ZeroAmount,
    DustOnly,
    AmountTooLarge,
    FeeExceedsAmount,
    NotAnEvmAddress,
    MalformedHook,
    MalformedMessage,
    BadForwarderFields,
}

impl CodecError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidStrkey => "InvalidStrkey",
            Self::UnsupportedStrkeyKind => "UnsupportedStrkeyKind",
            Self::NotAContract => "NotAContract",
            Self::ZeroAmount => "ZeroAmount",
            Self::DustOnly => "DustOnly",
            Self::AmountTooLarge => "AmountTooLarge",
            Self::FeeExceedsAmount => "FeeExceedsAmount",
            Self::NotAnEvmAddress => "NotAnEvmAddress",
            Self::MalformedHook => "MalformedHook",
            Self::MalformedMessage => "MalformedMessage",
            Self::BadForwarderFields => "BadForwarderFields",
        }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─── amounts ──────────────────────────────────────────────────────────────────

/// Stellar 7-decimal amount -> (CCTP 6-decimal burned amount, 7-decimal dust
/// that stays on Stellar).
pub fn stellar_to_cctp(stellar_raw: u128) -> Result<(u128, u128), CodecError> {
    if stellar_raw == 0 {
        return Err(CodecError::ZeroAmount);
    }
    if stellar_raw > MAX_STELLAR_AMOUNT {
        return Err(CodecError::AmountTooLarge);
    }
    let burned = stellar_raw / 10;
    if burned == 0 {
        return Err(CodecError::DustOnly);
    }
    Ok((burned, stellar_raw % 10))
}

/// CCTP 6-decimal amount -> Stellar 7-decimal amount minted.
pub fn cctp_to_stellar(cctp_raw: u128) -> Result<u128, CodecError> {
    if cctp_raw == 0 {
        return Err(CodecError::ZeroAmount);
    }
    let scaled = cctp_raw.checked_mul(10).ok_or(CodecError::AmountTooLarge)?;
    if scaled > MAX_STELLAR_AMOUNT {
        return Err(CodecError::AmountTooLarge);
    }
    Ok(scaled)
}

// ─── addresses ────────────────────────────────────────────────────────────────

/// Left-pad an EVM address into a CCTP bytes32 field.
pub fn evm_address_to_bytes32(address: &[u8; 20]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(address);
    out
}

/// Recover an EVM address; the upper 12 bytes must be zero.
pub fn bytes32_to_evm_address(value: &[u8; 32]) -> Result<[u8; 20], CodecError> {
    if value[..12].iter().any(|&b| b != 0) {
        return Err(CodecError::NotAnEvmAddress);
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&value[12..]);
    Ok(out)
}

/// Contract strkey (C...) -> raw 32-byte id for a CCTP bytes32 field.
/// Accounts and muxed accounts are rejected: CCTP cannot express them.
pub fn contract_to_bytes32(contract: &str) -> Result<[u8; 32], CodecError> {
    let key = strkey::decode(contract)?;
    if key.kind != StrkeyKind::Contract {
        return Err(CodecError::NotAContract);
    }
    Ok(key.key)
}

/// Raw 32-byte contract id -> contract strkey.
pub fn bytes32_to_contract(value: &[u8; 32]) -> String {
    strkey::encode(StrkeyKind::Contract, value, None)
}

// ─── forwarder hook data ──────────────────────────────────────────────────────

/// Build `CctpForwarder` hook data for a final Stellar recipient (G, C or M).
pub fn forwarder_hook(recipient: &str, payload: &[u8]) -> Result<Vec<u8>, CodecError> {
    strkey::decode(recipient)?;
    let recipient = recipient.as_bytes();
    let mut out = Vec::with_capacity(HOOK_HEADER_LEN + recipient.len() + payload.len());
    out.extend_from_slice(&[0u8; 24]);
    out.extend_from_slice(&HOOK_VERSION.to_be_bytes());
    out.extend_from_slice(&(recipient.len() as u32).to_be_bytes());
    out.extend_from_slice(recipient);
    out.extend_from_slice(payload);
    Ok(out)
}

/// Parse forwarder hook data -> (validated recipient strkey, payload).
pub fn parse_forwarder_hook(hook: &[u8]) -> Result<(String, &[u8]), CodecError> {
    if hook.len() < HOOK_HEADER_LEN || hook[..24].iter().any(|&b| b != 0) {
        return Err(CodecError::MalformedHook);
    }
    if read_u32(hook, 24) != HOOK_VERSION {
        return Err(CodecError::MalformedHook);
    }
    let len = read_u32(hook, 28) as usize;
    let end = HOOK_HEADER_LEN
        .checked_add(len)
        .filter(|&end| end <= hook.len())
        .ok_or(CodecError::MalformedHook)?;
    let recipient =
        core::str::from_utf8(&hook[HOOK_HEADER_LEN..end]).map_err(|_| CodecError::MalformedHook)?;
    strkey::decode(recipient)?;
    Ok((String::from(recipient), &hook[end..]))
}

// ─── raw message decoding (MessageV2 + BurnMessageV2) ────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BurnMessage {
    pub version: u32,
    pub burn_token: [u8; 32],
    pub mint_recipient: [u8; 32],
    pub amount: u128,
    pub message_sender: [u8; 32],
    pub max_fee: u128,
    pub fee_executed: u128,
    pub expiration_block: u128,
    pub hook_data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub version: u32,
    pub source_domain: u32,
    pub destination_domain: u32,
    pub nonce: [u8; 32],
    pub sender: [u8; 32],
    pub recipient: [u8; 32],
    pub destination_caller: [u8; 32],
    pub min_finality_threshold: u32,
    pub finality_threshold_executed: u32,
    pub burn: BurnMessage,
}

fn read_u32(data: &[u8], at: usize) -> u32 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&data[at..at + 4]);
    u32::from_be_bytes(b)
}

fn read_b32(data: &[u8], at: usize) -> [u8; 32] {
    let mut b = [0u8; 32];
    b.copy_from_slice(&data[at..at + 32]);
    b
}

/// uint256 -> u128; values that do not fit are rejected.
fn read_u256(data: &[u8], at: usize) -> Result<u128, CodecError> {
    if data[at..at + 16].iter().any(|&b| b != 0) {
        return Err(CodecError::MalformedMessage);
    }
    let mut b = [0u8; 16];
    b.copy_from_slice(&data[at + 16..at + 32]);
    Ok(u128::from_be_bytes(b))
}

/// Decode a raw CCTP V2 burn message (use this when API address fields are null).
pub fn decode_message(data: &[u8]) -> Result<Message, CodecError> {
    if data.len() < HEADER_LEN + BURN_BODY_LEN {
        return Err(CodecError::MalformedMessage);
    }
    let version = read_u32(data, 0);
    let body = &data[HEADER_LEN..];
    let burn_version = read_u32(body, 0);
    if version != MESSAGE_VERSION || burn_version != BURN_MESSAGE_VERSION {
        return Err(CodecError::MalformedMessage);
    }
    let burn = BurnMessage {
        version: burn_version,
        burn_token: read_b32(body, 4),
        mint_recipient: read_b32(body, 36),
        amount: read_u256(body, 68)?,
        message_sender: read_b32(body, 100),
        max_fee: read_u256(body, 132)?,
        fee_executed: read_u256(body, 164)?,
        expiration_block: read_u256(body, 196)?,
        hook_data: body[BURN_BODY_LEN..].to_vec(),
    };
    if burn.fee_executed > burn.amount {
        return Err(CodecError::MalformedMessage);
    }
    Ok(Message {
        version,
        source_domain: read_u32(data, 4),
        destination_domain: read_u32(data, 8),
        nonce: read_b32(data, 12),
        sender: read_b32(data, 44),
        recipient: read_b32(data, 76),
        destination_caller: read_b32(data, 108),
        min_finality_threshold: read_u32(data, 140),
        finality_threshold_executed: read_u32(data, 144),
        burn,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboundToStellar {
    pub forward_recipient: String,
    pub hook_payload: Vec<u8>,
    /// 7-decimal amount the forwarder delivers: (amount - feeExecuted) * 10.
    pub minted_stellar_amount: u128,
}

/// Validate an inbound message against the expected `CctpForwarder`.
pub fn inbound_to_stellar(
    message: &Message,
    forwarder: &str,
) -> Result<InboundToStellar, CodecError> {
    let forwarder = contract_to_bytes32(forwarder)?;
    if message.destination_domain != STELLAR_DOMAIN
        || message.burn.mint_recipient != forwarder
        || message.destination_caller != forwarder
    {
        return Err(CodecError::BadForwarderFields);
    }
    let (forward_recipient, payload) = parse_forwarder_hook(&message.burn.hook_data)?;
    let net = message.burn.amount - message.burn.fee_executed;
    Ok(InboundToStellar {
        forward_recipient,
        hook_payload: payload.to_vec(),
        minted_stellar_amount: cctp_to_stellar(net)?,
    })
}

// ─── outbound builders ────────────────────────────────────────────────────────

/// Arguments for Stellar `TokenMessengerMinter.deposit_for_burn`
/// (amount and max_fee in 7-decimal Stellar subunits).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StellarBurn {
    pub amount: u128,
    pub destination_domain: u32,
    pub mint_recipient: [u8; 32],
    pub destination_caller: [u8; 32],
    pub max_fee: u128,
    pub min_finality_threshold: u32,
    /// What the message will carry (6 decimals) and what stays on Stellar.
    pub expected_burned: u128,
    pub retained_dust: u128,
}

/// Build a Stellar -> EVM burn that mints to `evm_recipient` (the user's
/// scoped execution account). `destination_caller` may restrict who can
/// complete the mint (zero = anyone).
pub fn stellar_burn(
    amount: u128,
    destination_domain: u32,
    evm_recipient: &[u8; 20],
    destination_caller: Option<&[u8; 20]>,
    max_fee: u128,
    min_finality_threshold: u32,
) -> Result<StellarBurn, CodecError> {
    if destination_domain == STELLAR_DOMAIN {
        return Err(CodecError::BadForwarderFields);
    }
    let (expected_burned, retained_dust) = stellar_to_cctp(amount)?;
    if max_fee >= amount {
        return Err(CodecError::FeeExceedsAmount);
    }
    if *evm_recipient == [0u8; 20] {
        return Err(CodecError::NotAnEvmAddress);
    }
    Ok(StellarBurn {
        amount,
        destination_domain,
        mint_recipient: evm_address_to_bytes32(evm_recipient),
        destination_caller: destination_caller
            .map(evm_address_to_bytes32)
            .unwrap_or([0u8; 32]),
        max_fee,
        min_finality_threshold,
        expected_burned,
        retained_dust,
    })
}

/// Arguments for EVM `TokenMessengerV2.depositForBurnWithHook` to Stellar
/// (6-decimal amounts). Both bytes32 fields are the forwarder by construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvmBurnToStellar {
    pub amount: u128,
    pub destination_domain: u32,
    pub mint_recipient: [u8; 32],
    pub destination_caller: [u8; 32],
    pub max_fee: u128,
    pub min_finality_threshold: u32,
    pub hook_data: Vec<u8>,
}

/// Build an EVM -> Stellar burn. Malformed destinations fail here, before any
/// burn is signed.
pub fn evm_burn_to_stellar(
    amount: u128,
    forwarder: &str,
    recipient: &str,
    max_fee: u128,
    min_finality_threshold: u32,
) -> Result<EvmBurnToStellar, CodecError> {
    let forwarder = contract_to_bytes32(forwarder)?;
    let hook_data = forwarder_hook(recipient, &[])?;
    if amount == 0 {
        return Err(CodecError::ZeroAmount);
    }
    if max_fee >= amount {
        return Err(CodecError::FeeExceedsAmount);
    }
    // The minted Stellar amount must also be representable.
    cctp_to_stellar(amount - max_fee)?;
    Ok(EvmBurnToStellar {
        amount,
        destination_domain: STELLAR_DOMAIN,
        mint_recipient: forwarder,
        destination_caller: forwarder,
        max_fee,
        min_finality_threshold,
        hook_data,
    })
}
