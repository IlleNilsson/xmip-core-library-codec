//! How deep a nested structure is read before it is refused as hostile.
//!
//! A reader that recurses once a level — an Avro datum, a Protocol Buffers
//! message, a BER element, a GraphQL type — is walked by the bytes it is
//! given, so its stack is the sender's to choose unless the depth is
//! capped. The cap is one number and one refusal for every such reader,
//! the SECS-II item's 64 levels, which no honest message comes near: a
//! reader passes the depth it is at and `?`s the answer.

use crate::CodecError;

/// The most levels a nested structure is read to.
pub const MAX_DEPTH: usize = 64;

/// `Ok` at `depth` levels down, `MAX_DEPTH` itself included; past it the
/// refusal, `"<what> nests past the 64 levels Xmip reads"`, where `what`
/// names the structure: `"an Avro datum"`, `"a BER element"`.
///
/// # Errors
/// Where `depth` is over [`MAX_DEPTH`].
pub fn within(depth: usize, what: &str) -> Result<(), CodecError> {
    if depth > MAX_DEPTH {
        return Err(CodecError::new(format!(
            "{what} nests past the {MAX_DEPTH} levels Xmip reads"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_level_is_read_and_the_one_past_it_refused() {
        assert!(within(0, "a test tree").is_ok());
        assert!(within(MAX_DEPTH, "a test tree").is_ok());
        assert_eq!(
            within(MAX_DEPTH + 1, "a test tree")
                .expect_err("deeper")
                .message,
            "a test tree nests past the 64 levels Xmip reads"
        );
    }
}
