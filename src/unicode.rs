//! Unicode's encoding forms: UTF-8, UTF-16 and UTF-32, the last two in
//! either byte order, each named by the word a setting declares it with.
//!
//! ADR-0038, amendment 2026-09-26: a payload is bytes, and text is read
//! from it only where a Contract, a transform or a technology's settings
//! declare text — in one of these forms, UTF-8 where none is named. Both
//! directions are strict: bytes that are not the declared form are
//! refused, never repaired, because a replacement character is a payload
//! silently changed.
//!
//! UTF-16 is [`crate::utf16`]'s; UTF-32 is written here, since nothing
//! else in the estate speaks it.

use crate::{CodecError, Result};

/// One of Unicode's encoding forms, byte order included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Form {
    /// UTF-8, the default wherever no form is declared.
    #[default]
    Utf8,
    /// UTF-16, low byte first.
    Utf16Le,
    /// UTF-16, high byte first.
    Utf16Be,
    /// UTF-32, low byte first.
    Utf32Le,
    /// UTF-32, high byte first.
    Utf32Be,
}

impl Form {
    /// Every form, in the order a setting lists their words.
    pub const ALL: [Self; 5] = [
        Self::Utf8,
        Self::Utf16Le,
        Self::Utf16Be,
        Self::Utf32Le,
        Self::Utf32Be,
    ];

    /// The words a setting may name a form by, in [`Form::ALL`]'s order:
    /// what a setting's choices list.
    pub const WORDS: [&'static str; 5] = [
        Self::ALL[0].word(),
        Self::ALL[1].word(),
        Self::ALL[2].word(),
        Self::ALL[3].word(),
        Self::ALL[4].word(),
    ];

    /// The form's word, as a setting names it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Utf32Le => "utf-32le",
            Self::Utf32Be => "utf-32be",
        }
    }

    /// The form `word` names, exactly as [`Form::word`] writes it.
    ///
    /// # Errors
    /// A word that names no form.
    pub fn named(word: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|form| form.word() == word)
            .ok_or_else(|| {
                CodecError::new(format!(
                    "{word:?} is not a Unicode encoding form: one of {}",
                    Self::WORDS.join(", ")
                ))
            })
    }

    /// `text` as this form's bytes.
    #[must_use]
    pub fn encode(self, text: &str) -> Vec<u8> {
        match self {
            Self::Utf8 => text.as_bytes().to_vec(),
            Self::Utf16Le => crate::utf16::encode(text),
            Self::Utf16Be => crate::utf16::encode_be(text),
            Self::Utf32Le => text
                .chars()
                .flat_map(|c| u32::from(c).to_le_bytes())
                .collect(),
            Self::Utf32Be => text
                .chars()
                .flat_map(|c| u32::from(c).to_be_bytes())
                .collect(),
        }
    }

    /// The text `bytes` spell in this form, strictly.
    ///
    /// # Errors
    /// Bytes that are not this form: an invalid sequence, a length the
    /// code unit does not divide, an unpaired surrogate, a code point past
    /// U+10FFFF.
    pub fn decode(self, bytes: &[u8]) -> Result<String> {
        match self {
            Self::Utf8 => std::str::from_utf8(bytes)
                .map(str::to_owned)
                .map_err(|error| CodecError::new(format!("not UTF-8: {error}"))),
            Self::Utf16Le => crate::utf16::decode(bytes),
            Self::Utf16Be => crate::utf16::decode_be(bytes),
            Self::Utf32Le => utf32(bytes, u32::from_le_bytes),
            Self::Utf32Be => utf32(bytes, u32::from_be_bytes),
        }
    }
}

fn utf32(bytes: &[u8], unit: fn([u8; 4]) -> u32) -> Result<String> {
    let (units, rest) = bytes.as_chunks::<4>();
    if !rest.is_empty() {
        return Err(CodecError::new(format!(
            "{} bytes are not UTF-32: a code unit is four",
            bytes.len()
        )));
    }
    units
        .iter()
        .map(|four| {
            let point = unit(*four);
            char::from_u32(point).ok_or_else(|| {
                CodecError::new(format!("not UTF-32: {point:#x} is not a scalar value"))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_round_trips_text_and_is_named_by_its_word() {
        for form in Form::ALL {
            for text in ["", "orders", "Jürgen", "\u{1F600}"] {
                assert_eq!(form.decode(&form.encode(text)).expect("round"), text);
            }
            assert_eq!(Form::named(form.word()).expect("named"), form);
        }
        assert_eq!(Form::default(), Form::Utf8);
        assert_eq!(Form::Utf32Be.encode("A"), [0, 0, 0, 0x41]);
        assert_eq!(Form::Utf32Le.encode("A"), [0x41, 0, 0, 0]);
    }

    #[test]
    fn bytes_that_are_not_the_form_are_refused_never_repaired() {
        let refused = Form::Utf8.decode(&[0xff, 0xfe]).expect_err("not UTF-8");
        assert!(refused.message.contains("UTF-8"), "{refused}");
        assert!(Form::Utf16Le.decode(&[0x41]).is_err());
        assert!(Form::Utf32Le.decode(&[0x41, 0, 0]).is_err(), "not four");
        assert!(
            Form::Utf32Be.decode(&[0, 0, 0xd8, 0]).is_err(),
            "a surrogate"
        );
        assert!(
            Form::Utf32Le.decode(&[0, 0, 0x11, 0]).is_err(),
            "past U+10FFFF"
        );
        assert!(Form::named("latin-1").is_err());
    }
}
