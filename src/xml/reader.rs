//! XML in memory, read a piece at a time, and the order a document's
//! pieces keep: nothing but white space, comments, processing instructions
//! and a document type declaration around one root element, and every
//! element closed by its own end tag.
//!
//! A [`Reader::document`] holds the text to that order and says where it
//! leaves it; a [`Reader::fragment`] reads pieces as they come, for a part
//! of a document such as what lies between two tags. Either gives every
//! piece with where it lies and its text as written, so what reads them —
//! the [`tree`](super::tree), a shape walking a Stream — decides what it
//! needs. [`Streamed`](super::streamed::Streamed) keeps the same order over
//! bytes as they arrive.

use super::event::{self, Event, Malformed, Piece};
use super::malformed;

/// Where in a document's order the reading is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// A fragment: no order is kept.
    Anywhere,
    /// Before the root element.
    Prolog,
    /// Inside it.
    Inside,
    /// After it.
    Trailer,
}

/// The order a document's pieces keep, checked a piece at a time. That an
/// end tag closes the element open last is `quick-xml`'s check, which keeps
/// the open names; this keeps how many are open and where the document is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Order {
    place: Place,
    depth: usize,
}

impl Order {
    pub(crate) const fn document() -> Self {
        Self {
            place: Place::Prolog,
            depth: 0,
        }
    }

    pub(crate) const fn fragment() -> Self {
        Self {
            place: Place::Anywhere,
            depth: 0,
        }
    }

    /// How many elements are open.
    #[inline]
    pub(crate) const fn depth(&self) -> usize {
        self.depth
    }

    /// Whether the reading is inside a document's root.
    #[inline]
    pub(crate) const fn inside(&self) -> bool {
        matches!(self.place, Place::Inside)
    }

    /// `piece` taken in order.
    #[inline]
    pub(crate) fn admit(&mut self, piece: &Piece<'_>) -> Result<(), Malformed> {
        let at = piece.span.start;
        match (self.place, piece.event) {
            (Place::Prolog | Place::Inside, Event::Start(tag)) => {
                self.open(tag.empty());
                Ok(())
            }
            (Place::Inside, Event::End(_)) => {
                self.close();
                Ok(())
            }
            (Place::Anywhere, Event::Start(tag)) => {
                self.depth += usize::from(!tag.empty());
                Ok(())
            }
            (Place::Anywhere, Event::End(_)) => {
                self.depth = self.depth.saturating_sub(1);
                Ok(())
            }
            (Place::Prolog | Place::Trailer, Event::Text(text)) => {
                match text.bytes().position(|byte| !byte.is_ascii_whitespace()) {
                    None => Ok(()),
                    Some(offset) => Err(Malformed::new(self.outside(), at + offset)),
                }
            }
            (Place::Inside, Event::Doctype(_)) => Err(Malformed::new("expected a tag", at)),
            // Outside the root: an element, an end tag, a reference or a
            // CDATA section, or a document type declaration after it.
            (
                Place::Prolog | Place::Trailer,
                Event::Start(_) | Event::End(_) | Event::Reference(_) | Event::CData(_),
            )
            | (Place::Trailer, Event::Doctype(_)) => Err(Malformed::new(self.outside(), at)),
            // Anything in a fragment; text, references, CDATA, comments and
            // instructions inside the root; comments, instructions and a
            // document type declaration before it.
            _ => Ok(()),
        }
    }

    /// The text ends at `at`: whether a document did too.
    pub(crate) const fn finish(&self, at: usize) -> Result<(), Malformed> {
        match self.place {
            Place::Prolog => Err(Malformed::new("no root element", at)),
            Place::Inside => Err(Malformed::new("the element is never closed", at)),
            Place::Anywhere | Place::Trailer => Ok(()),
        }
    }

    /// A refusal said for where the document is: before or after the
    /// root, what is not a tag is not where one may stand.
    pub(crate) const fn placed(&self, malformed: Malformed) -> Malformed {
        let tag = matches!(malformed.reason.as_bytes(), b"expected a tag");
        match self.place {
            Place::Prolog | Place::Trailer if tag => Malformed::new(self.outside(), malformed.at),
            _ => malformed,
        }
    }

    const fn outside(&self) -> &'static str {
        match self.place {
            Place::Trailer => "content after the document",
            _ => "expected the root element",
        }
    }

    #[inline]
    fn open(&mut self, empty: bool) {
        if empty {
            if self.place == Place::Prolog {
                self.place = Place::Trailer;
            }
            return;
        }
        self.depth += 1;
        self.place = Place::Inside;
    }

    #[inline]
    fn close(&mut self) {
        self.depth -= 1;
        if self.depth == 0 {
            self.place = Place::Trailer;
        }
    }
}

