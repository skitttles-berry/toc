use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use crate::{
    MAX_STEPS,
    error::PipelineError,
    pipeline::{ExecutionOutcome, ExecutionTarget, StepTrace},
};

pub(super) const VISIBLE_TEXT_BYTE_BUDGET: usize = 4 * 1024;
pub(super) const TEXT_VIEW_UNAVAILABLE_MESSAGE: &str = "Switch to Hex view";
pub(super) const LONG_RUNNING_AFTER: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub(super) struct Artifact {
    bytes: Arc<[u8]>,
    is_utf8: bool,
}

impl Artifact {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        let is_utf8 = std::str::from_utf8(&bytes).is_ok();
        Self {
            bytes: Arc::from(bytes),
            is_utf8,
        }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn is_utf8(&self) -> bool {
        self.is_utf8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ViewMode {
    Smart,
    Text,
    Hex,
    Trace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EffectiveView {
    Text,
    Hex,
    Trace,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Final,
    Step(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Idle,
    Debouncing {
        deadline: Instant,
    },
    Running {
        started_at: Instant,
        target: ExecutionTarget,
        notice_visible: bool,
    },
    Ready,
    Failed(PipelineError),
    Cancelled,
}

impl Status {
    pub(super) fn running(started_at: Instant, target: ExecutionTarget) -> Self {
        Self::Running {
            started_at,
            target,
            notice_visible: false,
        }
    }

    pub(super) fn running_target(&self) -> Option<ExecutionTarget> {
        match self {
            Self::Running { target, .. } => Some(*target),
            _ => None,
        }
    }

    pub(super) fn long_running_notice(&self) -> bool {
        matches!(
            self,
            Self::Running {
                notice_visible: true,
                ..
            }
        )
    }
}

pub(super) enum Lifecycle {
    Invalidate {
        deadline: Instant,
    },
    Start {
        started_at: Instant,
        target: ExecutionTarget,
    },
    Finish {
        target: ExecutionTarget,
        outcome: ExecutionOutcome,
        traces: Vec<StepTrace>,
    },
    RestoreFinal,
    Cancel,
    Tick {
        now: Instant,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LifecycleChange {
    Unchanged,
    Changed,
    StartFinal,
    FinalUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Viewport {
    pub(super) rows: usize,
    pub(super) columns: usize,
}

#[allow(dead_code)]
pub(super) struct Summary<'a> {
    pub(super) source: Source,
    pub(super) requested_view: ViewMode,
    pub(super) effective_view: EffectiveView,
    pub(super) status: &'a Status,
    pub(super) ready_bytes: Option<&'a [u8]>,
    pub(super) artifact: Option<&'a Artifact>,
    pub(super) traces: &'a [StepTrace],
}

pub(super) struct Output {
    pub(super) source: Source,
    pub(super) view: ViewMode,
    pub(super) status: Status,
    pub(super) final_artifact: Option<Artifact>,
    pub(super) final_traces: Vec<StepTrace>,
    pub(super) active_artifact: Option<Artifact>,
    pub(super) traces: Vec<StepTrace>,
    pub(super) byte_offset: usize,
    pub(super) row_offset: usize,
    pub(super) viewport: Option<Viewport>,
}

impl Output {
    pub(super) fn new() -> Self {
        Self {
            source: Source::Final,
            view: ViewMode::Smart,
            status: Status::Idle,
            final_artifact: None,
            final_traces: Vec::new(),
            active_artifact: None,
            traces: Vec::new(),
            byte_offset: 0,
            row_offset: 0,
            viewport: None,
        }
    }

    pub(super) fn update(&mut self, lifecycle: Lifecycle) -> LifecycleChange {
        match lifecycle {
            Lifecycle::Invalidate { deadline } => {
                self.status = Status::Debouncing { deadline };
                self.final_artifact = None;
                self.final_traces.clear();
                LifecycleChange::Changed
            }
            Lifecycle::Start { started_at, target } => {
                self.status = Status::running(started_at, target);
                LifecycleChange::Changed
            }
            Lifecycle::Finish {
                target,
                outcome,
                mut traces,
            } => {
                traces.truncate(MAX_STEPS);
                self.source = match target {
                    ExecutionTarget::Final => Source::Final,
                    ExecutionTarget::Step(index) => Source::Step(index),
                };
                self.traces = traces;
                self.byte_offset = 0;
                self.row_offset = 0;
                match outcome {
                    ExecutionOutcome::Success(bytes) => {
                        let artifact = Artifact::new(bytes);
                        if target == ExecutionTarget::Final {
                            self.final_artifact = Some(artifact.clone());
                            self.final_traces.clone_from(&self.traces);
                        }
                        self.active_artifact = Some(artifact);
                        self.status = Status::Ready;
                    }
                    ExecutionOutcome::Failed(error) => {
                        self.active_artifact = None;
                        if target == ExecutionTarget::Final {
                            self.final_artifact = None;
                            self.final_traces.clear();
                        }
                        self.status = Status::Failed(error);
                    }
                    ExecutionOutcome::Cancelled => {
                        self.active_artifact = None;
                        if target == ExecutionTarget::Final {
                            self.final_artifact = None;
                            self.final_traces.clear();
                        }
                        self.status = Status::Cancelled;
                    }
                }
                LifecycleChange::Changed
            }
            Lifecycle::RestoreFinal => {
                let Some(artifact) = self.final_artifact.clone() else {
                    return LifecycleChange::FinalUnavailable;
                };
                self.source = Source::Final;
                self.status = Status::Ready;
                self.active_artifact = Some(artifact);
                self.traces.clone_from(&self.final_traces);
                self.byte_offset = 0;
                self.row_offset = 0;
                LifecycleChange::Changed
            }
            Lifecycle::Cancel => {
                if !matches!(
                    self.status,
                    Status::Debouncing { .. } | Status::Running { .. }
                ) {
                    return LifecycleChange::Unchanged;
                }
                self.status = Status::Cancelled;
                self.active_artifact = None;
                self.traces.clear();
                LifecycleChange::Changed
            }
            Lifecycle::Tick { now } => match &mut self.status {
                Status::Debouncing { deadline } if now >= *deadline => {
                    self.status = Status::running(now, ExecutionTarget::Final);
                    LifecycleChange::StartFinal
                }
                Status::Running {
                    started_at,
                    notice_visible,
                    ..
                } if !*notice_visible
                    && now.saturating_duration_since(*started_at) >= LONG_RUNNING_AFTER =>
                {
                    *notice_visible = true;
                    LifecycleChange::Changed
                }
                _ => LifecycleChange::Unchanged,
            },
        }
    }

    pub(super) fn summary(&self) -> Summary<'_> {
        let artifact = self.active_artifact.as_ref();
        Summary {
            source: self.source,
            requested_view: self.view,
            effective_view: effective_view(
                self.view,
                artifact,
                matches!(self.status, Status::Failed(_)),
            ),
            status: &self.status,
            ready_bytes: matches!(self.status, Status::Ready)
                .then(|| artifact.map(Artifact::bytes))
                .flatten(),
            artifact,
            traces: &self.traces,
        }
    }

    pub(super) fn copy_artifact(&self) -> Option<Artifact> {
        matches!(self.status, Status::Ready)
            .then(|| self.active_artifact.clone())
            .flatten()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct TextWindow {
    pub text: String,
    pub next_offset: usize,
    pub inspected_bytes: usize,
}

pub(super) fn effective_view(
    mode: ViewMode,
    artifact: Option<&Artifact>,
    failed: bool,
) -> EffectiveView {
    match mode {
        ViewMode::Smart if failed => EffectiveView::Trace,
        ViewMode::Smart => match artifact {
            Some(artifact) if artifact.is_utf8() => EffectiveView::Text,
            Some(_) => EffectiveView::Hex,
            None => EffectiveView::Unavailable,
        },
        ViewMode::Text => match artifact {
            Some(artifact) if !artifact.is_utf8() => EffectiveView::Unavailable,
            Some(_) => EffectiveView::Text,
            None => EffectiveView::Unavailable,
        },
        ViewMode::Hex => EffectiveView::Hex,
        ViewMode::Trace => EffectiveView::Trace,
    }
}

fn utf8_boundary_at_or_before(bytes: &[u8], offset: usize) -> usize {
    let mut boundary = offset.min(bytes.len());
    while boundary > 0 && boundary < bytes.len() && bytes[boundary] & 0b1100_0000 == 0b1000_0000 {
        boundary -= 1;
    }
    boundary
}

fn next_utf8_boundary(bytes: &[u8], offset: usize) -> usize {
    let mut boundary = offset.saturating_add(1).min(bytes.len());
    while boundary < bytes.len() && bytes[boundary] & 0b1100_0000 == 0b1000_0000 {
        boundary += 1;
    }
    boundary
}

fn bounded_utf8_text(artifact: &Artifact, offset: usize) -> Option<(usize, &str)> {
    if !artifact.is_utf8() {
        return None;
    }
    let bytes = artifact.bytes();
    let start = utf8_boundary_at_or_before(bytes, offset);
    let end = utf8_boundary_at_or_before(
        bytes,
        start
            .saturating_add(VISIBLE_TEXT_BYTE_BUDGET)
            .min(bytes.len()),
    );
    std::str::from_utf8(&bytes[start..end])
        .ok()
        .map(|text| (start, text))
}

fn is_dangerous_text_control(character: char) -> bool {
    character == '\r' || crate::error::is_dangerous_control(character)
}

pub(super) fn render_text_window(
    artifact: &Artifact,
    offset: usize,
    rows: usize,
    columns: usize,
) -> TextWindow {
    let Some((start, source)) = bounded_utf8_text(artifact, offset) else {
        return TextWindow {
            text: String::new(),
            next_offset: 0,
            inspected_bytes: 0,
        };
    };
    let bytes = artifact.bytes();
    let truncated = start + source.len() < bytes.len();
    let mut output = String::new();
    let mut cursor = start;
    let mut row = 0;
    let mut used_width = 0;
    let mut fallback = None;

    if rows > 0 && columns > 0 {
        for (relative, grapheme) in source.grapheme_indices(true) {
            if truncated && relative + grapheme.len() == source.len() {
                fallback = Some((start + source.len(), true));
                break;
            }
            if grapheme == "\r\n" {
                let escaped_cr = "\\x0d";
                if output.len() + escaped_cr.len() <= VISIBLE_TEXT_BYTE_BUDGET
                    && used_width + escaped_cr.width() <= columns
                {
                    output.push_str(escaped_cr);
                }
                cursor = start + relative + grapheme.len();
                if row + 1 >= rows || output.len() == VISIBLE_TEXT_BYTE_BUDGET {
                    break;
                }
                output.push('\n');
                row += 1;
                used_width = 0;
                continue;
            }
            if grapheme == "\n" {
                if row + 1 >= rows || output.len() == VISIBLE_TEXT_BYTE_BUDGET {
                    fallback = Some((start + relative + grapheme.len(), false));
                    break;
                }
                output.push('\n');
                cursor = start + relative + grapheme.len();
                row += 1;
                used_width = 0;
                continue;
            }
            let dangerous = grapheme.chars().any(is_dangerous_text_control);
            let escaped = dangerous.then(|| {
                crate::error::escape_controls(grapheme, grapheme.chars().count())
                    .replace('\r', "\\x0d")
            });
            let rendered = escaped.as_deref().unwrap_or(grapheme);
            let rendered_width = rendered.width();
            if output.len() + rendered.len() > VISIBLE_TEXT_BYTE_BUDGET {
                fallback = Some((start + relative + grapheme.len(), false));
                break;
            }
            if rendered_width > columns {
                fallback = Some((start + relative + grapheme.len(), false));
                break;
            }
            if used_width + rendered_width > columns {
                if row + 1 >= rows || output.len() + 1 + rendered.len() > VISIBLE_TEXT_BYTE_BUDGET {
                    fallback = Some((start + relative, false));
                    break;
                }
                output.push('\n');
                row += 1;
                used_width = 0;
            }
            output.push_str(rendered);
            cursor = start + relative + grapheme.len();
            used_width += rendered_width;
        }
    }

    if rows > 0 && columns > 0 && cursor == start && start < bytes.len() {
        let (next, show_placeholder) =
            fallback.unwrap_or_else(|| (next_utf8_boundary(bytes, start), false));
        if show_placeholder {
            output.push('…');
        }
        cursor = next;
    }

    TextWindow {
        next_offset: cursor,
        inspected_bytes: cursor.saturating_sub(start),
        text: output,
    }
}

pub(super) fn next_text_offset(artifact: &Artifact, offset: usize) -> usize {
    render_text_window(artifact, offset, 1, 1).next_offset
}

pub(super) fn previous_text_offset(artifact: &Artifact, offset: usize) -> usize {
    if !artifact.is_utf8() {
        return 0;
    }
    let bytes = artifact.bytes();
    let end = utf8_boundary_at_or_before(bytes, offset);
    let start = utf8_boundary_at_or_before(bytes, end.saturating_sub(VISIBLE_TEXT_BYTE_BUDGET));
    std::str::from_utf8(&bytes[start..end])
        .ok()
        .and_then(|text| text.grapheme_indices(true).next_back())
        .map_or(start, |(relative, _)| start + relative)
}

pub(super) fn last_text_offset(artifact: &Artifact) -> usize {
    previous_text_offset(artifact, artifact.bytes().len())
}

pub(super) fn next_text_page_offset(
    artifact: &Artifact,
    offset: usize,
    rows: usize,
    columns: usize,
) -> usize {
    if rows == 0 || columns == 0 {
        return offset.min(artifact.bytes().len());
    }
    let next = render_text_window(artifact, offset, rows, columns).next_offset;
    if next >= artifact.bytes().len() {
        offset.min(artifact.bytes().len())
    } else {
        next
    }
}

pub(super) fn previous_text_page_offset(
    artifact: &Artifact,
    offset: usize,
    rows: usize,
    columns: usize,
) -> usize {
    if !artifact.is_utf8() || rows == 0 || columns == 0 {
        return 0;
    }
    let bytes = artifact.bytes();
    let target = utf8_boundary_at_or_before(bytes, offset);
    if target == 0 {
        return 0;
    }
    let search_start =
        utf8_boundary_at_or_before(bytes, target.saturating_sub(VISIBLE_TEXT_BYTE_BUDGET));
    let mut candidate = search_start;
    // ponytail: this scan is capped at 4 KiB; cache page starts only if profiling
    // shows repeated reverse navigation spending measurable time here.
    while candidate < target {
        if render_text_window(artifact, candidate, rows, columns).next_offset >= target {
            return candidate;
        }
        let next = next_text_offset(artifact, candidate);
        if next <= candidate {
            break;
        }
        candidate = next.min(target);
    }
    search_start
}

pub(super) fn last_text_page_offset(artifact: &Artifact, rows: usize, columns: usize) -> usize {
    previous_text_page_offset(artifact, artifact.bytes().len(), rows, columns)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct HexRow<'a> {
    pub(super) offset: usize,
    pub(super) bytes: &'a [u8],
}

pub(super) fn hex_bytes_per_row(columns: usize) -> usize {
    if columns < 60 { 8 } else { 16 }
}

fn hex_row_cost(columns: usize) -> usize {
    match columns {
        78.. => 77,
        60..=77 => 59,
        _ => 34,
    }
}

pub(super) fn hex_visible_row_capacity(rows: usize, columns: usize) -> usize {
    let row_cost = hex_row_cost(columns);
    let budget_rows = VISIBLE_TEXT_BYTE_BUDGET.saturating_sub(row_cost) / row_cost.max(1);
    rows.min(budget_rows)
}

pub(super) fn visible_hex_rows<'a>(
    artifact: &'a Artifact,
    row_offset: usize,
    rows: usize,
    columns: usize,
) -> Vec<HexRow<'a>> {
    let bytes_per_row = hex_bytes_per_row(columns);
    let visible_rows = hex_visible_row_capacity(rows, columns);
    let mut visible = Vec::with_capacity(visible_rows);
    for row in row_offset..row_offset.saturating_add(visible_rows) {
        let Some(offset) = row.checked_mul(bytes_per_row) else {
            break;
        };
        if offset >= artifact.bytes().len() {
            break;
        }
        let end = offset
            .saturating_add(bytes_per_row)
            .min(artifact.bytes().len());
        visible.push(HexRow {
            offset,
            bytes: &artifact.bytes()[offset..end],
        });
    }
    visible
}

pub(super) fn trace_failure_detail_height(
    traces: &[crate::pipeline::StepTrace],
    area_height: usize,
) -> usize {
    let has_failure_detail = traces
        .iter()
        .take(crate::MAX_STEPS)
        .find(|trace| trace.status == crate::pipeline::StepStatus::Failed)
        .is_some_and(|trace| trace.error.is_some());
    if !has_failure_detail {
        0
    } else if area_height >= 5 {
        3
    } else {
        area_height.saturating_sub(2).min(2)
    }
}

pub(super) fn trace_visible_row_capacity(
    traces: &[crate::pipeline::StepTrace],
    area_height: usize,
    columns: usize,
) -> usize {
    let detail_height = trace_failure_detail_height(traces, area_height);
    let geometry_rows = area_height.saturating_sub(detail_height).saturating_sub(1);
    let header_cost = if columns >= 70 { 34 } else { 23 };
    let detail_cost = detail_height
        .min(2)
        .saturating_mul(columns.saturating_sub(1));
    let budget_rows = VISIBLE_TEXT_BYTE_BUDGET
        .saturating_sub(header_cost)
        .saturating_sub(detail_cost)
        / columns.max(1);
    geometry_rows.min(budget_rows)
}

pub(super) fn trace_start_row(
    traces: &[crate::pipeline::StepTrace],
    row_offset: usize,
    area_height: usize,
) -> usize {
    if area_height >= 5 {
        return row_offset;
    }
    traces
        .iter()
        .take(crate::MAX_STEPS)
        .position(|trace| trace.status == crate::pipeline::StepStatus::Failed)
        .unwrap_or(row_offset)
}

pub(super) fn trace_status(status: crate::pipeline::StepStatus) -> &'static str {
    match status {
        crate::pipeline::StepStatus::Succeeded => "OK",
        crate::pipeline::StepStatus::Disabled => "OFF",
        crate::pipeline::StepStatus::Failed => "ERROR",
        crate::pipeline::StepStatus::NotExecuted => "NOT RUN",
        crate::pipeline::StepStatus::Cancelled => "CANCELLED",
    }
}

pub(super) fn render_transform_error_summary(error: &crate::error::TransformError) -> String {
    use crate::error::{JsonErrorKind, TransformError};

    match error {
        TransformError::InvalidUtf8Input => "input is not valid UTF-8".to_string(),
        TransformError::InvalidIpAddress => "invalid IP address".to_string(),
        TransformError::InvalidUtf16 { position } => {
            format!("invalid UTF-16 at byte {position}")
        }
        TransformError::InvalidBase64 {
            position: Some(position),
        } => {
            format!("invalid Base64 at byte {position}")
        }
        TransformError::InvalidBase64 { position: None } => "invalid Base64 padding".to_string(),
        TransformError::InvalidBase32 {
            position: Some(position),
        } => {
            format!("invalid Base32 at byte {position}")
        }
        TransformError::InvalidBase32 { position: None } => "invalid Base32 padding".to_string(),
        TransformError::InvalidUrl { position } => {
            format!("invalid percent escape at byte {position}")
        }
        TransformError::InvalidHex { position } => {
            format!("invalid hex character at byte {position}")
        }
        TransformError::OddHexDigitCount { digits } => {
            format!("hex input has an odd number of digits: {digits}")
        }
        TransformError::InvalidUtf8Output { total_bytes, .. } => {
            format!("output is not valid UTF-8 ({total_bytes} bytes)")
        }
        TransformError::InvalidJson { line, column, kind } => {
            let reason = match kind {
                JsonErrorKind::Syntax => "invalid JSON syntax",
                JsonErrorKind::DuplicateKey => "duplicate JSON object key",
                JsonErrorKind::Bom => "UTF-8 BOM is not allowed",
                JsonErrorKind::DepthExceeded => "JSON depth exceeds 128",
                JsonErrorKind::ExpectedString => "expected a JSON string",
            };
            format!("{reason} at line {line}, column {column}")
        }
        TransformError::InvalidJwtPart => "invalid JWT part".to_string(),
        TransformError::InvalidGzip => "invalid Gzip data".to_string(),
        TransformError::InvalidZlib => "invalid zlib data".to_string(),
        TransformError::TooManyLines { limit } => {
            format!("input exceeds {limit} logical lines")
        }
        TransformError::OutputTooLarge { limit } => format!("output exceeds {limit} bytes"),
    }
}

pub(super) fn render_pipeline_error_summary(error: &crate::error::PipelineError) -> String {
    match error {
        crate::error::PipelineError::TooManySteps { max } => {
            format!("chain exceeds {max} steps")
        }
        crate::error::PipelineError::Step {
            step,
            transform_id,
            source,
        } => format!(
            "step {step} ({}) failed: {}",
            crate::error::escape_external(transform_id, 128),
            render_transform_error_summary(source)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::StepStatus;

    fn finish(
        output: &mut Output,
        target: crate::pipeline::ExecutionTarget,
        bytes: &[u8],
        traces: Vec<StepTrace>,
    ) {
        assert_eq!(
            output.update(Lifecycle::Start {
                started_at: std::time::Instant::now(),
                target,
            }),
            LifecycleChange::Changed
        );
        assert_eq!(
            output.update(Lifecycle::Finish {
                target,
                outcome: crate::pipeline::ExecutionOutcome::Success(bytes.to_vec()),
                traces,
            }),
            LifecycleChange::Changed
        );
    }

    #[test]
    fn selected_output_preserves_the_final_snapshot_for_restore() {
        let mut output = Output::new();
        let final_traces = vec![StepTrace {
            step: 1,
            transform_id: "base64-encode",
            input_bytes: Some(3),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        }];
        let step_traces = vec![StepTrace {
            step: 2,
            transform_id: "hex-encode",
            input_bytes: Some(4),
            output_bytes: Some(8),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        }];
        finish(
            &mut output,
            crate::pipeline::ExecutionTarget::Final,
            b"final",
            final_traces.clone(),
        );
        finish(
            &mut output,
            crate::pipeline::ExecutionTarget::Step(0),
            b"step",
            step_traces,
        );

        assert_eq!(
            output.update(Lifecycle::RestoreFinal),
            LifecycleChange::Changed
        );
        let summary = output.summary();
        assert_eq!(summary.source, Source::Final);
        assert_eq!(summary.artifact.unwrap().bytes(), b"final");
        assert_eq!(summary.traces, final_traces);
    }

    #[test]
    fn invalidation_keeps_current_output_but_blocks_copy_and_final_restore() {
        let mut output = Output::new();
        finish(
            &mut output,
            crate::pipeline::ExecutionTarget::Final,
            b"final",
            Vec::new(),
        );

        assert_eq!(
            output.update(Lifecycle::Invalidate {
                deadline: std::time::Instant::now(),
            }),
            LifecycleChange::Changed
        );
        assert_eq!(output.summary().artifact.unwrap().bytes(), b"final");
        assert!(output.copy_artifact().is_none());
        assert_eq!(
            output.update(Lifecycle::RestoreFinal),
            LifecycleChange::FinalUnavailable
        );
    }

    #[test]
    fn tick_requests_final_once_at_deadline() {
        let mut output = Output::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1);
        output.update(Lifecycle::Invalidate { deadline });

        assert_eq!(
            output.update(Lifecycle::Tick {
                now: deadline - std::time::Duration::from_nanos(1),
            }),
            LifecycleChange::Unchanged
        );
        assert_eq!(
            output.update(Lifecycle::Tick { now: deadline }),
            LifecycleChange::StartFinal
        );
        assert_eq!(
            output.update(Lifecycle::Tick { now: deadline }),
            LifecycleChange::Unchanged
        );
    }

    #[test]
    fn tick_shows_the_long_running_notice_once_at_threshold() {
        let mut output = Output::new();
        let started_at = std::time::Instant::now();
        output.update(Lifecycle::Start {
            started_at,
            target: crate::pipeline::ExecutionTarget::Final,
        });

        assert_eq!(
            output.update(Lifecycle::Tick {
                now: started_at + LONG_RUNNING_AFTER - std::time::Duration::from_nanos(1),
            }),
            LifecycleChange::Unchanged
        );
        assert_eq!(
            output.update(Lifecycle::Tick {
                now: started_at + LONG_RUNNING_AFTER,
            }),
            LifecycleChange::Changed
        );
        assert_eq!(
            output.update(Lifecycle::Tick {
                now: started_at + LONG_RUNNING_AFTER,
            }),
            LifecycleChange::Unchanged
        );
    }

    #[test]
    fn summarizes_utf16_errors_without_input_content() {
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::InvalidUtf16 {
                position: 4,
            }),
            "invalid UTF-16 at byte 4"
        );
    }

    #[test]
    fn summarizes_ip_errors_without_input_content() {
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::InvalidIpAddress),
            "invalid IP address"
        );
    }

    #[test]
    fn summarizes_base32_errors_without_input_content() {
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::InvalidBase32 {
                position: Some(7),
            }),
            "invalid Base32 at byte 7"
        );
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::InvalidBase32 {
                position: None,
            }),
            "invalid Base32 padding"
        );
    }

    #[test]
    fn summarizes_gzip_errors_without_input_content() {
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::InvalidGzip),
            "invalid Gzip data"
        );
    }

    #[test]
    fn summarizes_zlib_errors_without_input_content() {
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::InvalidZlib),
            "invalid zlib data"
        );
    }

    #[test]
    fn summarizes_line_limit_errors_without_input_content() {
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::TooManyLines {
                limit: 1_000_000,
            }),
            "input exceeds 1000000 logical lines"
        );
    }

    #[test]
    fn summarizes_expected_json_string_errors_without_input_content() {
        assert_eq!(
            render_transform_error_summary(&crate::error::TransformError::InvalidJson {
                line: 1,
                column: 3,
                kind: crate::error::JsonErrorKind::ExpectedString,
            }),
            "expected a JSON string at line 1, column 3"
        );
    }

    #[test]
    fn smart_uses_trace_for_failure_text_for_utf8_and_hex_for_binary() {
        assert_eq!(
            effective_view(ViewMode::Smart, None, true),
            EffectiveView::Trace
        );
        assert_eq!(
            effective_view(
                ViewMode::Smart,
                Some(&Artifact::new(b"hello".to_vec())),
                false
            ),
            EffectiveView::Text
        );
        assert_eq!(
            effective_view(ViewMode::Smart, Some(&Artifact::new(vec![0xff])), false),
            EffectiveView::Hex
        );
    }

    #[test]
    fn pinned_text_for_binary_is_unavailable_without_changing_mode() {
        assert_eq!(
            effective_view(ViewMode::Text, Some(&Artifact::new(vec![0xff])), false),
            EffectiveView::Unavailable
        );
        assert_eq!(
            effective_view(ViewMode::Hex, Some(&Artifact::new(b"text".to_vec())), true),
            EffectiveView::Hex
        );
    }

    #[test]
    fn text_window_starts_at_utf8_boundary_and_preserves_tabs_and_newlines() {
        let artifact = Artifact::new("a界\tb\nnext".as_bytes().to_vec());

        let window = render_text_window(&artifact, 2, 2, 8);

        assert_eq!(window.text, "界\tb\nnext");
        assert_eq!(window.next_offset, artifact.bytes().len());
        assert!(window.inspected_bytes <= VISIBLE_TEXT_BYTE_BUDGET);
        assert!(window.text.len() <= VISIBLE_TEXT_BYTE_BUDGET);
    }

    #[test]
    fn text_window_soft_wraps_across_all_visible_rows() {
        let artifact = Artifact::new(b"abcdefgh".to_vec());

        let full = render_text_window(&artifact, 0, 3, 3);
        assert_eq!(full.text, "abc\ndef\ngh");
        assert_eq!(full.next_offset, 8);

        let first = render_text_window(&artifact, 0, 1, 3);
        assert_eq!(first.text, "abc");
        assert_eq!(first.next_offset, 3);

        let second = render_text_window(&artifact, first.next_offset, 1, 3);
        assert_eq!(second.text, "def");
        assert_eq!(second.next_offset, 6);
    }

    #[test]
    fn text_window_soft_wraps_wide_graphemes_by_display_width() {
        let artifact = Artifact::new("界界界".as_bytes().to_vec());

        let window = render_text_window(&artifact, 0, 2, 4);

        assert_eq!(window.text, "界界\n界");
        assert_eq!(window.next_offset, artifact.bytes().len());
    }

    #[test]
    fn text_window_wraps_escaped_controls_without_changing_source() {
        let source = b"a\x1bb".to_vec();
        let artifact = Artifact::new(source.clone());

        let window = render_text_window(&artifact, 0, 2, 4);

        assert_eq!(window.text, "a\n\\x1b");
        assert_eq!(window.next_offset, 2);
        assert_eq!(artifact.bytes(), source.as_slice());
    }

    #[test]
    fn text_window_escapes_every_dangerous_c0_and_c1_control() {
        let mut text = String::from("tab\tnewline\ncarriage\r");
        text.extend((0..=0x1f).filter_map(char::from_u32));
        text.extend((0x7f..=0x9f).filter_map(char::from_u32));
        text.push_str("\u{1b}]52;c;secret\u{7}");
        let artifact = Artifact::new(text.into_bytes());

        let window = render_text_window(&artifact, 0, 8, 4_096);

        assert!(window.text.starts_with("tab\tnewline\ncarriage\\x0d"));
        assert!(window.text.contains("\\x00"));
        assert!(window.text.contains("\\x1b]52;c;secret\\x07"));
        assert!(!window.text.contains('\r'));
        assert!(!window.text.chars().any(crate::error::is_dangerous_control));
        assert!(window.inspected_bytes <= VISIBLE_TEXT_BYTE_BUDGET);
        assert!(window.text.len() <= VISIBLE_TEXT_BYTE_BUDGET);
    }

    #[test]
    fn text_window_stops_before_a_long_line_exceeds_either_budget() {
        let artifact = Artifact::new("界".repeat(3_000).into_bytes());

        let window = render_text_window(&artifact, 0, 1, 8_192);

        assert!(window.inspected_bytes <= VISIBLE_TEXT_BYTE_BUDGET);
        assert!(window.text.len() <= VISIBLE_TEXT_BYTE_BUDGET);
        assert!(window.next_offset > 0);
        assert!(window.next_offset < artifact.bytes().len());
        assert!(
            std::str::from_utf8(artifact.bytes())
                .unwrap()
                .is_char_boundary(window.next_offset)
        );
    }

    #[test]
    fn text_window_advances_past_a_non_displayable_first_grapheme() {
        let cases = [
            ("\nnext".to_string(), 1, 1, ""),
            ("界".to_string(), 1, 1, ""),
            (
                format!("e{}", "\u{301}".repeat(VISIBLE_TEXT_BYTE_BUDGET)),
                1,
                80,
                "…",
            ),
        ];

        for (text, rows, columns, expected) in cases {
            let artifact = Artifact::new(text.into_bytes());
            let window = render_text_window(&artifact, 0, rows, columns);
            let text = std::str::from_utf8(artifact.bytes()).unwrap();

            assert_eq!(window.text, expected);
            assert!(window.next_offset > 0);
            assert!(window.next_offset <= artifact.bytes().len());
            assert!(text.is_char_boundary(window.next_offset));
        }
    }

    #[test]
    fn truncated_first_grapheme_uses_a_visible_bounded_fallback() {
        let text = format!("a{}b", "\u{301}".repeat(3_000));
        let artifact = Artifact::new(text.into_bytes());

        let window = render_text_window(&artifact, 0, 1, 80);

        assert!(!window.text.is_empty());
        assert!(window.next_offset > 1);
        assert!(window.next_offset <= 4 * 1024);
        assert!(window.inspected_bytes <= 4 * 1024);
        assert!(
            std::str::from_utf8(artifact.bytes())
                .unwrap()
                .is_char_boundary(window.next_offset)
        );
    }

    #[test]
    fn text_window_treats_crlf_as_a_row_boundary_without_skipping_next_line() {
        let artifact = Artifact::new(b"\r\nsecret\n".to_vec());

        let first = render_text_window(&artifact, 0, 1, 80);
        let second = render_text_window(&artifact, first.next_offset, 1, 80);
        let text = std::str::from_utf8(artifact.bytes()).unwrap();

        assert_eq!(first.text, "\\x0d");
        assert!(!first.text.contains('\r'));
        assert!(!first.text.contains("secret"));
        assert_eq!(first.next_offset, 2);
        assert_eq!(second.text, "secret");
        assert!(text.is_char_boundary(first.next_offset));
        assert!(text.is_char_boundary(second.next_offset));
    }

    #[test]
    fn text_window_handles_newline_dense_sixty_four_mebibyte_artifacts_without_line_indexing() {
        let artifact = Artifact::new(vec![b'\n'; 64 * 1024 * 1024]);

        let window = render_text_window(&artifact, 0, 8, 80);

        assert_eq!(artifact.bytes().len(), 64 * 1024 * 1024);
        assert!(window.inspected_bytes <= VISIBLE_TEXT_BYTE_BUDGET);
        assert!(window.text.len() <= VISIBLE_TEXT_BYTE_BUDGET);
        assert!(window.next_offset <= VISIBLE_TEXT_BYTE_BUDGET);
    }

    #[test]
    fn text_window_validates_only_a_bounded_utf8_slice_of_a_large_artifact() {
        let artifact = Artifact::new(vec![b'x'; 64 * 1024 * 1024]);

        let (start, text) = bounded_utf8_text(&artifact, 3).unwrap();

        assert_eq!(start, 3);
        assert_eq!(text.len(), VISIBLE_TEXT_BYTE_BUDGET);
        assert!(text.len() <= VISIBLE_TEXT_BYTE_BUDGET);
    }

    #[test]
    fn text_page_boundaries_reuse_rendering_rules_for_ascii_and_unicode() {
        for (text, rows, columns, next, previous, last) in [
            ("abcdef", 2, 2, 4, 0, 2),
            ("가나다라", 1, 4, 6, 0, 6),
            ("a\u{301}bc", 1, 2, 4, 0, 3),
            ("a\nb\nc", 2, 10, 3, 0, 2),
            ("\u{1b}ab", 1, 4, 1, 0, 1),
        ] {
            let artifact = Artifact::new(text.as_bytes().to_vec());

            assert_eq!(
                next_text_page_offset(&artifact, 0, rows, columns),
                next,
                "next page for {text:?}"
            );
            assert_eq!(
                previous_text_page_offset(&artifact, next, rows, columns),
                previous,
                "previous page for {text:?}"
            );
            assert_eq!(
                last_text_page_offset(&artifact, rows, columns),
                last,
                "last page for {text:?}"
            );
            assert_eq!(
                next_text_page_offset(&artifact, last, rows, columns),
                last,
                "PageDown must stop when the end is already visible for {text:?}"
            );
        }
    }

    #[test]
    fn reverse_text_page_search_never_inspects_beyond_four_kibibytes() {
        let artifact = Artifact::new("가".repeat(8_192).into_bytes());
        let offset = artifact.bytes().len();

        let previous = previous_text_page_offset(&artifact, offset, 2, 4);

        assert!(offset.saturating_sub(previous) <= VISIBLE_TEXT_BYTE_BUDGET);
        assert!(
            std::str::from_utf8(artifact.bytes())
                .unwrap()
                .is_char_boundary(previous)
        );
    }

    #[test]
    fn hex_rows_switch_between_sixteen_and_eight_bytes_at_exact_widths() {
        let artifact = Artifact::new((0..40).collect());
        assert_eq!(hex_bytes_per_row(78), 16);
        assert_eq!(hex_bytes_per_row(60), 16);
        assert_eq!(hex_bytes_per_row(59), 8);

        let wide = visible_hex_rows(&artifact, 1, 2, 78);
        assert_eq!(wide[0].offset, 16);
        assert_eq!(
            wide[0].bytes,
            &[
                16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31
            ],
        );

        let narrow = visible_hex_rows(&artifact, 1, 2, 59);
        assert_eq!(narrow[0].offset, 8);
        assert_eq!(narrow[0].bytes, &[8, 9, 10, 11, 12, 13, 14, 15]);
        assert_eq!(narrow[1].offset, 16);
    }

    #[test]
    fn hex_rows_are_bounded_by_the_existing_view_budget() {
        let artifact = Artifact::new(vec![0xff; 64 * 1024]);
        for columns in [38, 60, 78] {
            let rows = visible_hex_rows(&artifact, 0, 10_000, columns);
            let row_cost = match columns {
                78.. => 77,
                60..=77 => 59,
                _ => 34,
            };
            let rendered_cost = rows.len().saturating_add(1).saturating_mul(row_cost);
            assert!(rendered_cost <= VISIBLE_TEXT_BYTE_BUDGET);
        }
    }

    #[test]
    fn visible_data_row_capacity_excludes_headers_and_trace_failure_detail() {
        use crate::{
            error::TransformError,
            pipeline::{StepStatus, StepTrace},
        };

        assert_eq!(hex_visible_row_capacity(9, 78), 9);
        assert!(hex_visible_row_capacity(10_000, 78) < 10_000);

        let success = StepTrace {
            step: 1,
            transform_id: "base64-encode",
            input_bytes: Some(1),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        };
        let failure = StepTrace {
            status: StepStatus::Failed,
            error: Some(TransformError::InvalidBase64 { position: Some(0) }),
            ..success.clone()
        };

        assert_eq!(
            trace_visible_row_capacity(std::slice::from_ref(&success), 8, 80),
            7
        );
        assert_eq!(
            trace_failure_detail_height(std::slice::from_ref(&failure), 8),
            3
        );
        assert_eq!(
            trace_visible_row_capacity(std::slice::from_ref(&failure), 8, 80),
            4
        );
        assert_eq!(
            trace_failure_detail_height(std::slice::from_ref(&failure), 4),
            2
        );
        assert_eq!(
            trace_visible_row_capacity(std::slice::from_ref(&failure), 4, 80),
            1
        );

        let traces = [success, failure];
        assert_eq!(trace_start_row(&traces, 0, 4), 1);
        assert_eq!(trace_start_row(&traces, 0, 5), 0);
    }

    #[test]
    #[ignore = "release-only UTF-8 validation measurement"]
    fn utf8_validation_release_measurement() {
        const WARMUPS: usize = 5;
        const SAMPLES: usize = 30;

        let bytes = vec![b'a'; crate::TUI_OUTPUT_LIMIT];
        let measure = || {
            let started = std::time::Instant::now();
            assert!(std::str::from_utf8(std::hint::black_box(bytes.as_slice())).is_ok());
            started.elapsed()
        };

        for _ in 0..WARMUPS {
            std::hint::black_box(measure());
        }
        let mut samples = (0..SAMPLES).map(|_| measure()).collect::<Vec<_>>();
        samples.sort_unstable();
        eprintln!(
            "UTF-8 validation release measurement: warmups={WARMUPS}, samples={SAMPLES}, min={:?}, median={:?}, max={:?}",
            samples[0],
            samples[SAMPLES / 2],
            samples[SAMPLES - 1]
        );
    }
}
