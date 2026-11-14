//! Text in the encodings the WHATWG Encoding Standard names — a legacy
//! partner's `windows-1252`, `iso-8859-1`, `shift_jis`, and UTF-16 — read
//! into UTF-8 and written from it, whole or a piece at a time.
//!
//! Strict both ways (ADR-0038, amendment 2026-09-26): bytes that are not the
//! encoding, and a character the encoding cannot write, are refused, never
//! replaced, because a replacement character is a payload silently changed.
//! A byte order mark is never read here: what reads one says which encoding
//! it means. The standard's labels name an encoding, and its "replacement"
//! encoding, which reads anything as one refusal, is named by none. The one
//! place `encoding_rs` is used: the XML reader's edge (`xml::decoded`) and
//! the Character Encoding prepare unit read and write through it. The
//! Unicode forms the standard does not write — UTF-16 out, UTF-32 both ways
//! — are [`unicode`](crate::unicode)'s.

use std::borrow::Cow;

use encoding_rs::{DecoderResult, EncoderResult, Encoding};

/// One of the standard's encodings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Charset(&'static Encoding);

impl Charset {
    /// UTF-8.
    pub const UTF_8: Self = Self(encoding_rs::UTF_8);
    /// UTF-16, little-endian.
    pub const UTF_16LE: Self = Self(encoding_rs::UTF_16LE);
    /// UTF-16, big-endian.
    pub const UTF_16BE: Self = Self(encoding_rs::UTF_16BE);

    /// The encoding `label` names, by any of the standard's labels, case
    /// and surrounding white space aside.
    #[must_use]
    pub fn named(label: &str) -> Option<Self> {
        Encoding::for_label_no_replacement(label.as_bytes()).map(Self)
    }

    /// Its name as the standard writes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.0.name()
    }

    /// Whether each of its characters is one byte.
    #[must_use]
    pub fn is_single_byte(self) -> bool {
        self.0.is_single_byte()
    }

    /// Whether text is written in it, not only read: the standard writes
    /// UTF-16 and a few others as another encoding.
    #[must_use]
    pub fn is_written(self) -> bool {
        self.0.output_encoding() == self.0
    }

    /// `bytes` read whole, `None` where they are not this encoding; UTF-8
    /// that is valid is borrowed as it is.
    #[must_use]
    pub fn decode(self, bytes: &[u8]) -> Option<Cow<'_, str>> {
        self.0
            .decode_without_bom_handling_and_without_replacement(bytes)
    }

    /// A reading a piece at a time.
    #[must_use]
    pub fn decoder(self) -> Decoder {
        Decoder(self.0.new_decoder_without_bom_handling())
    }

    /// A writing a piece at a time.
    #[must_use]
    pub fn encoder(self) -> Encoder {
        Encoder(self.0.new_encoder())
    }
}

/// What a piece read came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoded {
    /// The piece is used up.
    InputEmpty,
    /// The room given is full; give more and the rest of the piece.
    OutputFull,
    /// Bytes that are not the encoding end this many bytes back.
    Malformed(u8),
}

/// What a piece written came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoded {
    /// The piece is used up.
    InputEmpty,
    /// The room given is full; give more and the rest of the piece.
    OutputFull,
    /// A character the encoding cannot write.
    Unmappable(char),
}

/// The standard's encoding read into UTF-8 a piece at a time.
pub struct Decoder(encoding_rs::Decoder);

impl Decoder {
    /// `bytes` read into `utf8`, `last` where no piece follows: what it came
    /// to, how many bytes it read and how many it wrote.
    pub fn decode(&mut self, bytes: &[u8], utf8: &mut [u8], last: bool) -> (Decoded, usize, usize) {
        let (result, read, written) = self.0.decode_to_utf8_without_replacement(bytes, utf8, last);
        let step = match result {
            DecoderResult::InputEmpty => Decoded::InputEmpty,
            DecoderResult::OutputFull => Decoded::OutputFull,
            DecoderResult::Malformed(length, _) => Decoded::Malformed(length),
        };
        (step, read, written)
    }
}

/// UTF-8 written in the standard's encoding a piece at a time.
pub struct Encoder(encoding_rs::Encoder);

impl Encoder {
    /// `text` written into `bytes`, `last` where no piece follows: what it
    /// came to, how many bytes of `text` it read and how many it wrote.
    pub fn encode(&mut self, text: &str, bytes: &mut [u8], last: bool) -> (Encoded, usize, usize) {
        let (result, read, written) = self
            .0
            .encode_from_utf8_without_replacement(text, bytes, last);
        let step = match result {
            EncoderResult::InputEmpty => Encoded::InputEmpty,
            EncoderResult::OutputFull => Encoded::OutputFull,
            EncoderResult::Unmappable(character) => Encoded::Unmappable(character),
        };
        (step, read, written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_names_an_encoding_and_the_replacement_encoding_is_named_by_none() {
        let latin = Charset::named(" ISO-8859-1 ").expect("named");
        assert_eq!(latin.name(), "windows-1252");
        assert!(latin.is_single_byte() && latin.is_written());
        assert!(!Charset::named("utf-16").expect("named").is_written());
        assert_eq!(Charset::named("iso-2022-kr"), None);
        assert_eq!(Charset::named("klingon"), None);
    }

    #[test]
    fn text_is_read_and_written_strictly_whole_and_in_pieces() {
        let latin = Charset::named("windows-1252").expect("named");
        assert_eq!(latin.decode(b"caf\xE9").expect("read"), "café");
        assert!(Charset::UTF_8.decode(b"ok\xFF").is_none());
        assert!(matches!(
            Charset::UTF_8.decode(b"ok"),
            Some(Cow::Borrowed("ok"))
        ));
        let mut utf8 = [0u8; 16];
        let (step, read, written) = latin.decoder().decode(b"\x80", &mut utf8, true);
        assert_eq!(
            (step, read, &utf8[..written]),
            (Decoded::InputEmpty, 1, "€".as_bytes())
        );
        let mut bytes = [0u8; 16];
        let (step, _, _) = latin.encoder().encode("日", &mut bytes, true);
        assert_eq!(step, Encoded::Unmappable('日'));
        let (step, _, _) = Charset::UTF_8.decoder().decode(b"\xFF", &mut utf8, true);
        assert!(matches!(step, Decoded::Malformed(_)));
    }
}
