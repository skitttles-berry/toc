use crate::error::TransformError;

fn write(bytes: &[u8], output: &mut Vec<u8>, output_limit: usize) -> Result<(), TransformError> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    if new_len > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    output.extend_from_slice(bytes);
    Ok(())
}

fn transform(
    input: &[u8],
    output_limit: usize,
    replacements: &[(&[u8], &[u8])],
) -> Result<Vec<u8>, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let mut output = Vec::with_capacity(input.len().min(output_limit));
    let mut index = 0;
    while index < input.len() {
        if let Some((source, replacement)) = replacements
            .iter()
            .find(|(source, _)| input[index..].starts_with(source))
        {
            write(replacement, &mut output, output_limit)?;
            index += source.len();
        } else {
            write(&input[index..index + 1], &mut output, output_limit)?;
            index += 1;
        }
    }
    Ok(output)
}

pub(super) fn defang(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    transform(
        input,
        output_limit,
        &[
            (b"https://", b"hxxps[://]"),
            (b"http://", b"hxxp[://]"),
            (b".", b"[.]"),
        ],
    )
}

pub(super) fn refang(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    transform(
        input,
        output_limit,
        &[
            (b"hxxps[://]", b"https://"),
            (b"hxxp[://]", b"http://"),
            (b"[.]", b"."),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn defang_replaces_only_exact_lowercase_protocols_then_dots() {
        assert_eq!(
            defang(b"https://a.b http://c.d HTTP://e.f", 96).unwrap(),
            b"hxxps[://]a[.]b hxxp[://]c[.]d HTTP://e[.]f"
        );
    }

    #[test]
    fn refang_is_the_exact_inverse_for_defanged_iocs() {
        let input = b"https://a.b http://c.d";
        assert_eq!(refang(&defang(input, 64).unwrap(), 64).unwrap(), input);
    }

    #[test]
    fn ioc_transforms_require_utf8_and_enforce_output_limits() {
        assert_eq!(
            defang(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            refang(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            defang(b"http://a.b", 10).unwrap_err(),
            TransformError::OutputTooLarge { limit: 10 }
        );
        assert_eq!(
            refang(b"hxxp[://]a[.]b", 9).unwrap_err(),
            TransformError::OutputTooLarge { limit: 9 }
        );
    }
}
