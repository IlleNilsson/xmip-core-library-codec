//! A size against the most that carries it whole: one sentence for every
//! refusal of a size over its ceiling, a protocol's (`net::ceiling`, which
//! words its refusals here) and a step's alike — the most a decompressing
//! step makes of one Stream (`xmip-core-prepare`). Here, below the network,
//! so a standard unit that works offline words it without reaching one.

use crate::CodecError;

/// `Ok` when `size` bytes fit under `ceiling`; otherwise the refusal,
/// `"<size> bytes is over the <ceiling> <carrier>"`, where `carrier` says
/// what holds that many: `"one datagram carries"`, `"bytes this step
/// decompresses"`.
///
/// # Errors
/// Where `size` is over `ceiling`: the payload's failure, so it says the
/// same thing next time.
pub fn within(size: usize, ceiling: usize, carrier: &str) -> Result<(), CodecError> {
    if size > ceiling {
        return Err(CodecError::new(format!(
            "{size} bytes is over the {ceiling} {carrier}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_at_the_ceiling_fits_and_one_byte_over_is_refused() {
        assert!(within(8, 8, "one test frame carries").is_ok());
        let error = within(9, 8, "one test frame carries").expect_err("over");
        assert_eq!(
            error.message,
            "9 bytes is over the 8 one test frame carries"
        );
    }
}
