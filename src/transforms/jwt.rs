use crate::{
    error::TransformError,
    transforms::{base64url, json},
};

const WARNING: &[u8] = b"Signature not verified";

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn trim_token(input: &[u8]) -> &[u8] {
    let start = input
        .iter()
        .position(|byte| !is_ascii_whitespace(*byte))
        .unwrap_or(input.len());
    let end = input
        .iter()
        .rposition(|byte| !is_ascii_whitespace(*byte))
        .map_or(start, |index| index + 1);
    &input[start..end]
}

fn invalid_part<T>() -> Result<T, TransformError> {
    Err(TransformError::InvalidJwtPart)
}

fn decode_part(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    if input.iter().copied().any(is_ascii_whitespace) {
        return invalid_part();
    }
    base64url::decode(input, output_limit).map_err(|error| match error {
        TransformError::OutputTooLarge { .. } => error,
        _ => TransformError::InvalidJwtPart,
    })
}

fn write(output: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), TransformError> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(TransformError::OutputTooLarge { limit })?;
    if new_len > limit {
        return Err(TransformError::OutputTooLarge { limit });
    }
    output.extend_from_slice(bytes);
    Ok(())
}

fn write_indented(
    output: &mut Vec<u8>,
    value: &[u8],
    output_limit: usize,
) -> Result<(), TransformError> {
    for (index, line) in value.split(|byte| *byte == b'\n').enumerate() {
        if index > 0 {
            write(output, b"\n  ", output_limit)?;
        }
        write(output, line, output_limit)?;
    }
    Ok(())
}

fn format_part(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let decoded = decode_part(input, output_limit)?;
    json::format_object(&decoded, output_limit).map_err(|error| match error {
        TransformError::OutputTooLarge { .. } => error,
        _ => TransformError::InvalidJwtPart,
    })
}

pub(super) fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let token = trim_token(input);
    let mut parts = token.split(|byte| *byte == b'.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return invalid_part();
    };

    let header = format_part(header, output_limit)?;
    let payload = format_part(payload, output_limit)?;
    decode_part(signature, output_limit)?;

    let mut output = Vec::new();
    write(&mut output, b"{\n  \"header\": ", output_limit)?;
    write_indented(&mut output, &header, output_limit)?;
    write(&mut output, b",\n  \"payload\": ", output_limit)?;
    write_indented(&mut output, &payload, output_limit)?;
    write(&mut output, b",\n  \"signature\": \"", output_limit)?;
    write(&mut output, signature, output_limit)?;
    write(&mut output, b"\",\n  \"warning\": \"", output_limit)?;
    write(&mut output, WARNING, output_limit)?;
    write(&mut output, b"\"\n}", output_limit)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_compact_jws_with_stable_two_space_json() {
        let token = b" \teyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\r\n";
        let expected = br#"{
  "header": {
    "alg": "none",
    "typ": "JWT"
  },
  "payload": {
    "sub": "1234567890",
    "name": "John Doe",
    "iat": 1516239022
  },
  "signature": "",
  "warning": "Signature not verified"
}"#;

        assert_eq!(decode(token, 1024).unwrap(), expected);
    }

    #[test]
    fn requires_exactly_three_compact_segments() {
        for token in [b"e30".as_slice(), b"e30.e30", b"e30.e30..extra"] {
            assert_eq!(
                decode(token, 1024).unwrap_err(),
                TransformError::InvalidJwtPart
            );
        }
    }

    #[test]
    fn rejects_noncanonical_base64url_and_internal_whitespace() {
        for token in [
            b"e31.e30.".as_slice(),
            b"e30.e31.".as_slice(),
            b"e30.e30.Zh".as_slice(),
            b"e3 0.e30.".as_slice(),
            b"e30.e3\t0.".as_slice(),
            b"\x0be30.e30.".as_slice(),
        ] {
            assert_eq!(
                decode(token, 1024).unwrap_err(),
                TransformError::InvalidJwtPart
            );
        }
    }

    #[test]
    fn requires_strict_json_objects_without_duplicate_keys_or_excessive_depth() {
        for token in [
            b"bnVsbA.e30.".as_slice(),
            b"eyJhIjoxLCJhIjoyfQ.e30.".as_slice(),
        ] {
            assert_eq!(
                decode(token, 4096).unwrap_err(),
                TransformError::InvalidJwtPart
            );
        }

        let nested = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        let payload = base64url::encode(nested.as_bytes(), 4096).unwrap();
        let token = [b"e30.".as_slice(), payload.as_slice(), b"."].concat();
        assert_eq!(
            decode(&token, 4096).unwrap_err(),
            TransformError::InvalidJwtPart
        );
    }

    #[test]
    fn preserves_canonical_signature_and_accepts_an_empty_signature() {
        let token = b"e30.e30.Zg";
        let expected = br#"{
  "header": {},
  "payload": {},
  "signature": "Zg",
  "warning": "Signature not verified"
}"#;

        assert_eq!(decode(token, 1024).unwrap(), expected);
        assert!(decode(b"e30.e30.", 1024).is_ok());
    }

    #[test]
    fn reports_output_limit_without_a_partial_result() {
        let full = br#"{
  "header": {},
  "payload": {},
  "signature": "",
  "warning": "Signature not verified"
}"#;
        assert_eq!(
            decode(b"e30.e30.", full.len() - 1).unwrap_err(),
            TransformError::OutputTooLarge {
                limit: full.len() - 1
            }
        );
    }
}
