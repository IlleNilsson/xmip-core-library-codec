//! Reading a binary message's fields in order: the one byte cursor.
//!
//! Every binary protocol reads the same way — take the next so many bytes,
//! never past the end, and know where it has got to. Until 2026-09-24
//! fourteen crates each carried that cursor: the transport capability's,
//! three technologies wrapping it, eight technologies with their own, the
//! SSH key reader and the Matter TLV reader, each with its own off-by-one
//! waiting to happen. What differs between protocols is what a field means
//! — XDR's padding, AMQP's short string, `MySQL`'s length-encoded integer —
//! and that stays with each protocol, written over this cursor in its own
//! words.

use crate::{CodecError, Result, varint};

/// Where reading has got to in a message.
#[derive(Clone, Debug)]
pub struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    /// A cursor at the start of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    /// How many bytes have been read.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.at
    }

    /// True when nothing remains.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.at >= self.bytes.len()
    }

    /// What remains, not taken.
    #[must_use]
    pub fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.at.min(self.bytes.len())..]
    }

    /// The next byte, not taken.
    #[must_use]
    pub fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// The next `count` bytes. On a refusal the cursor does not move.
    ///
    /// # Errors
    /// Fewer than `count` bytes remain.
    pub fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| {
                CodecError::new(format!(
                    "a field of {count} bytes runs past the end, {} remain",
                    self.remaining().len()
                ))
            })?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    /// Everything that remains, taken.
    pub fn take_rest(&mut self) -> &'a [u8] {
        let rest = self.remaining();
        self.at = self.bytes.len();
        rest
    }

    /// The bytes up to `delimiter`, taken with the delimiter, which is not
    /// returned: a NUL-terminated string when `delimiter` is zero.
    ///
    /// # Errors
    /// No `delimiter` remains.
    pub fn take_until(&mut self, delimiter: u8) -> Result<&'a [u8]> {
        let rest = self.remaining();
        let length = rest
            .iter()
            .position(|byte| *byte == delimiter)
            .ok_or_else(|| CodecError::new(format!("a field that no {delimiter:#04x} ends")))?;
        self.at += length + 1;
        Ok(&rest[..length])
    }

    /// Past the next `count` bytes.
    ///
    /// # Errors
    /// Fewer than `count` bytes remain.
    pub fn skip(&mut self, count: usize) -> Result<()> {
        self.take(count).map(|_| ())
    }

    /// The next `N` bytes as an array.
    ///
    /// # Errors
    /// Fewer than `N` bytes remain.
    pub fn take_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    /// The next byte.
    ///
    /// # Errors
    /// Nothing remains.
    pub fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    /// The next byte as a two's-complement integer.
    ///
    /// # Errors
    /// Nothing remains.
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.byte()?.cast_signed())
    }

    /// The next varint ([`varint`]).
    ///
    /// # Errors
    /// The bytes end inside the varint, or it runs past ten bytes.
    pub fn varint(&mut self) -> Result<u64> {
        let (value, length) = varint::decode(self.remaining())?;
        self.at += length;
        Ok(value)
    }
}

/// A fixed-width number read in each byte order, one pair of methods a type.
macro_rules! numbers {
    ($($number:ty: $big:ident, $little:ident;)*) => {
        impl Cursor<'_> {$(
            #[doc = concat!("The next `", stringify!($number), "`, big-endian.")]
            ///
            /// # Errors
            /// Fewer bytes remain than the number is wide.
            pub fn $big(&mut self) -> Result<$number> {
                Ok(<$number>::from_be_bytes(self.take_array()?))
            }

            #[doc = concat!("The next `", stringify!($number), "`, little-endian.")]
            ///
            /// # Errors
            /// Fewer bytes remain than the number is wide.
            pub fn $little(&mut self) -> Result<$number> {
                Ok(<$number>::from_le_bytes(self.take_array()?))
            }
        )*}
    };
}

numbers! {
    u16: u16_be, u16_le;
    u32: u32_be, u32_le;
    u64: u64_be, u64_le;
    i16: i16_be, i16_le;
    i32: i32_be, i32_le;
    i64: i64_be, i64_le;
    f32: f32_be, f32_le;
    f64: f64_be, f64_le;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_read_in_order_and_what_remains_is_what_was_not_read() {
        let mut cursor = Cursor::new(&[1, 0, 2, 3, 4, 5]);
        assert_eq!(cursor.byte().expect("a byte"), 1);
        assert_eq!(cursor.u16_be().expect("two"), 2);
        cursor.skip(1).expect("one more");
        assert_eq!(cursor.position(), 4);
        assert_eq!(cursor.peek(), Some(4));
        assert_eq!(cursor.remaining(), &[4, 5]);
        assert_eq!(cursor.take(2).expect("the rest"), &[4, 5]);
        assert!(cursor.is_empty());
        assert_eq!(cursor.peek(), None);
    }

    #[test]
    fn a_number_reads_in_the_byte_order_asked_for() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(Cursor::new(&bytes).u32_be().expect("be"), 0x0102_0304);
        assert_eq!(Cursor::new(&bytes).u32_le().expect("le"), 0x0403_0201);
        assert_eq!(
            Cursor::new(&bytes).u64_be().expect("be"),
            0x0102_0304_0506_0708
        );
        assert_eq!(Cursor::new(&bytes).u16_le().expect("le"), 0x0201);
        assert_eq!(Cursor::new(&[0xff, 0xfe]).i16_be().expect("be"), -2);
        assert_eq!(
            Cursor::new(&[0xfe, 0xff, 0xff, 0xff]).i32_le().expect("le"),
            -2
        );
        assert_eq!(Cursor::new(&[0x80]).i8().expect("i8"), i8::MIN);
        let half = 0.5f64.to_le_bytes();
        assert!((Cursor::new(&half).f64_le().expect("le") - 0.5).abs() < f64::EPSILON);
        let mut short = Cursor::new(&bytes[..3]);
        assert!(short.u32_be().is_err());
        assert_eq!(short.position(), 0, "a refused read moves nothing");
    }

    #[test]
    fn a_field_past_the_end_is_refused_and_moves_nothing() {
        let mut cursor = Cursor::new(&[1, 2]);
        let error = cursor.take(3).expect_err("past the end");
        assert!(error.message.contains("runs past"), "{}", error.message);
        assert!(error.message.contains("2 remain"), "{}", error.message);
        assert_eq!(cursor.remaining(), &[1, 2]);
        assert!(cursor.take(usize::MAX).is_err());
        assert_eq!(cursor.take_rest(), &[1, 2]);
        assert!(cursor.is_empty());
        assert!(cursor.byte().is_err());
    }

    #[test]
    fn a_delimited_field_is_taken_with_its_delimiter() {
        let mut cursor = Cursor::new(b"user\0db\0rest");
        assert_eq!(cursor.take_until(0).expect("user"), b"user");
        assert_eq!(cursor.take_until(0).expect("db"), b"db");
        assert!(cursor.take_until(0).is_err(), "no NUL ends it");
        assert_eq!(cursor.remaining(), b"rest");
    }

    #[test]
    fn a_varint_is_read_where_the_cursor_is() {
        let mut cursor = Cursor::new(&[7, 0xac, 0x02, 0x80]);
        cursor.skip(1).expect("skip");
        assert_eq!(cursor.varint().expect("varint"), 300);
        let error = cursor.varint().expect_err("cut off");
        assert_eq!(error.message, "the bytes end inside a varint");
        assert_eq!(cursor.position(), 3);
    }
}
