use crate::error::TransformError;

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

pub fn encode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;

    let required = input.iter().try_fold(0usize, |length, byte| {
        length.checked_add(if is_unreserved(*byte) { 1 } else { 3 })
    });
    let required = required.ok_or(TransformError::OutputTooLarge {
        limit: output_limit,
    })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = Vec::with_capacity(required);
    for byte in input {
        if is_unreserved(*byte) {
            output.push(*byte);
        } else {
            output.extend_from_slice(&[b'%', HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]);
        }
    }
    Ok(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;

    let mut output = Vec::with_capacity(input.len().min(output_limit));
    let mut index = 0;
    while index < input.len() {
        if input[index] != b'%' {
            if output.len() == output_limit {
                return Err(TransformError::OutputTooLarge {
                    limit: output_limit,
                });
            }
            output.push(input[index]);
            index += 1;
            continue;
        }
        if index + 2 >= input.len() {
            return Err(TransformError::InvalidUrl { position: index });
        }
        let high =
            hex_value(input[index + 1]).ok_or(TransformError::InvalidUrl { position: index })?;
        let low =
            hex_value(input[index + 2]).ok_or(TransformError::InvalidUrl { position: index })?;
        if output.len() == output_limit {
            return Err(TransformError::OutputTooLarge {
                limit: output_limit,
            });
        }
        output.push((high << 4) | low);
        index += 3;
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn encodes_only_unreserved_ascii_and_uses_uppercase_hex() {
        assert_eq!(
            encode("a b+c/한".as_bytes(), 128).unwrap(),
            b"a%20b%2Bc%2F%ED%95%9C"
        );
    }

    #[test]
    fn decode_does_not_treat_plus_as_space() {
        assert_eq!(decode(b"a+b%20c", 32).unwrap(), b"a+b c");
        assert_eq!(decode(b"%2f%2F", 8).unwrap(), b"//");
    }

    #[test]
    fn decode_rejects_incomplete_or_nonhex_escape() {
        assert_eq!(
            decode(b"%", 16).unwrap_err(),
            TransformError::InvalidUrl { position: 0 }
        );
        assert_eq!(
            decode(b"%GG", 16).unwrap_err(),
            TransformError::InvalidUrl { position: 0 }
        );
    }

    #[test]
    fn decode_returns_non_utf8_bytes_for_pipeline_policy() {
        assert_eq!(decode(b"%FF", 1024).unwrap(), vec![0xff]);
    }

    #[test]
    fn encode_checks_expansion_before_allocation() {
        assert_eq!(
            encode(b" ", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
    }

    #[test]
    fn empty_and_unicode_values_round_trip_with_bounded_decode() {
        assert_eq!(encode(b"", 0).unwrap(), b"");
        assert_eq!(decode(b"", 0).unwrap(), b"");
        let encoded = encode("한 글+".as_bytes(), 128).unwrap();
        assert_eq!(decode(&encoded, 128).unwrap(), "한 글+".as_bytes());
        assert_eq!(
            decode(b"abc", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
    }
}
