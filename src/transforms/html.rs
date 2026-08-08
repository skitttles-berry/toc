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

impl LimitedOutput {
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransformError> {
        let new_len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(TransformError::OutputTooLarge { limit: self.limit })?;
        if new_len > self.limit {
            return Err(TransformError::OutputTooLarge { limit: self.limit });
        }
        self.bytes.extend_from_slice(bytes);
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

enum Entity {
    Named(&'static str),
    Character(char),
}

fn decoded_character(number: u32) -> Option<char> {
    let character = char::try_from(number).ok()?;
    match character {
        '\t' | '\n' | '\u{000c}' | '\r' => Some(character),
        '\0'..='\u{001f}' => None,
        _ => Some(character),
    }
}

fn numeric_entity(digits: &[u8], radix: u32) -> Option<char> {
    let mut number = 0u32;
    for digit in digits {
        let digit = match digit {
            b'0'..=b'9' => u32::from(digit - b'0'),
            b'a'..=b'f' if radix == 16 => u32::from(digit - b'a' + 10),
            b'A'..=b'F' if radix == 16 => u32::from(digit - b'A' + 10),
            _ => return None,
        };
        if digit >= radix {
            return None;
        }
        number = number.checked_mul(radix)?.checked_add(digit)?;
    }
    decoded_character(number)
}

fn entity(text: &str, start: usize) -> Option<(usize, Entity)> {
    let bytes = text.as_bytes();
    let mut end = start + 1;
    while end < bytes.len() && bytes[end] != b';' {
        if bytes[end] == b'&' {
            return None;
        }
        end += 1;
    }
    if end == bytes.len() {
        return None;
    }

    let body = &bytes[start + 1..end];
    let value = if let Some(digits) = body
        .strip_prefix(b"#x")
        .or_else(|| body.strip_prefix(b"#X"))
    {
        Entity::Character(numeric_entity(digits, 16)?)
    } else if let Some(digits) = body.strip_prefix(b"#") {
        Entity::Character(numeric_entity(digits, 10)?)
    } else {
        let index = html_escape::NAMED_ENTITIES
            .binary_search_by(|(name, _)| name.cmp(&body))
            .ok()?;
        Entity::Named(html_escape::NAMED_ENTITIES[index].1)
    };
    Some((end + 1, value))
}

fn write_entity(entity: Entity, output: &mut LimitedOutput) -> Result<(), TransformError> {
    match entity {
        Entity::Named(value) => output.write(value.as_bytes()),
        Entity::Character(value) => {
            let mut bytes = [0; 4];
            output.write(value.encode_utf8(&mut bytes).as_bytes())
        }
    }
}

fn decode_to(input: &[u8], output: &mut LimitedOutput) -> Result<(), TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let mut index = 0;
    while index < text.len() {
        if output.bytes.len() == output.limit {
            return Err(TransformError::OutputTooLarge {
                limit: output.limit,
            });
        }
        if text.as_bytes()[index] == b'&'
            && let Some((end, entity)) = entity(text, index)
        {
            write_entity(entity, output)?;
            index = end;
            continue;
        }
        output.write(&text.as_bytes()[index..index + 1])?;
        index += 1;
    }
    Ok(())
}

pub(super) fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let mut output = LimitedOutput::new(output_limit);
    decode_to(input, &mut output)?;
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

    #[test]
    fn decode_does_not_allocate_for_a_large_invalid_candidate_before_a_tiny_limit() {
        let input = format!("&#{};", "9".repeat(100_000));
        let mut output = LimitedOutput::new(0);
        assert_eq!(
            decode_to(input.as_bytes(), &mut output),
            Err(TransformError::OutputTooLarge { limit: 0 })
        );
        assert_eq!(output.bytes.capacity(), 0);
    }

    #[test]
    fn decode_accepts_numeric_entities_with_unbounded_leading_zeroes() {
        let input = format!("&#{}65;", "0".repeat(100_000));
        assert_eq!(decode(input.as_bytes(), 1).unwrap(), b"A");
    }
}
