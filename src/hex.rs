//! Base 16 (RFC 4648 section 8): bytes as hexadecimal digits, two a byte,
//! and the digits back to bytes.
//!
//! It is how a digest is written in a header or a log, how a DHCP option
//! line, an SSDP header, a SQL binary literal and a queue manager's
//! identifier spell bytes, and how an NT hash is copied out of a SAM. Until
//! 2026-09-24 twelve crates each wrote the two by hand, one of them
//! accepting `+f` as a byte because `u8::from_str_radix` takes a sign.
//!
//! A number is spelled here too: [`number`] reads hex digits as one
//! integer, and [`prefixed_number`] the `0x7e8` a target names a bus
//! identifier, an object index or a node address in. Until 2026-09-28
//! eleven technologies read those with `from_str_radix`, each taking
//! `0x+7e8`.

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

/// The number `digits` spell in hex, either case, where `T` holds it;
/// nothing else is a digit — no sign, no space, no `0x`.
///
/// # Errors
/// No digits, a character that is not a hex digit, or a number wider than
/// `T`.
pub fn number<T: TryFrom<u64>>(digits: &str) -> Result<T> {
    let refused = || CodecError::new(format!("not a hex number: {digits:?}"));
    if digits.is_empty() {
        return Err(refused());
    }
    let mut number: u64 = 0;
    for digit in digits.bytes() {
        let digit = value(digit).ok_or_else(refused)?;
        number = number
            .checked_mul(16)
            .map(|shifted| shifted | u64::from(digit))
            .ok_or_else(|| CodecError::new(format!("a hex number too wide: {digits:?}")))?;
    }
    T::try_from(number).map_err(|_| CodecError::new(format!("a hex number too wide: {digits:?}")))
}

/// A number written `0x` and hex digits, as a target names an identifier,
/// an index or an address — `0x7e8` — read by [`number`].
///
/// # Errors
/// Text that does not open with `0x`, or digits [`number`] refuses.
pub fn prefixed_number<T: TryFrom<u64>>(text: &str) -> Result<T> {
    text.strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .ok_or_else(|| CodecError::new(format!("not 0x and a hex number: {text:?}")))
        .and_then(number)
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
    fn a_number_is_read_as_wide_as_its_type_and_never_with_a_sign() {
        assert_eq!(number::<u16>("7e8"), Ok(0x7e8));
        assert_eq!(number::<u32>("FFFFFFFF"), Ok(u32::MAX));
        assert_eq!(prefixed_number::<u16>("0x1a2B"), Ok(0x1a2b));
        assert_eq!(prefixed_number::<u8>("0X0f"), Ok(0x0f));
        assert_eq!(number::<u64>("000000000000000000001"), Ok(1));
        for bad in ["", "+f", "-1", " f", "0x1", "g"] {
            let error = number::<u32>(bad).expect_err(bad);
            assert!(error.message.contains("not a hex number"), "{bad}");
        }
        for bad in ["0x+f", "0x-1", "0x", "+0x1", "7e8", "x7"] {
            assert!(prefixed_number::<u32>(bad).is_err(), "{bad}");
        }
        assert!(
            number::<u8>("100")
                .expect_err("wide")
                .message
                .contains("too wide")
        );
        assert!(
            number::<u64>("1ffffffffffffffff").is_err(),
            "wider than 64 bits"
        );
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
