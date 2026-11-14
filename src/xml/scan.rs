//! The flat scan a protocol's XML is read with: an element's content, text
//! or attribute by its local name, whatever prefix the peer chose (`eb:`,
//! `ns2:`, `D:`, none).
//!
//! The documents a protocol meets are flat — an S3 listing naming keys, a
//! `WebDAV` multistatus, an ebMS header, an SRMP envelope — and the one
//! question asked of them is that. A same-named element nested inside
//! another is part of the outer one's content; comments, CDATA sections and
//! processing instructions are read as what they are, never as markup. It
//! reads a fragment as readily as a document, over the backend every XML
//! reading goes through, and stops where the text stops being XML. A
//! document that needs a tree reads one ([`tree`](super::tree)).

use quick_xml::events::Event as Cut;

use super::event::{self, Tag};
use crate::Result;

/// One element the scan found: its start tag and what lies between it and
/// its end tag, still escaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element<'a> {
    tag: Tag<'a>,
    content: &'a str,
}

impl<'a> Element<'a> {
    /// What lies between the start and the end tag, markup and entities
    /// as written; empty for an element that closed itself (`<x/>`).
    #[must_use]
    pub const fn content(&self) -> &'a str {
        self.content
    }

    /// Whether the element closed itself (`<x/>`), so it has no content at
    /// all rather than empty content.
    #[must_use]
    pub const fn closed_itself(&self) -> bool {
        self.tag.empty()
    }

    /// The content as text, entities read.
    ///
    /// # Errors
    ///
    /// The content holds an entity XML does not define.
    pub fn text(&self) -> Result<String> {
        super::unescape(self.content)
    }

    /// The value of the attribute whose name, or local name, is `name`,
    /// entities read.
    ///
    /// # Errors
    ///
    /// The value holds an entity XML does not define.
    pub fn attribute(&self, name: &str) -> Result<Option<String>> {
        for written in self.tag.attributes().map_while(std::result::Result::ok) {
            let local = written.name.rsplit(':').next();
            if written.name == name || local == Some(name) {
                return super::unescape(written.value).map(Some);
            }
        }
        Ok(None)
    }
}

/// Every element in `xml` whose local name is `name`, in document order;
/// one found inside another of the same name is part of the outer one's
/// content. The scan ends at an element that is never closed, and where
/// the text stops being XML.
#[must_use]
pub fn elements<'a, 'n>(xml: &'a str, name: &'n str) -> Elements<'a, 'n> {
    let mut cut = event::configured(xml.as_bytes());
    cut.config_mut().check_end_names = false;
    Elements {
        text: xml,
        cut,
        name,
    }
}

/// The elements [`elements`] finds. The scan reads the backend's pieces as
/// they come and makes nothing of a piece but the names it compares — the
/// one question asked of a flat document is answered at the speed the
/// backend cuts — and gives the found element in the reader's terms.
pub struct Elements<'a, 'n> {
    text: &'a str,
    cut: quick_xml::Reader<&'a [u8]>,
    name: &'n str,
}

impl<'a> Iterator for Elements<'a, '_> {
    type Item = Element<'a>;

    fn next(&mut self) -> Option<Element<'a>> {
        loop {
            let at = event::index(self.cut.buffer_position());
            let (start, empty) = match self.cut.read_event().ok()? {
                Cut::Start(start) => (start, false),
                Cut::Empty(start) => (start, true),
                Cut::Eof => return None,
                _ => continue,
            };
            if local(start.name().0) != self.name {
                continue;
            }
            let written: &str = &start;
            let from = event::offset(self.text, written);
            let tag = Tag::new(
                &self.text[from..from + written.len()],
                start.name().0.len(),
                empty,
                at,
            );
            if empty {
                return Some(Element { tag, content: "" });
            }
            let opened = event::index(self.cut.buffer_position());
            let mut depth = 0usize;
            loop {
                let before = event::index(self.cut.buffer_position());
                match self.cut.read_event().ok()? {
                    Cut::Start(nested) if local(nested.name().0) == self.name => depth += 1,
                    Cut::End(closing) if local(closing.name().0) == self.name => {
                        if depth == 0 {
                            let content = &self.text[opened..before];
                            return Some(Element { tag, content });
                        }
                        depth -= 1;
                    }
                    Cut::Eof => return None,
                    _ => {}
                }
            }
        }
    }
}

fn local(name: &str) -> &str {
    name.split_once(':').map_or(name, |(_, local)| local)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn markup_inside_a_comment_or_cdata_is_not_read_as_an_element() {
        let xml = "<!-- <X>no</X> --><Y><![CDATA[<X>no</X>]]></Y><X>yes</X>";
        assert_eq!(texts(xml, "X").expect("read"), ["yes"]);
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
}
