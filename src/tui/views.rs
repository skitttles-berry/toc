use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

const VISIBLE_TEXT_BYTE_BUDGET: usize = 4 * 1024;

pub(super) struct PreviewDocument {
    pub(super) raw: Arc<str>,
    pub(super) line_starts: Vec<usize>,
}

impl PreviewDocument {
    pub(super) fn new(raw: String) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(raw.match_indices('\n').map(|(index, _)| index + 1));
        Self {
            raw: Arc::from(raw),
            line_starts,
        }
    }

    fn line(&self, index: usize) -> Option<&str> {
        let start = *self.line_starts.get(index)?;
        let end = self
            .line_starts
            .get(index + 1)
            .map_or(self.raw.len(), |next| next.saturating_sub(1));
        self.raw.get(start..end)
    }
}

pub(super) fn visible_safe_text(
    document: &PreviewDocument,
    first_line: usize,
    rows: usize,
    columns: usize,
) -> String {
    let mut output = String::new();
    let mut remaining = VISIBLE_TEXT_BYTE_BUDGET;
    for row in 0..rows {
        let Some(line) = document.line(first_line + row) else {
            break;
        };
        if row > 0 {
            if remaining == 0 {
                break;
            }
            output.push('\n');
            remaining -= 1;
        }
        let mut prefix_end = line.len().min(remaining);
        while !line.is_char_boundary(prefix_end) {
            prefix_end -= 1;
        }
        let truncated = prefix_end < line.len();
        let prefix = &line[..prefix_end];
        let mut used = 0;
        for (offset, grapheme) in prefix.grapheme_indices(true) {
            if truncated && offset + grapheme.len() == prefix.len() {
                // ponytail: bounded prefixes may omit their last grapheme; use a cached grapheme index for complete display.
                remaining = 0;
                break;
            }
            let dangerous = grapheme.chars().any(crate::error::is_dangerous_control);
            let escaped = dangerous
                .then(|| crate::error::escape_controls(grapheme, grapheme.chars().count()));
            let rendered = escaped.as_deref().unwrap_or(grapheme);
            let cost = grapheme.len().max(rendered.len());
            if cost > remaining {
                remaining = 0;
                break;
            }
            remaining -= cost;
            let width = rendered.width();
            if used + width > columns {
                break;
            }
            output.push_str(rendered);
            used += width;
        }
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::TUI_INPUT_LIMIT;

    #[test]
    fn preview_escapes_only_visible_dangerous_controls() {
        let document = PreviewDocument::new("safe\u{1b}[2J\nnext".to_string());
        assert_eq!(visible_safe_text(&document, 0, 1, 12), "safe\\x1b[2J");
        assert_eq!(visible_safe_text(&document, 1, 1, 12), "next");
        assert_eq!(&*document.raw, "safe\u{1b}[2J\nnext");
    }
    #[test]
    fn preview_clips_wide_and_combining_text_by_terminal_cell_width() {
        let document = PreviewDocument::new("界a\ne\u{301}x".to_string());

        assert_eq!(visible_safe_text(&document, 0, 1, 1), "");
        assert_eq!(visible_safe_text(&document, 0, 1, 2), "界");
        assert_eq!(visible_safe_text(&document, 1, 1, 1), "e\u{301}");
    }
    #[test]
    fn preview_preserves_emoji_zwj_grapheme_when_it_fits() {
        let document = PreviewDocument::new("👩‍💻x".to_string());

        assert_eq!(visible_safe_text(&document, 0, 1, 1), "");
        assert_eq!(visible_safe_text(&document, 0, 1, 2), "👩‍💻");
    }
    #[test]
    fn preview_bounds_zero_width_grapheme_output() {
        let combining_mark = '\u{301}';
        let repeats = (TUI_INPUT_LIMIT - 1) / combining_mark.len_utf8();
        let document =
            PreviewDocument::new(format!("e{}", combining_mark.to_string().repeat(repeats)));

        let visible = visible_safe_text(&document, 0, 1, 1);

        assert!(document.raw.len() > 4_096);
        assert!(visible.len() <= 4_096);
    }
}
