use std::borrow::Cow;

use data_encoding::BASE32;

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

fn is_symbol(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'2'..=b'7')
}

fn decoded_len(symbols: usize) -> Option<usize> {
    let remainder = symbols % 8;
    let tail = match remainder {
        0 => 0,
        2 => 1,
        4 => 2,
        5 => 3,
        7 => 4,
        _ => return None,
    };
    symbols.checked_div(8)?.checked_mul(5)?.checked_add(tail)
}

pub(super) fn encode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let required = input
        .len()
        .checked_add(4)
        .and_then(|length| length.checked_div(5))
        .and_then(|groups| groups.checked_mul(8))
        .ok_or(TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(BASE32.encode(input).into_bytes())
}

pub(super) fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let compact = compact_input(input);
    let padding_start = compact
        .iter()
        .position(|byte| *byte == b'=')
        .unwrap_or(compact.len());

    for (position, byte) in compact.iter().copied().enumerate() {
        if position < padding_start {
            if !is_symbol(byte) {
                return Err(TransformError::InvalidBase32 {
                    position: Some(original_offset(input, position)),
                });
            }
        } else if byte != b'=' {
            return Err(TransformError::InvalidBase32 {
                position: Some(original_offset(input, position)),
            });
        }
    }

    if !compact.len().is_multiple_of(8) {
        return Err(TransformError::InvalidBase32 { position: None });
    }
    let symbols = padding_start;
    let required = decoded_len(symbols).ok_or(TransformError::InvalidBase32 { position: None })?;
    let expected_padding = (8 - symbols % 8) % 8;
    if compact.len() - symbols != expected_padding {
        return Err(TransformError::InvalidBase32 { position: None });
    }
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = Vec::with_capacity(required);
    let mut bits = 0usize;
    let mut value = 0u16;
    for (position, byte) in compact[..symbols].iter().copied().enumerate() {
        let symbol = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => unreachable!("first pass validated every Base32 symbol"),
        };
        value = (value << 5) | u16::from(symbol);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            output.push((value >> bits) as u8);
            value &= (1 << bits) - 1;
        }
        if bits > 0 && position + 1 == symbols && value != 0 {
            return Err(TransformError::InvalidBase32 {
                position: Some(original_offset(input, position)),
            });
        }
    }
    debug_assert_eq!(output.len(), required);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn encodes_uppercase_rfc4648_base32_with_canonical_padding() {
        assert_eq!(encode(b"f", 8).unwrap(), b"MY======");
        assert_eq!(encode(b"foo", 8).unwrap(), b"MZXW6===");
        assert_eq!(
            encode(b"f", 7).unwrap_err(),
            TransformError::OutputTooLarge { limit: 7 }
        );
    }

    #[test]
    fn decode_accepts_ascii_case_and_only_four_ascii_whitespace_bytes() {
        assert_eq!(decode(b" mzxw6===\t\r\n", 3).unwrap(), b"foo");
        assert_eq!(
            decode(b"MZXW6===\x0b", 8).unwrap_err(),
            TransformError::InvalidBase32 { position: Some(8) }
        );
    }

    #[test]
    fn decode_requires_canonical_padding_and_trailing_bits() {
        assert_eq!(
            decode(b"MZXW6", 8).unwrap_err(),
            TransformError::InvalidBase32 { position: None }
        );
        assert_eq!(
            decode(b"MY=====", 8).unwrap_err(),
            TransformError::InvalidBase32 { position: None }
        );
        assert_eq!(
            decode(b"MZ======", 8).unwrap_err(),
            TransformError::InvalidBase32 { position: Some(1) }
        );
    }

    #[test]
    fn decode_maps_positions_and_obeys_limit() {
        assert_eq!(
            decode(b" M Z!====", 8).unwrap_err(),
            TransformError::InvalidBase32 { position: Some(4) }
        );
        assert_eq!(
            decode(b"MZXW6===", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
    }
}
