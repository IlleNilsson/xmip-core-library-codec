//! XML's pieces as the estate reads them: text, an entity reference, a
//! start or an end tag, a CDATA section, a comment, a processing
//! instruction, a document type declaration — each with where it lies.
//!
//! The bytes are cut by `quick-xml`, the one XML backend the estate links
//! (`test/ThirdParty.Test.ps1` holds it there), and given back in these
//! terms, so no crate above names it. It cuts the pieces and checks that
//! each is UTF-8; what a document's order is — one root, every element
//! closed by its own end tag — is [`reader`](super::reader)'s, and a tag's
//! name is checked here. Text is as written: its entities are pieces of
//! their own, and nothing is read until a reader asks.

use std::borrow::Cow;
use std::ops::Range;

use quick_xml::events::Event as Cut;
use quick_xml::events::attributes::Attributes as CutAttributes;

pub use super::malformed::Malformed;
use super::malformed::attribute_refused;

/// Where a piece lies in the text read: `start..end`, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    /// The first byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
}

impl Span {
    /// The bytes from `start` up to `end`.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The span as a range, to index with.
    #[must_use]
    pub const fn range(self) -> Range<usize> {
        self.start..self.end
    }

    /// How many bytes it covers.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Whether it covers none.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// One piece of XML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// Character data as written, up to the next tag or reference.
    Text(&'a str),
    /// An entity or character reference: what stands between `&` and `;`.
    Reference(&'a str),
    /// A start tag, or an element that closes itself.
    Start(Tag<'a>),
    /// An end tag's name.
    End(&'a str),
    /// What a CDATA section holds.
    CData(&'a str),
    /// What a comment holds.
    Comment(&'a str),
    /// A processing instruction, the XML declaration among them (its
    /// target `xml`), and what follows its target.
    Instruction { target: &'a str, data: &'a str },
    /// A document type declaration.
    Doctype(&'a str),
}

/// A piece, and where it lies whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece<'a> {
    /// What it is.
    pub event: Event<'a>,
    /// From its `<` (or first character) to one past its end.
    pub span: Span,
}

/// A start tag: its name and its attributes as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag<'a> {
    /// The name and the attributes, between `<` and `>`.
    content: &'a str,
    name: usize,
    empty: bool,
    at: usize,
}

impl<'a> Tag<'a> {
    /// The tag whose name and attributes are `content`, its name `name`
    /// bytes long, its `<` at `at`.
    #[inline]
    pub(crate) const fn new(content: &'a str, name: usize, empty: bool, at: usize) -> Self {
        Self {
            content,
            name,
            empty,
            at,
        }
    }

    /// Its name as written, prefix and all.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &'a str {
        &self.content[..self.name]
    }

    /// Its name without a prefix.
    #[must_use]
    #[inline]
    pub fn local(&self) -> &'a str {
        let name = self.name();
        name.split_once(':').map_or(name, |(_, local)| local)
    }

    /// Whether the element closes itself, `<a/>`.
    #[must_use]
    #[inline]
    pub const fn empty(&self) -> bool {
        self.empty
    }

    /// Where its `<` is.
    #[must_use]
    #[inline]
    pub const fn at(&self) -> usize {
        self.at
    }

    /// Its attributes in the order written. A name given twice is not
    /// looked for here, which would cost every tag an allocation; the
    /// [`tree`](super::tree) refuses one.
    #[must_use]
    #[inline]
    pub fn attributes(&self) -> Attributes<'a> {
        let mut cut = CutAttributes::new(self.content, self.name);
        cut.with_checks(false);
        Attributes {
            cut,
            at: self.at + 1,
        }
    }
}

/// One attribute as a tag writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Written<'a> {
    /// Its name, prefix and all.
    pub name: &'a str,
    /// Its value between the quotes, entities not read.
    pub value: &'a str,
}

/// A start tag's attributes, each checked as it is read: a name, `=` and
/// a quoted value.
#[derive(Clone, Debug)]
pub struct Attributes<'a> {
    cut: CutAttributes<'a>,
    /// Where the tag's content begins in the text.
    at: usize,
}

impl<'a> Iterator for Attributes<'a> {
    type Item = Result<Written<'a>, Malformed>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(match self.cut.next()? {
            Ok(attribute) => match attribute.value {
                Cow::Borrowed(value) => Ok(Written {
                    name: attribute.key.0,
                    value,
                }),
                Cow::Owned(_) => Err(Malformed::new("an attribute value not as written", self.at)),
            },
            Err(refused) => Err(attribute_refused(&refused, self.at)),
        })
    }
}

/// Whether `character` may begin an XML name.
#[inline]
pub(crate) fn is_name_start(character: char) -> bool {
    character.is_alphabetic() || character == '_' || character == ':' || !character.is_ascii()
}

