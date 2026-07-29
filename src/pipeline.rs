use crate::{
    MAX_STEPS,
    error::{PipelineError, TransformError},
    transforms::TransformDefinition,
};

#[derive(Clone, Copy)]
pub struct TransformStep {
    pub definition: &'static TransformDefinition,
    pub enabled: bool,
}

pub fn execute(
    mut input: Vec<u8>,
    steps: &[TransformStep],
    output_limit: usize,
) -> Result<Vec<u8>, PipelineError> {
    if steps.len() > MAX_STEPS {
        return Err(PipelineError::TooManySteps { max: MAX_STEPS });
    }

    for (index, step) in steps.iter().enumerate() {
        if !step.enabled {
            continue;
        }
        if !step.definition.accepts_binary && std::str::from_utf8(&input).is_err() {
            return Err(PipelineError::Step {
                step: index + 1,
                transform_id: step.definition.id,
                source: TransformError::InvalidUtf8Input,
            });
        }
        input = (step.definition.apply)(&input, output_limit).map_err(|source| {
            PipelineError::Step {
                step: index + 1,
                transform_id: step.definition.id,
                source,
            }
        })?;
        if input.len() > output_limit {
            return Err(PipelineError::Step {
                step: index + 1,
                transform_id: step.definition.id,
                source: TransformError::OutputTooLarge {
                    limit: output_limit,
                },
            });
        }
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::transform_by_id;

    fn step(id: &str, enabled: bool) -> TransformStep {
        TransformStep {
            definition: transform_by_id(id).unwrap(),
            enabled,
        }
    }

    #[test]
    fn runs_enabled_steps_in_order_and_skips_disabled_steps() {
        let steps = [
            step("base64-encode", true),
            step("base64-encode", false),
            step("base64-decode", true),
        ];
        assert_eq!(execute(b"hello".to_vec(), &steps, 1024).unwrap(), b"hello");
    }

    #[test]
    fn reports_one_based_failing_step_and_transform_id() {
        let steps = [step("base64-decode", true)];
        let error = execute(b"!".to_vec(), &steps, 1024).unwrap_err();
        assert!(matches!(
            error,
            PipelineError::Step {
                step: 1,
                transform_id: "base64-decode",
                ..
            }
        ));
    }

    #[test]
    fn rejects_more_than_32_steps() {
        let steps = vec![step("base64-encode", false); 33];
        assert_eq!(
            execute(Vec::new(), &steps, 1024).unwrap_err(),
            PipelineError::TooManySteps { max: 32 }
        );
    }

    #[test]
    fn accepts_32_steps_and_repeats_the_same_transform() {
        let boundary = vec![step("base64-encode", false); 32];
        assert_eq!(execute(b"x".to_vec(), &boundary, 1024).unwrap(), b"x");

        let repeated = [step("base64-encode", true), step("base64-encode", true)];
        assert_eq!(
            execute(b"foo".to_vec(), &repeated, 1024).unwrap(),
            b"Wm05dg=="
        );
    }

    #[test]
    fn stops_at_the_failing_intermediate_output_limit() {
        let steps = [step("base64-encode", true), step("base64-decode", true)];
        assert!(matches!(
            execute(b"foo".to_vec(), &steps, 3),
            Err(PipelineError::Step {
                step: 1,
                source: TransformError::OutputTooLarge { limit: 3 },
                ..
            })
        ));
    }

    #[test]
    fn chains_url_decode_into_base64_encode() {
        let steps = [step("url-decode", true), step("base64-encode", true)];
        assert_eq!(
            execute(b"hello%20world".to_vec(), &steps, 1024).unwrap(),
            b"aGVsbG8gd29ybGQ="
        );
    }
}
