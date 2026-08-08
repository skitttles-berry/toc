use crate::error::TransformError;

pub(super) fn apply(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    if input.len() > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(input
        .iter()
        .map(|byte| match byte {
            b'a'..=b'z' => b'a' + (byte - b'a' + 13) % 26,
            b'A'..=b'Z' => b'A' + (byte - b'A' + 13) % 26,
            _ => *byte,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn rotates_ascii_letters_only() {
        assert_eq!(
            apply("Abc NOP xyz 한글 123".as_bytes(), 64).unwrap(),
            "Nop ABC klm 한글 123".as_bytes()
        );
    }

    #[test]
    fn rot13_requires_utf8_and_is_its_own_inverse_with_a_limit() {
        assert_eq!(
            apply(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            apply(b"abc", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(apply(&apply(b"Rust", 4).unwrap(), 4).unwrap(), b"Rust");
    }
}