/// What `quick-xml` cut, given as a piece whose text is `content`: the
/// characters the cut holds, as it gives them. `at` and `end` are where
/// the cut lies in the whole.
///
/// # Errors
/// A tag whose name is not one.
#[inline]
pub(crate) fn piece(
    shape: Shape,
    content: &str,
    at: usize,
    end: usize,
) -> Result<Piece<'_>, Malformed> {
    let named = || content.chars().next().is_some_and(is_name_start);
    let event = match shape {
        Shape::Start { name, empty } if named() => Event::Start(Tag {
            content,
            name,
            empty,
            at,
        }),
        Shape::Start { .. } => return Err(Malformed::new("expected a tag", at)),
        Shape::End if named() => Event::End(content),
        Shape::End => return Err(Malformed::new("expected an element name", at + 2)),
        Shape::Text => Event::Text(content),
        Shape::Reference => Event::Reference(content),
        Shape::CData => Event::CData(content),
        Shape::Comment => Event::Comment(content),
        Shape::Instruction { target } => instruction(content, target),
        Shape::Doctype => Event::Doctype(content),
    };
    Ok(Piece {
        event,
        span: Span::new(at, end),
    })
}

/// What `quick-xml` cut, apart from its characters.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Shape {
    Start { name: usize, empty: bool },
    End,
    Text,
    Reference,
    CData,
    Comment,
    Instruction { target: usize },
    Doctype,
}

/// The shape of what `quick-xml` cut; `None` at the end of the input.
#[inline]
pub(crate) fn shape(cut: &Cut<'_>) -> Option<Shape> {
    Some(match cut {
        Cut::Start(start) => Shape::Start {
            name: start.name().0.len(),
            empty: false,
        },
        Cut::Empty(start) => Shape::Start {
            name: start.name().0.len(),
            empty: true,
        },
        Cut::End(_) => Shape::End,
        Cut::Text(_) => Shape::Text,
        Cut::GeneralRef(_) => Shape::Reference,
        Cut::CData(_) => Shape::CData,
        Cut::Comment(_) => Shape::Comment,
        Cut::Decl(_) => Shape::Instruction { target: 3 },
        Cut::PI(instruction) => Shape::Instruction {
            target: instruction.target().len(),
        },
        Cut::DocType(_) => Shape::Doctype,
        Cut::Eof => return None,
    })
}

fn instruction(content: &str, target: usize) -> Event<'_> {
    let target = target.min(content.len());
    Event::Instruction {
        target: &content[..target],
        data: content[target..].trim_start_matches([' ', '\t', '\r', '\n']),
    }
}

/// The characters `quick-xml` gives for what it cut.
#[inline]
pub(crate) fn content<'c>(cut: &'c Cut<'_>) -> &'c str {
    match cut {
        Cut::Start(start) | Cut::Empty(start) => start,
        Cut::End(end) => end,
        Cut::Text(text) | Cut::Comment(text) | Cut::DocType(text) => text,
        Cut::CData(data) => data,
        Cut::Decl(declaration) => declaration,
        Cut::PI(instruction) => instruction,
        Cut::GeneralRef(reference) => reference,
        Cut::Eof => "",
    }
}

/// The characters `quick-xml` cut, borrowed where it lends them for as long
/// as its buffer lives, and copied into `held` where it does not: a tag's.
#[inline]
pub(crate) fn kept<'b>(cut: Cut<'b>, held: &'b mut String) -> &'b str {
    let lent = match cut {
        Cut::Text(text) | Cut::Comment(text) | Cut::DocType(text) => text.into_inner(),
        Cut::CData(data) => data.into_inner(),
        Cut::PI(instruction) => instruction.into_inner(),
        Cut::GeneralRef(reference) => reference.into_inner(),
        other => {
            held.clear();
            held.push_str(content(&other));
            return held;
        }
    };
    match lent {
        Cow::Borrowed(text) => text,
        Cow::Owned(text) => {
            *held = text;
            held
        }
    }
}

/// Where `part`, a slice of `whole`, begins in it.
#[inline]
pub(crate) fn offset(whole: &str, part: &str) -> usize {
    let at = (part.as_ptr() as usize).wrapping_sub(whole.as_ptr() as usize);
    debug_assert!(at + part.len() <= whole.len(), "a part outside the whole");
    at
}

/// A `quick-xml` reader with the estate's settings: nothing trimmed or
/// expanded, an end tag matched to the element it ends by `quick-xml`,
/// which keeps the open names anyway, and one with nothing open left to
/// [`reader`](super::reader)'s order to say where it stands.
pub(crate) fn configured<R>(input: R) -> quick_xml::Reader<R> {
    let mut reader = quick_xml::Reader::from_reader(input);
    let config = reader.config_mut();
    config.check_end_names = true;
    config.allow_unmatched_ends = true;
    config.expand_empty_elements = false;
    config.trim_text(false);
    reader
}

/// A position `quick-xml` gives, as an index.
#[inline]
pub(crate) fn index(position: u64) -> usize {
    usize::try_from(position).unwrap_or(usize::MAX)
}
