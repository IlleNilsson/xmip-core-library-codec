//! TOML 1.0 basic strings: quoting text so any TOML reader takes back
//! exactly that text, and reading one back.
//!
//! Quoting escapes what TOML requires — the quotation mark, the backslash
//! and every control character: U+0000 to U+001F and U+007F — using the short
//! escapes where TOML has one and `\uXXXX` elsewhere. Until 2026-09-24 the
//! node's declaration escaped three characters, so a carriage return in a
//! path wrote a file no TOML reader accepts, while the file archive escaped
//! them all. Reading is strict: an unescaped control character other than
//! tab, an unknown escape, or a `\u` naming no character is refused.

use crate::char_reader::CharReader;
use crate::{CodecError, Result};
use std::fmt::Write;

/// `text` as a TOML basic string, quotes included.
#[must_use]
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\u{0}'..='\u{1f}' | '\u{7f}' => {
                // Writing to a String cannot fail.
                let _ = write!(out, "\\u{:04X}", u32::from(character));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The text of `literal`, which is one basic string and nothing else.
///
/// # Errors
///
/// `literal` is not exactly one well-formed basic string.
pub fn unquote(literal: &str) -> Result<String> {
    let (text, after) = unquote_prefix(literal)?;
    if after.is_empty() {
        Ok(text)
    } else {
        Err(CodecError::new(format!(
            "{after:?} follows the basic string in {literal:?}"
        )))
    }
}

/// The basic string `text` opens with, and what follows it.
///
/// # Errors
///
/// `text` does not open with a quotation mark, the string never closes, or
/// it holds an unescaped control character other than tab, an unknown
/// escape, or a `\u` or `\U` naming no character.
pub fn unquote_prefix(text: &str) -> Result<(String, &str)> {
    let mut reader = CharReader::new(text);
    if !reader.eat('"') {
        return Err(CodecError::new(format!("{text:?} is not a basic string")));
    }
    let mut out = String::new();
    loop {
        match reader.bump() {
            None => {
                return Err(CodecError::new(format!("{text:?} never closes")));
            }
            Some('"') => return Ok((out, reader.rest())),
            Some('\\') => out.push(escape(&mut reader)?),
            Some(control @ ('\u{0}'..='\u{8}' | '\u{a}'..='\u{1f}' | '\u{7f}')) => {
                return Err(CodecError::new(format!(
                    "an unescaped control character U+{:04X} in {text:?}",
                    u32::from(control)
                )));
            }
            Some(other) => out.push(other),
        }
    }
}

/// The character the escape after a backslash stands for.
fn escape(reader: &mut CharReader<'_>) -> Result<char> {
    Ok(match reader.bump() {
        Some('b') => '\u{8}',
        Some('t') => '\t',
        Some('n') => '\n',
        Some('f') => '\u{c}',
        Some('r') => '\r',
        Some('"') => '"',
        Some('\\') => '\\',
        Some('u') => code_point(reader, 4)?,
        Some('U') => code_point(reader, 8)?,
        Some(other) => return Err(CodecError::new(format!("an unknown escape \\{other}"))),
        None => return Err(CodecError::new("a backslash ends the text")),
    })
}

fn code_point(reader: &mut CharReader<'_>, digits: usize) -> Result<char> {
    let hex = reader.peek_while(|character| character.is_ascii_hexdigit());
    let hex = hex.get(..digits).unwrap_or(hex);
    reader.eat_str(hex);
    if hex.len() != digits {
        return Err(CodecError::new(format!(
            "\\u{hex} is not {digits} hexadecimal digits"
        )));
    }
    u32::from_str_radix(hex, 16)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| CodecError::new(format!("\\u{hex} is not a character")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_escapes_what_toml_requires_and_nothing_else() {
        assert_eq!(
            quote("C:\\Program Files\\\"x\""),
            r#""C:\\Program Files\\\"x\"""#
        );
        assert_eq!(quote("a\tb\nc\rd\u{8}\u{c}"), r#""a\tb\nc\rd\b\f""#);
        assert_eq!(quote("\u{0}\u{1f}\u{7f}"), r#""\u0000\u001F\u007F""#);
        assert_eq!(
            quote("é\u{a0}\u{85}\u{1f600}"),
            "\"é\u{a0}\u{85}\u{1f600}\""
        );
    }

    #[test]
    fn every_character_reads_back_as_itself() {
        let every: String = (0..=0x2100u32)
            .chain([0x1_f600, 0x10_ffff])
            .filter_map(char::from_u32)
            .collect();
        assert_eq!(unquote(&quote(&every)).expect("reads back"), every);
    }

    #[test]
    fn a_prefix_leaves_what_follows() {
        let (key, rest) = unquote_prefix(r#""a = b" = "v""#).expect("reads");
        assert_eq!((key.as_str(), rest), ("a = b", r#" = "v""#));
        assert_eq!(
            unquote(r#""\U0001F600\u00E9""#).expect("reads"),
            "\u{1f600}é"
        );
    }

    #[test]
    fn what_toml_refuses_is_refused() {
        for bad in [
            "plain",
            "\"open",
            "\"a\"b\"",
            "\"\\q\"",
            "\"\\u12\"",
            "\"\\uD800\"",
            "\"\\UFFFFFFFF\"",
            "\"a\nb\"",
            "\"\u{7f}\"",
            "\"\\",
        ] {
            assert!(unquote(bad).is_err(), "{bad:?} should be refused");
        }
        assert_eq!(unquote("\"a\tb\"").expect("a raw tab is allowed"), "a\tb");
    }
}
