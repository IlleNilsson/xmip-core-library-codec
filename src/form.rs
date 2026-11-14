//! A value's one binary form: how a record is written after what a buffer
//! holds and read back from where a cursor is, the one trait for it.
//!
//! Until 2026-11-07 two crates each declared it: Xmip Storage's records
//! (`xmip-core-persist`) and a Work Process's forms
//! (`xmip-core-capability-process`), the same two methods over the same
//! [`Cursor`], one answering in the store's error and one in this crate's,
//! and the store wrapping every read of a number to say so. What a value's
//! fields are stays with its crate, written over [`ByteWriter`], [`Cursor`]
//! and [`crate::field`]; a refusal of what the bytes say is a
//! [`CodecError`], and a crate turns it into its own error once, where the
//! bytes came in.
//!
//! A record kept or handed on opens with the number of its form, so a
//! reader of a later form can still tell what an earlier one wrote:
//! [`numbered`] writes it before a value, [`opened`] reads and checks it,
//! refusing another number in words, and [`from_numbered`] reads the value
//! after it. Until 2026-11-13 the Journey's, the Message's, the identity
//! facts', a Subscription's standing and a Work Process's forms each wrote
//! and checked it themselves.
//!
//! [`ByteWriter`]: crate::writer::ByteWriter

use crate::cursor::Cursor;
use crate::writer::ByteWriter;
use crate::{CodecError, Result};

/// A value written in its one binary form and read back from it.
pub trait Form: Sized {
    /// Written after what `out` holds.
    fn write(&self, out: &mut Vec<u8>);

    /// Read from where `cursor` is.
    ///
    /// # Errors
    /// The bytes are cut short or say what no value of it is.
    fn read(cursor: &mut Cursor<'_>) -> Result<Self>;

    /// The value alone, as bytes.
    #[must_use]
    fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write(&mut out);
        out
    }

    /// The value `bytes` hold, and nothing after it.
    ///
    /// # Errors
    /// As [`Form::read`], or bytes are left over after the value.
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(bytes);
        let value = Self::read(&mut cursor)?;
        if cursor.is_empty() {
            Ok(value)
        } else {
            Err(CodecError::new(format!(
                "{} bytes left after the value",
                cursor.remaining().len()
            )))
        }
    }
}

/// A value boxed is the value: a large one is kept on the heap.
impl<T: Form> Form for Box<T> {
    fn write(&self, out: &mut Vec<u8>) {
        (**self).write(out);
    }

    fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        T::read(cursor).map(Box::new)
    }
}

/// A run of values: their count, four bytes big-endian, then each in its
/// form. A run counted by a varint is [`crate::field::many`].
impl<T: Form> Form for Vec<T> {
    fn write(&self, out: &mut Vec<u8>) {
        out.u32_be(u32::try_from(self.len()).unwrap_or(u32::MAX));
        for item in self {
            item.write(out);
        }
    }

    fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        let count = cursor.u32_be()?;
        (0..count).map(|_| T::read(cursor)).collect()
    }
}

/// `value` after `number`, the number of its form, written after what `out`
/// holds.
pub fn numbered<T: Form>(out: &mut Vec<u8>, number: u8, value: &T) {
    out.byte(number);
    value.write(out);
}

/// Read the number a form opens with from where `cursor` is, and go on
/// only where it is `number`.
///
/// # Errors
/// Nothing is there, or another number is, said as `what` of that form.
pub fn opened(cursor: &mut Cursor<'_>, number: u8, what: &str) -> Result<()> {
    let found = cursor.byte()?;
    if found == number {
        Ok(())
    } else {
        Err(CodecError::new(format!(
            "{what} of form {found}, and this reads form {number}"
        )))
    }
}

/// The value `bytes` hold after `number`, the number of its form, and
/// nothing after it.
///
/// # Errors
/// As [`opened`], then as [`Form::from_bytes`].
pub fn from_numbered<T: Form>(bytes: &[u8], number: u8, what: &str) -> Result<T> {
    let mut cursor = Cursor::new(bytes);
    opened(&mut cursor, number, what)?;
    T::from_bytes(cursor.remaining())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field;

    #[derive(Debug, PartialEq, Eq)]
    struct Named {
        number: u64,
        name: String,
    }

    impl Form for Named {
        fn write(&self, out: &mut Vec<u8>) {
            out.u64_be(self.number);
            field::text(out, &self.name);
        }

        fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
            let number = cursor.u64_be()?;
            if number == 0 {
                return Err(CodecError::new("no value is numbered 0"));
            }
            Ok(Self {
                number,
                name: field::read_text(cursor)?,
            })
        }
    }

    fn named(number: u64) -> Named {
        Named {
            number,
            name: format!("n{number}"),
        }
    }

    #[test]
    fn a_value_comes_back_from_its_bytes_as_it_was() {
        let value = named(7);
        assert_eq!(Named::from_bytes(&value.bytes()).expect("read"), value);
        let boxed = Box::new(named(8));
        assert_eq!(
            Box::<Named>::from_bytes(&boxed.bytes()).expect("boxed"),
            boxed
        );
        let run = vec![named(1), named(2)];
        let bytes = run.bytes();
        assert_eq!(&bytes[..4], &[0, 0, 0, 2], "counted in four bytes");
        assert_eq!(Vec::<Named>::from_bytes(&bytes).expect("run"), run);
    }

    #[test]
    fn bytes_cut_short_left_over_or_not_a_value_are_refused() {
        let bytes = named(7).bytes();
        assert!(Named::from_bytes(&bytes[..bytes.len() - 1]).is_err());
        let longer = [bytes.as_slice(), &[0, 0]].concat();
        let refused = Named::from_bytes(&longer).expect_err("left over");
        assert_eq!(refused.message, "2 bytes left after the value");
        let zero = Named {
            number: 0,
            name: String::new(),
        };
        let refused = Named::from_bytes(&zero.bytes()).expect_err("not one");
        assert_eq!(refused.message, "no value is numbered 0");
        assert!(Vec::<Named>::from_bytes(&[0, 0, 0, 1]).is_err());
    }

    #[test]
    fn a_numbered_value_opens_with_its_form_and_another_form_is_refused_in_words() {
        let value = named(7);
        let mut record = Vec::new();
        numbered(&mut record, 3, &value);
        assert_eq!(record[0], 3);
        assert_eq!(&record[1..], value.bytes().as_slice());
        assert_eq!(from_numbered::<Named>(&record, 3, "a name"), Ok(value));
        let refused = from_numbered::<Named>(&record, 4, "a name").expect_err("form 3");
        assert_eq!(refused.message, "a name of form 3, and this reads form 4");
        assert!(from_numbered::<Named>(&[], 3, "a name").is_err());
        let longer = [record.as_slice(), &[0]].concat();
        assert!(from_numbered::<Named>(&longer, 3, "a name").is_err());
        let mut cursor = Cursor::new(&record);
        opened(&mut cursor, 3, "a name").expect("form 3");
        assert_eq!(Named::read(&mut cursor), Ok(named(7)));
    }
}