/// XML in memory, read a piece at a time.
pub struct Reader<'a> {
    text: &'a str,
    cut: quick_xml::Reader<&'a [u8]>,
    /// Where the text `cut` reads begins: past a byte order mark.
    base: usize,
    order: Order,
    done: bool,
}

impl<'a> Reader<'a> {
    /// `text` read as one document, a byte order mark before it set aside,
    /// held to a document's order.
    #[must_use]
    pub fn document(text: &'a str) -> Self {
        let base = if text.starts_with('\u{feff}') { 3 } else { 0 };
        Self::over(text, base, Order::document())
    }

    /// `text` read as pieces in whatever order they come, an end tag not
    /// held to the element open before it: a protocol's answer is read for
    /// what it names even where its nesting is not a document's.
    #[must_use]
    pub fn fragment(text: &'a str) -> Self {
        let mut reader = Self::over(text, 0, Order::fragment());
        reader.cut.config_mut().check_end_names = false;
        reader
    }

    fn over(text: &'a str, base: usize, order: Order) -> Self {
        Self {
            text,
            cut: event::configured(&text.as_bytes()[base..]),
            base,
            order,
            done: false,
        }
    }

    /// The text read, which every span indexes.
    #[must_use]
    pub const fn text(&self) -> &'a str {
        self.text
    }

    /// How many elements are open.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.order.depth()
    }

    /// The next piece, or `None` where the text ends, a document's order
    /// held.
    ///
    /// # Errors
    /// The text is not XML there, or leaves a document's order.
    #[inline]
    pub fn read(&mut self) -> Result<Option<Piece<'a>>, Malformed> {
        self.next(false)
    }

    /// The next start tag, or `None` where the text ends: every piece
    /// between is read and held to a document's order, and what lies inside
    /// the root but is no tag — text, references, CDATA sections, comments,
    /// instructions — is passed over without being made a piece. What a walk
    /// that reads only the elements asks for, at the backend's speed.
    ///
    /// # Errors
    /// The text is not XML there, or leaves a document's order.
    #[inline]
    pub fn read_tag(&mut self) -> Result<Option<Piece<'a>>, Malformed> {
        self.next(true)
    }

    #[inline]
    fn next(&mut self, tags: bool) -> Result<Option<Piece<'a>>, Malformed> {
        use quick_xml::events::Event as Cut;
        loop {
            if self.done {
                return Ok(None);
            }
            let at = self.base + event::index(self.cut.buffer_position());
            let cut = match self.cut.read_event() {
                Ok(cut) => cut,
                Err(error) => {
                    self.done = true;
                    let where_ = self.base + event::index(self.cut.error_position());
                    return Err(self.order.placed(malformed::refused(&error, where_)));
                }
            };
            let content_inside = !matches!(
                cut,
                Cut::Start(_) | Cut::Empty(_) | Cut::End(_) | Cut::DocType(_) | Cut::Eof
            );
            if tags && content_inside && self.order.inside() {
                continue;
            }
            let Some(shape) = event::shape(&cut) else {
                self.done = true;
                self.order.finish(self.text.len())?;
                return Ok(None);
            };
            let end = self.base + event::index(self.cut.buffer_position());
            let written = event::content(&cut);
            let start = event::offset(self.text, written);
            let content = &self.text[start..start + written.len()];
            let piece = event::piece(shape, content, at, end)
                .and_then(|piece| self.order.admit(&piece).map(|()| piece))
                .map_err(|refused| {
                    self.done = true;
                    self.order.placed(refused)
                })?;
            if !tags || matches!(piece.event, Event::Start(_)) {
                return Ok(Some(piece));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walked(text: &str) -> Result<usize, Malformed> {
        let mut reader = Reader::document(text);
        let mut pieces = 0;
        while let Some(piece) = reader.read()? {
            if let Event::Start(tag) = piece.event {
                tag.attributes().try_for_each(|written| written.map(drop))?;
            }
            pieces += 1;
        }
        Ok(pieces)
    }

    fn stop(text: &str) -> (&'static str, usize) {
        let refused = walked(text).expect_err("refused");
        (refused.reason, refused.at)
    }

    #[test]
    fn a_document_is_read_through_its_prolog_root_and_trailer() {
        let text = "\u{feff}<?xml version=\"1.0\"?>\n<!-- c -->\
<!DOCTYPE a [<!ENTITY x \"y\">]><p:a k=\"v>\"><b><![CDATA[<x>]]></b><c/>\
t &amp; u<?pi x?></p:a>\n<!--z-->";
        assert_eq!(walked(text), Ok(16));
        assert_eq!(walked("<a/>"), Ok(1));
    }

    #[test]
    fn every_piece_is_given_with_where_it_lies() {
        let text = "<?xml version=\"1.0\"?><p:a k='v' b = \"2\">t&amp;<![CDATA[<x>]]><e/>\
                    <?go now?></p:a >";
        let mut reader = Reader::document(text);
        let mut pieces = Vec::new();
        while let Some(piece) = reader.read().expect("reads") {
            pieces.push(piece);
        }
        let whole = |index: usize| &text[pieces[index].span.range()];
        assert!(
            matches!(pieces[0].event, Event::Instruction { target: "xml", data }
            if data == "version=\"1.0\"")
        );
        let Event::Start(tag) = pieces[1].event else {
            panic!("{:?}", pieces[1]);
        };
        assert_eq!(
            (tag.name(), tag.local(), tag.empty(), tag.at()),
            ("p:a", "a", false, 21)
        );
        let written: Vec<(&str, &str)> = tag
            .attributes()
            .map(|w| w.map(|w| (w.name, w.value)))
            .collect::<Result<_, _>>()
            .expect("attributes");
        assert_eq!(written, [("k", "v"), ("b", "2")]);
        assert_eq!(pieces[2].event, Event::Text("t"));
        assert_eq!(pieces[3].event, Event::Reference("amp"));
        assert_eq!(whole(3), "&amp;");
        assert_eq!(pieces[4].event, Event::CData("<x>"));
        assert!(matches!(pieces[5].event, Event::Start(tag) if tag.empty()));
        assert_eq!(
            pieces[6].event,
            Event::Instruction {
                target: "go",
                data: "now"
            }
        );
        assert_eq!(pieces[7].event, Event::End("p:a"));
        assert_eq!(whole(7), "</p:a >");
    }

    #[test]
    fn the_first_byte_that_leaves_a_documents_order_is_where_it_stops() {
        assert_eq!(
            stop("<a><b></a>"),
            ("the end tag does not close the open element", 6)
        );
        assert_eq!(stop("<a><b>"), ("the element is never closed", 6));
        assert_eq!(stop("<a/><b/>"), ("content after the document", 4));
        assert_eq!(stop("<a/>\n<b/>"), ("content after the document", 5));
        assert_eq!(stop("<a/>x"), ("content after the document", 4));
        assert_eq!(stop("  "), ("no root element", 2));
        assert_eq!(stop(""), ("no root element", 0));
        assert_eq!(stop("text"), ("expected the root element", 0));
        assert_eq!(stop("<a><!-- x</a>"), ("unterminated comment", 3));
        assert_eq!(stop("<a/><![CDATA[x]]>"), ("content after the document", 4));
        assert_eq!(stop("<a/></a>"), ("content after the document", 4));
        assert_eq!(stop("<a x=1/>"), ("expected a quoted attribute value", 5));
        assert_eq!(stop("<a x/>"), ("expected = after the attribute name", 4));
    }

    #[test]
    fn reading_tags_alone_gives_the_start_tags_and_holds_the_same_order() {
        let mut reader =
            Reader::document("<?xml version='1.0'?><a>t&amp;<b x='1'/><![CDATA[c]]></a>");
        let mut names = Vec::new();
        while let Some(piece) = reader.read_tag().expect("reads") {
            let Event::Start(tag) = piece.event else {
                panic!("{piece:?}");
            };
            names.push(tag.name());
        }
        assert_eq!(names, ["a", "b"]);
        let refused = |text: &str| {
            let mut reader = Reader::document(text);
            loop {
                match reader.read_tag() {
                    Ok(Some(_)) => {}
                    Ok(None) => panic!("{text} read"),
                    Err(refused) => return (refused.reason, refused.at),
                }
            }
        };
        assert_eq!(refused("text<a/>"), ("expected the root element", 0));
        assert_eq!(
            refused("<a><b></a>"),
            ("the end tag does not close the open element", 6)
        );
        assert_eq!(refused("<a/>x"), ("content after the document", 4));
        assert_eq!(refused("<a>"), ("the element is never closed", 3));
    }

    #[test]
    fn a_fragment_is_read_in_whatever_order_it_comes() {
        let mut reader = Reader::fragment("x</a><b>y");
        let mut events = Vec::new();
        while let Some(piece) = reader.read().expect("reads") {
            events.push(piece.event);
        }
        assert_eq!(events.len(), 4);
        assert_eq!(events[1], Event::End("a"));
        let mut crossed = Reader::fragment("<a><b></a></b>");
        while crossed.read().expect("a fragment holds no order").is_some() {}
    }
}
