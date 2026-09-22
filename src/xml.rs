//! XML 1.0 character data: escaping what a writer puts between tags and in
//! attribute values, and unescaping what a reader takes out.
//!
//! Unescaping is strict. The five predefined entities and decimal and
//! hexadecimal character references are read; anything else is refused
//! rather than passed through, because text read one way by one reader and
//! another way by the next is how two gates came to disagree about one
//! name. Canonicalization (C14N) escapes by rules of its own and is not
//! this.

use crate::{CodecError, Result};

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
fn entity(name: &str) -> Result<char> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
