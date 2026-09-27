//! Random bytes from the operating system's random source.
//!
//! One source for the whole estate: a key, a salt, a nonce, a challenge, a
//! masking key and an id nobody else carries all come from here, and here
//! comes from the operating system's cryptographically secure generator
//! (`ProcessPrng` on Windows, `getrandom(2)` on Linux, `getentropy` on the
//! BSDs and macOS), never from the clock and never from a generator this
//! process seeds. Until 2026-09-27 each protocol drew its own from the clock
//! and a counter, and a server challenge was as guessable as the time.

/// Fill `bytes` from the operating system's random source.
///
/// # Panics
///
/// Where the operating system has no random source to draw from. Every
/// system Xmip runs on has one that does not fail once it is seeded — as the
/// standard library's `HashMap` keys assume — so a failure is a machine
/// that cannot be trusted with a secret, not a condition to carry on past.
pub fn fill(bytes: &mut [u8]) {
    getrandom::fill(bytes).expect("the operating system's random source failed");
}

/// `N` bytes from the operating system's random source.
///
/// # Panics
///
/// As [`fill`].
#[must_use]
pub fn array<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    fill(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_draws_differ_and_a_fill_covers_every_byte() {
        let first: [u8; 32] = array();
        let second: [u8; 32] = array();
        assert_ne!(first, second, "two draws of 256 bits");

        let mut long = vec![0u8; 4096];
        fill(&mut long);
        let seen: std::collections::BTreeSet<u8> = long.iter().copied().collect();
        assert!(seen.len() > 200, "{} byte values of 256 seen", seen.len());
    }
}
