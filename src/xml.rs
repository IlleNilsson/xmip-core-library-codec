//! XML 1.0 character data: escaping what a writer puts between tags and in
//! attribute values, and unescaping what a reader takes out; and the scan a
//! protocol's flat XML is read with.
//!
//! Unescaping is strict. The five predefined entities and decimal and
//! hexadecimal character references are read; anything else is refused
//! rather than passed through, because text read one way by one reader and
//! another way by the next is how two gates came to disagree about one
//! name. Canonicalization (C14N) escapes by rules of its own and is not
//! this.
//!
//! The scan is not a parser. The documents a protocol meets are flat — an
//! S3 listing naming keys, a `WebDAV` multistatus, an ebMS header, an SRMP
//! envelope — and the one question asked of them is an element's content,
//! text or attribute by its local name, whatever prefix the peer chose
//! (`eb:`, `ns2:`, `D:`, none). A same-named element nested inside another
//! is stepped over; markup inside a CDATA section or a comment is not
//! understood. A document that needs a tree names a contract technology.
//! Until 2026-09-28 the transport capability scanned by exact name, AS4
//! and `WebDAV` each by local name with a scanner of their own, and MSMQ
//! read an attribute by splitting the text.

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

/// One element the scan found: its open tag and what lies between it and
/// its close, still escaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element<'a> {
    open: &'a str,
    content: &'a str,
    closed_itself: bool,
}

impl<'a> Element<'a> {
    /// What lies between the open and the close tag, markup and entities
    /// as written; empty for an element that closed itself (`<x/>`).
    #[must_use]
    pub const fn content(&self) -> &'a str {
        self.content
    }

    /// Whether the element closed itself (`<x/>`), so it has no content at
    /// all rather than empty content.
    #[must_use]
    pub const fn closed_itself(&self) -> bool {
        self.closed_itself
    }

    /// The content as text, entities unescaped.
    ///
    /// # Errors
    ///
    /// The content holds an entity XML does not define.
    pub fn text(&self) -> Result<String> {
        unescape(self.content)
    }

    /// The value of the attribute whose name, or local name, is `name`,
    /// unescaped.
    ///
    /// # Errors
    ///
    /// The value holds an entity XML does not define.
    pub fn attribute(&self, name: &str) -> Result<Option<String>> {
        let mut rest = self
            .open
            .trim_start_matches(|c: char| !c.is_ascii_whitespace());
        loop {
            rest = rest.trim_start();
            let Some(equals) = rest.find('=') else {
                return Ok(None);
            };
            let key = rest[..equals].trim();
            let after = rest[equals + 1..].trim_start();
            let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                return Ok(None);
            };
            let value = &after[1..];
            let Some(end) = value.find(quote) else {
                return Ok(None);
            };
            if key == name || key.rsplit(':').next() == Some(name) {
                return unescape(&value[..end]).map(Some);
            }
            rest = &value[end + 1..];
        }
    }
}

/// Every element in `xml` whose local name is `name`, in document order;
/// one found inside another of the same name is part of the outer one's
/// content. The scan ends at an element that is never closed.
#[must_use]
pub const fn elements<'a, 'n>(xml: &'a str, name: &'n str) -> Elements<'a, 'n> {
    Elements { xml, name, at: 0 }
}

/// The elements [`elements`] finds.
#[derive(Clone, Debug)]
pub struct Elements<'a, 'n> {
    xml: &'a str,
    name: &'n str,
    at: usize,
}

impl<'a> Iterator for Elements<'a, '_> {
    type Item = Element<'a>;

    fn next(&mut self) -> Option<Element<'a>> {
        loop {
            let found = tag(self.xml, self.at)?;
            self.at = found.close + 1;
            if found.local != self.name || found.closing {
                continue;
            }
            if found.closed_itself {
                return Some(Element {
                    open: found.raw,
                    content: "",
                    closed_itself: true,
                });
            }
            let start = found.close + 1;
            let end = start + closing(&self.xml[start..], self.name)?;
            self.at = end + self.xml[end..].find('>')? + 1;
            return Some(Element {
                open: found.raw,
                content: &self.xml[start..end],
                closed_itself: false,
            });
        }
    }
}

/// The content of the first element whose local name is `name`, escaped.
#[must_use]
pub fn content<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    elements(xml, name).next().map(|element| element.content)
}

/// The text of the first element whose local name is `name`.
///
/// # Errors
///
/// Its text holds an entity XML does not define.
pub fn text(xml: &str, name: &str) -> Result<Option<String>> {
    elements(xml, name).next().map(|e| e.text()).transpose()
}

