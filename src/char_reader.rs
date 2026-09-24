//! A reader over text one character at a time: what every lexer in the
//! estate walks its source with.
//!
//! Every position the reader gives is a byte offset on a character boundary,
//! so a lexer may slice the text at any of them. Until 2026-09-24 each lexer
//! counted for itself, and two stepped over whitespace as one byte: U+00A0 is
//! two, and the next slice landed inside it and panicked. Lines and columns
//! count from 1; a column counts characters, not bytes. What a character
//! means — whitespace, a name, a quote — is the language's own; the reader
//! offers Unicode whitespace because that is the one test every lexer shares.

/// A position in `text`, always on a character boundary.
#[derive(Clone, Debug)]
pub struct CharReader<'a> {
    text: &'a str,
    at: usize,
    line: usize,
    column: usize,
}

impl<'a> CharReader<'a> {
    /// At the first character of `text`, line 1, column 1.
    #[must_use]
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            at: 0,
            line: 1,
            column: 1,
        }
    }

    /// The whole text being read.
    #[must_use]
    pub const fn text(&self) -> &'a str {
        self.text
    }

    /// The byte offset of the next character.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.at
    }

    /// The line of the next character, from 1.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// The column of the next character in characters, from 1.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }

    /// Everything not yet read.
    #[must_use]
    pub fn rest(&self) -> &'a str {
        self.text.get(self.at..).unwrap_or_default()
    }

    /// What was read since `start`, an offset this reader gave; empty for an
    /// offset it did not.
    #[must_use]
    pub fn since(&self, start: usize) -> &'a str {
        self.text.get(start..self.at).unwrap_or_default()
    }

    /// Whether everything has been read.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.at >= self.text.len()
    }

    /// The next character, not taken.
    #[must_use]
    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    /// The character `ahead` past the next, not taken: `peek_nth(0)` is
    /// `peek()`.
    #[must_use]
    pub fn peek_nth(&self, ahead: usize) -> Option<char> {
        self.rest().chars().nth(ahead)
    }

    /// Whether the text not yet read opens with `prefix`.
    #[must_use]
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    /// The characters from here while `keep` holds, not taken.
    #[must_use]
    pub fn peek_while(&self, mut keep: impl FnMut(char) -> bool) -> &'a str {
        let rest = self.rest();
        let end = rest
            .char_indices()
            .find(|&(_, character)| !keep(character))
            .map_or(rest.len(), |(at, _)| at);
        rest.get(..end).unwrap_or_default()
    }

    /// The next character, taken; `None` at the end, where the reader stays.
    pub fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.at += character.len_utf8();
        if character == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(character)
    }

    /// Take `expected` if it is next.
    pub fn eat(&mut self, expected: char) -> bool {
        let found = self.peek() == Some(expected);
        if found {
            self.bump();
        }
        found
    }

    /// Take `prefix` if the text not yet read opens with it.
    pub fn eat_str(&mut self, prefix: &str) -> bool {
        let found = self.starts_with(prefix);
        if found {
            for _ in prefix.chars() {
                self.bump();
            }
        }
        found
    }

    /// The characters from here while `keep` holds, taken.
    pub fn take_while(&mut self, keep: impl FnMut(char) -> bool) -> &'a str {
        let taken = self.peek_while(keep);
        self.eat_str(taken);
        taken
    }

    /// Past every Unicode whitespace character from here; whether there was
    /// any.
    pub fn skip_whitespace(&mut self) -> bool {
        !self.take_while(char::is_whitespace).is_empty()
    }

    /// Through the next `end`, taken with it; `false` when the text ends
    /// first, and everything is read.
    pub fn skip_past(&mut self, end: &str) -> bool {
        while !self.is_done() {
            if self.eat_str(end) {
                return true;
            }
            self.bump();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multibyte_characters_are_one_step_each() {
        let mut reader = CharReader::new("a\u{a0}\u{3000}é\u{1f600}b");
        assert_eq!(reader.bump(), Some('a'));
        assert!(reader.skip_whitespace());
        assert_eq!(reader.offset(), 1 + 2 + 3);
        assert_eq!(reader.column(), 4);
        assert_eq!(reader.take_while(char::is_alphabetic), "é");
        assert_eq!(reader.peek(), Some('\u{1f600}'));
        assert_eq!(reader.peek_nth(1), Some('b'));
        assert_eq!(reader.bump(), Some('\u{1f600}'));
        assert_eq!(reader.rest(), "b");
        assert_eq!(reader.since(1), "\u{a0}\u{3000}é\u{1f600}");
        assert_eq!(reader.bump(), Some('b'));
        assert!(reader.is_done());
        assert_eq!(reader.bump(), None);
        assert_eq!(reader.peek(), None);
    }

    #[test]
    fn lines_and_columns_follow_the_newlines() {
        let mut reader = CharReader::new("ab\n\u{a0}c");
        assert!(reader.eat_str("ab\n"));
        assert_eq!((reader.line(), reader.column()), (2, 1));
        reader.skip_whitespace();
        assert_eq!((reader.line(), reader.column()), (2, 2));
        assert!(!reader.eat('x'));
        assert!(reader.eat('c'));
    }

    #[test]
    fn peeking_takes_nothing_and_skipping_finds_its_end() {
        let mut reader = CharReader::new("12.5x /* c\u{a0} */ y");
        assert_eq!(
            reader.peek_while(|c| c.is_ascii_digit() || c == '.'),
            "12.5"
        );
        assert_eq!(reader.offset(), 0);
        assert!(reader.starts_with("12"));
        reader.take_while(|c| c != ' ');
        reader.skip_whitespace();
        assert!(reader.eat_str("/*"));
        assert!(reader.skip_past("*/"));
        assert_eq!(reader.rest(), " y");
        assert!(!reader.skip_past("*/"));
        assert!(reader.is_done());
        assert_eq!(reader.since(usize::MAX), "");
    }
}
