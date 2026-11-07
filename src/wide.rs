//! A wide string: text as UTF-16 code units in the machine's own order,
//! ended by one NUL — how the Windows API takes text and gives it back.
//!
//! The API reads a wide string up to its first NUL, so text holding one
//! would reach it cut short: [`encode`] and [`terminated`] refuse it rather
//! than pass on less than they were given, and a [`Wide`] is only ever
//! made by them, so whoever hands its [`Wide::as_ptr`] to a call knows it
//! ends in its NUL. [`decode_lossy`] reads to the first NUL, or the end
//! where there is none, each unpaired surrogate as U+FFFD, for the names
//! and text Windows hands back.
//!
//! Until 2026-11-07 the named pipe wrote its paths through the
//! `widestring` crate, and the Event Log writer, the private file, the
//! runtime's directory watch and its tests each ended their text with a
//! NUL by hand, none of them refusing a NUL inside it. A wide string Windows wrote at a pointer is
//! still measured to its NUL by the caller's own unsafe code, since this
//! crate writes none, and read here once measured.

use crate::{CodecError, Result};

/// A wide string: code units ended by one NUL, and holding no other.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Wide(Vec<u16>);

impl Wide {
    /// The code units, the NUL that ends them included.
    #[must_use]
    pub fn units(&self) -> &[u16] {
        &self.0
    }

    /// Where the code units begin, as a Windows call takes a wide string:
    /// valid while this lives, and read up to the NUL that ends it.
    #[must_use]
    pub fn as_ptr(&self) -> *const u16 {
        self.0.as_ptr()
    }
}

/// `text` as a wide string, ended by its NUL.
///
/// # Errors
/// `text` holds a NUL, where Windows would read it cut short.
pub fn encode(text: &str) -> Result<Wide> {
    terminated(text.encode_utf16())
}

/// The code `units` — a path's, as `OsStrExt::encode_wide` gives them on
/// Windows — as a wide string, ended by its NUL.
///
/// # Errors
/// A NUL among `units`, where Windows would read them cut short.
pub fn terminated(units: impl IntoIterator<Item = u16>) -> Result<Wide> {
    let mut wide: Vec<u16> = units.into_iter().collect();
    if let Some(at) = wide.iter().position(|&unit| unit == 0) {
        return Err(CodecError::new(format!(
            "not a wide string: a NUL at code unit {at} would end it there"
        )));
    }
    wide.push(0);
    Ok(Wide(wide))
}

/// The text `units` spell up to their first NUL, or all of them where
/// there is none, each unpaired surrogate as U+FFFD.
#[must_use]
pub fn decode_lossy(units: &[u16]) -> String {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        encode(text).expect("wide").units().to_vec()
    }

    #[test]
    fn text_is_written_as_utf_16_ended_by_one_nul() {
        assert_eq!(units("Xö"), [0x58, 0xf6, 0]);
        assert_eq!(units(""), [0]);
        assert_eq!(units("\u{1F600}"), [0xd83d, 0xde00, 0]);
        let path = terminated([0x41, 0x42]).expect("wide");
        assert_eq!(path.units(), [0x41, 0x42, 0]);
        assert_eq!(path.as_ptr(), path.units().as_ptr());
    }

    #[test]
    fn a_nul_inside_the_text_is_refused() {
        let refused = encode("a\0b").expect_err("interior NUL");
        assert!(refused.message.contains("code unit 1"), "{refused}");
        assert!(
            terminated([0x41, 0]).is_err(),
            "a trailing NUL is inside too"
        );
    }

    #[test]
    fn a_wide_string_is_read_to_its_first_nul_or_its_end() {
        for text in ["", "Jürgen", "S-1-5-32-545", "\u{1F600}"] {
            assert_eq!(decode_lossy(&units(text)), text);
        }
        assert_eq!(decode_lossy(&[0x41, 0, 0x42]), "A");
        assert_eq!(decode_lossy(&[0x41, 0x42]), "AB");
        assert_eq!(decode_lossy(&[0xd800, 0x41, 0]), "\u{FFFD}A");
    }
}
