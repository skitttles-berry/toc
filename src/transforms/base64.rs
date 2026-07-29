use base64::{Engine as _, encoded_len, engine::general_purpose::STANDARD};

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
}
