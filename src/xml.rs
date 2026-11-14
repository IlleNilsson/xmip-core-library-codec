//! XML: the estate's one reader, and its character data written and read.
//!
//! **One reader.** Every XML the estate reads goes through here (the
//! owner, 2026-11-13: *Do we have all code in the modules, repos that we
//! should*; until that day five readers did it five ways). The pieces are
//! cut by `quick-xml`, the one XML backend linked, and given back in the
//! estate's terms ([`event`]); [`decoded`] reads bytes in the encoding they
//! declare; [`reader`] reads text in memory a piece at a time, or a start
//! tag at a time, and holds a document to its order, [`streamed`] does the same over
//! bytes as they arrive, [`tree`] reads a document into a tree with its
//! namespaces resolved and where each node lies, and the flat scan
//! ([`elements`], [`content`], [`text`], [`texts`], [`attribute`]) answers
//! a protocol's one question of its flat document: an element by its local
//! name, whatever its prefix. A Shape walking a Stream, a contract holding
//! a document to a schema, a SAML gate, a BPMN design, a transport reading
//! an answer and a conversion streaming a Stream all read here.
//!
//! **Character data.** Escaping what a writer puts between tags and in
//! attribute values, and reading back what a reader takes out. Reading is
//! strict: the five predefined entities and decimal and hexadecimal
//! character references are read; anything else is refused rather than
//! passed through, because text read one way by one reader and another way
//! by the next is how two gates came to disagree about one name.
//! Canonicalization (C14N) escapes by rules of its own and is not this.

mod decoded;
pub mod event;
mod malformed;
pub mod reader;
mod scan;
pub mod streamed;
pub mod tree;

use std::borrow::Cow;

use crate::{CodecError, Result};

pub use decoded::decoded;
pub use event::{Event, Malformed, Piece, Span, Tag};
pub use reader::Reader;
pub use scan::{Element, Elements, attribute, content, elements, text, texts};

/// `text` as element content: `&`, `<` and `>` escaped, and `"` too, so
/// the same text is safe inside a quoted attribute.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

/// Character data as text: the five predefined entities and every
/// character reference replaced by what it stands for.
///
/// # Errors
///
/// An `&` with no `;` after it, an entity XML does not predefine, or a
/// character reference that names no character.
pub fn unescape(text: &str) -> Result<String> {
    if !text.contains('&') {
        return Ok(text.to_string());
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let end = after
            .find(';')
            .ok_or_else(|| CodecError::new("an XML entity is not terminated"))?;
        out.push(entity(&after[..end])?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// What one entity or character reference, without its `&` and `;`,
/// stands for.
///
/// # Errors
///
/// An entity XML does not predefine, or a character reference that names
/// no character.
pub fn entity(name: &str) -> Result<char> {
    let reference = |digits: &str, radix: u32| {
        u32::from_str_radix(digits, radix)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| {
                CodecError::new(format!(
                    "the XML character reference '&{name};' names no character"
                ))
            })
    };
    match name {
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "amp" => Ok('&'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        _ => {
            if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                reference(hex, 16)
            } else if let Some(decimal) = name.strip_prefix('#') {
                reference(decimal, 10)
            } else {
                Err(CodecError::new(format!(
                    "the XML holds the entity '&{name};', which XML does not predefine"
                )))
            }
        }
    }
}

/// Text as XML 1.0 reads it: every line end — a carriage return and line
/// feed, or a carriage return alone — one line feed (XML 1.0 section 2.11).
#[must_use]
pub fn line_ends(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// An attribute's value as XML 1.0 reads it: line ends made one, then each
/// tab, carriage return and line feed written in it a space, and its
/// entities and character references read — a character a reference names
/// is kept as it is (XML 1.0 section 3.3.3).
///
/// # Errors
///
/// The value holds an entity XML does not define.
pub fn attribute_value(written: &str) -> Result<Cow<'_, str>> {
    let special = |byte: &u8| matches!(byte, b'&' | b'\t' | b'\r' | b'\n');
    if !written.as_bytes().iter().any(special) {
        return Ok(Cow::Borrowed(written));
    }
    let lined = line_ends(written);
    let mut out = String::with_capacity(lined.len());
    let mut rest: &str = &lined;
    while let Some(at) = rest.find(['&', '\t', '\n']) {
        out.push_str(&rest[..at]);
        if rest.as_bytes()[at] == b'&' {
            let after = &rest[at + 1..];
            let end = after
                .find(';')
                .ok_or_else(|| CodecError::new("an XML entity is not terminated"))?;
            out.push(entity(&after[..end])?);
            rest = &after[end + 1..];
        } else {
            out.push(' ');
            rest = &rest[at + 1..];
        }
    }
    out.push_str(rest);
    Ok(Cow::Owned(out))
}

/// Where byte `at` of `text` is as a person counts: its line and its column
/// in characters, both from 1 — what a finding names a place by.
#[must_use]
pub fn line_and_column(text: &str, at: usize) -> (usize, usize) {
    let mut before = at.min(text.len());
    while !text.is_char_boundary(before) {
        before -= 1;
    }
    let before = &text[..before];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |newline| newline + 1);
    (line, before[start..].chars().count() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_is_placed_by_line_and_column() {
        assert_eq!(line_and_column("<a>\n  <b>", 6), (2, 3));
        assert_eq!(line_and_column("<a>", 0), (1, 1));
        assert_eq!(line_and_column("é<", 2), (1, 2));
        assert_eq!(line_and_column("ab", 99), (1, 3));
    }

    #[test]
    fn escaped_text_reads_back_whole() {
        let text = r#"in/<a> & "b" 'c'"#;
        let escaped = escape(text);
        assert_eq!(escaped, "in/&lt;a&gt; &amp; &quot;b&quot; 'c'");
        assert_eq!(unescape(&escaped).expect("read"), text);
    }

    #[test]
    fn character_references_are_read_in_both_radixes() {
        assert_eq!(unescape("jane&#64;corp").expect("read"), "jane@corp");
        assert_eq!(unescape("jane&#x40;corp").expect("read"), "jane@corp");
        assert_eq!(unescape("&apos;&#X41;&amp;").expect("read"), "'A&");
        assert_eq!(unescape("plain").expect("read"), "plain");
    }

    #[test]
    fn what_is_not_an_entity_is_refused_saying_which() {
        let refused = |text: &str| unescape(text).expect_err("refused").message;
        assert!(refused("a &nbsp; b").contains("&nbsp;"));
        assert!(refused("a & b").contains("not terminated"));
        assert!(refused("&#xD800;").contains("names no character"));
        assert!(refused("&#;").contains("names no character"));
    }

    #[test]
    fn line_ends_and_an_attributes_white_space_are_read_as_xml_reads_them() {
        assert_eq!(line_ends("a\r\nb\rc\n"), "a\nb\nc\n");
        assert!(matches!(line_ends("plain"), Cow::Borrowed("plain")));
        assert_eq!(
            attribute_value("a\r\n\tb&#10;c&amp;").expect("read"),
            "a  b\nc&"
        );
        assert!(matches!(
            attribute_value("plain").expect("read"),
            Cow::Borrowed("plain")
        ));
        assert!(attribute_value("&nbsp;").is_err());
    }
}
