use crate::error::TransformError;

fn encode(
    input: &[u8],
    output_limit: usize,
    byte_order: fn(u16) -> [u8; 2],
) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let output_len =
        text.encode_utf16()
            .count()
            .checked_mul(2)
            .ok_or(TransformError::OutputTooLarge {
                limit: output_limit,
            })?;
    if output_len > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = Vec::with_capacity(output_len);
    for code_unit in text.encode_utf16() {
        output.extend_from_slice(&byte_order(code_unit));
    }
    Ok(output)
}

fn decode(
    input: &[u8],
    output_limit: usize,
    byte_order: fn([u8; 2]) -> u16,
) -> Result<Vec<u8>, TransformError> {
    if !input.len().is_multiple_of(2) {
        return Err(TransformError::InvalidUtf16 {
            position: input.len() - 1,
        });
    }

    let mut output = String::with_capacity(input.len().min(output_limit));
    let mut position = 0;
    while position < input.len() {
        let first = byte_order([input[position], input[position + 1]]);
        let scalar = match first {
            0xd800..=0xdbff => {
                if position + 3 >= input.len() {
                    return Err(TransformError::InvalidUtf16 { position });
                }
                let second = byte_order([input[position + 2], input[position + 3]]);
                if !(0xdc00..=0xdfff).contains(&second) {
                    return Err(TransformError::InvalidUtf16 { position });
                }
                position += 4;
                0x10000 + (((u32::from(first) - 0xd800) << 10) | (u32::from(second) - 0xdc00))
            }
            0xdc00..=0xdfff => {
                return Err(TransformError::InvalidUtf16 { position });
            }
            _ => {
                position += 2;
                u32::from(first)
            }
        };

        let character = char::from_u32(scalar).expect("validated UTF-16 scalar");
        let new_len = output.len().checked_add(character.len_utf8()).ok_or(
            TransformError::OutputTooLarge {
                limit: output_limit,
            },
        )?;
        if new_len > output_limit {
            return Err(TransformError::OutputTooLarge {
                limit: output_limit,
            });
        }
        output.push(character);
    }
    Ok(output.into_bytes())
}

pub(super) fn encode_le(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    encode(input, output_limit, u16::to_le_bytes)
}

pub(super) fn decode_le(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    decode(input, output_limit, u16::from_le_bytes)
}

pub(super) fn encode_be(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    encode(input, output_limit, u16::to_be_bytes)
}

pub(super) fn decode_be(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    decode(input, output_limit, u16::from_be_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_both_endians_without_a_bom() {
        let text = "A한😀";
        let le = [0x41, 0x00, 0x5c, 0xd5, 0x3d, 0xd8, 0x00, 0xde];
        let be = [0x00, 0x41, 0xd5, 0x5c, 0xd8, 0x3d, 0xde, 0x00];

        assert_eq!(encode_le(text.as_bytes(), 8).unwrap(), le);
        assert_eq!(encode_be(text.as_bytes(), 8).unwrap(), be);
        assert_eq!(decode_le(&le, text.len()).unwrap(), text.as_bytes());
        assert_eq!(decode_be(&be, text.len()).unwrap(), text.as_bytes());
        assert_eq!(encode_le(b"", 0).unwrap(), b"");
        assert_eq!(decode_be(b"", 0).unwrap(), b"");
    }

    #[test]
    fn reports_exact_byte_offsets_for_invalid_utf16() {
        assert_eq!(
            decode_le(&[0x41], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 0 }
        );
        assert_eq!(
            decode_le(&[0x00, 0xd8, 0x41, 0x00], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 0 }
        );
        assert_eq!(
            decode_le(&[0x41, 0x00, 0x00, 0xd8], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 2 }
        );
        assert_eq!(
            decode_le(&[0x3d, 0xd8, 0x00, 0xde, 0x00, 0xdc], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 4 }
        );
    }

    #[test]
    fn preserves_bom_code_units_as_characters() {
        assert_eq!(
            decode_le(&[0xff, 0xfe, 0x41, 0x00], 4).unwrap(),
            "\u{feff}A".as_bytes()
        );
        assert_eq!(decode_be(&[0xfe, 0xff], 3).unwrap(), "\u{feff}".as_bytes());
    }

    #[test]
    fn utf16_transforms_enforce_utf8_and_output_limits() {
        assert_eq!(
            encode_le(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            encode_be("😀".as_bytes(), 3).unwrap_err(),
            TransformError::OutputTooLarge { limit: 3 }
        );
        assert_eq!(
            decode_le(&[0x5c, 0xd5], 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(decode_le(&[0x5c, 0xd5], 3).unwrap(), "한".as_bytes());
    }
}
