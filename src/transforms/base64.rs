use std::fmt::Write as _;

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

fn original_offset(input: &[u8], compact_offset: usize) -> usize {
    input
        .iter()
        .enumerate()
        .filter(|(_, byte)| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
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
    let compact: Vec<u8> = input
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .collect();

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
        .decode_slice(&compact, &mut decoded)
        .map_err(|error| match error {
            DecodeSliceError::DecodeError(error) => invalid_base64(input, error),
            DecodeSliceError::OutputSliceTooSmall => {
                TransformError::InvalidBase64 { position: None }
            }
        })?;
    decoded.truncate(written);
    if std::str::from_utf8(&decoded).is_err() {
        return Err(TransformError::InvalidUtf8Output {
            preview_hex: hex_preview(&decoded),
        });
    }
    Ok(decoded)
}

pub(super) fn hex_preview(bytes: &[u8]) -> String {
    let mut preview = String::with_capacity(bytes.len().min(64) * 2);
    for byte in bytes.iter().take(64) {
        write!(&mut preview, "{byte:02x}").expect("writing to String cannot fail");
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn invalid_utf8_result_has_at_most_64_preview_bytes() {
        let error = decode(b"/w==", 16).unwrap_err();
        assert_eq!(
            error,
            TransformError::InvalidUtf8Output {
                preview_hex: "ff".to_string()
            }
        );

        let encoded = encode(&[0xff; 65], 1024).unwrap();
        let TransformError::InvalidUtf8Output { preview_hex } = decode(&encoded, 1024).unwrap_err()
        else {
            panic!("expected invalid UTF-8 output");
        };
        assert_eq!(preview_hex.len(), 128);
        assert!(preview_hex.bytes().all(|byte| byte == b'f'));
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
