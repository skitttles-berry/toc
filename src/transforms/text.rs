use crate::error::TransformError;

fn utf8(input: &[u8]) -> Result<&str, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)
}

pub(super) fn trim(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let output = utf8(input)?.trim().as_bytes();
    if output.len() > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(output.to_vec())
}

fn map_case<I>(
    input: &[u8],
    output_limit: usize,
    map: impl Fn(char) -> I,
) -> Result<Vec<u8>, TransformError>
where
    I: IntoIterator<Item = char>,
{
    let input = utf8(input)?;
    let mut output = String::with_capacity(input.len().min(output_limit));
    for character in input.chars() {
        for mapped in map(character) {
            let new_len = output.len().checked_add(mapped.len_utf8()).ok_or(
                TransformError::OutputTooLarge {
                    limit: output_limit,
                },
            )?;
            if new_len > output_limit {
                return Err(TransformError::OutputTooLarge {
                    limit: output_limit,
                });
            }
            output.push(mapped);
        }
    }
    Ok(output.into_bytes())
}

pub(super) fn lowercase(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    map_case(input, output_limit, char::to_lowercase)
}

pub(super) fn uppercase(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    map_case(input, output_limit, char::to_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::TransformFn;

    #[test]
    fn trims_only_unicode_whitespace_at_both_ends() {
        assert_eq!(
            trim("\u{2003}a \n b\u{3000}".as_bytes(), 5).unwrap(),
            b"a \n b"
        );
        assert_eq!(trim("\u{2003}\n".as_bytes(), 0).unwrap(), b"");
        assert_eq!(
            trim(b" x ", 0).unwrap_err(),
            TransformError::OutputTooLarge { limit: 0 }
        );
    }

    #[test]
    fn unicode_case_mapping_expands_only_within_the_limit() {
        assert_eq!(lowercase("İ".as_bytes(), 3).unwrap(), "i\u{307}".as_bytes());
        assert_eq!(uppercase("ß".as_bytes(), 2).unwrap(), b"SS");
        assert_eq!(
            lowercase("İ".as_bytes(), 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(
            uppercase("ß".as_bytes(), 1).unwrap_err(),
            TransformError::OutputTooLarge { limit: 1 }
        );
    }

    #[test]
    fn text_transforms_reject_invalid_utf8() {
        let transforms: [TransformFn; 3] = [trim, lowercase, uppercase];
        for transform in transforms {
            assert_eq!(transform(b"", 0).unwrap(), b"");
            assert_eq!(
                transform(&[0xff], 8).unwrap_err(),
                TransformError::InvalidUtf8Input
            );
        }
    }
}
