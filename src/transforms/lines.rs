use std::collections::HashSet;

use crate::error::TransformError;

const MAX_LINES: usize = 1_000_000;

struct ParsedLines<'a> {
    lines: Vec<&'a str>,
    terminal_newline: bool,
}

fn parse(input: &[u8]) -> Result<ParsedLines<'_>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    if text.is_empty() {
        return Ok(ParsedLines {
            lines: Vec::new(),
            terminal_newline: false,
        });
    }

    let terminal_newline = input.ends_with(b"\n");
    let line_count =
        input.iter().filter(|byte| **byte == b'\n').count() + usize::from(!terminal_newline);
    if line_count > MAX_LINES {
        return Err(TransformError::TooManyLines { limit: MAX_LINES });
    }

    let mut lines = Vec::with_capacity(line_count);
    let mut start = 0;
    for (separator, byte) in input.iter().copied().enumerate() {
        if byte != b'\n' {
            continue;
        }
        let end = if separator > start && input[separator - 1] == b'\r' {
            separator - 1
        } else {
            separator
        };
        lines.push(&text[start..end]);
        start = separator + 1;
    }
    if start < input.len() {
        lines.push(&text[start..]);
    }
    debug_assert_eq!(lines.len(), line_count);

    Ok(ParsedLines {
        lines,
        terminal_newline,
    })
}

fn render(
    lines: &[&str],
    terminal_newline: bool,
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let separators = lines.len().saturating_sub(1) + usize::from(terminal_newline);
    let required = lines
        .iter()
        .try_fold(separators, |length, line| length.checked_add(line.len()));
    let required = required.ok_or(TransformError::OutputTooLarge {
        limit: output_limit,
    })?;
    if required > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = Vec::with_capacity(required);
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            output.push(b'\n');
        }
        output.extend_from_slice(line.as_bytes());
    }
    if terminal_newline {
        output.push(b'\n');
    }
    Ok(output)
}

pub(super) fn sort(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let ParsedLines {
        mut lines,
        terminal_newline,
    } = parse(input)?;
    lines.sort_unstable();
    render(&lines, terminal_newline, output_limit)
}

pub(super) fn remove_duplicates(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let ParsedLines {
        mut lines,
        terminal_newline,
    } = parse(input)?;
    let mut seen = HashSet::with_capacity(lines.len());
    lines.retain(|line| seen.insert(*line));
    render(&lines, terminal_newline, output_limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn empty_input_has_no_logical_lines() {
        assert_eq!(sort(b"", 0).unwrap(), b"");
        assert_eq!(remove_duplicates(b"", 0).unwrap(), b"");
    }

    #[test]
    fn sort_normalizes_lf_and_crlf_but_keeps_bare_cr_data() {
        assert_eq!(sort(b"b\r\na\rc\n", 6).unwrap(), b"a\rc\nb\n");
        assert_eq!(sort(b"b\na", 3).unwrap(), b"a\nb");
        assert_eq!(sort(b"b\na\r", 4).unwrap(), b"a\r\nb");
    }

    #[test]
    fn sort_preserves_terminal_newline_and_empty_logical_lines() {
        assert_eq!(sort(b"\n", 1).unwrap(), b"\n");
        assert_eq!(sort(b"a\n\n", 3).unwrap(), b"\na\n");
    }

    #[test]
    fn sort_uses_unicode_scalar_order() {
        assert_eq!(
            sort("😀\nβ\nA\nä\n".as_bytes(), 13).unwrap(),
            "A\nä\nβ\n😀\n".as_bytes()
        );
    }

    #[test]
    fn dedup_keeps_the_first_exact_normalized_line() {
        assert_eq!(
            remove_duplicates(b"b\r\nA\nb\nA\r\n", 4).unwrap(),
            b"b\nA\n"
        );
        assert_eq!(remove_duplicates(b"a\r\na\n", 2).unwrap(), b"a\n");
        assert_eq!(remove_duplicates(b"a\ra\n", 4).unwrap(), b"a\ra\n");
    }

    #[test]
    fn line_transforms_reject_invalid_utf8_and_excess_output() {
        assert_eq!(
            sort(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            remove_duplicates(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            sort(b"b\na", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(
            remove_duplicates(b"a\na", 0).unwrap_err(),
            TransformError::OutputTooLarge { limit: 0 }
        );
    }

    #[test]
    fn accepts_one_million_lines_and_rejects_one_more_before_rendering() {
        let boundary = "\n".repeat(MAX_LINES);
        assert_eq!(
            sort(boundary.as_bytes(), boundary.len()).unwrap(),
            boundary.as_bytes()
        );
        assert_eq!(remove_duplicates(boundary.as_bytes(), 1).unwrap(), b"\n");

        let too_many = "\n".repeat(MAX_LINES + 1);
        assert_eq!(
            sort(too_many.as_bytes(), 0).unwrap_err(),
            TransformError::TooManyLines { limit: MAX_LINES }
        );
        assert_eq!(
            remove_duplicates(too_many.as_bytes(), 0).unwrap_err(),
            TransformError::TooManyLines { limit: MAX_LINES }
        );
    }
}
