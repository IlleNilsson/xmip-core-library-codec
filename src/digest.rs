//! SHA-256 (FIPS 180-4), HMAC (RFC 2104) and PBKDF2 (RFC 8018): the
//! estate's digest, a signed request's and a token's tag, a credential's
//! verifier, an SSH packet's integrity, a certificate's fingerprint and
//! key identifier.
//!
//! One implementation from one backend: SHA-256 from `sha2` and its HMAC
//! from `hmac`, pure Rust, `sha2` on the processor's SHA extensions where
//! it has them. Until 2026-11-13 twelve crates took `sha2` themselves,
//! seven `hmac`, the credential store derived PBKDF2 by hand and
//! provisioning and the TLS library hashed through aws-lc-rs.
//!
//! SHA-1 is [`crate::sha1`], the estate's own; its HMAC and PBKDF2 are
//! here for the one protocol that still names them, Kerberos's
//! `aes256-cts-hmac-sha1-96` (RFC 3962), built on it by RFC 2104's
//! construction ([`hmac_by`]), which `NTLMv2`'s HMAC-MD5 takes too.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Digest;

/// A SHA-256 digest, and an HMAC-SHA256 tag, is thirty-two bytes.
pub const SHA256_LENGTH: usize = 32;

/// The block HMAC pads its key to for SHA-1 and SHA-256 alike.
const BLOCK: usize = 64;

/// The SHA-256 digest of `message`.
#[must_use]
pub fn sha256(message: &[u8]) -> [u8; SHA256_LENGTH] {
    sha2::Sha256::digest(message).into()
}

/// A SHA-256 digest taken in parts: what is written in pieces — a key
/// derivation's inputs, a chain's fields — without joining them first.
#[derive(Clone, Default)]
pub struct Sha256(sha2::Sha256);

impl core::fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Sha256")
    }
}

impl Sha256 {
    /// A digest of nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `bytes` taken in after what came before.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The digest of everything taken in.
    #[must_use]
    pub fn finish(self) -> [u8; SHA256_LENGTH] {
        self.0.finalize().into()
    }
}

/// HMAC-SHA256 of `message` under `key`.
#[must_use]
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; SHA256_LENGTH] {
    let mut mac = HmacSha256::new(key);
    mac.update(message);
    mac.finish()
}

/// An HMAC-SHA256 taken in parts — a sequence number then a packet, a
/// string to sign built up — then finished as a tag or checked against
/// one in constant time.
#[derive(Clone)]
pub struct HmacSha256(Hmac<sha2::Sha256>);

impl HmacSha256 {
    /// Keyed by `key`, of any length, over nothing yet.
    ///
    /// # Panics
    ///
    /// Never: HMAC takes a key of any length (RFC 2104 section 3).
    #[must_use]
    pub fn new(key: &[u8]) -> Self {
        Self(Hmac::new_from_slice(key).expect("HMAC takes a key of any length"))
    }

    /// `bytes` taken in after what came before.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The tag over everything taken in.
    #[must_use]
    pub fn finish(self) -> [u8; SHA256_LENGTH] {
        self.0.finalize().into_bytes().into()
    }

    /// Whether `tag` is the tag over everything taken in, compared in time
    /// that never depends on where the two differ.
    #[must_use]
    pub fn verify(self, tag: &[u8]) -> bool {
        self.0.verify_slice(tag).is_ok()
    }
}

impl core::fmt::Debug for HmacSha256 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("HmacSha256(key not shown)")
    }
}

/// HMAC-SHA1 of `message` under `key`, over the estate's SHA-1.
#[must_use]
pub fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; crate::sha1::DIGEST_LENGTH] {
    hmac_by(crate::sha1::digest, key, message)
}

/// The HMAC of `message` under `key` by RFC 2104's construction over
/// `hash`, any digest of a 64-byte block given whole: SHA-1 here, and
/// MD5, which `NTLMv2` (MS-NLMP) still names, where its gate holds it.
#[must_use]
pub fn hmac_by<const N: usize>(
    hash: impl Fn(&[u8]) -> [u8; N],
    key: &[u8],
    message: &[u8],
) -> [u8; N] {
    let mut padded = [0u8; BLOCK];
    if key.len() > BLOCK {
        padded[..N].copy_from_slice(&hash(key));
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let keyed = |pad: u8, then: &[u8]| {
        let mut input: Vec<u8> = padded.iter().map(|byte| byte ^ pad).collect();
        input.extend_from_slice(then);
        hash(&input)
    };
    keyed(0x5c, &keyed(0x36, message))
}

/// PBKDF2-HMAC-SHA256 of `password` with `salt`, `iterations` rounds,
/// filling `out`. Zero iterations are taken as one.
pub fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) {
    let keyed = HmacSha256::new(password);
    pbkdf2(
        |data| {
            let mut mac = keyed.clone();
            mac.update(data);
            mac.finish()
        },
        salt,
        iterations,
        out,
    );
}

