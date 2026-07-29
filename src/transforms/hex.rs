use crate::error::{TransformError, hex_preview};

const HEX: &[u8; 16] = b"0123456789abcdef";

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_ignored(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn encode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let required = input
        .len()
        .checked_mul(2)
        .ok_or(TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = Vec::with_capacity(required);
    for &byte in input {
        output.push(HEX[(byte >> 4) as usize]);
        output.push(HEX[(byte & 0x0f) as usize]);
    }
    Ok(output)
}

pub fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;

    let mut digits = 0usize;
    for (position, byte) in input.iter().copied().enumerate() {
        if is_ignored(byte) {
            continue;
        }
        if hex_value(byte).is_none() {
            return Err(TransformError::InvalidHex { position });
        }
        digits = digits
            .checked_add(1)
            .ok_or(TransformError::OutputTooLarge {
                limit: output_limit,
            })?;
    }

    if !digits.is_multiple_of(2) {
        return Err(TransformError::OddHexDigitCount { digits });
    }
    let required = digits / 2;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = Vec::with_capacity(required);
    let mut high = None;
    for byte in input.iter().copied() {
        if is_ignored(byte) {
            continue;
        }
        let value = hex_value(byte).expect("first pass validated every hex digit");
        if let Some(high) = high.take() {
            output.push((high << 4) | value);
        } else {
            high = Some(value);
        }
    }
    debug_assert!(high.is_none());
    debug_assert_eq!(output.len(), required);

    if std::str::from_utf8(&output).is_err() {
        return Err(TransformError::InvalidUtf8Output {
            preview_hex: hex_preview(&output),
            total_bytes: output.len(),
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn encodes_arbitrary_bytes_as_lowercase_without_extras() {
        assert_eq!(encode(b"", 0).unwrap(), b"");
        assert_eq!(encode(&[0x00, 0xff, b'A'], 6).unwrap(), b"00ff41");
        assert_eq!(
            encode(&[0xff], 1).unwrap_err(),
            TransformError::OutputTooLarge { limit: 1 }
        );
    }

    #[test]
    fn decodes_mixed_case_and_only_four_ascii_whitespace_bytes() {
        assert_eq!(decode(b"", 0).unwrap(), b"");
        assert_eq!(decode(b"48 65\t6C\r6c\n6F", 5).unwrap(), b"Hello");
    }

    #[test]
    fn reports_original_offsets_for_forbidden_bytes() {
        assert_eq!(
            decode(b"0x", 16).unwrap_err(),
            TransformError::InvalidHex { position: 1 }
        );
        assert_eq!(
            decode(b"41:42", 16).unwrap_err(),
            TransformError::InvalidHex { position: 2 }
        );
        assert_eq!(
            decode(b"41,42", 16).unwrap_err(),
            TransformError::InvalidHex { position: 2 }
        );
        assert_eq!(
            decode(b"41\xc2\xa042", 16).unwrap_err(),
            TransformError::InvalidHex { position: 2 }
        );
        assert_eq!(
            decode(b"\xff", 16).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
    }

    #[test]
    fn reports_odd_digit_count_after_ignoring_allowed_whitespace() {
        assert_eq!(
            decode(b" 0A f\n", 16).unwrap_err(),
            TransformError::OddHexDigitCount { digits: 3 }
        );
    }

    #[test]
    fn checks_decode_limit_and_reuses_bounded_utf8_preview() {
        assert_eq!(decode(b"41", 1).unwrap(), b"A");
        assert_eq!(
            decode(b"4142", 1).unwrap_err(),
            TransformError::OutputTooLarge { limit: 1 }
        );

        let encoded = "ff".repeat(65);
        let TransformError::InvalidUtf8Output {
            preview_hex,
            total_bytes,
        } = decode(encoded.as_bytes(), 65).unwrap_err()
        else {
            panic!("expected invalid UTF-8 output");
        };
        assert_eq!(preview_hex, "ff".repeat(64));
        assert_eq!(total_bytes, 65);
    }
}
