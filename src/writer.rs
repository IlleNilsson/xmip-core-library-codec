//! Writing a binary message's fields in order: the one byte writer, the
//! counterpart of [`crate::cursor::Cursor`].
//!
//! A message is built in a `Vec<u8>`, and [`ByteWriter`] is what writing a
//! field into one means: a byte, bytes, a number in either byte order, a
//! varint. Until 2026-09-24 Kafka, AMQP, `RabbitMQ`, XDR, OPC UA, SFTP and
//! `MySQL` each carried their own `put_u32` or writer type around
//! `extend_from_slice(&value.to_be_bytes())`. What a field means — a
//! counted string, padding to four — stays with each protocol, written
//! over this in its own words.

use crate::varint;

/// Appending a message's fields to the bytes written so far.
pub trait ByteWriter {
    /// One byte.
    fn byte(&mut self, value: u8) -> &mut Self;

    /// One byte, two's complement.
    fn i8(&mut self, value: i8) -> &mut Self {
        self.byte(value.cast_unsigned())
    }

    /// `bytes` as they are.
    fn bytes(&mut self, bytes: &[u8]) -> &mut Self;

    /// `value` as a varint ([`varint`]).
    fn varint(&mut self, value: u64) -> &mut Self {
        self.bytes(&varint::encode(value))
    }

    /// `value`, big-endian.
    fn u16_be(&mut self, value: u16) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn u16_le(&mut self, value: u16) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// `value`, big-endian.
    fn u32_be(&mut self, value: u32) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn u32_le(&mut self, value: u32) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// `value`, big-endian.
    fn u64_be(&mut self, value: u64) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn u64_le(&mut self, value: u64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// `value`, big-endian.
    fn i16_be(&mut self, value: i16) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn i16_le(&mut self, value: i16) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// `value`, big-endian.
    fn i32_be(&mut self, value: i32) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn i32_le(&mut self, value: i32) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// `value`, big-endian.
    fn i64_be(&mut self, value: i64) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn i64_le(&mut self, value: i64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// `value`, big-endian.
    fn f32_be(&mut self, value: f32) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn f32_le(&mut self, value: f32) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }

    /// `value`, big-endian.
    fn f64_be(&mut self, value: f64) -> &mut Self {
        self.bytes(&value.to_be_bytes())
    }

    /// `value`, little-endian.
    fn f64_le(&mut self, value: f64) -> &mut Self {
        self.bytes(&value.to_le_bytes())
    }
}

impl ByteWriter for Vec<u8> {
    fn byte(&mut self, value: u8) -> &mut Self {
        self.push(value);
        self
    }

    fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.extend_from_slice(bytes);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cursor::Cursor;

    #[test]
    fn what_is_written_reads_back_off_the_cursor_in_order() {
        let mut out = Vec::new();
        out.byte(1)
            .i8(-1)
            .u16_be(0x0102)
            .u16_le(0x0102)
            .u32_be(7)
            .u32_le(7)
            .u64_be(1 << 40)
            .u64_le(1 << 40)
            .i16_be(-2)
            .i16_le(-2)
            .i32_be(-70_000)
            .i32_le(-70_000)
            .i64_be(i64::MIN)
            .i64_le(i64::MIN)
            .f32_be(1.5)
            .f32_le(1.5)
            .f64_be(-0.25)
            .f64_le(-0.25)
            .varint(300)
            .bytes(b"end");
        assert_eq!(&out[2..6], &[1, 2, 2, 1]);
        let mut cursor = Cursor::new(&out);
        assert_eq!(cursor.byte().expect("u8"), 1);
        assert_eq!(cursor.i8().expect("i8"), -1);
        assert_eq!(cursor.u16_be().expect("u16"), 0x0102);
        assert_eq!(cursor.u16_le().expect("u16"), 0x0102);
        assert_eq!(cursor.u32_be().expect("u32"), 7);
        assert_eq!(cursor.u32_le().expect("u32"), 7);
        assert_eq!(cursor.u64_be().expect("u64"), 1 << 40);
        assert_eq!(cursor.u64_le().expect("u64"), 1 << 40);
        assert_eq!(cursor.i16_be().expect("i16"), -2);
        assert_eq!(cursor.i16_le().expect("i16"), -2);
        assert_eq!(cursor.i32_be().expect("i32"), -70_000);
        assert_eq!(cursor.i32_le().expect("i32"), -70_000);
        assert_eq!(cursor.i64_be().expect("i64"), i64::MIN);
        assert_eq!(cursor.i64_le().expect("i64"), i64::MIN);
        assert!((cursor.f32_be().expect("f32") - 1.5).abs() < f32::EPSILON);
        assert!((cursor.f32_le().expect("f32") - 1.5).abs() < f32::EPSILON);
        assert!((cursor.f64_be().expect("f64") + 0.25).abs() < f64::EPSILON);
        assert!((cursor.f64_le().expect("f64") + 0.25).abs() < f64::EPSILON);
        assert_eq!(cursor.varint().expect("varint"), 300);
        assert_eq!(cursor.take_rest(), b"end");
    }
}
