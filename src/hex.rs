//! Base 16 (RFC 4648 section 8): bytes as hexadecimal digits, two a byte,
//! and the digits back to bytes.
//!
//! It is how a digest is written in a header or a log, how a DHCP option
//! line, an SSDP header, a SQL binary literal and a queue manager's
//! identifier spell bytes, and how an NT hash is copied out of a SAM. Until
//! 2026-09-24 twelve crates each wrote the two by hand, one of them
//! accepting `+f` as a byte because `u8::from_str_radix` takes a sign.

use crate::{CodecError, Result};

const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// `bytes` as lower-case hex pairs, two digits a byte and nothing between.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// The bytes `digits` spell, in either case; nothing else is a digit — no
/// sign, no space, no `0x`.
///
/// # Errors
/// An odd number of digits, or a character that is not a hex digit.
pub fn decode(digits: &str) -> Result<Vec<u8>> {
    let pairs = digits.as_bytes();
    if !pairs.len().is_multiple_of(2) {
        return Err(CodecError::new(format!(
            "an odd number of hex digits: {digits:?}"
        )));
    }
    pairs
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            value(pair[0])
                .zip(value(pair[1]))
                .map(|(high, low)| (high << 4) | low)
                .ok_or_else(|| CodecError::new(format!("not hex: {digits:?}")))
        })
        .collect()
}

/// The value of one hex digit.
const fn value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648 section 10, which writes base 16 in upper case.
    const VECTORS: [(&str, &str); 7] = [
        ("", ""),
        ("f", "66"),
        ("fo", "666F"),
        ("foo", "666F6F"),
        ("foob", "666F6F62"),
        ("fooba", "666F6F6261"),
        ("foobar", "666F6F626172"),
    ];

    #[test]
    fn the_rfc_4648_vectors_encode_in_lower_case_and_decode_in_either() {
        for (text, digits) in VECTORS {
            assert_eq!(encode(text.as_bytes()), digits.to_lowercase(), "{text}");
            assert_eq!(decode(digits).expect("upper"), text.as_bytes(), "{text}");
            assert_eq!(
                decode(&digits.to_lowercase()).expect("lower"),
                text.as_bytes()
            );
        }
    }

    #[test]
    fn every_byte_comes_back() {
        let every: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&every)).expect("round"), every);
        assert_eq!(encode(&[0, 0x7f, 0xff]), "007fff");
    }

    #[test]
    fn what_is_not_two_hex_digits_a_byte_is_refused_by_reason() {
        assert!(decode("abc").expect_err("odd").message.contains("odd"));
        for bad in ["zz", "+f", "-1", " f", "0x", "é0"] {
            let error = decode(bad).expect_err(bad);
            assert!(error.message.contains("hex"), "{bad}: {}", error.message);
        }
    }
}
