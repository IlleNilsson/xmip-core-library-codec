//! Base 64 (RFC 4648 section 4), the standard alphabet, and base64url
//! (section 5), the alphabet a URL and a JOSE token take: bytes as text, and
//! the text back to bytes.
//!
//! An AS2 receipt carries its message integrity check in it, a WebSocket
//! handshake its accept key, and an SSH fingerprint its digest, unpadded.
//! Until 2026-09-24 each of the three wrote its own encoder, and AS2 a
//! decoder that took a `=` anywhere in the text; and the identity gates read
//! every Basic credential, Kerberos and NTLM token, SAML assertion and JWT
//! through a crate of their own. The two alphabets differ in two characters
//! and nothing else, so one encoder and one decoder serve both.

use crate::{CodecError, Result};

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Not a digit of the alphabet a [`values`] table was built from.
const NOT_A_DIGIT: u8 = 0xff;

/// Each byte's value in `alphabet`, or [`NOT_A_DIGIT`]: a lookup per
/// character, where a search of the alphabet was too slow for a payload at
/// a transport's ceiling (google-pub-sub's brim timed out, 2026-09-24).
const fn values(alphabet: &[u8; 64]) -> [u8; 256] {
    let mut table = [NOT_A_DIGIT; 256];
    let mut value: u8 = 0;
    while value < 64 {
        table[alphabet[value as usize] as usize] = value;
        value += 1;
    }
    table
}

const STANDARD_VALUES: [u8; 256] = values(STANDARD);
const URL_SAFE_VALUES: [u8; 256] = values(URL_SAFE);

/// `bytes` in base 64, padded with `=` to a multiple of four characters.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    padded(encoding(bytes, STANDARD))
}

/// `bytes` in base 64 without the padding, as SSH writes a fingerprint.
#[must_use]
pub fn encode_unpadded(bytes: &[u8]) -> String {
    encoding(bytes, STANDARD)
}

/// The bytes `text` spells in base 64, padded or not.
///
/// Strict, as RFC 4648 section 3.3 asks of a decoder: nothing outside the
/// alphabet — no line break, no space — padding only at the end and only as
/// much as the length needs, and no bits set past the last byte.
///
/// # Errors
/// A character outside the alphabet, padding in the wrong place, a length
/// no encoding has, or bits set past the last byte.
pub fn decode(text: &str) -> Result<Vec<u8>> {
    decoding(text, &STANDARD_VALUES, "base 64")
}

/// `bytes` in base64url, padded with `=` to a multiple of four characters.
#[must_use]
pub fn encode_url(bytes: &[u8]) -> String {
    padded(encoding(bytes, URL_SAFE))
}

/// `bytes` in base64url without the padding, as a JWS writes each part
/// (RFC 7515 section 2).
#[must_use]
pub fn encode_url_unpadded(bytes: &[u8]) -> String {
    encoding(bytes, URL_SAFE)
}

/// The bytes `text` spells in base64url, padded or not, as strictly as
/// [`decode`] reads base 64.
///
/// # Errors
/// As [`decode`].
pub fn decode_url(text: &str) -> Result<Vec<u8>> {
    decoding(text, &URL_SAFE_VALUES, "base64url")
}

fn padded(mut out: String) -> String {
    while !out.len().is_multiple_of(4) {
        out.push('=');
    }
    out
}

fn encoding(bytes: &[u8], alphabet: &[u8; 64]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (index, byte)| {
            word | (u32::from(*byte) << (16 - 8 * index))
        });
        for index in 0..=chunk.len() {
            let sextet = (word >> (18 - 6 * index)) & 0x3f;
            out.push(char::from(alphabet[sextet as usize]));
        }
    }
    out
}

fn decoding(text: &str, values: &[u8; 256], name: &str) -> Result<Vec<u8>> {
    // The text is quoted only while short: a refused payload of megabytes is
    // named by its length, not copied into the message.
    let refused = |why: &str| {
        let shown = if text.len() <= 80 {
            format!("{text:?}")
        } else {
            format!("{} characters", text.len())
        };
        CodecError::new(format!("not {name} ({why}): {shown}"))
    };
    let digits = text.trim_end_matches('=');
    let padding = text.len() - digits.len();
    if padding > 2 || (padding > 0 && !text.len().is_multiple_of(4)) {
        return Err(refused("the padding does not fit the length"));
    }
    if digits.len() % 4 == 1 {
        return Err(refused("a length no encoding has"));
    }
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.as_bytes().chunks(4) {
        let mut word = 0u32;
        for (index, digit) in chunk.iter().enumerate() {
            let value = values[usize::from(*digit)];
            if value == NOT_A_DIGIT {
                return Err(refused("a character outside the alphabet"));
            }
            word |= u32::from(value) << (18 - 6 * index);
        }
        let whole = chunk.len() - 1;
        let bytes = word.to_be_bytes();
        if bytes[1 + whole..].iter().any(|byte| *byte != 0) {
            return Err(refused("bits set past the last byte"));
        }
        out.extend_from_slice(&bytes[1..=whole]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648 section 10.
    const VECTORS: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn the_rfc_4648_vectors_encode_and_decode_padded_or_not() {
        for (text, encoded) in VECTORS {
            let bare = encoded.trim_end_matches('=');
            assert_eq!(encode(text.as_bytes()), encoded, "{text}");
            assert_eq!(encode_unpadded(text.as_bytes()), bare, "{text}");
            assert_eq!(decode(encoded).expect("padded"), text.as_bytes(), "{text}");
            assert_eq!(decode(bare).expect("unpadded"), text.as_bytes(), "{text}");
            // The alphabets differ only past `9`, which none of these reach.
            assert_eq!(encode_url(text.as_bytes()), encoded, "{text}");
            assert_eq!(encode_url_unpadded(text.as_bytes()), bare, "{text}");
            assert_eq!(decode_url(bare).expect("url"), text.as_bytes(), "{text}");
        }
    }

    #[test]
    fn the_url_alphabet_spells_62_and_63_as_minus_and_underscore() {
        // RFC 4648 section 5: 0xfb 0xff is `-_8` where section 4 has `+/8`.
        assert_eq!(encode(&[0xfb, 0xff]), "+/8=");
        assert_eq!(encode_url(&[0xfb, 0xff]), "-_8=");
        assert_eq!(encode_url_unpadded(&[0xfb, 0xff]), "-_8");
        assert_eq!(decode_url("-_8").expect("url"), [0xfb, 0xff]);
        assert!(
            decode_url("+/8=").is_err(),
            "the standard alphabet is not url"
        );
        assert!(decode("-_8=").is_err(), "the url alphabet is not standard");
    }

    #[test]
    fn every_byte_comes_back() {
        let every: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&every)).expect("round"), every);
        assert_eq!(
            decode_url(&encode_url_unpadded(&every)).expect("round"),
            every
        );
        assert!(encode(&every).contains('+') && encode(&every).contains('/'));
    }

    #[test]
    fn what_is_not_base_64_is_refused_by_reason() {
        for (bad, why) in [
            ("not base64!", "alphabet"),
            ("Zm9v\r\nYmFy", "alphabet"),
            ("Zm=9", "alphabet"),
            ("Zg===", "padding"),
            ("Zm8==", "padding"),
            ("Zm9vY", "length"),
            ("Zh==", "bits"),
            ("Zm9=", "bits"),
        ] {
            let error = decode(bad).expect_err(bad);
            assert!(error.message.contains(why), "{bad}: {}", error.message);
            let error = decode_url(bad).expect_err(bad);
            assert!(error.message.contains(why), "{bad}: {}", error.message);
        }
    }
}
