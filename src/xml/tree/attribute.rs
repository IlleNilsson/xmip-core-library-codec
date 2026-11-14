//! An attribute of an element in a [`Document`]: a handle that reads it,
//! cheap to copy, compared by where it stands in the document.

use std::ops::Range;

use super::{Document, Node};

/// One attribute of an element.
#[derive(Clone, Copy, Debug)]
pub struct Attribute<'d> {
    document: &'d Document<'d>,
    index: usize,
}

impl<'d> Attribute<'d> {
    pub(super) const fn new(document: &'d Document<'d>, index: usize) -> Self {
        Self { document, index }
    }

    fn data(self) -> &'d super::AttributeData {
        &self.document.attributes[self.index]
    }

    /// Its place among every attribute of the document, in document order.
    #[must_use]
    pub const fn id(self) -> usize {
        self.index
    }

    /// Its name as written, prefix and all.
    #[must_use]
    pub fn name(self) -> &'d str {
        self.document.text(self.data().name)
    }

    /// Its name without its prefix.
    #[must_use]
    pub fn local(self) -> &'d str {
        &self.name()[self.data().local..]
    }

    /// Its prefix, empty where it has none.
    #[must_use]
    pub fn prefix(self) -> &'d str {
        let local = self.data().local;
        if local == 0 {
            ""
        } else {
            &self.name()[..local - 1]
        }
    }

    /// The namespace it is in: none unless its name has a prefix.
    #[must_use]
    pub fn namespace(self) -> Option<&'d str> {
        self.document.namespace(self.data().namespace)
    }

    /// Its value, entities read and white space made spaces.
    #[must_use]
    pub fn value(self) -> &'d str {
        self.document.value(self.data().value)
    }

    /// Where its value lies between its quotes, as written.
    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.data().written.range()
    }

    /// The quote its value is written in, `"` or `'`.
    #[must_use]
    pub fn quote(self) -> u8 {
        self.data().quote
    }

    /// The element it belongs to.
    #[must_use]
    pub fn owner(self) -> Node<'d> {
        Node::new(self.document, self.data().owner)
    }
}

impl PartialEq for Attribute<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.document, other.document) && self.index == other.index
    }
}

impl Eq for Attribute<'_> {}

impl std::hash::Hash for Attribute<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}
