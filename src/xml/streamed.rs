//! XML read a piece at a time as its bytes arrive, never held whole: what
//! a conversion of a Stream larger than memory reads with.
//!
//! The same pieces and the same document order as the
//! [`Reader`](super::reader::Reader), cut by the same backend; a piece's
//! span counts from the first byte of the input. What is held at a time is
//! the piece being read: a tag, or a run of text up to the next tag or
//! reference.

use std::io::BufRead;

use super::event::{self, Malformed, Piece};
use super::malformed;
use super::reader::Order;

/// Why streamed XML could not be read: it is not XML, or the input failed.
#[derive(Debug)]
pub enum Unread {
    /// The bytes are not XML.
    Malformed(Malformed),
    /// Reading the input failed.
    Input(std::io::Error),
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(malformed) => malformed.fmt(f),
            Self::Input(failed) => write!(f, "the input failed: {failed}"),
        }
    }
}

impl std::error::Error for Unread {}

/// The input, the first bytes read to look for a byte order mark put back
/// before it.
type Input<R> = std::io::Chain<std::io::Cursor<Vec<u8>>, R>;

/// XML read from `R` a piece at a time, held to a document's order.
pub struct Streamed<R> {
    cut: quick_xml::Reader<Input<R>>,
    /// What the backend reads a piece into.
    buffer: Vec<u8>,
    /// The piece read last, as text.
    held: String,
    /// Where the input begins: past a byte order mark.
    base: usize,
    order: Order,
    done: bool,
}

impl<R: BufRead> Streamed<R> {
    /// `input` read as one document, a byte order mark before it set aside.
    ///
    /// # Errors
    /// Reading the first bytes failed.
    pub fn document(mut input: R) -> Result<Self, Unread> {
        let mut first = Vec::with_capacity(3);
        while first.len() < 3 {
            let arrived = input.fill_buf().map_err(Unread::Input)?;
            if arrived.is_empty() {
                break;
            }
            let taken = arrived.len().min(3 - first.len());
            first.extend_from_slice(&arrived[..taken]);
            input.consume(taken);
        }
        let base = if first == b"\xef\xbb\xbf" {
            first.clear();
            3
        } else {
            0
        };
        Ok(Self {
            cut: event::configured(std::io::Read::chain(std::io::Cursor::new(first), input)),
            buffer: Vec::new(),
            held: String::new(),
            base,
            order: Order::document(),
            done: false,
        })
    }

    /// The next piece, or `None` where the input ends, a document's order
    /// held.
    ///
    /// # Errors
    /// The bytes are not XML there or leave a document's order, or the
    /// input failed.
    pub fn read(&mut self) -> Result<Option<Piece<'_>>, Unread> {
        let Self {
            cut: reader,
            buffer,
            held,
            base,
            order,
            done,
        } = self;
        if *done {
            return Ok(None);
        }
        let at = *base + event::index(reader.buffer_position());
        buffer.clear();
        let cut = match reader.read_event_into(buffer) {
            Ok(cut) => cut,
            Err(quick_xml::Error::Io(failed)) => {
                *done = true;
                return Err(Unread::Input(std::io::Error::new(
                    failed.kind(),
                    failed.to_string(),
                )));
            }
            Err(error) => {
                *done = true;
                let where_ = *base + event::index(reader.error_position());
                return Err(Unread::Malformed(
                    order.placed(malformed::refused(&error, where_)),
                ));
            }
        };
        let end = *base + event::index(reader.buffer_position());
        let Some(shape) = event::shape(&cut) else {
            *done = true;
            order.finish(end).map_err(Unread::Malformed)?;
            return Ok(None);
        };
        let content = event::kept(cut, held);
        let piece = event::piece(shape, content, at, end)
            .and_then(|piece| order.admit(&piece).map(|()| piece))
            .map_err(|refused| {
                *done = true;
                Unread::Malformed(order.placed(refused))
            })?;
        Ok(Some(piece))
    }
}

#[cfg(test)]
mod tests {
    use super::super::event::Event;
    use super::*;
    use std::io::BufReader;

    /// Bytes handed over `step` at a time.
    struct Trickle<'a> {
        bytes: &'a [u8],
        step: usize,
    }

    impl std::io::Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let taken = self.step.min(out.len()).min(self.bytes.len());
            out[..taken].copy_from_slice(&self.bytes[..taken]);
            self.bytes = &self.bytes[taken..];
            Ok(taken)
        }
    }

    fn pieces(text: &str, step: usize) -> Result<Vec<(String, usize, usize)>, Unread> {
        let input = BufReader::with_capacity(
            step,
            Trickle {
                bytes: text.as_bytes(),
                step,
            },
        );
        let mut streamed = Streamed::document(input)?;
        let mut out = Vec::new();
        while let Some(piece) = streamed.read()? {
            let what = match piece.event {
                Event::Start(tag) => format!("<{}>", tag.name()),
                Event::End(name) => format!("</{name}>"),
                other => format!("{other:?}"),
            };
            out.push((what, piece.span.start, piece.span.end));
        }
        Ok(out)
    }

    #[test]
    fn a_document_reads_the_same_whatever_size_its_bytes_arrive_in() {
        let text = "\u{feff}<?xml version=\"1.0\"?><!-- a comment --><o:order xmlns:o='urn:o' \
                    id=\"1\"><line>one &amp; two</line><![CDATA[<raw>]]><empty/></o:order>\n";
        let whole = pieces(text, 4096).expect("reads");
        for step in [1, 2, 3, 7, 16] {
            assert_eq!(pieces(text, step).expect("reads"), whole, "{step}");
        }
        assert_eq!(whole.len(), 12);
        assert_eq!(whole[2].0, "<o:order>");
        assert_eq!(
            &text[whole[2].1..whole[2].2],
            "<o:order xmlns:o='urn:o' id=\"1\">"
        );
    }

    #[test]
    fn what_leaves_a_documents_order_is_refused_at_its_byte_in_the_whole() {
        let refused = |text: &str| match pieces(text, 3) {
            Err(Unread::Malformed(malformed)) => (malformed.reason, malformed.at),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            refused("<a><b></a>"),
            ("the end tag does not close the open element", 6)
        );
        assert_eq!(refused("<a><b>"), ("the element is never closed", 6));
        assert_eq!(refused("<a/>\n<b/>"), ("content after the document", 5));
    }
}
