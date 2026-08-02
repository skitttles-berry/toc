use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

pub(super) const VISIBLE_TEXT_BYTE_BUDGET: usize = 4 * 1024;
pub(super) const TEXT_VIEW_UNAVAILABLE_MESSAGE: &str = "Switch to Hex view";

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

#[derive(Debug, PartialEq, Eq)]
pub(super) struct HexRow<'a> {
    pub(super) offset: usize,
    pub(super) bytes: &'a [u8],
}

pub(super) fn hex_bytes_per_row(columns: usize) -> usize {
    if columns < 60 { 8 } else { 16 }
}

pub(super) fn visible_hex_rows<'a>(
    artifact: &'a Artifact,
    row_offset: usize,
    rows: usize,
    columns: usize,
) -> Vec<HexRow<'a>> {
    let bytes_per_row = hex_bytes_per_row(columns);
    let row_cost = match columns {
        78.. => 77,
        60..=77 => 59,
        _ => 34,
    };
    let budget_rows = VISIBLE_TEXT_BYTE_BUDGET.saturating_sub(row_cost) / row_cost.max(1);
    let mut visible = Vec::with_capacity(rows.min(budget_rows));
    for row in row_offset..row_offset.saturating_add(rows.min(budget_rows)) {
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
        TransformError::InvalidBase64 {
            position: Some(position),
        } => {
            format!("invalid Base64 at byte {position}")
        }
        TransformError::InvalidBase64 { position: None } => "invalid Base64 padding".to_string(),
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
            };
            format!("{reason} at line {line}, column {column}")
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
