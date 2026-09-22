#![forbid(unsafe_code)]

//! Encoding primitives every layer shares: how a format's characters and
//! bytes are written and read back, never what they mean.
//!
//! One copy of each, because copies drift: until 2026-09-22 the two SAML
//! gates unescaped one assertion two ways, so a `NameID` of `jane&#64;corp`
//! was one principal to the first gate and another to the second.

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
