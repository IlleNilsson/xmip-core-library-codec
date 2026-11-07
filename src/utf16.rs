//! UTF-16: two bytes a code unit, and the bytes back to text. Little-endian
//! is how the Windows protocols write it, low byte first; big-endian is
//! the other byte order Unicode names, for a Party that declares it.
//!
//! NTLM names its user, domain and workstation in it, SMB2 its paths and
//! file names, TDS (where it is called UCS-2) its login and its SQL. Until
//! 2026-09-24 the identity capability, the SMB transport and the SQL Server
//! transport each wrote the two by hand.
//!
//! Two readings, because the protocols need two: [`decode`] refuses bytes
//! that are not UTF-16 — an odd length, an unpaired surrogate — for a name
//! a gate decides on and for any payload; [`decode_lossy`] replaces an
//! unpaired surrogate with U+FFFD and drops an odd trailing byte, for a
//! protocol's own name that is only shown, never for a payload (ADR-0038).

use crate::{CodecError, Result};

/// `text` as UTF-16 code units, each little-endian.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// `text` as UTF-16 code units, each big-endian.
#[must_use]
pub fn encode_be(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// The text little-endian `bytes` spell, strictly.
///
/// # Errors
/// An odd number of bytes, or a surrogate without its pair.
pub fn decode(bytes: &[u8]) -> Result<String> {
    strictly(bytes, String::from_utf16le, u16::from_le_bytes)
}

/// The text big-endian `bytes` spell, strictly.
///
/// # Errors
/// An odd number of bytes, or a surrogate without its pair.
pub fn decode_be(bytes: &[u8]) -> Result<String> {
    strictly(bytes, String::from_utf16be, u16::from_be_bytes)
}

/// `bytes` read by the standard library's `read`; on a refusal, the
/// surrogate without its pair found again by `unit`, since the standard
/// library's error does not say which it was.
fn strictly(
    bytes: &[u8],
    read: fn(&[u8]) -> core::result::Result<String, std::string::FromUtf16Error>,
    unit: fn([u8; 2]) -> u16,
) -> Result<String> {
    let (units, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(CodecError::new(format!(
            "{} bytes are not UTF-16: a code unit is two",
            bytes.len()
        )));
    }
    read(bytes).map_err(|_| {
        let unpaired = char::decode_utf16(units.iter().map(|pair| unit(*pair)))
            .find_map(core::result::Result::err)
            .map_or(0, |error| error.unpaired_surrogate());
        CodecError::new(format!(
            "not UTF-16: the surrogate {unpaired:#06x} has no pair"
        ))
    })
}

/// The text little-endian `bytes` spell, each unpaired surrogate as U+FFFD
/// and an odd trailing byte dropped.
#[must_use]
pub fn decode_lossy(bytes: &[u8]) -> String {
    let even = bytes.len() & !1;
    String::from_utf16le_lossy(&bytes[..even])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_written_low_byte_first_and_read_back() {
        assert_eq!(encode("Az"), [0x41, 0, 0x7a, 0]);
        assert_eq!(encode("\u{1F600}"), [0x3d, 0xd8, 0x00, 0xde]);
        for text in ["", "orders.edi", "Jürgen", "\u{3000}", "\u{1F600}"] {
            assert_eq!(decode(&encode(text)).expect("UTF-16"), text);
            assert_eq!(decode_lossy(&encode(text)), text);
        }
    }

    #[test]
    fn the_strict_reading_refuses_what_the_lossy_one_repairs() {
        let odd = [0x41, 0x00, 0x42];
        assert!(decode(&odd).expect_err("odd").message.contains("two"));
        assert_eq!(decode_lossy(&odd), "A");

        let unpaired = [0x00, 0xd8, 0x41, 0x00];
        let refused = decode(&unpaired).expect_err("unpaired");
        assert!(refused.message.contains("0xd800"), "{refused}");
        assert_eq!(decode_lossy(&unpaired), "\u{FFFD}A");
    }

    #[test]
    fn big_endian_is_written_high_byte_first_and_read_back_strictly() {
        assert_eq!(encode_be("Az"), [0, 0x41, 0, 0x7a]);
        for text in ["", "Jürgen", "\u{1F600}"] {
            assert_eq!(decode_be(&encode_be(text)).expect("UTF-16BE"), text);
        }
        assert!(decode_be(&[0xd8, 0x00, 0x00, 0x41]).is_err(), "unpaired");
        assert!(decode_be(&[0x00]).is_err(), "odd");
    }
}