/// PBKDF2-HMAC-SHA1 of `password` with `salt`, `iterations` rounds,
/// filling `out`. Zero iterations are taken as one.
pub fn pbkdf2_hmac_sha1(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) {
    pbkdf2(|data| hmac_sha1(password, data), salt, iterations, out);
}

/// RFC 8018 section 5.2 over the keyed `prf`: each block of `out` the
/// exclusive or of `iterations` chained applications to the salt and the
/// block's index.
fn pbkdf2<const N: usize>(
    prf: impl Fn(&[u8]) -> [u8; N],
    salt: &[u8],
    iterations: u32,
    out: &mut [u8],
) {
    let mut salted = Vec::with_capacity(salt.len() + 4);
    for (index, chunk) in (1u32..).zip(out.chunks_mut(N)) {
        salted.clear();
        salted.extend_from_slice(salt);
        salted.extend_from_slice(&index.to_be_bytes());
        let mut block = prf(&salted);
        let mut derived = block;
        for _ in 1..iterations.max(1) {
            block = prf(&block);
            for (kept, next) in derived.iter_mut().zip(block) {
                *kept ^= next;
            }
        }
        chunk.copy_from_slice(&derived[..chunk.len()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    #[test]
    fn the_fips_180_examples_digest_to_their_published_values() {
        assert_eq!(
            hex::encode(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let two = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            hex::encode(&sha256(two)),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        let mut parts = Sha256::new();
        parts.update(&two[..20]);
        parts.update(&two[20..]);
        assert_eq!(parts.finish(), sha256(two));
    }

    /// RFC 4231 test cases 1, 2 and 6 (a key longer than the block), and
    /// RFC 2202's 1, 2 and 6 for SHA-1.
    #[test]
    fn the_published_test_cases_sign_to_their_tags() {
        let jefe = b"what do ya want for nothing?";
        let long = b"Test Using Larger Than Block-Size Key - Hash Key First";
        assert_eq!(
            hex::encode(&hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex::encode(&hmac_sha256(b"Jefe", jefe)),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex::encode(&hmac_sha256(&[0xaa; 131], long)),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
        assert_eq!(
            hex::encode(&hmac_sha1(&[0x0b; 20], b"Hi There")),
            "b617318655057264e28bc0b6fb378c8ef146be00"
        );
        assert_eq!(
            hex::encode(&hmac_sha1(b"Jefe", jefe)),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
        assert_eq!(
            hex::encode(&hmac_sha1(&[0xaa; 80], long)),
            "aa4ae5e15272d00e95705637ce8a3b55ed402112"
        );
    }

    #[test]
    fn a_tag_taken_in_parts_is_the_whole_ones_and_checks_only_against_it() {
        let mut parts = HmacSha256::new(b"Jefe");
        parts.update(b"what do ya want ");
        parts.update(b"for nothing?");
        let whole = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(parts.clone().finish(), whole);
        assert!(parts.clone().verify(&whole));
        let mut altered = whole;
        altered[31] ^= 1;
        assert!(!parts.clone().verify(&altered));
        assert!(!parts.verify(&whole[..16]));
    }

    /// RFC 6070's vectors for HMAC-SHA1 (one block, and two of 25 bytes),
    /// and RFC 7914 section 11's for HMAC-SHA256.
    #[test]
    fn the_published_vectors_derive_to_their_keys() {
        let mut out = [0u8; 20];
        pbkdf2_hmac_sha1(b"password", b"salt", 1, &mut out);
        assert_eq!(
            hex::encode(&out),
            "0c60c80f961f0e71f3a9b524af6012062fe037a6"
        );
        pbkdf2_hmac_sha1(b"password", b"salt", 4096, &mut out);
        assert_eq!(
            hex::encode(&out),
            "4b007901b765489abead49d926f721d065a429c1"
        );
        let mut longer = [0u8; 25];
        let (password, salt) = (
            b"passwordPASSWORDpassword",
            b"saltSALTsaltSALTsaltSALTsaltSALTsalt",
        );
        pbkdf2_hmac_sha1(password, salt, 4096, &mut longer);
        assert_eq!(
            hex::encode(&longer),
            "3d2eec4fe41c849b80c8d83662c0e44a8b291a964cf2f07038"
        );

        let mut long = [0u8; 64];
        pbkdf2_hmac_sha256(b"passwd", b"salt", 1, &mut long);
        assert_eq!(
            hex::encode(&long),
            "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc\
             49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"
        );
        let (mut zero, mut one) = ([0u8; 32], [0u8; 32]);
        pbkdf2_hmac_sha256(b"p", b"s", 0, &mut zero);
        pbkdf2_hmac_sha256(b"p", b"s", 1, &mut one);
        assert_eq!(zero, one);
    }
}
