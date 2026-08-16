use std::{collections::HashSet, fmt, io};

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use crate::error::{JsonErrorKind, TransformError};

const DUPLICATE_KEY_MARKER: &str = "duplicate object key";

struct ValidateSeed;
struct ValidateVisitor;

impl<'de> DeserializeSeed<'de> for ValidateSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_any(ValidateVisitor)
    }
}

impl<'de> Visitor<'de> for ValidateVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_string<E>(self, _: String) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element_seed(ValidateSeed)?.is_some() {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom(DUPLICATE_KEY_MARKER));
            }
            map.next_value_seed(ValidateSeed)?;
        }
        Ok(())
    }
}

fn check_depth(input: &[u8]) -> Result<(), TransformError> {
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut line = 1;
    let mut column = 1;

    for &byte in input {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else {
            match byte {
                b'"' => in_string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 128 {
                        return Err(TransformError::InvalidJson {
                            line,
                            column,
                            kind: JsonErrorKind::DepthExceeded,
                        });
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }

        if byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Ok(())
}

fn validate(input: &[u8]) -> Result<(), TransformError> {
    if input.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(TransformError::InvalidJson {
            line: 1,
            column: 1,
            kind: JsonErrorKind::Bom,
        });
    }
    check_depth(input)?;

    let mut deserializer = serde_json::Deserializer::from_slice(input);
    deserializer.disable_recursion_limit();
    let result = ValidateSeed
        .deserialize(&mut deserializer)
        .and_then(|()| deserializer.end());
    result.map_err(|error| {
        let kind = if error.to_string().contains(DUPLICATE_KEY_MARKER) {
            JsonErrorKind::DuplicateKey
        } else {
            JsonErrorKind::Syntax
        };
        TransformError::InvalidJson {
            line: error.line(),
            column: error.column(),
            kind,
        }
    })
}

struct LimitedOutput {
    bytes: Vec<u8>,
    limit: usize,
}

impl LimitedOutput {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    fn push(&mut self, byte: u8) -> Result<(), TransformError> {
        if self.bytes.len() == self.limit {
            return Err(TransformError::OutputTooLarge { limit: self.limit });
        }
        self.bytes.push(byte);
        Ok(())
    }

    fn extend(&mut self, bytes: &[u8]) -> Result<(), TransformError> {
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

    fn newline_and_indent(&mut self, depth: usize) -> Result<(), TransformError> {
        self.push(b'\n')?;
        for _ in 0..depth * 2 {
            self.push(b' ')?;
        }
        Ok(())
    }
}

impl io::Write for LimitedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.extend(bytes)
            .map(|()| bytes.len())
            .map_err(|_| io::Error::other("output limit"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Pretty,
    Minify,
}

fn begin_token(
    output: &mut LimitedOutput,
    containers: &mut [bool],
    mode: Mode,
) -> Result<(), TransformError> {
    if let Some(non_empty) = containers.last_mut()
        && !*non_empty
    {
        *non_empty = true;
        if matches!(mode, Mode::Pretty) {
            output.newline_and_indent(containers.len())?;
        }
    }
    Ok(())
}

fn transform(
    input: &[u8],
    output_limit: usize,
    mode: Mode,
    object_required: bool,
) -> Result<Vec<u8>, TransformError> {
    validate(input)?;
    if object_required
        && input
            .iter()
            .copied()
            .find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            != Some(b'{')
    {
        return Err(TransformError::InvalidJson {
            line: 1,
            column: 1,
            kind: JsonErrorKind::Syntax,
        });
    }

    let mut output = LimitedOutput::new(output_limit);
    let mut containers: Vec<bool> = Vec::new();
    let mut index = 0;

    while index < input.len() {
        match input[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            b'{' | b'[' => {
                begin_token(&mut output, &mut containers, mode)?;
                output.push(input[index])?;
                containers.push(false);
                if containers.len() > 128 {
                    return Err(TransformError::InvalidJson {
                        line: 0,
                        column: 0,
                        kind: JsonErrorKind::DepthExceeded,
                    });
                }
                index += 1;
            }
            b'}' | b']' => {
                let non_empty = containers
                    .pop()
                    .expect("validated JSON has balanced containers");
                if non_empty && matches!(mode, Mode::Pretty) {
                    output.newline_and_indent(containers.len())?;
                }
                output.push(input[index])?;
                index += 1;
            }
            b',' => {
                output.push(b',')?;
                if matches!(mode, Mode::Pretty) {
                    output.newline_and_indent(containers.len())?;
                }
                index += 1;
            }
            b':' => {
                output.push(b':')?;
                if matches!(mode, Mode::Pretty) {
                    output.push(b' ')?;
                }
                index += 1;
            }
            b'"' => {
                begin_token(&mut output, &mut containers, mode)?;
                let start = index;
                index += 1;
                while input[index] != b'"' {
                    if input[index] == b'\\' {
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                index += 1;
                output.extend(&input[start..index])?;
            }
            _ => {
                begin_token(&mut output, &mut containers, mode)?;
                let start = index;
                while index < input.len()
                    && !matches!(
                        input[index],
                        b' ' | b'\t' | b'\r' | b'\n' | b'{' | b'}' | b'[' | b']' | b',' | b':'
                    )
                {
                    index += 1;
                }
                output.extend(&input[start..index])?;
            }
        }
    }
    Ok(output.bytes)
}

pub fn format(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    transform(input, output_limit, Mode::Pretty, false)
}

pub(super) fn format_object(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    transform(input, output_limit, Mode::Pretty, true)
}

pub fn minify(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    transform(input, output_limit, Mode::Minify, false)
}

#[allow(dead_code)]
pub(super) fn string_encode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let mut output = LimitedOutput::new(output_limit);
    serde_json::to_writer(&mut output, text).map_err(|_| TransformError::OutputTooLarge {
        limit: output_limit,
    })?;
    Ok(output.bytes)
}

#[allow(dead_code)]
pub(super) fn string_decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    validate(input)?;
    let output =
        serde_json::from_slice::<String>(input).map_err(|error| TransformError::InvalidJson {
            line: error.line(),
            column: error.column(),
            kind: JsonErrorKind::ExpectedString,
        })?;
    if output.len() > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(output.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_prints_structure_without_rewriting_tokens_or_order() {
        let input = br#"{"\u0061":1.00,"b":[true,null],"n":1e999999}"#;
        let expected = br#"{
  "\u0061": 1.00,
  "b": [
    true,
    null
  ],
  "n": 1e999999
}"#;
        assert_eq!(format(input, 1024).unwrap(), expected);
    }

    #[test]
    fn minifies_only_whitespace_outside_strings() {
        let input = b" { \"a\" : \"x y\", \"b\" : 1.00 } \n";
        assert_eq!(minify(input, 1024).unwrap(), br#"{"a":"x y","b":1.00}"#);
    }

    #[test]
    fn accepts_top_level_scalars() {
        for (input, expected) in [
            (br#" "\u0061" "#.as_slice(), br#""\u0061""#.as_slice()),
            (b" 1e2 ".as_slice(), b"1e2".as_slice()),
            (b" true ".as_slice(), b"true".as_slice()),
            (b" false ".as_slice(), b"false".as_slice()),
            (b" null ".as_slice(), b"null".as_slice()),
            (b" [] ".as_slice(), b"[]".as_slice()),
            (b" {} ".as_slice(), b"{}".as_slice()),
        ] {
            assert_eq!(minify(input, 64).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_decoded_duplicate_keys_without_exposing_the_key() {
        for input in [
            br#"{"a":1,"\u0061":2}"#.as_slice(),
            br#"{"outer":{"a":1,"\u0061":2}}"#.as_slice(),
        ] {
            let error = format(input, 128).unwrap_err();
            assert!(matches!(
                error,
                TransformError::InvalidJson {
                    kind: JsonErrorKind::DuplicateKey,
                    ..
                }
            ));
        }
    }

    #[test]
    fn rejects_comments_trailing_commas_and_bom() {
        for input in [
            b"{/*x*/\"a\":1}".as_slice(),
            b"{\"a\":1,}".as_slice(),
            b"\xef\xbb\xbf{}".as_slice(),
        ] {
            assert!(matches!(
                format(input, 128),
                Err(TransformError::InvalidJson { .. })
            ));
        }
    }

    #[test]
    fn permits_depth_128_and_reports_the_129th_opening_bracket_position() {
        let depth_128 = format!("{}0{}", "[".repeat(128), "]".repeat(128));
        let depth_129 = format!("[\n{}0{}", "[".repeat(128), "]".repeat(129));
        assert!(minify(depth_128.as_bytes(), 1024).is_ok());
        assert_eq!(
            minify(depth_129.as_bytes(), 1024).unwrap_err(),
            TransformError::InvalidJson {
                line: 2,
                column: 128,
                kind: JsonErrorKind::DepthExceeded,
            }
        );
    }

    #[test]
    fn ignores_opening_brackets_in_escaped_strings_when_locating_depth_errors() {
        let input = format!(r#"{{"\"[":{}0{}}}"#, "[".repeat(128), "]".repeat(128));

        assert_eq!(
            minify(input.as_bytes(), 1024).unwrap_err(),
            TransformError::InvalidJson {
                line: 1,
                column: 135,
                kind: JsonErrorKind::DepthExceeded,
            }
        );
    }

    #[test]
    fn counts_crlf_as_one_newline_when_locating_depth_errors() {
        let input = format!("[\r\n{}0{}", "[".repeat(128), "]".repeat(129));

        assert_eq!(
            minify(input.as_bytes(), 1024).unwrap_err(),
            TransformError::InvalidJson {
                line: 2,
                column: 128,
                kind: JsonErrorKind::DepthExceeded,
            }
        );
    }

    #[test]
    fn pretty_output_stops_at_limit() {
        assert_eq!(
            format(b"[1]", 4).unwrap_err(),
            TransformError::OutputTooLarge { limit: 4 }
        );
    }

    #[test]
    fn accepts_crlf_input_but_emits_only_structural_lf_without_final_newline() {
        let output = format(b"{\r\n\"a\":1\r\n}", 128).unwrap();
        assert_eq!(output, b"{\n  \"a\": 1\n}");
        assert!(!output.ends_with(b"\n"));
    }

    #[test]
    fn encodes_a_complete_json_string_literal() {
        let input = "\"\\\0\n한";
        assert_eq!(
            string_encode(input.as_bytes(), 64).unwrap(),
            r#""\"\\\u0000\n한""#.as_bytes()
        );
        assert_eq!(string_encode(b"\"", 4).unwrap(), br#""\"""#);
    }

    #[test]
    fn decodes_one_string_with_json_whitespace_and_surrogate_pairs() {
        assert_eq!(
            string_decode(" \t\"\\uD83D\\uDE00 한\"\r\n".as_bytes(), 8).unwrap(),
            "😀 한".as_bytes()
        );
    }

    #[test]
    fn rejects_non_strings_bom_trailing_data_and_invalid_strings() {
        for input in [b"{}".as_slice(), b"[]", b"0", b"true", b"null"] {
            assert!(matches!(
                string_decode(input, 8),
                Err(TransformError::InvalidJson {
                    kind: JsonErrorKind::ExpectedString,
                    ..
                })
            ));
        }

        for input in [
            br#""x" 0"#.as_slice(),
            br#""\q""#.as_slice(),
            br#""\uD800""#.as_slice(),
        ] {
            assert!(matches!(
                string_decode(input, 8),
                Err(TransformError::InvalidJson {
                    kind: JsonErrorKind::Syntax,
                    ..
                })
            ));
        }

        assert!(matches!(
            string_decode(b"\xef\xbb\xbf\"x\"", 8),
            Err(TransformError::InvalidJson {
                kind: JsonErrorKind::Bom,
                ..
            })
        ));
    }

    #[test]
    fn json_string_transforms_enforce_utf8_and_byte_limits() {
        assert_eq!(
            string_encode(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            string_decode(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            string_encode(b"\"", 3).unwrap_err(),
            TransformError::OutputTooLarge { limit: 3 }
        );
        assert_eq!(
            string_decode("\"é\"".as_bytes(), 1).unwrap_err(),
            TransformError::OutputTooLarge { limit: 1 }
        );
    }
}
