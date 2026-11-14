//! A node of a [`Document`]: a handle that reads the tree, cheap to copy,
//! compared and ordered by where it stands in the document.

use std::borrow::Cow;
use std::ops::Range;

use super::{Attribute, Document, Kind, NodeData};

/// One node: the document node, an element, a text, a comment or a
/// processing instruction.
#[derive(Clone, Copy, Debug)]
pub struct Node<'d> {
    document: &'d Document<'d>,
    index: usize,
}

impl<'d> Node<'d> {
    pub(super) const fn new(document: &'d Document<'d>, index: usize) -> Self {
        Self { document, index }
    }

    /// Its place in document order, which no other node of the document
    /// shares.
    #[must_use]
    pub const fn id(self) -> usize {
        self.index
    }

    /// The document it is a node of.
    #[must_use]
    pub const fn document(self) -> &'d Document<'d> {
        self.document
    }

    fn data(self) -> &'d NodeData {
        &self.document.nodes[self.index]
    }

    fn element(self) -> Option<&'d super::ElementData> {
        match self.data().kind {
            Kind::Element(index) => Some(&self.document.elements[index]),
            _ => None,
        }
    }

    /// Whether it is the document node.
    #[must_use]
    pub fn is_root(self) -> bool {
        matches!(self.data().kind, Kind::Root)
    }

    /// Whether it is an element.
    #[must_use]
    pub fn is_element(self) -> bool {
        matches!(self.data().kind, Kind::Element(_))
    }

    /// Whether it is text.
    #[must_use]
    pub fn is_text(self) -> bool {
        matches!(self.data().kind, Kind::Text(_))
    }

    /// Whether it is a comment.
    #[must_use]
    pub fn is_comment(self) -> bool {
        matches!(self.data().kind, Kind::Comment(_))
    }

    /// Whether it is a processing instruction.
    #[must_use]
    pub fn is_instruction(self) -> bool {
        matches!(self.data().kind, Kind::Instruction { .. })
    }

    /// An element's name as written, prefix and all; a processing
    /// instruction's target; empty for anything else.
    #[must_use]
    pub fn name(self) -> &'d str {
        match self.data().kind {
            Kind::Element(index) => self.document.text(self.document.elements[index].name),
            Kind::Instruction { target, .. } => self.document.text(target),
            _ => "",
        }
    }

    /// An element's name without its prefix; a processing instruction's
    /// target; empty for anything else.
    #[must_use]
    pub fn local(self) -> &'d str {
        match self.element() {
            Some(element) => &self.name()[element.local..],
            None => self.name(),
        }
    }

    /// An element's prefix, empty where it has none.
    #[must_use]
    pub fn prefix(self) -> &'d str {
        match self.element() {
            Some(element) if element.local > 0 => &self.name()[..element.local - 1],
            _ => "",
        }
    }

    /// The namespace an element is in, `None` where it is in none.
    #[must_use]
    pub fn namespace(self) -> Option<&'d str> {
        self.element()
            .and_then(|element| self.document.namespace(element.namespace))
    }

    /// Whether it is an element named `local` in `namespace`, `None` being
    /// no namespace.
    #[must_use]
    pub fn is(self, namespace: Option<&str>, local: &str) -> bool {
        self.is_element() && self.local() == local && self.namespace() == namespace
    }

    /// The value of the attribute `local` in no namespace.
    #[must_use]
    pub fn attribute(self, local: &str) -> Option<&'d str> {
        self.attributes()
            .find(|attribute| attribute.namespace().is_none() && attribute.local() == local)
            .map(Attribute::value)
    }

    /// The value of the attribute `local` in `namespace`.
    #[must_use]
    pub fn attribute_in(self, namespace: &str, local: &str) -> Option<&'d str> {
        self.attributes()
            .find(|attribute| {
                attribute.namespace() == Some(namespace) && attribute.local() == local
            })
            .map(Attribute::value)
    }

    /// Whether it has the attribute `local` in no namespace.
    #[must_use]
    pub fn has_attribute(self, local: &str) -> bool {
        self.attribute(local).is_some()
    }

    /// An element's attributes in the order written, its namespace
    /// declarations not among them.
    pub fn attributes(self) -> impl Iterator<Item = Attribute<'d>> + use<'d> {
        let (start, end) = self.element().map_or((0, 0), |element| element.attributes);
        let document = self.document;
        (start..end).map(move |index| Attribute::new(document, index))
    }

    /// The namespaces an element declares, prefix then namespace, in the
    /// order written; the default namespace's prefix is empty.
    pub fn declarations(self) -> impl Iterator<Item = (&'d str, &'d str)> + use<'d> {
        let (start, end) = self
            .element()
            .map_or((0, 0), |element| element.declarations);
        let document = self.document;
        document.declarations[start..end]
            .iter()
            .map(move |declaration| {
                (
                    document.text(declaration.prefix),
                    document.value(declaration.uri),
                )
            })
    }

    /// The node it stands in; `None` for the document node.
    #[must_use]
    pub fn parent(self) -> Option<Self> {
        (self.index > 0).then(|| Self::new(self.document, self.data().parent))
    }

    /// Its first child.
    #[must_use]
    pub fn first_child(self) -> Option<Self> {
        let first = self.index + 1;
        (first < self.data().end).then(|| Self::new(self.document, first))
    }

    /// The node after it in its parent.
    #[must_use]
    pub fn next_sibling(self) -> Option<Self> {
        let next = self.data().next;
        (next > 0).then(|| Self::new(self.document, next))
    }

    /// The nodes it holds, in order.
    #[must_use]
    pub fn children(self) -> Children<'d> {
        Children {
            next: self.first_child(),
        }
    }

    /// The elements it holds, in order.
    pub fn elements(self) -> impl Iterator<Item = Self> + use<'d> {
        self.children().filter(|node| node.is_element())
    }

    /// Every node inside it at any depth, in document order, itself not
    /// among them.
    pub fn descendants(self) -> impl Iterator<Item = Self> + use<'d> {
        let document = self.document;
        (self.index + 1..self.data().end).map(move |index| Self::new(document, index))
    }

    /// A text's text, a comment's, a processing instruction's data; for an
    /// element or the document node, its first child's where that is text.
    #[must_use]
    pub fn text(self) -> Option<&'d str> {
        match self.data().kind {
            Kind::Text(value) => Some(self.document.value(value)),
            Kind::Comment(span) | Kind::Instruction { data: span, .. } => {
                Some(self.document.text(span))
            }
            Kind::Root | Kind::Element(_) => self
                .first_child()
                .filter(|child| child.is_text())
                .and_then(Self::text),
        }
    }

    /// The text directly inside it, in order.
    pub fn texts(self) -> impl Iterator<Item = &'d str> + use<'d> {
        self.children()
            .filter(|child| child.is_text())
            .filter_map(Self::text)
    }

    /// Its string value as `XPath` 1.0 says: for an element or the document
    /// node every text inside it at any depth, in order; for anything else
    /// its own text.
    #[must_use]
    pub fn string_value(self) -> Cow<'d, str> {
        if !(self.is_element() || self.is_root()) {
            return Cow::Borrowed(self.text().unwrap_or(""));
        }
        let mut texts = self
            .descendants()
            .filter(|node| node.is_text())
            .filter_map(Self::text);
        let Some(first) = texts.next() else {
            return Cow::Borrowed("");
        };
        match texts.next() {
            None => Cow::Borrowed(first),
            Some(second) => {
                let mut whole = String::from(first);
                whole.push_str(second);
                texts.for_each(|text| whole.push_str(text));
                Cow::Owned(whole)
            }
        }
    }

    /// Where it lies whole in the text.
    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.data().span.range()
    }

    /// Where an element's start tag ends; for anything else its end.
    #[must_use]
    pub fn open_end(self) -> usize {
        self.element()
            .map_or(self.data().span.end, |element| element.open_end)
    }

    /// Where an element's end tag starts, its end where it closed itself;
    /// for anything else its end.
    #[must_use]
    pub fn close_start(self) -> usize {
        self.element()
            .map_or(self.data().span.end, |element| element.close_start)
    }

    /// Whether an element closed itself, `<a/>`.
    #[must_use]
    pub fn closed_itself(self) -> bool {
        self.element()
            .is_some_and(|element| element.open_end == self.data().span.end)
    }
}

impl PartialEq for Node<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.document, other.document) && self.index == other.index
    }
}

impl Eq for Node<'_> {}

impl std::hash::Hash for Node<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl PartialOrd for Node<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Node<'_> {
    /// Document order.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index.cmp(&other.index)
    }
}

/// The nodes one node holds, in order.
#[derive(Clone, Debug)]
pub struct Children<'d> {
    next: Option<Node<'d>>,
}

impl<'d> Iterator for Children<'d> {
    type Item = Node<'d>;

    fn next(&mut self) -> Option<Node<'d>> {
        let here = self.next?;
        self.next = here.next_sibling();
        Some(here)
    }
}
