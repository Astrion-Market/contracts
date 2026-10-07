use alloc::string::String;

use crate::{network::NetworkKind, ValidationError, MAX_RAW_AMOUNT};

/// Parse a canonical raw amount: decimal digits, no sign, no leading zeros.
/// Zero is returned as Ok(0); callers decide whether zero is allowed.
pub fn parse_raw_amount(raw: &str) -> Result<u128, ValidationError> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ValidationError::InvalidAmount);
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return Err(ValidationError::InvalidAmount);
    }
    if raw.len() > 19 {
        return Err(ValidationError::AmountTooLarge);
    }
    let mut value: u128 = 0;
    for b in raw.bytes() {
        value = value * 10 + u128::from(b - b'0');
    }
    if value > MAX_RAW_AMOUNT {
        return Err(ValidationError::AmountTooLarge);
    }
    Ok(value)
}

/// Canonical uint256 decimal string (shape only; at most 78 digits).
pub fn is_uint256_decimal(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 78
        && raw.bytes().all(|b| b.is_ascii_digit())
        && !(raw.len() > 1 && raw.starts_with('0'))
}

pub fn is_evm_address(s: &str) -> bool {
    s.len() == 42 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Strkey shape check (prefix, length, base32 alphabet). Checksum
/// verification lives in the CCTP codec.
pub fn is_stellar_address(s: &str) -> bool {
    let len_ok = match s.as_bytes().first() {
        Some(b'G') | Some(b'C') => s.len() == 56,
        Some(b'M') => s.len() == 69,
        _ => false,
    };
    len_ok && s.bytes().all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b))
}

pub fn is_address_of_kind(s: &str, kind: NetworkKind) -> bool {
    match kind {
        NetworkKind::Evm => is_evm_address(s),
        NetworkKind::Stellar => is_stellar_address(s),
    }
}

/// Lowercase 0x-prefixed bytes32.
pub fn is_bytes32(s: &str) -> bool {
    s.len() == 66
        && s.starts_with("0x")
        && s[2..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(66);
    out.push_str("0x");
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
