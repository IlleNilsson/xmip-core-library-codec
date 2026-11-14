//! Why text is not XML, in the estate's words and at the byte where: what
//! every XML reading refuses with, and the backend's failures said so.

use quick_xml::events::attributes::AttrError;

/// Why text is not XML, and the byte where that was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Malformed {
    /// What is wrong, in words.
    pub reason: &'static str,
    /// The byte at which the text stopped being XML.
    pub at: usize,
}

impl Malformed {
    /// `reason`, found at byte `at`.
    #[must_use]
    pub const fn new(reason: &'static str, at: usize) -> Self {
        Self { reason, at }
    }
}

impl std::fmt::Display for Malformed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.reason, self.at)
    }
}

impl std::error::Error for Malformed {}

/// A `quick-xml` failure in the estate's words, at `at`.
pub(crate) fn refused(error: &quick_xml::Error, at: usize) -> Malformed {
    use quick_xml::errors::{IllFormedError, SyntaxError};
    let reason = match error {
        quick_xml::Error::Syntax(syntax) => match syntax {
            SyntaxError::InvalidBangMarkup => "expected a tag",
            SyntaxError::UnclosedPI | SyntaxError::UnclosedXmlDecl => {
                "unterminated processing instruction"
            }
            SyntaxError::UnclosedComment => "unterminated comment",
            SyntaxError::UnclosedDoctype => "unterminated document type declaration",
            SyntaxError::UnclosedCData => "unterminated CDATA section",
            SyntaxError::UnclosedTag => "unterminated tag",
            SyntaxError::UnclosedSingleQuotedAttributeValue
            | SyntaxError::UnclosedDoubleQuotedAttributeValue => "unterminated attribute value",
        },
        quick_xml::Error::IllFormed(ill) => match ill {
            IllFormedError::MissingDeclVersion(_) => "an XML declaration without a version",
            IllFormedError::UnknownVersion => "an XML version other than 1.0 or 1.1",
            IllFormedError::MissingDoctypeName => "a document type declaration without a name",
            IllFormedError::UnclosedReference => "an & that begins no reference",
            IllFormedError::DoubleHyphenInComment => "a comment holding --",
            _ => "the end tag does not close the open element",
        },
        quick_xml::Error::Encoding(_) => "not UTF-8",
        quick_xml::Error::InvalidAttr(_) => "an attribute that is not one",
        _ => "not XML",
    };
    Malformed::new(reason, at)
}

/// An attribute `quick-xml` could not read, in the estate's words, `at`
/// being where the tag's content begins.
pub(crate) fn attribute_refused(refused: &AttrError, at: usize) -> Malformed {
    match *refused {
        AttrError::ExpectedEq(offset) => {
            Malformed::new("expected = after the attribute name", at + offset)
        }
        AttrError::ExpectedValue(offset) | AttrError::UnquotedValue(offset) => {
            Malformed::new("expected a quoted attribute value", at + offset)
        }
        AttrError::ExpectedQuote(offset, _) => {
            Malformed::new("unterminated attribute value", at + offset)
        }
        AttrError::Duplicated(offset, _) => {
            Malformed::new("an attribute is given twice", at + offset)
        }
    }
}
