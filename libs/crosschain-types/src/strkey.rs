//! Stellar strkey codec (SEP-23) for the recipient kinds CCTP routing uses:
//! G (ed25519 account), C (contract), M (muxed account).
//!
//! Layout: base32(version byte || payload || crc16-xmodem LE), RFC 4648
//! alphabet, no padding. Decoding is strict: canonical length, zero trailing
//! bits, valid checksum, and a supported version byte.

use alloc::{string::String, vec::Vec};

use crate::cctp::CodecError;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

const VERSION_ACCOUNT: u8 = 6 << 3; // 'G'
const VERSION_CONTRACT: u8 = 2 << 3; // 'C'
const VERSION_MUXED: u8 = 12 << 3; // 'M'

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrkeyKind {
    Account,
    Contract,
    Muxed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Strkey {
    pub kind: StrkeyKind,
    /// ed25519 public key (G, M) or contract id (C).
    pub key: [u8; 32],
    /// Muxed id (M only).
    pub muxed_id: Option<u64>,
}

fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn base32_decode(s: &str) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for c in s.bytes() {
        let value = ALPHABET
            .iter()
            .position(|&a| a == c)
            .ok_or(CodecError::InvalidStrkey)? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    // Canonical encodings leave fewer than 5 trailing bits, all zero.
    if bits >= 5 || buffer != 0 {
        return Err(CodecError::InvalidStrkey);
    }
    Ok(out)
}

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &byte in data {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Strictly decode a G, C or M strkey.
pub fn decode(s: &str) -> Result<Strkey, CodecError> {
    if s.len() != 56 && s.len() != 69 {
        return Err(CodecError::InvalidStrkey);
    }
    let raw = base32_decode(s)?;
    if raw.len() < 3 {
        return Err(CodecError::InvalidStrkey);
    }
    let (body, checksum) = raw.split_at(raw.len() - 2);
    let expected = crc16_xmodem(body).to_le_bytes();
    if checksum != expected {
        return Err(CodecError::InvalidStrkey);
    }
    let (version, payload) = (body[0], &body[1..]);
    let (kind, expected_len) = match version {
        VERSION_ACCOUNT => (StrkeyKind::Account, 32),
        VERSION_CONTRACT => (StrkeyKind::Contract, 32),
        VERSION_MUXED => (StrkeyKind::Muxed, 40),
        _ => return Err(CodecError::UnsupportedStrkeyKind),
    };
    if payload.len() != expected_len {
        return Err(CodecError::InvalidStrkey);
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&payload[..32]);
    let muxed_id = (kind == StrkeyKind::Muxed).then(|| {
        let mut id = [0u8; 8];
        id.copy_from_slice(&payload[32..40]);
        u64::from_be_bytes(id)
    });
    Ok(Strkey {
        kind,
        key,
        muxed_id,
    })
}

/// Encode a strkey. `muxed_id` is required for (and only used by) Muxed.
pub fn encode(kind: StrkeyKind, key: &[u8; 32], muxed_id: Option<u64>) -> String {
    let mut body = Vec::with_capacity(43);
    body.push(match kind {
        StrkeyKind::Account => VERSION_ACCOUNT,
        StrkeyKind::Contract => VERSION_CONTRACT,
        StrkeyKind::Muxed => VERSION_MUXED,
    });
    body.extend_from_slice(key);
    if kind == StrkeyKind::Muxed {
        body.extend_from_slice(&muxed_id.unwrap_or(0).to_be_bytes());
    }
    let checksum = crc16_xmodem(&body).to_le_bytes();
    body.extend_from_slice(&checksum);
    base32_encode(&body)
}
