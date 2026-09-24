#![forbid(unsafe_code)]

//! Encoding primitives every layer shares: how a format's characters and
//! bytes are written and read back, never what they mean.
//!
//! One copy of each, because copies drift: until 2026-09-22 the two SAML
//! gates unescaped one assertion two ways, so a `NameID` of `jane&#64;corp`
//! was one principal to the first gate and another to the second; until
//! 2026-09-24 two path lexers stepped over whitespace a byte at a time and
//! panicked inside U+00A0, and two TOML writers quoted a string two ways.
//!
//! [`char_reader`] is the character reader every lexer walks its text with;
//! [`toml`] quotes and reads TOML basic strings; [`xml`] escapes and
//! unescapes XML character data.

pub mod char_reader;
pub mod toml;
pub mod xml;

/// Why text or bytes are not the encoding they claim to be.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodecError {
    /// What was wrong, in words.
    pub message: String,
}

impl CodecError {
    /// A failure saying `message`.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl core::fmt::Display for CodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for CodecError {}

/// The result of decoding.
pub type Result<T> = core::result::Result<T, CodecError>;
