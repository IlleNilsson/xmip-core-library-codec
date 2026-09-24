//! SQL's delimited text: a string literal or a quoted identifier, written
//! between two delimiters with the closing one doubled inside, and read
//! back with the doubling undone (ISO/IEC 9075, 5.3 and 5.4).
//!
//! Which delimiters a server uses is its dialect's to say: `'…'` for a
//! literal everywhere, `"…"` for an identifier in ISO and `PostgreSQL`,
//! `[…]` in T-SQL, `` `…` `` in `MySQL`. What else a dialect adds — T-SQL's
//! `N` before a Unicode literal, `MySQL`'s backslash escapes — stays with
//! the dialect. Until 2026-09-24 the SQL transports, the SQL script archive
//! and the SQL contract each wrote this rule themselves, and the `MySQL` and
//! `PostgreSQL` far ends read an identifier's doubled delimiter as its end.

use crate::char_reader::CharReader;
use crate::{CodecError, Result};

/// The two characters a run of SQL text is written between.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Delimiter {
    pub open: char,
    pub close: char,
}

impl Delimiter {
    /// A string literal: `'it''s'`.
    pub const STRING: Self = Self::new('\'', '\'');
    /// An ISO identifier, as `PostgreSQL` and Oracle write one: `"a ""b"""`.
    pub const IDENTIFIER: Self = Self::new('"', '"');
    /// A T-SQL identifier: `[in]]box]`.
    pub const BRACKET: Self = Self::new('[', ']');
    /// A `MySQL` identifier, in backticks with a backtick in it doubled.
    pub const BACKTICK: Self = Self::new('`', '`');

    /// Text written between `open` and `close`.
    #[must_use]
    pub const fn new(open: char, close: char) -> Self {
        Self { open, close }
    }

    /// `text` between the delimiters, every closing delimiter in it doubled.
    #[must_use]
    pub fn quote(self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 2);
        out.push(self.open);
        for character in text.chars() {
            out.push(character);
            if character == self.close {
                out.push(character);
            }
        }
        out.push(self.close);
        out
    }

    /// The run `reader` stands at, read past its closing delimiter, the
    /// doubling undone.
    ///
    /// # Errors
    /// The text does not open with the opening delimiter, or ends before
    /// the run closes.
    pub fn read(self, reader: &mut CharReader<'_>) -> Result<String> {
        if !reader.eat(self.open) {
            return Err(CodecError::new(format!("no {} opens the text", self.open)));
        }
        let mut out = String::new();
        loop {
            match reader.bump() {
                Some(character) if character == self.close => {
                    if !reader.eat(self.close) {
                        return Ok(out);
                    }
                    out.push(character);
                }
                Some(character) => out.push(character),
                None => {
                    return Err(CodecError::new(format!(
                        "the text opened with {} is not terminated",
                        self.open
                    )));
                }
            }
        }
    }

    /// The run `text` opens with, and what follows it.
    ///
    /// # Errors
    /// As [`Delimiter::read`].
    pub fn unquote_prefix(self, text: &str) -> Result<(String, &str)> {
        let mut reader = CharReader::new(text);
        let run = self.read(&mut reader)?;
        Ok((run, reader.rest()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_doubles_the_closing_delimiter_and_nothing_else() {
        assert_eq!(Delimiter::STRING.quote("it's"), "'it''s'");
        assert_eq!(Delimiter::STRING.quote("back\\slash"), "'back\\slash'");
        assert_eq!(Delimiter::STRING.quote(""), "''");
        assert_eq!(Delimiter::IDENTIFIER.quote("in\"box"), "\"in\"\"box\"");
        assert_eq!(Delimiter::BRACKET.quote("[in]box]"), "[[in]]box]]]");
        assert_eq!(Delimiter::BACKTICK.quote("in`box"), "`in``box`");
    }

    #[test]
    fn every_quoted_text_reads_back_as_itself_with_what_follows() {
        let texts = [
            "",
            "it's",
            "''",
            "a ]] b",
            "Zoë 名前\u{a0}\u{1f600}",
            "`\"[]'",
        ];
        for delimiter in [
            Delimiter::STRING,
            Delimiter::IDENTIFIER,
            Delimiter::BRACKET,
            Delimiter::BACKTICK,
        ] {
            for text in texts {
                let quoted = format!("{}, rest", delimiter.quote(text));
                let (back, after) = delimiter.unquote_prefix(&quoted).expect(text);
                assert_eq!((back.as_str(), after), (text, ", rest"), "{delimiter:?}");
            }
        }
    }

    #[test]
    fn a_run_that_does_not_open_or_never_closes_is_refused_without_panic() {
        assert!(Delimiter::STRING.unquote_prefix("x'").is_err());
        let quoted = Delimiter::STRING.quote("Zoë's 名前");
        for cut in (1..quoted.len()).filter(|&at| quoted.is_char_boundary(at)) {
            // A cut between a doubled quote reads as a shorter literal.
            match Delimiter::STRING.unquote_prefix(&quoted[..cut]) {
                Ok((text, after)) => assert_eq!((text.as_str(), after), ("Zoë", "")),
                Err(refused) => assert!(refused.message.contains("not terminated")),
            }
        }
        let failure = Delimiter::BRACKET
            .unquote_prefix("[open")
            .expect_err("open");
        assert!(failure.message.contains("not terminated"), "{failure}");
    }
}
