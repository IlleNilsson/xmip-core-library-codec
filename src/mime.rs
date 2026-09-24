//! MIME as every layer writes and reads it: a header's media type and its
//! parameters (RFC 2045 section 5.1), and a multipart body (RFC 2046
//! section 5.1) — parts, each its headers and its bytes, between the
//! delimiters of a boundary.
//!
//! A delimiter is `--boundary` at the start of a line, followed by
//! transport padding and the line's end, or by the `--` that closes the
//! body; a part's bytes are exactly what lies between its blank line and
//! the next delimiter, less the line break that belongs to the delimiter.
//! The preamble and the epilogue are dropped. Every part is written
//! `binary`: its bytes as they are, between a boundary [`boundary`] makes
//! unique to this process.
//!
//! Until 2026-09-24 AS2's receipt, AS4's SOAP with attachments and MSMQ's
//! SRMP each wrote and read a multipart body of their own, and the message
//! capability a fourth: MSMQ took a boundary in the middle of a line as a
//! delimiter, AS2 split the body wherever the boundary's text appeared, and
//! AS2 read a boundary parameter only in lower case with nothing around
//! its `=`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The longest boundary RFC 2046 allows.
const BOUNDARY_MAX: usize = 70;

/// The media type of a header value without its parameters, lower-case:
/// `multipart/related` of `Multipart/Related; boundary=b1`.
#[must_use]
pub fn media_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// The `name` parameter of a media type or a header value — `charset` of
/// `text/csv; charset=utf-8`, `boundary` of a multipart type, `name` of a
/// Content-Disposition — matched without regard to case, its quotes taken
/// off. A value whose quote never closes is kept as it came.
#[must_use]
pub fn parameter<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    value.split(';').skip(1).find_map(|parameter| {
        let (key, value) = parameter.split_once('=')?;
        if !key.trim().eq_ignore_ascii_case(name) {
            return None;
        }
        let value = value.trim();
        Some(
            value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value),
        )
    })
}

/// One part of a multipart body: its headers, in order, and its bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Part {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Part {
    /// A part carrying `body`, no headers yet.
    #[must_use]
    pub fn new(body: impl Into<Vec<u8>>) -> Self {
        Self {
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// With one more header.
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// One header's value, however its name was capitalised.
    #[must_use]
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The part's `Content-Type`.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.header_value("Content-Type")
    }

    /// The part's `Content-ID`, its angle brackets off.
    #[must_use]
    pub fn content_id(&self) -> Option<&str> {
        self.header_value("Content-ID").map(|value| {
            value
                .strip_prefix('<')
                .and_then(|v| v.strip_suffix('>'))
                .unwrap_or(value)
        })
    }
}

/// Why a body is not the multipart its boundary says, and where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: &'static str,
    /// The byte the reason was found at.
    pub offset: usize,
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} at byte {}", self.reason, self.offset)
    }
}

impl core::error::Error for Refusal {}

/// A boundary no body from this process is written around: the moment and
/// a count, so two in one nanosecond still differ.
#[must_use]
pub fn boundary() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("=_xmip_{nanos:x}_{count:x}")
}

/// `parts` as one multipart body under `boundary`.
#[must_use]
pub fn write(boundary: &str, parts: &[Part]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        for (name, value) in &part.headers {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&part.body);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    out
}

/// The boundary a body declares by opening with `--boundary` on its first
/// line, where no media type declared one.
#[must_use]
pub fn opening_boundary(bytes: &[u8]) -> Option<&str> {
    let rest = bytes.strip_prefix(b"--")?;
    let end = rest.iter().position(|b| *b == b'\r' || *b == b'\n')?;
    let boundary = rest[..end].trim_ascii_end();
    let sound = !boundary.is_empty()
        && boundary.len() <= BOUNDARY_MAX
        && boundary
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"'()+_,-./:=?".contains(b));
    sound.then(|| std::str::from_utf8(boundary).ok()).flatten()
}

/// The parts between the delimiters of `boundary`.
///
/// # Errors
/// No opening delimiter, no closing delimiter, or a part whose headers
/// never end.
pub fn read(bytes: &[u8], boundary: &str) -> Result<Vec<Part>, Refusal> {
    let delimiter = [b"--", boundary.as_bytes()].concat();
    let mut at = delimiter_at(bytes, &delimiter, 0).ok_or(Refusal {
        reason: "no opening boundary",
        offset: 0,
    })?;
    let mut parts = Vec::new();
    loop {
        at += delimiter.len();
        if bytes[at..].starts_with(b"--") {
            return Ok(parts);
        }
        let start = past_line(bytes, at);
        let Some(next) = delimiter_at(bytes, &delimiter, start) else {
            return Err(Refusal {
                reason: "no closing boundary",
                offset: bytes.len(),
            });
        };
        parts.push(part(&bytes[start..next], start)?);
        at = next;
    }
}

/// The next `delimiter` at the start of a line, at or after `from`.
fn delimiter_at(bytes: &[u8], delimiter: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    loop {
        let found = at
            + bytes
                .get(at..)?
                .windows(delimiter.len())
                .position(|w| w == delimiter)?;
        let on_a_line = found == 0 || bytes[found - 1] == b'\n';
        if on_a_line && closes_the_line(&bytes[found + delimiter.len()..]) {
            return Some(found);
        }
        at = found + 1;
    }
}

