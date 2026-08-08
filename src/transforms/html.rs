use std::io::{self, Write};

use crate::error::TransformError;

struct LimitedOutput {
    bytes: Vec<u8>,
    limit: usize,
}

impl LimitedOutput {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(1024)),
            limit,
        }
    }
}

impl Write for LimitedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let new_len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::from(io::ErrorKind::WriteZero))?;
        if new_len > self.limit {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn encode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let required = text.chars().try_fold(0usize, |length, character| {
        length.checked_add(match character {
            '&' => 5,
            '<' | '>' => 4,
            _ => character.len_utf8(),
        })
    });
    let required = required.ok_or(TransformError::OutputTooLarge {
        limit: output_limit,
    })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(html_escape::encode_text(text).into_owned().into_bytes())
}

pub(super) fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let mut output = LimitedOutput::new(output_limit);
    html_escape::decode_html_entities_to_writer(text, &mut output).map_err(|_| {
        TransformError::OutputTooLarge {
            limit: output_limit,
        }
    })?;
    Ok(output.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn encodes_only_text_context_ampersand_less_than_and_greater_than() {
        assert_eq!(encode(b"'\"&<>", 16).unwrap(), b"'\"&amp;&lt;&gt;");
        assert_eq!(
            encode("한&글".as_bytes(), 16).unwrap(),
            "한&amp;글".as_bytes()
        );
    }

    #[test]
    fn decode_requires_exact_valid_semicolon_terminated_entities() {
        assert_eq!(
            decode(b"&amp; &#x41; &#65; &quot;", 32).unwrap(),
            b"& A A \""
        );
        assert_eq!(
            decode(b"&amp &unknown; &#xZZ; &#65", 64).unwrap(),
            b"&amp &unknown; &#xZZ; &#65"
        );
    }

    #[test]
    fn html_transforms_require_utf8_and_enforce_output_limits() {
        assert_eq!(
            encode(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            decode(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            encode(b"&", 4).unwrap_err(),
            TransformError::OutputTooLarge { limit: 4 }
        );
        assert_eq!(
            decode(b"&amp;", 0).unwrap_err(),
            TransformError::OutputTooLarge { limit: 0 }
        );
    }
}
