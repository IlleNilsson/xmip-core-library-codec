//! Comparing a secret without telling the peer how much of it was right.
//!
//! A password, a token, a key, a MAC or a scramble compared with `==`
//! returns at the first byte that differs, and the time that takes is the
//! length of the matching prefix: a peer that measures it guesses a secret
//! one byte at a time. [`equal`] takes time that depends on the lengths and
//! never on where the bytes differ. Every comparison of a secret in the
//! estate calls it; until 2026-09-27 the authenticate capability held the
//! only copy and a dozen far ends compared with `!=`.

/// Whether `left` and `right` are the same bytes, in time that depends on
/// their lengths and never on where they differ. Unequal lengths are
/// unequal at once: a length is not the secret.
#[must_use]
pub fn equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let difference = left
        .iter()
        .zip(right)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b));
    core::hint::black_box(difference) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_constant_time_compare_is_still_a_compare() {
        assert!(equal(b"abc", b"abc"));
        assert!(!equal(b"abc", b"abd"));
        assert!(!equal(b"abc", b"ab"));
        assert!(!equal(b"xbc", b"abc"));
        assert!(equal(b"", b""));
    }
}
