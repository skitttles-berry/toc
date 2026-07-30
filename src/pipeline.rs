use std::time::{Duration, Instant};

use crate::{
    MAX_STEPS,
    error::{PipelineError, TransformError, invalid_utf8_output},
    transforms::TransformDefinition,
};

#[derive(Clone, Copy)]
pub struct TransformStep {
    pub definition: &'static TransformDefinition,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionPolicy {
    StrictText,
    #[allow(dead_code)] // Used by the TUI execution path added in the next task.
    AllowBinary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionTarget {
    Final,
    #[allow(dead_code)] // Used by selected-stage TUI execution in the next task.
    Step(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionOutcome {
    Success(Vec<u8>),
    Failed(PipelineError),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepStatus {
    Succeeded,
    Disabled,
    Failed,
    NotExecuted,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StepTrace {
    pub step: usize,
    pub transform_id: &'static str,
    pub input_bytes: Option<usize>,
    pub output_bytes: Option<usize>,
    pub elapsed: Option<Duration>,
    pub status: StepStatus,
    pub error: Option<TransformError>,
}

pub(crate) struct ExecutionRequest<'a> {
    pub request_id: u64,
    pub input: Vec<u8>,
    pub steps: &'a [TransformStep],
    pub output_limit: usize,
    pub policy: ExecutionPolicy,
    pub target: ExecutionTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutionReport {
    pub request_id: u64,
    pub target: ExecutionTarget,
    pub outcome: ExecutionOutcome,
    pub traces: Vec<StepTrace>,
}

pub(crate) fn execute_report(
    request: ExecutionRequest<'_>,
    is_cancelled: impl Fn() -> bool,
) -> ExecutionReport {
    let ExecutionRequest {
        request_id,
        mut input,
        steps,
        output_limit,
        policy,
        target,
    } = request;
    if steps.len() > MAX_STEPS {
        return ExecutionReport {
            request_id,
            target,
            outcome: ExecutionOutcome::Failed(PipelineError::TooManySteps { max: MAX_STEPS }),
            traces: Vec::new(),
        };
    }

    let target_len = match target {
        ExecutionTarget::Final => steps.len(),
        ExecutionTarget::Step(index) => {
            index.checked_add(1).unwrap_or(steps.len()).min(steps.len())
        }
    };
    let mut traces = Vec::with_capacity(steps.len());
    let mut outcome = None;

    for (index, step) in steps.iter().enumerate() {
        let step_number = index + 1;
        if outcome.is_some() || index >= target_len {
            traces.push(StepTrace {
                step: step_number,
                transform_id: step.definition.id,
                input_bytes: None,
                output_bytes: None,
                elapsed: None,
                status: StepStatus::NotExecuted,
                error: None,
            });
        } else if is_cancelled() {
            traces.push(StepTrace {
                step: step_number,
                transform_id: step.definition.id,
                input_bytes: Some(input.len()),
                output_bytes: None,
                elapsed: None,
                status: StepStatus::Cancelled,
                error: None,
            });
            outcome = Some(ExecutionOutcome::Cancelled);
        } else if !step.enabled {
            traces.push(StepTrace {
                step: step_number,
                transform_id: step.definition.id,
                input_bytes: Some(input.len()),
                output_bytes: Some(input.len()),
                elapsed: None,
                status: StepStatus::Disabled,
                error: None,
            });
        } else {
            let input_bytes = input.len();
            let started = Instant::now();
            let result = if !step.definition.accepts_binary && std::str::from_utf8(&input).is_err()
            {
                Err(TransformError::InvalidUtf8Input)
            } else {
                (step.definition.apply)(&input, output_limit).and_then(|output| {
                    if output.len() > output_limit {
                        Err(TransformError::OutputTooLarge {
                            limit: output_limit,
                        })
                    } else if policy == ExecutionPolicy::StrictText
                        && std::str::from_utf8(&output).is_err()
                    {
                        Err(invalid_utf8_output(&output))
                    } else {
                        Ok(output)
                    }
                })
            };

            match result {
                Ok(_) if is_cancelled() => {
                    traces.push(StepTrace {
                        step: step_number,
                        transform_id: step.definition.id,
                        input_bytes: Some(input_bytes),
                        output_bytes: None,
                        elapsed: None,
                        status: StepStatus::Cancelled,
                        error: None,
                    });
                    outcome = Some(ExecutionOutcome::Cancelled);
                }
                Ok(output) => {
                    let output_bytes = output.len();
                    traces.push(StepTrace {
                        step: step_number,
                        transform_id: step.definition.id,
                        input_bytes: Some(input_bytes),
                        output_bytes: Some(output_bytes),
                        elapsed: Some(started.elapsed()),
                        status: StepStatus::Succeeded,
                        error: None,
                    });
                    input = output;
                }
                Err(error) => {
                    traces.push(StepTrace {
                        step: step_number,
                        transform_id: step.definition.id,
                        input_bytes: Some(input_bytes),
                        output_bytes: None,
                        elapsed: None,
                        status: StepStatus::Failed,
                        error: Some(error.clone()),
                    });
                    outcome = Some(ExecutionOutcome::Failed(PipelineError::Step {
                        step: step_number,
                        transform_id: step.definition.id,
                        source: error,
                    }));
                }
            }
        }
    }

    ExecutionReport {
        request_id,
        target,
        outcome: outcome.unwrap_or_else(|| ExecutionOutcome::Success(input)),
        traces,
    }
}

pub fn execute(
    input: Vec<u8>,
    steps: &[TransformStep],
    output_limit: usize,
) -> Result<Vec<u8>, PipelineError> {
    match execute_report(
        ExecutionRequest {
            request_id: 0,
            input,
            steps,
            output_limit,
            policy: ExecutionPolicy::StrictText,
            target: ExecutionTarget::Final,
        },
        || false,
    )
    .outcome
    {
        ExecutionOutcome::Success(output) => Ok(output),
        ExecutionOutcome::Failed(error) => Err(error),
        ExecutionOutcome::Cancelled => unreachable!("strict synchronous execution cannot cancel"),
    }
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

    #[test]
    fn hex_transforms_share_binary_and_text_pipeline_rules() {
        let encoded = execute(vec![0x00, 0xff, b'A'], &[step("hex-encode", true)], 6).unwrap();
        assert_eq!(encoded, b"00ff41");

        let round_trip = execute(
            b"hello".to_vec(),
            &[step("hex-encode", true), step("hex-decode", true)],
            1024,
        )
        .unwrap();
        assert_eq!(round_trip, b"hello");
    }

    #[test]
    fn hex_failure_preserves_one_based_step_and_transform_id() {
        assert_eq!(
            execute(
                b"0x".to_vec(),
                &[step("url-decode", true), step("hex-decode", true)],
                1024,
            )
            .unwrap_err(),
            PipelineError::Step {
                step: 2,
                transform_id: "hex-decode",
                source: TransformError::InvalidHex { position: 1 },
            }
        );
    }

    fn request<'a>(
        input: &[u8],
        steps: &'a [TransformStep],
        policy: ExecutionPolicy,
        target: ExecutionTarget,
    ) -> ExecutionRequest<'a> {
        ExecutionRequest {
            request_id: 7,
            input: input.to_vec(),
            steps,
            output_limit: 1024,
            policy,
            target,
        }
    }

    #[test]
    fn allow_binary_preserves_decoder_bytes_but_public_execute_stays_strict() {
        let steps = [step("base64-decode", true)];
        let report = execute_report(
            ExecutionRequest {
                request_id: 7,
                input: b"/w==".to_vec(),
                steps: &steps,
                output_limit: 1024,
                policy: ExecutionPolicy::AllowBinary,
                target: ExecutionTarget::Final,
            },
            || false,
        );

        assert_eq!(report.request_id, 7);
        assert_eq!(report.outcome, ExecutionOutcome::Success(vec![0xff]));
        assert!(matches!(
            execute(b"/w==".to_vec(), &steps, 1024),
            Err(PipelineError::Step {
                step: 1,
                transform_id: "base64-decode",
                source: TransformError::InvalidUtf8Output { .. },
            })
        ));
    }

    #[test]
    fn allow_binary_chains_decoder_bytes_into_binary_encoder() {
        let steps = [step("base64-decode", true), step("hex-encode", true)];
        let report = execute_report(
            request(
                b"/w==",
                &steps,
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Final,
            ),
            || false,
        );

        assert_eq!(report.outcome, ExecutionOutcome::Success(b"ff".to_vec()));
        assert!(
            report
                .traces
                .iter()
                .all(|trace| trace.status == StepStatus::Succeeded)
        );
    }

    #[test]
    fn binary_output_stops_at_text_only_step() {
        let steps = [step("base64-decode", true), step("url-encode", true)];
        let report = execute_report(
            request(
                b"/w==",
                &steps,
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Final,
            ),
            || false,
        );

        assert_eq!(
            report.outcome,
            ExecutionOutcome::Failed(PipelineError::Step {
                step: 2,
                transform_id: "url-encode",
                source: TransformError::InvalidUtf8Input,
            })
        );
        assert_eq!(report.traces[1].status, StepStatus::Failed);
        assert_eq!(
            report.traces[1].error,
            Some(TransformError::InvalidUtf8Input)
        );
    }

    #[test]
    fn disabled_step_keeps_known_sizes_without_elapsed_time() {
        let steps = [step("base64-encode", false)];
        let report = execute_report(
            request(
                b"hello",
                &steps,
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Final,
            ),
            || false,
        );

        assert_eq!(report.outcome, ExecutionOutcome::Success(b"hello".to_vec()));
        assert_eq!(report.traces[0].status, StepStatus::Disabled);
        assert_eq!(report.traces[0].input_bytes, Some(5));
        assert_eq!(report.traces[0].output_bytes, Some(5));
        assert_eq!(report.traces[0].elapsed, None);
    }

    #[test]
    fn selected_disabled_stage_marks_later_steps_not_executed() {
        let steps = [
            step("base64-encode", true),
            step("base64-decode", false),
            step("hex-encode", true),
        ];
        let report = execute_report(
            request(
                b"hi",
                &steps,
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Step(1),
            ),
            || false,
        );

        assert_eq!(report.outcome, ExecutionOutcome::Success(b"aGk=".to_vec()));
        assert_eq!(
            report
                .traces
                .iter()
                .map(|trace| trace.status)
                .collect::<Vec<_>>(),
            vec![
                StepStatus::Succeeded,
                StepStatus::Disabled,
                StepStatus::NotExecuted
            ]
        );
        assert_eq!(report.traces[1].input_bytes, Some(4));
        assert_eq!(report.traces[1].output_bytes, Some(4));
        assert_eq!(report.traces[2].input_bytes, None);
    }

    #[test]
    fn failed_step_marks_later_steps_not_executed() {
        let steps = [step("base64-decode", true), step("hex-encode", true)];
        let report = execute_report(
            request(
                b"!",
                &steps,
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Final,
            ),
            || false,
        );

        assert_eq!(report.traces[0].status, StepStatus::Failed);
        assert_eq!(report.traces[1].status, StepStatus::NotExecuted);
    }

    #[test]
    fn cancellation_before_or_after_a_step_never_runs_later_steps() {
        let steps = [step("base64-encode", true), step("hex-encode", true)];
        let before = execute_report(
            request(
                b"x",
                &steps,
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Final,
            ),
            || true,
        );
        assert_eq!(before.outcome, ExecutionOutcome::Cancelled);
        assert_eq!(before.traces[0].status, StepStatus::Cancelled);
        assert_eq!(before.traces[1].status, StepStatus::NotExecuted);

        let checks = std::cell::Cell::new(0);
        let after = execute_report(
            request(
                b"x",
                &steps,
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Final,
            ),
            || {
                let check = checks.get();
                checks.set(check + 1);
                check == 1
            },
        );
        assert_eq!(after.outcome, ExecutionOutcome::Cancelled);
        assert_eq!(after.traces[0].status, StepStatus::Cancelled);
        assert_eq!(after.traces[1].status, StepStatus::NotExecuted);
    }

    #[test]
    fn empty_pipeline_returns_original_bytes_and_empty_trace() {
        let report = execute_report(
            request(
                b"hello",
                &[],
                ExecutionPolicy::AllowBinary,
                ExecutionTarget::Final,
            ),
            || false,
        );
        assert_eq!(report.outcome, ExecutionOutcome::Success(b"hello".to_vec()));
        assert!(report.traces.is_empty());
    }

    #[test]
    fn step_limit_and_output_limit_match_under_both_policies() {
        for policy in [ExecutionPolicy::StrictText, ExecutionPolicy::AllowBinary] {
            let allowed = vec![step("base64-encode", false); 32];
            let report = execute_report(
                request(b"x", &allowed, policy, ExecutionTarget::Final),
                || false,
            );
            assert_eq!(report.outcome, ExecutionOutcome::Success(b"x".to_vec()));

            let too_many = vec![step("base64-encode", false); 33];
            let report = execute_report(
                request(b"x", &too_many, policy, ExecutionTarget::Final),
                || false,
            );
            assert_eq!(
                report.outcome,
                ExecutionOutcome::Failed(PipelineError::TooManySteps { max: 32 })
            );
            assert!(report.traces.is_empty());

            let limited = [step("base64-encode", true)];
            let mut limited_request = request(b"foo", &limited, policy, ExecutionTarget::Final);
            limited_request.output_limit = 3;
            let report = execute_report(limited_request, || false);
            assert_eq!(
                report.outcome,
                ExecutionOutcome::Failed(PipelineError::Step {
                    step: 1,
                    transform_id: "base64-encode",
                    source: TransformError::OutputTooLarge { limit: 3 },
                })
            );
        }
    }
}