/// The text of every element whose local name is `name`, in order.
///
/// # Errors
///
/// A text holds an entity XML does not define.
pub fn texts(xml: &str, name: &str) -> Result<Vec<String>> {
    elements(xml, name).map(|element| element.text()).collect()
}

/// The value of `attribute` on the first element whose local name is
/// `name`.
///
/// # Errors
///
/// The value holds an entity XML does not define.
pub fn attribute(xml: &str, name: &str, attribute: &str) -> Result<Option<String>> {
    match elements(xml, name).next() {
        Some(element) => element.attribute(attribute),
        None => Ok(None),
    }
}

/// One tag as it opens or closes an element.
struct Tag<'a> {
    /// Where its `<` is.
    open: usize,
    /// Where its `>` is.
    close: usize,
    /// What lies between the two.
    raw: &'a str,
    /// Its name without a prefix.
    local: &'a str,
    /// Whether it closes an element (`</x>`).
    closing: bool,
    /// Whether it opens and closes one (`<x/>`).
    closed_itself: bool,
}

/// The next tag at or after `from`.
fn tag(xml: &str, from: usize) -> Option<Tag<'_>> {
    let open = from + xml.get(from..)?.find('<')?;
    let close = open + xml[open..].find('>')?;
    let raw = &xml[open + 1..close];
    let closing = raw.starts_with('/');
    let local = raw
        .trim_start_matches('/')
        .split(|c: char| c.is_ascii_whitespace() || c == '/')
        .next()
        .and_then(|qualified| qualified.rsplit(':').next())
        .unwrap_or("");
    Some(Tag {
        open,
        close,
        raw,
        local,
        closing,
        closed_itself: !closing && raw.ends_with('/'),
    })
}

/// Where the close tag of `name` begins in `rest`, nested same-named
/// elements stepped over.
fn closing(rest: &str, name: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = 0;
    loop {
        let found = tag(rest, at)?;
        if found.local == name {
            if found.closing {
                if depth == 0 {
                    return Some(found.open);
                }
                depth -= 1;
            } else if !found.closed_itself {
                depth += 1;
            }
        }
        at = found.close + 1;
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
    fn an_element_is_found_by_local_name_whatever_its_prefix() {
        let xml = "<?xml version=\"1.0\"?><a:X>1</a:X><Y>2</Y><Xy>3</Xy><ns2:X>4&amp;</ns2:X>";
        assert_eq!(content(xml, "X"), Some("1"));
        assert_eq!(content(xml, "Y"), Some("2"));
        assert_eq!(content(xml, "Absent"), None);
        assert_eq!(texts(xml, "X").expect("read"), ["1", "4&"]);
        assert_eq!(text(xml, "Xy").expect("read").as_deref(), Some("3"));
        assert_eq!(content("<X>never closed", "X"), None);
        assert!(texts("<Key>a&nbsp;b</Key>", "Key").is_err());
    }

    #[test]
    fn a_nested_same_named_element_is_part_of_the_outer_ones_content() {
        let xml = "<D:response><D:href>/a</D:href><D:response>inner</D:response></D:response>\
                   <response><href>/b</href></response>";
        let found: Vec<&str> = elements(xml, "response").map(|e| e.content()).collect();
        assert_eq!(found.len(), 2);
        assert!(
            found[0].ends_with("<D:response>inner</D:response>"),
            "{}",
            found[0]
        );
        assert_eq!(content(found[1], "href"), Some("/b"));
    }

    #[test]
    fn an_element_that_closed_itself_is_found_with_no_content() {
        let found = elements("<X/><X>late</X>", "X").next().expect("found");
        assert!(found.closed_itself());
        assert_eq!(found.content(), "");
        assert_eq!(content("<D:collection/>", "collection"), Some(""));
    }

    #[test]
    fn an_attribute_is_read_by_name_or_local_name_either_quote_unescaped() {
        let xml =
            "<eb:PartInfo xhref=\"no\" href=\"cid:a&amp;b\"/><Service eb:type='t'>s</Service>";
        assert_eq!(
            attribute(xml, "PartInfo", "href").expect("read").as_deref(),
            Some("cid:a&b")
        );
        assert_eq!(
            attribute(xml, "Service", "type").expect("read").as_deref(),
            Some("t")
        );
        assert_eq!(attribute(xml, "PartInfo", "absent").expect("read"), None);
        assert_eq!(attribute(xml, "Absent", "href").expect("read"), None);
        assert!(attribute("<P a=\"&nbsp;\"/>", "P", "a").is_err());
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