/// Whether what follows a delimiter is a line's end — transport padding,
/// then a line break or the end of the body — or the `--` that closes.
fn closes_the_line(after: &[u8]) -> bool {
    let rest = after
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'))
        .map_or(&[][..], |n| &after[n..]);
    rest.is_empty() || rest.starts_with(b"--") || rest[0] == b'\r' || rest[0] == b'\n'
}

/// The byte after the line break that ends the line at `at`.
fn past_line(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(bytes.len(), |n| at + n + 1)
}

/// One part from its headers and body, the body's trailing line break
/// (which belongs to the delimiter after it) removed.
fn part(section: &[u8], offset: usize) -> Result<Part, Refusal> {
    let (head, body) = split_headers(section).ok_or(Refusal {
        reason: "no blank line after the part headers",
        offset,
    })?;
    let body = body
        .strip_suffix(b"\r\n")
        .or_else(|| body.strip_suffix(b"\n"))
        .unwrap_or(body);
    Ok(Part {
        headers: headers(&String::from_utf8_lossy(head)),
        body: body.to_vec(),
    })
}

/// The headers and the body, cut at the first blank line.
fn split_headers(section: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut at = 0;
    while at < section.len() {
        let line_end = section[at..].iter().position(|b| *b == b'\n')?;
        let line = section[at..at + line_end]
            .strip_suffix(b"\r")
            .unwrap_or(&section[at..at + line_end]);
        if line.is_empty() {
            return Some((&section[..at], &section[at + line_end + 1..]));
        }
        at += line_end + 1;
    }
    None
}

/// Each header, a line that opens with a space or a tab continuing the one
/// before it (RFC 5322 section 2.2.3).
fn headers(head: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in head.lines() {
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = out.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            out.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parameter_is_found_by_name_regardless_of_case_and_unquoted() {
        assert_eq!(parameter("a/b; x=\"1\"; Y=2", "y"), Some("2"));
        assert_eq!(parameter("a/b; x=\"1\"; Y=2", "x"), Some("1"));
        assert_eq!(
            parameter("multipart/report; Boundary = \"q r\"", "boundary"),
            Some("q r"),
            "AS2 read only boundary= until 2026-09-24"
        );
        assert_eq!(parameter("a/b; x=\"1", "x"), Some("\"1"));
        assert_eq!(parameter("a/b; x", "x"), None);
        assert_eq!(parameter("x=1", "x"), None);
        assert_eq!(
            media_type(" Multipart/Related ; type=x"),
            "multipart/related"
        );
    }

    #[test]
    fn a_written_body_reads_back_with_every_byte_of_every_part() {
        let every: Vec<u8> = (0..=255).collect();
        let parts = vec![
            Part::new(b"<Envelope/>".to_vec())
                .header("Content-Type", "application/soap+xml")
                .header("Content-ID", "<root@xmip>"),
            Part::new(every).header("Content-Type", "application/octet-stream"),
            Part::new(Vec::new()).header("Content-ID", "empty"),
            Part::new(b"\r\n--x\r\n".to_vec()),
        ];
        let boundary = boundary();
        assert_ne!(boundary, super::boundary(), "unique");
        let body = write(&boundary, &parts);
        let back = read(&body, &boundary).expect("read");
        assert_eq!(back, parts);
        assert_eq!(back[0].content_id(), Some("root@xmip"));
        assert_eq!(back[2].content_id(), Some("empty"));
        assert_eq!(back[1].content_type(), Some("application/octet-stream"));
    }

    #[test]
    fn a_delimiter_counts_only_at_the_start_of_a_line() {
        // MSMQ took the boundary in the middle of a line until 2026-09-24.
        let bytes = b"x--b\r\n--b\r\n\r\npayload --b inside\r\n--bx\r\n--b--";
        assert_eq!(delimiter_at(bytes, b"--b", 0), Some(6));
        let parts = read(bytes, "b").expect("cuts");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].body, b"payload --b inside\r\n--bx");
    }

    #[test]
    fn headers_fold_and_a_body_may_be_empty_or_hollow() {
        let bytes = b"preamble\n--b \ncontent-type: A/B;\n\tcharset=x\nCONTENT-ID: <c>\n\n--b--\n";
        let parts = read(bytes, "b").expect("cuts");
        assert_eq!(parts[0].body, b"");
        assert_eq!(parts[0].content_type(), Some("A/B; charset=x"));
        assert_eq!(parts[0].content_id(), Some("c"));
        assert_eq!(opening_boundary(b"--=_Part_1\r\n"), Some("=_Part_1"));
        assert_eq!(opening_boundary(b"--a b\r\n"), None);
        let refused = read(b"no boundary", "b").expect_err("none");
        assert_eq!(refused.reason, "no opening boundary");
        assert!(read(b"--b\r\nContent-Type: x\r\n\r\nnever closes", "b").is_err());
        assert!(read(b"--b\r\nno blank line\r\n--b--", "b").is_err());
    }
}
