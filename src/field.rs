//! The fields a compact record form writes besides numbers, once: bytes and
//! text counted by a varint, a value that may be absent, a run of values
//! counted by a varint, and a value of a closed list by its place in it.
//!
//! A Message's and a Journey's forms in the Ledger are written with these
//! (`xmip-core-message`, `xmip-core-journey`), so the two read alike and
//! neither carries a copy of the other's walk. A protocol whose wire says
//! otherwise — a length of four bytes, a presence written as a tag — keeps
//! its own words over [`Cursor`] and [`ByteWriter`].

use crate::cursor::Cursor;
use crate::writer::ByteWriter;
use crate::{CodecError, Result};

/// `bytes`, after their length as a varint.
pub fn counted(out: &mut Vec<u8>, bytes: &[u8]) {
    out.varint(bytes.len() as u64).bytes(bytes);
}

/// Bytes [`counted`] wrote.
///
/// # Errors
/// The length is cut short or runs past the end.
pub fn read_counted<'a>(cursor: &mut Cursor<'a>) -> Result<&'a [u8]> {
    let length = usize::try_from(cursor.varint()?)
        .map_err(|_| CodecError::new("a length past this machine's"))?;
    cursor.take(length)
}

/// `text`, counted, as its UTF-8 bytes.
pub fn text(out: &mut Vec<u8>, text: &str) {
    counted(out, text.as_bytes());
}

/// Text [`text`] wrote.
///
/// # Errors
/// As [`read_counted`], or the bytes are not UTF-8.
pub fn read_text(cursor: &mut Cursor<'_>) -> Result<String> {
    String::from_utf8(read_counted(cursor)?.to_vec())
        .map_err(|_| CodecError::new("text that is not UTF-8"))
}

/// A value that may be absent: one byte saying whether, then the value as
/// `write` writes it.
pub fn optional<T>(out: &mut Vec<u8>, value: Option<T>, write: impl FnOnce(&mut Vec<u8>, T)) {
    out.byte(u8::from(value.is_some()));
    if let Some(value) = value {
        write(out, value);
    }
}

/// A value [`optional`] wrote, read by `read` where it is present.
///
/// # Errors
/// The presence byte is neither 0 nor 1, or `read` fails.
pub fn read_optional<'a, T>(
    cursor: &mut Cursor<'a>,
    read: impl FnOnce(&mut Cursor<'a>) -> Result<T>,
) -> Result<Option<T>> {
    match cursor.byte()? {
        0 => Ok(None),
        1 => read(cursor).map(Some),
        other => Err(CodecError::new(format!(
            "{other} says neither absent nor present"
        ))),
    }
}

/// `items`, after their count as a varint, each as `write` writes it.
pub fn many<T>(out: &mut Vec<u8>, items: &[T], mut write: impl FnMut(&mut Vec<u8>, &T)) {
    out.varint(items.len() as u64);
    for item in items {
        write(out, item);
    }
}

/// Values [`many`] wrote, each read by `read`. Never more is set aside
/// ahead of time than the bytes left could hold.
///
/// # Errors
/// The count is cut short, or `read` fails.
pub fn read_many<'a, T>(
    cursor: &mut Cursor<'a>,
    mut read: impl FnMut(&mut Cursor<'a>) -> Result<T>,
) -> Result<Vec<T>> {
    let count = cursor.varint()?;
    let room = usize::try_from(count)
        .unwrap_or(usize::MAX)
        .min(cursor.remaining().len());
    let mut items = Vec::with_capacity(room);
    for _ in 0..count {
        items.push(read(cursor)?);
    }
    Ok(items)
}

/// `value`'s place in `all`, the closed list of its kind, which only grows
/// at its end; `u8::MAX` for a value the list does not hold.
pub fn place<T: PartialEq>(all: &[T], value: &T) -> u8 {
    all.iter()
        .position(|known| known == value)
        .and_then(|at| u8::try_from(at).ok())
        .unwrap_or(u8::MAX)
}

/// The value of `all` at `place`.
///
/// # Errors
/// No value of `all`, named `what` in the words, is at `place`.
pub fn placed<T: Copy>(all: &[T], place: u8, what: &str) -> Result<T> {
    all.get(usize::from(place))
        .copied()
        .ok_or_else(|| CodecError::new(format!("no {what} is numbered {place}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_reads_back_as_it_was_written() {
        let mut out = Vec::new();
        text(&mut out, "åäö");
        counted(&mut out, &[0, 255]);
        optional(&mut out, Some(7u8), |out, value| {
            out.byte(value);
        });
        optional(&mut out, None::<u8>, |out, value| {
            out.byte(value);
        });
        many(&mut out, &[1u8, 2, 3], |out, value| {
            out.byte(*value);
        });
        out.byte(place(&['a', 'b', 'c'], &'c'));

        let mut cursor = Cursor::new(&out);
        assert_eq!(read_text(&mut cursor).expect("text"), "åäö");
        assert_eq!(read_counted(&mut cursor).expect("bytes"), &[0, 255]);
        assert_eq!(
            read_optional(&mut cursor, Cursor::byte).expect("some"),
            Some(7)
        );
        assert_eq!(
            read_optional(&mut cursor, Cursor::byte).expect("none"),
            None
        );
        assert_eq!(
            read_many(&mut cursor, Cursor::byte).expect("many"),
            [1, 2, 3]
        );
        let at = cursor.byte().expect("a place");
        assert_eq!(placed(&['a', 'b', 'c'], at, "letter").expect("placed"), 'c');
        assert!(cursor.is_empty());
    }

    #[test]
    fn what_is_not_a_field_is_refused() {
        assert!(read_optional(&mut Cursor::new(&[2]), Cursor::byte).is_err());
        assert!(read_counted(&mut Cursor::new(&[5, 1])).is_err());
        assert!(read_text(&mut Cursor::new(&[1, 0xff])).is_err());
        let refused = placed(&['a'], 3, "letter").expect_err("nothing there");
        assert_eq!(refused.message, "no letter is numbered 3");
        assert_eq!(place(&['a'], &'z'), u8::MAX);
    }
}
