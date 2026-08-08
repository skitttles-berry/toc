use std::borrow::Cow;

use base64::{
    DecodeError, DecodeSliceError, Engine as _, encoded_len,
    engine::general_purpose::URL_SAFE_NO_PAD,
};

use crate::error::TransformError;

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

pub(super) fn encode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let required = encoded_len(input.len(), false).ok_or(TransformError::OutputTooLarge {
        limit: output_limit,
    })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = vec![0; required];
    let written = URL_SAFE_NO_PAD
        .encode_slice(input, &mut output)
        .expect("precomputed Base64URL buffer length must be sufficient");
    output.truncate(written);
    Ok(output)
}

pub(super) fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let compact = compact_input(input);
    if let Some(position) = compact.iter().position(|byte| *byte == b'=') {
        return Err(TransformError::InvalidBase64 {
            position: Some(original_offset(input, position)),
        });
    }
    if compact.len() % 4 == 1 {
        return Err(TransformError::InvalidBase64 {
            position: Some(input.len()),
        });
    }

    let remainder = compact.len() % 4;
    let required = compact
        .len()
        .checked_div(4)
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|length| {
            length.checked_add(usize::from(remainder == 2) + 2 * usize::from(remainder == 3))
        })
        .ok_or(TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = vec![0; required];
    let written = URL_SAFE_NO_PAD
        .decode_slice(compact.as_ref(), &mut output)
        .map_err(|error| match error {
            DecodeSliceError::DecodeError(error) => invalid_base64(input, error),
            DecodeSliceError::OutputSliceTooSmall => {
                TransformError::InvalidBase64 { position: None }
            }
        })?;
    output.truncate(written);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn matches_rfc4648_base64_vectors_without_padding() {
        for (plain, encoded) in [
            (b"".as_slice(), b"".as_slice()),
            (b"f", b"Zg"),
            (b"fo", b"Zm8"),
            (b"foo", b"Zm9v"),
            (b"foob", b"Zm9vYg"),
            (b"fooba", b"Zm9vYmE"),
            (b"foobar", b"Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain, encoded.len()).unwrap(), encoded);
            assert_eq!(decode(encoded, plain.len()).unwrap(), plain);
        }
        assert_eq!(encode(&[0xfb, 0xff], 3).unwrap(), b"-_8");
        assert_eq!(
            encode(b"foo", 3).unwrap_err(),
            TransformError::OutputTooLarge { limit: 3 }
        );
    }

    #[test]
    fn decode_ignores_only_four_ascii_whitespace_bytes() {
        assert_eq!(decode(b" Zg\t\r\n", 1).unwrap(), b"f");
        assert_eq!(
            decode(b"Zg\x0b", 8).unwrap_err(),
            TransformError::InvalidBase64 { position: Some(2) }
        );
    }

    #[test]
    fn decode_rejects_padding_invalid_alphabet_and_noncanonical_trailing_bits() {
        assert_eq!(
            decode(b"Zg=", 8).unwrap_err(),
            TransformError::InvalidBase64 { position: Some(2) }
        );
        assert_eq!(
            decode(b"Zm+", 8).unwrap_err(),
            TransformError::InvalidBase64 { position: Some(2) }
        );
        assert_eq!(
            decode(b"Zh", 8).unwrap_err(),
            TransformError::InvalidBase64 { position: Some(1) }
        );
    }

    #[test]
    fn decode_reports_original_position_after_whitespace_and_obeys_limit() {
        assert_eq!(
            decode(b" Z h", 8).unwrap_err(),
            TransformError::InvalidBase64 { position: Some(3) }
        );
        assert_eq!(
            decode(b"Zm9v", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
    }
}
