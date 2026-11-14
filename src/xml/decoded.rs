//! XML in whatever encoding it is written in, read as the text every reader
//! of the estate reads: the encoding a byte order mark says, or the one the
//! XML declaration names, or UTF-8 where neither says (XML 1.0 appendix F).
//!
//! Integration traffic carries XML declared as ISO-8859-1, Windows-1252 or
//! UTF-16, and what Xmip replaces reads it. The bytes are decoded here, once,
//! at the reader's edge, in the WHATWG encodings (`codec::charset`) — where
//! ISO-8859-1 is read as Windows-1252, as every browser reads it — and
//! strictly: bytes that are wrong for the encoding are refused at the first
//! that is, never replaced. UTF-8 is borrowed as it is, its byte order mark
//! kept for the reader to set aside, so a position in it is a byte of the
//! input; in any other encoding a position counts in the text decoded.

use std::borrow::Cow;

use super::malformed::Malformed;
use crate::charset::Charset;

const UTF_8: Charset = Charset::UTF_8;
const UTF_16BE: Charset = Charset::UTF_16BE;
const UTF_16LE: Charset = Charset::UTF_16LE;

/// `bytes` as text, decoded from the encoding they say they are in.
///
/// # Errors
/// The encoding the declaration names is one this reader does not know, or
/// the bytes are wrong for their encoding.
pub fn decoded(bytes: &[u8]) -> Result<Cow<'_, str>, Malformed> {
    let (encoding, mark) = match bytes {
        [0xEF, 0xBB, 0xBF, ..] => (UTF_8, 0),
        [0xFE, 0xFF, ..] => (UTF_16BE, 2),
        [0xFF, 0xFE, ..] => (UTF_16LE, 2),
        [0x3C, 0x00, 0x3F, 0x00, ..] => (UTF_16LE, 0),
        [0x00, 0x3C, 0x00, 0x3F, ..] => (UTF_16BE, 0),
        _ => (declared(bytes)?, 0),
    };
    if encoding == UTF_8 {
        return std::str::from_utf8(bytes)
            .map(Cow::Borrowed)
            .map_err(|wrong| Malformed::new("not UTF-8", wrong.valid_up_to()));
    }
    let body = &bytes[mark..];
    encoding
        .decode(body)
        .ok_or_else(|| Malformed::new("bytes that are not the encoding the document says", mark))
}

/// The encoding an ASCII-compatible document's declaration names, UTF-8
/// where it names none.
fn declared(bytes: &[u8]) -> Result<Charset, Malformed> {
    if !bytes.starts_with(b"<?xml") {
        return Ok(UTF_8);
    }
    let head = &bytes[..bytes.len().min(256)];
    let Some(close) = head.windows(2).position(|pair| pair == b"?>") else {
        return Ok(UTF_8);
    };
    let declaration = &head[..close];
    let Some(at) = declaration.windows(8).position(|word| word == b"encoding") else {
        return Ok(UTF_8);
    };
    let rest = declaration[at + 8..].trim_ascii_start();
    let Some(rest) = rest.strip_prefix(b"=") else {
        return Ok(UTF_8);
    };
    let rest = rest.trim_ascii_start();
    let Some((&quote, value)) = rest
        .split_first()
        .filter(|(q, _)| matches!(q, b'"' | b'\''))
    else {
        return Ok(UTF_8);
    };
    let label = value
        .split(|byte| *byte == quote)
        .next()
        .unwrap_or_default();
    std::str::from_utf8(label)
        .ok()
        .and_then(Charset::named)
        .ok_or_else(|| Malformed::new("an encoding this reader does not know", at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str, big: bool, mark: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        if mark {
            bytes.extend_from_slice(if big { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
        }
        for unit in text.encode_utf16() {
            let pair = if big {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            };
            bytes.extend_from_slice(&pair);
        }
        bytes
    }

    #[test]
    fn the_declared_encoding_or_the_mark_is_what_the_bytes_are_read_as() {
        let latin = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>caf\xe9</a>";
        assert_eq!(
            decoded(latin).expect("decoded"),
            "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>caf\u{e9}</a>"
        );
        let text = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a>\u{e9}</a>";
        for (big, mark) in [(false, true), (true, true), (false, false), (true, false)] {
            assert_eq!(decoded(&utf16(text, big, mark)).expect("decoded"), text);
        }
        let plain = "\u{feff}<a>\u{e9}</a>";
        assert!(matches!(decoded(plain.as_bytes()), Ok(Cow::Borrowed(read)) if read == plain));
    }

    #[test]
    fn bytes_wrong_for_their_encoding_are_refused_never_replaced() {
        assert_eq!(
            decoded(b"<a>\xff</a>").expect_err("refused"),
            Malformed::new("not UTF-8", 3)
        );
        let mut broken = utf16("<a>", false, true);
        broken.extend_from_slice(&[0x00, 0xD8]);
        assert!(decoded(&broken).is_err());
        let unknown = decoded(b"<?xml version=\"1.0\" encoding=\"x-klingon\"?><a/>");
        assert_eq!(
            unknown.expect_err("refused").reason,
            "an encoding this reader does not know"
        );
    }
}
