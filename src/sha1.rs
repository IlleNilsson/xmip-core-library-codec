//! SHA-1 (FIPS 180-4 section 6.1), where a protocol still names it: the
//! WebSocket accept key (RFC 6455) and `MySQL`'s native password.
//!
//! Broken for what a hash is meant for, and never chosen by Xmip where it
//! has the choice; the estate's archives and signatures use SHA-256. Until
//! 2026-09-24 the two protocols each carried the eighty rounds.

/// A SHA-1 digest is twenty bytes.
pub const DIGEST_LENGTH: usize = 20;

/// The SHA-1 digest of `message`.
///
/// The working variables a to e and the round index keep the standard's
/// single-letter names, so the lint is off for this one function rather
/// than renaming what everyone reads letter for letter.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn digest(message: &[u8]) -> [u8; DIGEST_LENGTH] {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    let bits = u64::try_from(message.len())
        .unwrap_or(u64::MAX)
        .wrapping_mul(8);
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bits.to_be_bytes());

    for block in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (word, bytes) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut out = [0u8; DIGEST_LENGTH];
    for (bytes, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *bytes = word.to_be_bytes();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    #[test]
    fn the_fips_180_examples_digest_to_their_published_values() {
        let cases: [(&[u8], &str); 3] = [
            (b"", "da39a3ee5e6b4b0d3255bfef95601890afd80709"),
            (b"abc", "a9993e364706816aba3e25717850c26c9cd0d89d"),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "84983e441c3bd26ebaae4aa1f95129e5e54670f1",
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(hex::encode(&digest(message)), expected);
        }
    }

    #[test]
    fn a_million_times_a_spans_many_blocks_and_digests_as_published() {
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex::encode(&digest(&million)),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    #[test]
    fn a_message_at_every_padding_boundary_digests_to_twenty_bytes() {
        for length in [55, 56, 63, 64, 65] {
            assert_eq!(digest(&vec![0; length]).len(), DIGEST_LENGTH);
        }
        assert_ne!(digest(&[0; 55]), digest(&[0; 56]));
    }
}
