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
//! Characters: [`char_reader`] is the character reader every lexer walks
//! its text with; [`toml`] quotes and reads TOML basic strings; [`xml`]
//! escapes and unescapes XML character data; [`percent`] writes and reads
//! RFC 3986's percent-encoding; [`mime`] reads a header's
//! media type and parameters and writes and reads a multipart body.
//!
//! Bytes: [`cursor`] reads a binary message's fields in order and
//! [`writer`] writes them; [`field`] is a compact record's counted bytes
//! and text, absent values and runs; [`form`] is a value's one binary form,
//! written and read over them; [`varint`] is the base-128 varint and its
//! zig-zag; [`hex`] and [`base64`] spell bytes as text; [`unicode`] is
//! text in the encoding form a setting declares, strictly both ways;
//! [`utf16`] is text as the Windows protocols write it; [`wide`] is text
//! as the Windows API takes it, ended by a NUL; [`crc`] is the one
//! parameterised CRC with each protocol's as a named constant; [`sha1`] is
//! the digest two protocols still name; [`constant_time`] compares a
//! secret without saying where it differs.
//!
//! Structure: [`depth`] is the one cap on how deep a recursive reader goes
//! before the bytes it is given are refused as hostile.
//!
//! Randomness: [`random`] fills bytes from the operating system's random
//! source, the one place a key, salt, nonce, challenge or id is drawn.
//!
//! Time: [`civil`] turns seconds since the epoch into a UTC date and time
//! of day and back, and reads RFC 3339 dates, times and offsets.

pub mod base64;
pub mod ceiling;
pub mod char_reader;
pub mod civil;
pub mod constant_time;
pub mod crc;
pub mod cursor;
pub mod depth;
pub mod field;
pub mod form;
pub mod hex;
pub mod mime;
pub mod percent;
pub mod random;
pub mod sha1;
pub mod toml;
pub mod unicode;
pub mod utf16;
pub mod varint;
pub mod wide;
pub mod writer;
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
