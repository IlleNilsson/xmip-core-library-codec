//! PEM (RFC 7468): DER in base64 between a label's two lines — what a
//! certificate, a chain, a set of anchors, a revocation list, a request
//! and a private key are written as, and `OPENSSH PRIVATE KEY` too.
//!
//! One reader and one writer for the estate. Until 2026-11-13 the TLS
//! library read through rustls-pki-types' `PemObject`, the authentication
//! gates through it again, provisioning, the SAML gate and SSH each by hand.

use crate::{CodecError, Result, base64};

/// How many base64 characters a written line holds (RFC 7468 section 2).
const LINE: usize = 64;

/// `der` as one PEM block under `label`, lines of 64 characters.
#[must_use]
pub fn encode(label: &str, der: &[u8]) -> String {
    encode_wrapped(label, der, LINE)
}

/// `der` as one PEM block under `label`, lines of `width` characters: 70
/// where `ssh-keygen` writes `OPENSSH PRIVATE KEY`, which any reader takes.
#[must_use]
pub fn encode_wrapped(label: &str, der: &[u8], width: usize) -> String {
    let text = base64::encode(der);
    let width = width.max(4);
    let mut out = String::with_capacity(text.len() + text.len() / width + 2 * label.len() + 34);
    out.push_str("-----BEGIN ");
    out.push_str(label);
    out.push_str("-----\n");
    for line in text.as_bytes().chunks(width) {
        out.push_str(&String::from_utf8_lossy(line));
        out.push('\n');
    }
    out.push_str("-----END ");
    out.push_str(label);
    out.push_str("-----\n");
    out
}

/// Every block under `label` in `text`, in order; a block under another
/// label is passed over unread.
///
/// # Errors
///
/// Where a block under `label` does not end, or its body is not base64.
pub fn decode(label: &str, text: &str) -> Result<Vec<Vec<u8>>> {
    let mut found = Vec::new();
    for block in Blocks::new(text) {
        let (named, body) = block?;
        if named == label {
            found.push(body_of(named, body)?);
        }
    }
    Ok(found)
}

/// The first block in `text` whose label is one of `labels`, the label and
/// its DER: a private key, say, which PEM writes under one of three.
/// `None` where `text` holds none.
///
/// # Errors
///
/// Where a block does not end, or that block's body is not base64.
pub fn first_of<'a>(labels: &[&'a str], text: &str) -> Result<Option<(&'a str, Vec<u8>)>> {
    for block in Blocks::new(text) {
        let (named, body) = block?;
        if let Some(label) = labels.iter().find(|label| **label == named) {
            return Ok(Some((label, body_of(named, body)?)));
        }
    }
    Ok(None)
}

/// A block's body, its whitespace dropped, read as base64.
fn body_of(label: &str, body: &str) -> Result<Vec<u8>> {
    let packed: String = body.split_whitespace().collect();
    base64::decode(&packed)
        .map_err(|failure| CodecError::new(format!("a {label} block: {}", failure.message)))
}

/// The blocks of a text in order: each its label and its body unread.
struct Blocks<'a> {
    rest: &'a str,
}

impl<'a> Blocks<'a> {
    const fn new(text: &'a str) -> Self {
        Self { rest: text }
    }
}

impl<'a> Iterator for Blocks<'a> {
    type Item = Result<(&'a str, &'a str)>;

    fn next(&mut self) -> Option<Self::Item> {
        const BEGIN: &str = "-----BEGIN ";
        const DASHES: &str = "-----";
        let start = self.rest.find(BEGIN)?;
        let after = &self.rest[start + BEGIN.len()..];
        let Some(close) = after.find(DASHES) else {
            self.rest = "";
            return Some(Err(CodecError::new("a PEM label that does not close")));
        };
        let label = &after[..close];
        let body = &after[close + DASHES.len()..];
        let end = format!("-----END {label}-----");
        let Some(stop) = body.find(&end) else {
            self.rest = "";
            return Some(Err(CodecError::new(format!(
                "a {label} block that does not end"
            ))));
        };
        self.rest = &body[stop + end.len()..];
        Some(Ok((label, &body[..stop])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_round_trips_and_other_labels_are_passed_over() {
        let der: Vec<u8> = (0..=200).collect();
        let text = format!(
            "{}{}",
            encode("PRIVATE KEY", b"secret"),
            encode("CERTIFICATE", &der)
        );
        assert!(text.lines().all(|line| line.len() <= LINE));
        assert_eq!(decode("CERTIFICATE", &text).expect("reads"), vec![der]);
        assert_eq!(
            decode("CERTIFICATE REQUEST", &text).expect("none"),
            [] as [Vec<u8>; 0]
        );
        assert!(decode("CERTIFICATE", "-----BEGIN CERTIFICATE-----\nAA").is_err());
    }

    #[test]
    fn a_label_is_whole_so_a_request_is_never_a_certificate() {
        let text = encode("CERTIFICATE REQUEST", b"asked");
        assert_eq!(
            decode("CERTIFICATE", &text).expect("reads"),
            [] as [Vec<u8>; 0]
        );
        assert_eq!(
            decode("CERTIFICATE REQUEST", &text).expect("reads"),
            [b"asked"]
        );
    }

    #[test]
    fn lines_ended_either_way_and_text_around_the_blocks_read_alike() {
        let written = encode("CERTIFICATE", &[7; 100]);
        let crlf = format!(
            "subject=CN=x\r\n{}\r\ntrailing",
            written.replace('\n', "\r\n")
        );
        assert_eq!(decode("CERTIFICATE", &crlf).expect("reads"), [vec![7; 100]]);
    }

    #[test]
    fn the_first_of_several_labels_is_found_in_text_order() {
        let text = format!(
            "{}{}",
            encode("EC PRIVATE KEY", b"sec1"),
            encode("PRIVATE KEY", b"pkcs8")
        );
        let labels = ["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"];
        let (label, der) = first_of(&labels, &text).expect("reads").expect("a key");
        assert_eq!((label, der.as_slice()), ("EC PRIVATE KEY", &b"sec1"[..]));
        assert!(first_of(&labels, "no key here").expect("reads").is_none());
    }

    #[test]
    fn a_body_that_is_not_base64_is_refused_naming_its_label() {
        let text = "-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----\n";
        let refused = decode("CERTIFICATE", text).expect_err("not base64");
        assert!(refused.message.contains("CERTIFICATE"), "{refused}");
        let unclosed = decode("CERTIFICATE", "-----BEGIN CERTIFICATE").expect_err("open");
        assert!(unclosed.message.contains("label"), "{unclosed}");
    }
}
