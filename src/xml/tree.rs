//! XML read into a tree: every element with its namespace resolved, its
//! attributes and the namespaces it declares, its text with entities read,
//! its comments and processing instructions, and where each lies in the
//! text — so a finding names its place and an edit is placed exactly.
//!
//! The nodes are kept in document order, an element's descendants right
//! after it, so the order of two nodes is the order of their ids and a
//! subtree is a run of ids. Adjacent text and CDATA sections are one text
//! node, as `XPath` sees them. Text is read as XML 1.0 reads it: line ends
//! made one line feed, an attribute's white space made spaces, and the five
//! predefined entities and character references read; any other entity is
//! refused, and so is a document type declaration, so no entity of anyone
//! else's is ever expanded or fetched. A document nests at most
//! [`depth::MAX_DEPTH`](crate::depth::MAX_DEPTH) elements unless
//! [`Options`] says fewer.
//!
//! A document borrows the text it was read from, or owns it
//! ([`Document::parse_owned`]); either way a value is a slice of that text
//! unless reading changed it.

mod attribute;
mod build;
mod node;

use std::borrow::Cow;

use super::event::Span;

pub use attribute::Attribute;
pub use node::{Children, Node};

/// The namespace the prefix `xml` is bound to in every document.
pub const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// Why text could not be read into a tree, and the byte where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// What is wrong, in words.
    pub message: String,
    /// The byte where it was found.
    pub at: usize,
}

impl Refused {
    pub(crate) fn new(message: impl Into<String>, at: usize) -> Self {
        Self {
            message: message.into(),
            at,
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for Refused {}

impl From<super::event::Malformed> for Refused {
    fn from(malformed: super::event::Malformed) -> Self {
        Self::new(malformed.reason, malformed.at)
    }
}

/// How a tree is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The most elements one nests inside another.
    pub depth: usize,
    /// Whether a CDATA section is read as text; refused where not.
    pub cdata: bool,
    /// Whether a processing instruction is read; refused where not.
    pub instructions: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            depth: crate::depth::MAX_DEPTH,
            cdata: true,
            instructions: true,
        }
    }
}

/// A value as the document gives it: the text it is written as, or what
/// reading it made of that where the two differ.
#[derive(Clone, Copy, Debug)]
enum Value {
    Written(Span),
    Read(usize),
}

/// The namespace a name is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Namespace {
    None,
    /// The `xml` prefix's.
    Xml,
    /// The one a declaration binds, by its index.
    Declared(usize),
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Root,
    /// An element, by its index among the elements.
    Element(usize),
    Text(Value),
    Comment(Span),
    Instruction {
        target: Span,
        data: Span,
    },
}

#[derive(Clone, Copy, Debug)]
struct NodeData {
    kind: Kind,
    parent: usize,
    /// The next sibling, or 0 where there is none: the root is no one's.
    next: usize,
    /// One past the last descendant.
    end: usize,
    /// Where the node lies whole.
    span: Span,
}

#[derive(Clone, Copy, Debug)]
struct ElementData {
    name: Span,
    /// Where the local name begins in `name`.
    local: usize,
    namespace: Namespace,
    attributes: (usize, usize),
    declarations: (usize, usize),
    open_end: usize,
    close_start: usize,
}

#[derive(Clone, Copy, Debug)]
struct AttributeData {
    name: Span,
    local: usize,
    namespace: Namespace,
    value: Value,
    /// The value between its quotes, as written.
    written: Span,
    quote: u8,
    owner: usize,
}

#[derive(Clone, Copy, Debug)]
struct Declaration {
    /// The prefix declared, empty for the default namespace.
    prefix: Span,
    uri: Value,
}

/// An XML document read into a tree.
#[derive(Clone, Debug)]
pub struct Document<'a> {
    source: Cow<'a, str>,
    nodes: Vec<NodeData>,
    elements: Vec<ElementData>,
    attributes: Vec<AttributeData>,
    declarations: Vec<Declaration>,
    read: Vec<String>,
}

impl<'a> Document<'a> {
    /// `text` read with the default [`Options`].
    ///
    /// # Errors
    /// The text is not one well-formed XML document, holds a document type
    /// declaration or an entity XML does not predefine, uses a prefix it
    /// never binds, gives an attribute twice, or nests too deep — each
    /// said with the byte where.
    pub fn parse(text: &'a str) -> Result<Self, Refused> {
        Self::parse_with(text, &Options::default())
    }

    /// `text` read with `options`.
    ///
    /// # Errors
    /// As [`Document::parse`], and what `options` refuses.
    pub fn parse_with(text: &'a str, options: &Options) -> Result<Self, Refused> {
        build::build(Cow::Borrowed(text), options)
    }

    /// `text` read with `options` into a document that owns it.
    ///
    /// # Errors
    /// As [`Document::parse_with`].
    pub fn parse_owned(text: String, options: &Options) -> Result<Document<'static>, Refused> {
        build::build(Cow::Owned(text), options)
    }

    /// The text the document was read from, which every range indexes.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The document node, parent of the root element and of the comments
    /// and processing instructions around it.
    #[must_use]
    pub const fn root(&self) -> Node<'_> {
        Node::new(self, 0)
    }

    /// The root element.
    ///
    /// # Panics
    /// Never: a document is read only where it has one.
    #[must_use]
    pub fn root_element(&self) -> Node<'_> {
        self.root()
            .children()
            .find(|node| node.is_element())
            .expect("a document read has a root element")
    }

    /// The node whose id is `id`, `None` past the last.
    #[must_use]
    pub fn node(&self, id: usize) -> Option<Node<'_>> {
        (id < self.nodes.len()).then(|| Node::new(self, id))
    }

    /// How many nodes the document has, the document node included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether it has only the document node, which a document read never
    /// does.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() < 2
    }

    fn text(&self, span: Span) -> &str {
        &self.source[span.range()]
    }

    fn value(&self, value: Value) -> &str {
        match value {
            Value::Written(span) => self.text(span),
            Value::Read(index) => &self.read[index],
        }
    }

    fn namespace(&self, namespace: Namespace) -> Option<&str> {
        match namespace {
            Namespace::None => None,
            Namespace::Xml => Some(XML_NAMESPACE),
            Namespace::Declared(index) => Some(self.value(self.declarations[index].uri)),
        }
    }
}

#[cfg(test)]
mod tests;
