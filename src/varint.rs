//! The base-128 varint (unsigned LEB128) and the zig-zag mapping that puts
//! a signed integer into it.
//!
//! Seven bits a byte, low bits first, the high bit saying another byte
//! follows; ten bytes carry sixty-four bits and an eleventh is refused.
//! Protocol Buffers writes every integer and length this way, Avro every
//! `long` zig-zagged over it, and a Kafka record batch its record fields.
//! Until 2026-09-24 the message layer, the contract capability and Kafka
//! each read it, and Kafka and both Avro crates each zig-zagged it.

use crate::CodecError;

/// Why the bytes are not a varint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VarintError {
    /// The bytes end while the high bit still says another follows.
    Unterminated,
    /// Ten bytes and the high bit still set.
    Overlong,
}

impl VarintError {
    /// What was wrong, in words.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unterminated => "the bytes end inside a varint",
            Self::Overlong => "a varint runs past ten bytes",
        }
    }
}

impl core::fmt::Display for VarintError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl core::error::Error for VarintError {}

impl From<VarintError> for CodecError {
    fn from(error: VarintError) -> Self {
        Self::new(error.as_str())
    }
}

/// The most bytes a varint of sixty-four bits takes.
pub const MAX_LENGTH: usize = 10;

/// The varint `bytes` open with: its value and how many bytes it took.
///
/// # Errors
/// The bytes end inside the varint, or it runs past ten bytes.
pub fn decode(bytes: &[u8]) -> Result<(u64, usize), VarintError> {
    let mut value: u64 = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if index >= MAX_LENGTH {
            return Err(VarintError::Overlong);
        }
        value |= u64::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err(VarintError::Unterminated)
}

/// `value` as a varint: the inverse of [`decode`].
#[must_use]
pub fn encode(value: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAX_LENGTH);
    encode_into(&mut out, value);
    out
}

/// `value` as a varint, appended to `out`.
pub fn encode_into(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let low = u8::try_from(value & 0x7f).unwrap_or(0);
        value >>= 7;
        if value == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

/// `value` zig-zagged: 0, -1, 1, -2 … to 0, 1, 2, 3 …, so a small negative
/// number is a short varint.
#[must_use]
pub const fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)).cast_unsigned()
}

/// The signed value `encoded` zig-zags: the inverse of [`zigzag`].
#[must_use]
pub const fn unzigzag(encoded: u64) -> i64 {
    (encoded >> 1).cast_signed() ^ -((encoded & 1).cast_signed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_varint_is_base_128_little_endian() {
        assert_eq!(decode(&[0x96, 0x01]), Ok((150, 2)));
        assert_eq!(decode(&[0xac, 0x02, 0xff]), Ok((300, 2)));
        assert_eq!(decode(&[0x00]), Ok((0, 1)));
        let largest = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        assert_eq!(decode(&largest), Ok((u64::MAX, 10)));
        assert_eq!(encode(300), [0xac, 0x02]);
        assert_eq!(encode(u64::MAX), largest);
    }

    #[test]
    fn a_varint_that_cannot_end_is_refused_by_why() {
        assert_eq!(decode(&[]), Err(VarintError::Unterminated));
        assert_eq!(decode(&[0x80]), Err(VarintError::Unterminated));
        assert_eq!(decode(&[0x80; 11]), Err(VarintError::Overlong));
        assert_eq!(
            CodecError::from(VarintError::Overlong).message,
            "a varint runs past ten bytes"
        );
    }

    #[test]
    fn every_width_reads_back_as_itself() {
        for shift in 0..64 {
            let value = 1u64 << shift;
            for value in [value - 1, value, value | 1] {
                let bytes = encode(value);
                assert_eq!(decode(&bytes), Ok((value, bytes.len())), "{value}");
            }
        }
    }

    #[test]
    fn zigzag_interleaves_the_signs_and_comes_back() {
        let pairs = [(0, 0), (-1, 1), (1, 2), (-2, 3), (2, 4)];
        for (signed, unsigned) in pairs {
            assert_eq!(zigzag(signed), unsigned);
            assert_eq!(unzigzag(unsigned), signed);
        }
        assert_eq!(zigzag(i64::MAX), u64::MAX - 1);
        assert_eq!(zigzag(i64::MIN), u64::MAX);
        for value in [i64::MIN, -300, -1, 0, 1, 300, i64::MAX] {
            assert_eq!(unzigzag(zigzag(value)), value);
        }
    }
}
