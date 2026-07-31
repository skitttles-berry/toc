use std::borrow::Cow;

use base64::{
    DecodeError, DecodeSliceError, Engine as _, encoded_len, engine::general_purpose::STANDARD,
};

use crate::error::TransformError;

pub fn encode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let required = encoded_len(input.len(), true).ok_or(TransformError::OutputTooLarge {
        limit: output_limit,
    })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = vec![0; required];
    let written = STANDARD
        .encode_slice(input, &mut output)
        .expect("precomputed Base64 buffer length must be sufficient");
    output.truncate(written);
    Ok(output)
}

fn is_ignored(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn compact_input(input: &[u8]) -> Cow<'_, [u8]> {
    if input.iter().copied().any(is_ignored) {
        Cow::Owned(
            input
                .iter()
                .copied()
                .filter(|byte| !is_ignored(*byte))
                .collect(),
        )
    } else {
        Cow::Borrowed(input)
    }
}

fn original_offset(input: &[u8], compact_offset: usize) -> usize {
    input
        .iter()
        .enumerate()
        .filter(|(_, byte)| !is_ignored(**byte))
        .nth(compact_offset)
        .map_or(input.len(), |(offset, _)| offset)
}

fn invalid_base64(input: &[u8], error: DecodeError) -> TransformError {
    let position = match error {
        DecodeError::InvalidByte(offset, _) | DecodeError::InvalidLength(offset) => {
            Some(original_offset(input, offset))
        }
        DecodeError::InvalidLastSymbol { offset, .. } => Some(original_offset(input, offset)),
        DecodeError::InvalidPadding => None,
    };
    TransformError::InvalidBase64 { position }
}

pub fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let compact = compact_input(input);

    if !compact.len().is_multiple_of(4) {
        return Err(TransformError::InvalidBase64 {
            position: Some(input.len()),
        });
    }
    let padding = usize::from(compact.ends_with(b"=")) + usize::from(compact.ends_with(b"=="));
    let required = (compact.len() / 4)
        .checked_mul(3)
        .and_then(|length| length.checked_sub(padding))
        .ok_or(TransformError::InvalidBase64 { position: None })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    let mut decoded = vec![0; required];
    let written = STANDARD
        .decode_slice(compact.as_ref(), &mut decoded)
        .map_err(|error| match error {
            DecodeSliceError::DecodeError(error) => invalid_base64(input, error),
            DecodeSliceError::OutputSliceTooSmall => {
                TransformError::InvalidBase64 { position: None }
            }
        })?;
    decoded.truncate(written);
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrows_plain_input_and_owns_only_whitespace_compaction() {
        let plain = b"Zm9v";
        assert!(matches!(
            compact_input(plain),
            Cow::Borrowed(value) if value == plain
        ));
        assert!(matches!(
            compact_input(b" Zm9v\t\r\n"),
            Cow::Owned(value) if value == b"Zm9v"
        ));
    }

    #[test]
    fn encodes_with_standard_padding() {
        assert_eq!(encode(b"", 0).unwrap(), b"");
        assert_eq!(encode(b"f", 4).unwrap(), b"Zg==");
        assert_eq!(encode(b"fo", 4).unwrap(), b"Zm8=");
        assert_eq!(encode(b"foo", 4).unwrap(), b"Zm9v");
        assert_eq!(encode(&[0x00, 0xff], 4).unwrap(), b"AP8=");
    }

    #[test]
    fn refuses_output_past_limit_before_encoding() {
        assert_eq!(
            encode(b"foo", 3).unwrap_err(),
            TransformError::OutputTooLarge { limit: 3 }
        );
    }

    #[test]
    fn decodes_ascii_whitespace_and_returns_utf8() {
        assert_eq!(decode(b" Zm9v\r\n", 16).unwrap(), b"foo");
    }

    #[test]
    fn rejects_missing_padding_and_noncanonical_last_bits() {
        assert!(matches!(
            decode(b"Zg", 16),
            Err(TransformError::InvalidBase64 { .. })
        ));
        assert!(matches!(
            decode(b"Zh==", 16),
            Err(TransformError::InvalidBase64 { .. })
        ));
        assert!(matches!(
            decode(b"Z!==", 16),
            Err(TransformError::InvalidBase64 { .. })
        ));
    }

    #[test]
    fn decode_returns_non_utf8_bytes_for_pipeline_policy() {
        assert_eq!(decode(b"/w==", 1024).unwrap(), vec![0xff]);
    }

    #[test]
    fn valid_utf8_round_trips_and_decode_obeys_output_limit() {
        let encoded = encode("한글".as_bytes(), 64).unwrap();
        assert_eq!(decode(&encoded, 64).unwrap(), "한글".as_bytes());
        assert_eq!(
            decode(b"Zm9v", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
    }
}
