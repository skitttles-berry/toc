use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use tui_textarea::{TextArea, WrapMode};

use crate::{
    MAX_STEPS, TUI_INPUT_LIMIT, TUI_INPUT_LINE_LIMIT, TUI_UNDO_HISTORY_LIMIT,
    pipeline::TransformStep,
    transforms::{TransformDefinition, transform_by_id, transforms},
};

use super::{
    views::PreviewDocument,
    worker::{PreviewJob, PreviewResult},
};

pub(super) const DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pane {
    Input,
    Preview,
    Chain,
}

pub(super) enum PreviewState {
    Idle,
    Debouncing { deadline: Instant },
    Running,
    Ready { document: PreviewDocument },
    Error { message: String },
}

pub(super) enum Modal {
    TransformPicker { query: String, selected: usize },
    QuitConfirm,
    UnsafeCopyConfirm,
}

pub(super) enum AppEvent {
    Key(KeyEvent, Instant),
    Paste(String, Instant),
    Tick(Instant),
    PreviewFinished(PreviewResult),
    ClipboardFinished(Result<(), String>),
    Resize,
}

pub(super) enum Effect {
    Submit(PreviewJob),
    Copy(Arc<str>),
    Quit(i32),
}

pub(super) struct App {
    pub(super) textarea: TextArea<'static>,
    pub(super) focus: Pane,
    pub(super) steps: Vec<TransformStep>,
    pub(super) selected_step: usize,
    pub(super) preview: PreviewState,
    pub(super) generation: u64,
    pub(super) modal: Option<Modal>,
    pub(super) status: Option<String>,
    pub(super) preview_scroll: usize,
    pub(super) no_color: bool,
    input_limit: usize,
    input_line_limit: usize,
    dirty: bool,
}

impl App {
    pub(super) fn new(now: Instant, no_color: bool) -> Self {
        Self::new_with_input_limits(now, no_color, TUI_INPUT_LIMIT, TUI_INPUT_LINE_LIMIT)
    }

    #[cfg(test)]
    fn new_with_input_limit(now: Instant, no_color: bool, input_limit: usize) -> Self {
        Self::new_with_input_limits(now, no_color, input_limit, TUI_INPUT_LINE_LIMIT)
    }

    fn new_with_input_limits(
        _: Instant,
        no_color: bool,
        input_limit: usize,
        input_line_limit: usize,
    ) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_max_histories(TUI_UNDO_HISTORY_LIMIT);
        textarea.set_wrap_mode(WrapMode::WordOrGlyph);
        textarea.set_cursor_line_style(Style::default());
        if no_color {
            textarea.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
            textarea.set_selection_style(Style::default().add_modifier(Modifier::REVERSED));
        } else {
            textarea.set_cursor_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
            textarea.set_selection_style(Style::default().fg(Color::Black).bg(Color::Yellow));
        }
        Self {
            textarea,
            focus: Pane::Input,
            steps: Vec::new(),
            selected_step: 0,
            preview: PreviewState::Idle,
            generation: 0,
            modal: None,
            status: None,
            preview_scroll: 0,
            no_color,
            input_limit,
            input_line_limit,
            dirty: true,
        }
    }

    pub(super) fn input_text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    fn input_len(&self) -> usize {
        self.textarea.lines().iter().map(String::len).sum::<usize>()
            + self.textarea.lines().len().saturating_sub(1)
    }

    fn selected_input_len(&self) -> usize {
        let Some((start, end)) = self.textarea.selection_range() else {
            return 0;
        };
        let lines = self.textarea.lines();
        let byte_offset = |(row, column): (usize, usize)| {
            let preceding = lines.iter().take(row).fold(0usize, |total, line| {
                total.saturating_add(line.len()).saturating_add(1)
            });
            let within_line = lines
                .get(row)
                .map(|line| {
                    line.char_indices()
                        .nth(column)
                        .map_or(line.len(), |(index, _)| index)
                })
                .unwrap_or(0);
            preceding.saturating_add(within_line)
        };
        byte_offset(end).saturating_sub(byte_offset(start))
    }

    fn selected_line_count(&self) -> usize {
        self.textarea
            .selection_range()
            .map_or(0, |(start, end)| end.0.saturating_sub(start.0))
    }

    fn can_insert(&self, bytes: usize, lines: usize) -> bool {
        let retained_bytes = self.input_len().saturating_sub(self.selected_input_len());
        let retained_lines = self
            .textarea
            .lines()
            .len()
            .saturating_sub(self.selected_line_count());
        retained_bytes
            .checked_add(bytes)
            .is_some_and(|length| length <= self.input_limit)
            && retained_lines
                .checked_add(lines)
                .is_some_and(|count| count <= self.input_line_limit)
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub(super) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn set_status(&mut self, status: Option<String>) {
        if self.status != status {
            self.status = status;
            self.mark_dirty();
        }
    }

    fn reject_input(&mut self) {
        self.set_status(Some("Input limit reached".to_string()));
    }

    fn changed(&mut self, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.preview = PreviewState::Debouncing {
            deadline: now + DEBOUNCE,
        };
        self.preview_scroll = 0;
        self.mark_dirty();
    }

    pub(super) fn insert_paste(&mut self, text: &str, now: Instant) -> bool {
        let retained = self.input_len().saturating_sub(self.selected_input_len());
        let remaining = self.input_limit.saturating_sub(retained);
        if text.len() > remaining.saturating_mul(2) {
            self.reject_input();
            return false;
        }
        let (normalized, replaced) = normalize_paste(text);
        if !self.can_insert(
            normalized.len(),
            normalized.bytes().filter(|byte| *byte == b'\n').count(),
        ) {
            self.reject_input();
            return false;
        }
        let modified = self.textarea.insert_str(normalized);
        if modified {
            self.set_status(
                (replaced > 0).then(|| format!("{replaced} control characters replaced")),
            );
            self.changed(now);
        }
        modified
    }

    pub(super) fn add_transform(&mut self, id: &str, now: Instant) -> bool {
        if self.steps.len() == MAX_STEPS {
            self.set_status(Some("Chain limit reached".to_string()));
            return false;
        }
        let Some(definition) = transform_by_id(id) else {
            return false;
        };
        self.steps.push(TransformStep {
            definition,
            enabled: true,
        });
        self.selected_step = self.steps.len() - 1;
        self.changed(now);
        true
    }

    fn toggle_selected(&mut self, now: Instant) {
        let Some(step) = self.steps.get_mut(self.selected_step) else {
            return;
        };
        step.enabled = !step.enabled;
        self.changed(now);
    }

    fn move_selected(&mut self, direction: i8, now: Instant) {
        let next = match direction {
            -1 if self.selected_step > 0 => self.selected_step - 1,
            1 if self.selected_step + 1 < self.steps.len() => self.selected_step + 1,
            _ => return,
        };
        self.steps.swap(self.selected_step, next);
        self.selected_step = next;
        self.changed(now);
    }

    fn delete_selected(&mut self, now: Instant) {
        if self.steps.get(self.selected_step).is_none() {
            return;
        }
        self.steps.remove(self.selected_step);
        self.selected_step = self.selected_step.min(self.steps.len().saturating_sub(1));
        self.changed(now);
    }

    #[cfg(test)]
    fn can_copy(&self) -> bool {
        matches!(self.preview, PreviewState::Ready { .. })
    }

    pub(super) fn open_picker(&mut self) {
        self.modal = Some(Modal::TransformPicker {
            query: String::new(),
            selected: 0,
        });
        self.mark_dirty();
    }

    fn picker_insert(&mut self, character: char) {
        if let Some(Modal::TransformPicker { query, selected }) = &mut self.modal {
            query.push(character);
            *selected = 0;
            self.mark_dirty();
        }
    }

    pub(super) fn filtered_transforms(&self) -> Vec<&'static TransformDefinition> {
        let query = match &self.modal {
            Some(Modal::TransformPicker { query, .. }) => query.to_ascii_lowercase(),
            _ => String::new(),
        };
        transforms()
            .iter()
            .filter(|transform| {
                query.is_empty()
                    || transform.id.contains(&query)
                    || transform.display_name.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }

    fn confirm_picker(&mut self, now: Instant) {
        let selected = match self.modal {
            Some(Modal::TransformPicker { selected, .. }) => selected,
            _ => return,
        };
        let id = self
            .filtered_transforms()
            .get(selected)
            .map(|transform| transform.id);
        self.modal = None;
        self.mark_dirty();
        if let Some(id) = id {
            self.add_transform(id, now);
        }
    }

    fn request_copy(&mut self) -> Vec<Effect> {
        let Some((raw, unsafe_raw)) = (match &self.preview {
            PreviewState::Ready { document } => Some((
                Arc::clone(&document.raw),
                crate::error::contains_dangerous_control(&document.raw),
            )),
            _ => None,
        }) else {
            return Vec::new();
        };
        if unsafe_raw {
            self.modal = Some(Modal::UnsafeCopyConfirm);
            self.mark_dirty();
            Vec::new()
        } else {
            vec![Effect::Copy(raw)]
        }
    }

    fn confirm_unsafe_copy(&mut self) -> Vec<Effect> {
        if !matches!(self.modal, Some(Modal::UnsafeCopyConfirm)) {
            return Vec::new();
        }
        self.modal = None;
        self.mark_dirty();
        match &self.preview {
            PreviewState::Ready { document } => vec![Effect::Copy(Arc::clone(&document.raw))],
            _ => Vec::new(),
        }
    }

    fn request_quit(&mut self) -> Vec<Effect> {
        if self.input_len() == 0 {
            vec![Effect::Quit(0)]
        } else {
            self.modal = Some(Modal::QuitConfirm);
            self.mark_dirty();
            Vec::new()
        }
    }

    pub(super) fn force_interrupt(&mut self) -> Vec<Effect> {
        self.modal = None;
        self.mark_dirty();
        vec![Effect::Quit(130)]
    }

    fn tick(&mut self, now: Instant) -> Vec<Effect> {
        let PreviewState::Debouncing { deadline } = &self.preview else {
            return Vec::new();
        };
        if now < *deadline {
            return Vec::new();
        }
        let generation = self.generation;
        self.preview = PreviewState::Running;
        self.mark_dirty();
        vec![Effect::Submit(PreviewJob {
            generation,
            input: self.input_text().into_bytes(),
            steps: self.steps.clone(),
        })]
    }

    fn finish_preview(&mut self, result: PreviewResult) {
        if result.generation != self.generation {
            return;
        }
        self.preview = match result.result {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => PreviewState::Ready {
                    document: PreviewDocument::new(text),
                },
                Err(_) => PreviewState::Error {
                    message: "Transform returned invalid UTF-8".to_string(),
                },
            },
            Err(error) => PreviewState::Error {
                message: crate::error::render_pipeline_error(&error),
            },
        };
        self.mark_dirty();
    }

    fn rotate_focus(&mut self, backwards: bool) {
        self.focus = match (self.focus, backwards) {
            (Pane::Input, false) | (Pane::Chain, true) => Pane::Preview,
            (Pane::Preview, false) | (Pane::Input, true) => Pane::Chain,
            (Pane::Chain, false) | (Pane::Preview, true) => Pane::Input,
        };
        self.mark_dirty();
    }

    fn handle_modal_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        match self.modal {
            Some(Modal::TransformPicker { .. }) => {
                let filtered_len = self.filtered_transforms().len();
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => {
                        self.modal = None;
                        self.mark_dirty();
                    }
                    (KeyCode::Enter, _) => self.confirm_picker(now),
                    (KeyCode::Backspace, _) => {
                        let mut changed = false;
                        if let Some(Modal::TransformPicker { query, selected }) = &mut self.modal {
                            changed = query.pop().is_some() || *selected != 0;
                            *selected = 0;
                        }
                        if changed {
                            self.mark_dirty();
                        }
                    }
                    (KeyCode::Up, _) => {
                        let mut changed = false;
                        if let Some(Modal::TransformPicker { selected, .. }) = &mut self.modal {
                            let next = selected.saturating_sub(1);
                            changed = *selected != next;
                            *selected = next;
                        }
                        if changed {
                            self.mark_dirty();
                        }
                    }
                    (KeyCode::Down, _) => {
                        let mut changed = false;
                        if let Some(Modal::TransformPicker { selected, .. }) = &mut self.modal {
                            let next = selected
                                .saturating_add(1)
                                .min(filtered_len.saturating_sub(1));
                            changed = *selected != next;
                            *selected = next;
                        }
                        if changed {
                            self.mark_dirty();
                        }
                    }
                    (KeyCode::Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                        self.picker_insert(character);
                    }
                    _ => {}
                }
                Vec::new()
            }
            Some(Modal::UnsafeCopyConfirm) => match key.code {
                KeyCode::Enter | KeyCode::Char('y' | 'Y') => self.confirm_unsafe_copy(),
                KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                    self.modal = None;
                    self.mark_dirty();
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Some(Modal::QuitConfirm) => match key.code {
                KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                    self.modal = None;
                    self.mark_dirty();
                    vec![Effect::Quit(0)]
                }
                KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                    self.modal = None;
                    self.mark_dirty();
                    Vec::new()
                }
                _ => Vec::new(),
            },
            None => Vec::new(),
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent, now: Instant) {
        if let KeyCode::Char(character) = key.code
            && crate::error::is_dangerous_control(character)
        {
            self.insert_paste(&character.to_string(), now);
            return;
        }
        let modifiers = key.modifiers;
        let input_growth = match key.code {
            KeyCode::Enter => Some((1, 1)),
            KeyCode::Char('\n' | '\r')
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                Some((1, 1))
            }
            KeyCode::Char('m')
                if modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                Some((1, 1))
            }
            KeyCode::Char(character)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                Some((character.len_utf8(), 0))
            }
            _ => None,
        };
        let yank = matches!(key.code, KeyCode::Char('y'))
            && modifiers.contains(KeyModifiers::CONTROL)
            && !modifiers.contains(KeyModifiers::ALT);
        if yank {
            let text = self.textarea.yank_text();
            if !self.can_insert(
                text.len(),
                text.bytes().filter(|byte| *byte == b'\n').count(),
            ) {
                self.reject_input();
                return;
            }
        }
        if let Some((bytes, lines)) = input_growth
            && !self.can_insert(bytes, lines)
        {
            self.reject_input();
            return;
        }
        let before = (self.textarea.cursor(), self.textarea.selection_range());
        if self.textarea.input(key) {
            self.changed(now);
        } else if before != (self.textarea.cursor(), self.textarea.selection_range()) {
            self.mark_dirty();
        }
    }

    fn handle_preview_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let line_count = match &self.preview {
            PreviewState::Ready { document } => document.line_starts.len(),
            _ => 0,
        };
        let last_line = line_count.saturating_sub(1);
        let before = self.preview_scroll;
        match key.code {
            KeyCode::Enter => return self.request_copy(),
            KeyCode::Up => self.preview_scroll = self.preview_scroll.saturating_sub(1),
            KeyCode::Down => {
                self.preview_scroll = self.preview_scroll.saturating_add(1).min(last_line);
            }
            KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(10),
            KeyCode::PageDown => {
                self.preview_scroll = self.preview_scroll.saturating_add(10).min(last_line);
            }
            _ => {}
        }
        if self.preview_scroll != before {
            self.mark_dirty();
        }
        Vec::new()
    }

    fn handle_chain_key(&mut self, key: KeyEvent, now: Instant) {
        match (key.code, key.modifiers) {
            (KeyCode::Up, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
                self.move_selected(-1, now);
            }
            (KeyCode::Down, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
                self.move_selected(1, now);
            }
            (KeyCode::Up, _) => {
                let next = self.selected_step.saturating_sub(1);
                if self.selected_step != next {
                    self.selected_step = next;
                    self.mark_dirty();
                }
            }
            (KeyCode::Down, _) => {
                let next = self
                    .selected_step
                    .saturating_add(1)
                    .min(self.steps.len().saturating_sub(1));
                if self.selected_step != next {
                    self.selected_step = next;
                    self.mark_dirty();
                }
            }
            (KeyCode::Char(' '), _) => self.toggle_selected(now),
            (KeyCode::Delete, _) => self.delete_selected(now),
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        if self.modal.is_some() {
            return self.handle_modal_key(key, now);
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('p' | 'P'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_picker();
                return Vec::new();
            }
            (KeyCode::Char('q' | 'Q'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                return self.request_quit();
            }
            (KeyCode::Tab, modifiers) if !modifiers.contains(KeyModifiers::SHIFT) => {
                self.rotate_focus(false);
                return Vec::new();
            }
            (KeyCode::BackTab, _) | (KeyCode::Tab, KeyModifiers::SHIFT) => {
                self.rotate_focus(true);
                return Vec::new();
            }
            _ => {}
        }
        match self.focus {
            Pane::Input => {
                self.handle_input_key(key, now);
                Vec::new()
            }
            Pane::Preview => self.handle_preview_key(key),
            Pane::Chain => {
                self.handle_chain_key(key, now);
                Vec::new()
            }
        }
    }

    pub(super) fn handle_event(&mut self, event: AppEvent) -> Vec<Effect> {
        match event {
            AppEvent::Tick(now) => self.tick(now),
            AppEvent::PreviewFinished(result) => {
                self.finish_preview(result);
                Vec::new()
            }
            AppEvent::Paste(text, now) if self.modal.is_none() && self.focus == Pane::Input => {
                self.insert_paste(&text, now);
                Vec::new()
            }
            AppEvent::Paste(_, _) => Vec::new(),
            AppEvent::ClipboardFinished(result) => {
                self.set_status(Some(match result {
                    Ok(()) => "Copied".to_string(),
                    Err(message) => crate::error::escape_external(&message, 512),
                }));
                Vec::new()
            }
            AppEvent::Key(key, now) => self.handle_key(key, now),
            AppEvent::Resize => {
                self.mark_dirty();
                Vec::new()
            }
        }
    }
}

fn normalize_paste(input: &str) -> (String, usize) {
    use std::fmt::Write as _;

    let normalized_lines = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::with_capacity(normalized_lines.len());
    let mut replaced = 0;
    for character in normalized_lines.chars() {
        if crate::error::is_dangerous_control(character) {
            write!(&mut output, "\\x{:02x}", character as u32)
                .expect("writing to String cannot fail");
            replaced += 1;
        } else {
            output.push(character);
        }
    }
    (output, replaced)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TUI_OUTPUT_LIMIT, error::PipelineError, pipeline::execute};
    use crossterm::event::{KeyCode, KeyModifiers};

    fn now() -> Instant {
        Instant::now()
    }

    fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers, now: Instant) -> Vec<Effect> {
        app.handle_event(AppEvent::Key(KeyEvent::new(code, modifiers), now))
    }

    #[test]
    fn input_uses_word_or_glyph_wrapping() {
        let app = App::new(now(), true);
        assert_eq!(app.textarea.wrap_mode(), WrapMode::WordOrGlyph);
    }
    #[test]
    fn picker_filters_by_id_or_name_and_adds_repeated_transform() {
        let start = now();
        let mut app = App::new(start, true);
        app.open_picker();
        for character in "JSON".chars() {
            app.picker_insert(character);
        }
        assert_eq!(
            app.filtered_transforms()
                .iter()
                .map(|transform| transform.id)
                .collect::<Vec<_>>(),
            ["format-json", "minify-json"]
        );
        app.confirm_picker(start);
        app.open_picker();
        for character in "json".chars() {
            app.picker_insert(character);
        }
        app.confirm_picker(start);
        assert_eq!(app.steps.len(), 2);
        assert_eq!(app.steps[0].definition.id, app.steps[1].definition.id);
    }
    #[test]
    fn picker_exposes_both_hex_transforms_from_the_shared_registry() {
        let start = now();
        let mut app = App::new(start, true);
        app.open_picker();
        for character in "hex".chars() {
            app.picker_insert(character);
        }
        let ids: Vec<_> = app
            .filtered_transforms()
            .iter()
            .map(|transform| transform.id)
            .collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(
            ids.iter()
                .copied()
                .collect::<std::collections::HashSet<_>>(),
            std::collections::HashSet::from(["hex-encode", "hex-decode"])
        );

        app.open_picker();
        for character in "hex-encode".chars() {
            app.picker_insert(character);
        }
        app.confirm_picker(start);
        assert_eq!(app.steps[0].definition.id, "hex-encode");
    }
    #[test]
    fn global_focus_and_picker_keys_do_not_edit_input() {
        let start = now();
        let mut app = App::new(start, true);

        key(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL, start);
        assert!(matches!(app.modal, Some(Modal::TransformPicker { .. })));
        assert_eq!(app.input_text(), "");

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Tab, KeyModifiers::NONE, start);
        assert_eq!(app.focus, Pane::Preview);
        key(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT, start);
        assert_eq!(app.focus, Pane::Input);
        assert_eq!(app.input_text(), "");
    }
    #[test]
    fn modal_keys_take_priority_over_focus_changes() {
        let start = now();
        let mut app = App::new(start, true);
        app.open_picker();

        key(&mut app, KeyCode::Tab, KeyModifiers::NONE, start);
        assert_eq!(app.focus, Pane::Input);
        assert!(matches!(app.modal, Some(Modal::TransformPicker { .. })));

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        assert!(app.modal.is_none());
    }
    #[test]
    fn picker_key_selection_clamps_and_backspace_edits_query() {
        let start = now();
        let mut app = App::new(start, true);
        app.open_picker();
        key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);
        assert!(app.filtered_transforms().is_empty());
        assert!(matches!(
            app.modal,
            Some(Modal::TransformPicker { selected: 0, .. })
        ));

        key(&mut app, KeyCode::Backspace, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Up, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);
        assert_eq!(app.steps.len(), 1);
        assert_eq!(app.steps[0].definition.id, "base64-encode");
    }
    #[test]
    fn unsafe_preview_requires_confirmation_before_copy_effect() {
        let mut app = App::new(now(), true);
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("x\u{1b}[2J".to_string()),
        };
        assert!(app.request_copy().is_empty());
        assert!(matches!(app.modal, Some(Modal::UnsafeCopyConfirm)));
        assert!(matches!(
            app.confirm_unsafe_copy().as_slice(),
            [Effect::Copy(_)]
        ));
    }
    #[test]
    fn only_ready_preview_can_be_copied() {
        let start = now();
        let mut app = App::new(start, true);
        assert!(app.request_copy().is_empty());
        app.preview = PreviewState::Debouncing {
            deadline: start + DEBOUNCE,
        };
        assert!(app.request_copy().is_empty());
        app.preview = PreviewState::Running;
        assert!(app.request_copy().is_empty());
        app.preview = PreviewState::Error {
            message: "failed".to_string(),
        };
        assert!(app.request_copy().is_empty());
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("safe".to_string()),
        };
        assert!(matches!(app.request_copy().as_slice(), [Effect::Copy(_)]));
    }
    #[test]
    fn confirmation_modal_keys_accept_or_cancel_explicit_actions() {
        let start = now();
        let mut app = App::new(start, true);
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("\u{1b}".to_string()),
        };

        app.request_copy();
        assert!(key(&mut app, KeyCode::Char('n'), KeyModifiers::NONE, start).is_empty());
        assert!(app.modal.is_none());

        app.request_copy();
        assert!(matches!(
            key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start).as_slice(),
            [Effect::Copy(_)]
        ));

        app.insert_paste("x", start);
        app.request_quit();
        assert!(matches!(
            key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE, start).as_slice(),
            [Effect::Quit(0)]
        ));
    }
    #[test]
    fn quit_requires_confirmation_only_when_input_is_not_empty() {
        let start = now();
        let mut empty = App::new(start, true);
        assert!(matches!(empty.request_quit().as_slice(), [Effect::Quit(0)]));

        let mut edited = App::new(start, true);
        edited.insert_paste("x", start);
        assert!(edited.request_quit().is_empty());
        assert!(matches!(edited.modal, Some(Modal::QuitConfirm)));
    }
    #[test]
    fn ctrl_q_uses_the_same_quit_confirmation_policy() {
        let start = now();
        let mut empty = App::new(start, true);
        assert!(matches!(
            key(&mut empty, KeyCode::Char('q'), KeyModifiers::CONTROL, start).as_slice(),
            [Effect::Quit(0)]
        ));

        let mut edited = App::new(start, true);
        edited.insert_paste("x", start);
        assert!(
            key(
                &mut edited,
                KeyCode::Char('q'),
                KeyModifiers::CONTROL,
                start
            )
            .is_empty()
        );
        assert!(matches!(edited.modal, Some(Modal::QuitConfirm)));
    }
    #[test]
    fn ctrl_c_returns_130_without_quit_confirmation() {
        let mut app = App::new(Instant::now(), true);
        app.insert_paste("unsaved", Instant::now());
        assert!(app.request_quit().is_empty());
        assert!(matches!(app.modal, Some(Modal::QuitConfirm)));
        let effects = app.force_interrupt();
        assert!(matches!(effects.as_slice(), [Effect::Quit(130)]));
        assert!(!matches!(app.modal, Some(Modal::QuitConfirm)));
    }
    #[test]
    fn preview_scroll_is_bounded_and_does_not_mutate_input_or_generation() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("source", start);
        app.generation = 7;
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("zero\none\ntwo".to_string()),
        };
        app.focus = Pane::Preview;

        key(&mut app, KeyCode::Up, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::PageUp, KeyModifiers::NONE, start);
        assert_eq!(app.preview_scroll, 0);
        key(&mut app, KeyCode::PageDown, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);
        assert_eq!(app.preview_scroll, 2);
        assert_eq!(app.input_text(), "source");
        assert_eq!(app.generation, 7);
    }
    #[test]
    fn preview_enter_requests_copy_without_editing_input() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("source", start);
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("result".to_string()),
        };
        app.focus = Pane::Preview;

        let effects = key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);
        assert!(matches!(effects.as_slice(), [Effect::Copy(raw)] if &**raw == "result"));
        assert_eq!(app.input_text(), "source");
    }
    #[test]
    fn chain_keys_select_toggle_reorder_and_delete_steps() {
        let start = now();
        let mut app = App::new(start, true);
        app.add_transform("base64-encode", start);
        app.add_transform("url-encode", start);
        app.add_transform("format-json", start);
        app.focus = Pane::Chain;
        app.selected_step = 0;

        key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);
        assert_eq!(app.selected_step, 1);
        key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE, start);
        assert!(!app.steps[1].enabled);
        key(&mut app, KeyCode::Up, KeyModifiers::SHIFT, start);
        assert_eq!(app.selected_step, 0);
        assert_eq!(app.steps[0].definition.id, "url-encode");
        key(&mut app, KeyCode::Delete, KeyModifiers::NONE, start);
        assert_eq!(app.steps.len(), 2);
        assert_eq!(app.steps[0].definition.id, "base64-encode");
    }
    #[test]
    fn dangerous_key_input_is_escaped_before_reaching_the_editor() {
        let start = now();
        let mut app = App::new(start, true);
        key(&mut app, KeyCode::Char('\u{85}'), KeyModifiers::NONE, start);
        assert_eq!(app.input_text(), "\\x85");
    }
    #[test]
    fn clipboard_failure_preserves_ready_preview() {
        let mut app = App::new(Instant::now(), true);
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("result".to_string()),
        };
        app.handle_event(AppEvent::ClipboardFinished(Err(
            "Clipboard unavailable".to_string()
        )));
        let PreviewState::Ready { document, .. } = &app.preview else {
            panic!("expected ready preview");
        };
        assert_eq!(&*document.raw, "result");
        assert_eq!(app.status.as_deref(), Some("Clipboard unavailable"));
    }
    #[test]
    fn starts_as_an_empty_non_destructive_workbench() {
        let app = App::new(now(), true);
        assert_eq!(app.input_text(), "");
        assert!(app.steps.is_empty());
        assert!(matches!(app.preview, PreviewState::Idle));
    }
    #[test]
    fn change_debounces_for_200_milliseconds() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("x", start);

        assert!(
            app.handle_event(AppEvent::Tick(start + Duration::from_millis(199)))
                .is_empty()
        );
        let effects = app.handle_event(AppEvent::Tick(start + Duration::from_millis(200)));
        assert!(matches!(effects.as_slice(), [Effect::Submit(_)]));
        assert!(matches!(app.preview, PreviewState::Running));
    }
    #[test]
    fn empty_chain_previews_input_without_overwriting_it() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("plain", start);
        let effects = app.handle_event(AppEvent::Tick(start + Duration::from_millis(200)));
        let [Effect::Submit(job)] = effects.as_slice() else {
            panic!("expected preview job");
        };
        let result = execute(job.input.clone(), &job.steps, TUI_OUTPUT_LIMIT);
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            generation: job.generation,
            result,
        }));
        assert_eq!(app.input_text(), "plain");
        let PreviewState::Ready { document, .. } = &app.preview else {
            panic!("expected ready preview");
        };
        assert_eq!(&*document.raw, "plain");
    }
    #[test]
    fn a_change_immediately_hides_the_previous_copyable_result() {
        let start = now();
        let mut app = App::new(start, true);
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("old".to_string()),
        };
        app.insert_paste("new", start);
        assert!(matches!(app.preview, PreviewState::Debouncing { .. }));
        assert!(!app.can_copy());
    }
    #[test]
    fn stale_worker_result_never_replaces_current_preview() {
        let start = now();
        let mut app = App::new(start, true);
        app.generation = 2;
        app.preview = PreviewState::Running;
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            generation: 1,
            result: Ok(b"old".to_vec()),
        }));
        assert!(matches!(app.preview, PreviewState::Running));
    }
    #[test]
    fn an_error_hides_previous_result_and_disables_copy() {
        let start = now();
        let mut app = App::new(start, true);
        app.generation = 2;
        app.preview = PreviewState::Ready {
            document: PreviewDocument::new("old".to_string()),
        };
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            generation: 2,
            result: Err(PipelineError::TooManySteps { max: 32 }),
        }));
        assert!(matches!(app.preview, PreviewState::Error { .. }));
        assert!(!app.can_copy());
    }
    #[test]
    fn chain_supports_repeat_toggle_move_delete_and_limit() {
        let start = now();
        let mut app = App::new(start, true);
        for _ in 0..32 {
            assert!(app.add_transform("url-decode", start));
        }
        assert!(!app.add_transform("url-decode", start));
        app.selected_step = 1;
        app.toggle_selected(start);
        assert!(!app.steps[1].enabled);
        app.move_selected(-1, start);
        assert_eq!(app.selected_step, 0);
        app.delete_selected(start);
        assert_eq!(app.steps.len(), 31);
    }
    #[test]
    fn normalizes_pasted_controls_and_line_endings() {
        let (normalized, replaced) = normalize_paste("a\r\nb\rc\u{1b}\u{0}");
        assert_eq!(normalized, "a\nb\nc\\x1b\\x00");
        assert_eq!(replaced, 2);
    }
    #[test]
    fn input_limit_rejects_the_mutation_and_keeps_old_text() {
        let start = now();
        let mut app = App::new_with_input_limit(start, true, 4);
        assert!(app.insert_paste("1234", start));
        assert!(!app.insert_paste("5", start));
        assert_eq!(app.input_text(), "1234");
    }
    #[test]
    fn tui_input_resources_use_fixed_limits() {
        let app = App::new(now(), true);

        assert_eq!(TUI_INPUT_LIMIT, 1024 * 1024);
        assert_eq!(TUI_INPUT_LINE_LIMIT, 65_536);
        assert_eq!(TUI_UNDO_HISTORY_LIMIT, 8);
        assert_eq!(app.textarea.max_histories(), TUI_UNDO_HISTORY_LIMIT);
    }
    #[test]
    fn paste_past_line_limit_preserves_input_and_preview_state() {
        let start = now();
        let mut app = App::new_with_input_limits(start, true, 8, 2);
        assert!(app.insert_paste("a\nb", start));
        let before = (app.input_text(), app.generation);

        assert!(!app.insert_paste("\nc", start));
        assert_eq!((app.input_text(), app.generation), before);
        assert!(matches!(app.preview, PreviewState::Debouncing { .. }));
        assert_eq!(app.status.as_deref(), Some("Input limit reached"));
    }
    #[test]
    fn enter_past_line_limit_preserves_input_and_preview_state() {
        let start = now();
        let mut app = App::new_with_input_limits(start, true, 8, 1);
        assert!(app.insert_paste("a", start));
        let before = (app.input_text(), app.generation);

        app.handle_event(AppEvent::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            start,
        ));

        assert_eq!((app.input_text(), app.generation), before);
        assert!(matches!(app.preview, PreviewState::Debouncing { .. }));
        assert_eq!(app.status.as_deref(), Some("Input limit reached"));
    }
    #[test]
    fn yank_past_byte_limit_preserves_input_state() {
        let start = now();
        let mut app = App::new_with_input_limit(start, true, 4);
        assert!(app.insert_paste("1234", start));
        app.textarea.select_all();
        key(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL, start);
        key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE, start);
        let before = (app.input_text(), app.generation);

        key(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL, start);

        assert_eq!((app.input_text(), app.generation), before);
        assert_eq!(app.status.as_deref(), Some("Input limit reached"));
    }
    #[test]
    fn yank_past_line_limit_preserves_input_state() {
        let start = now();
        let mut app = App::new_with_input_limits(start, true, 8, 2);
        assert!(app.insert_paste("a\nb", start));
        app.textarea.select_all();
        key(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL, start);
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);
        let before = (app.input_text(), app.generation);

        key(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL, start);

        assert_eq!((app.input_text(), app.generation), before);
        assert_eq!(app.status.as_deref(), Some("Input limit reached"));
    }
    #[test]
    fn rejected_key_replacement_restores_text_cursor_and_selection() {
        let start = now();
        let mut app = App::new_with_input_limit(start, true, 4);
        assert!(app.insert_paste("xxxx", start));
        app.handle_event(AppEvent::Key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT),
            start,
        ));
        let before = (
            app.input_text(),
            app.textarea.cursor(),
            app.textarea.selection_range(),
        );

        app.handle_event(AppEvent::Key(
            KeyEvent::new(KeyCode::Char('é'), KeyModifiers::NONE),
            start,
        ));

        assert_eq!(
            (
                app.input_text(),
                app.textarea.cursor(),
                app.textarea.selection_range(),
            ),
            before
        );
        assert_eq!(app.status.as_deref(), Some("Input limit reached"));
    }
    #[test]
    fn paste_can_replace_selected_multibyte_text_with_fewer_bytes() {
        let start = now();
        let mut app = App::new_with_input_limit(start, true, 4);
        assert!(app.insert_paste("abé", start));
        app.handle_event(AppEvent::Key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT),
            start,
        ));

        assert!(app.insert_paste("x", start));
        assert_eq!(app.input_text(), "abx");
    }
    #[test]
    fn paste_capacity_includes_newlines_inside_multiline_selection() {
        let start = now();
        let mut app = App::new_with_input_limit(start, true, 3);
        assert!(app.insert_paste("a\nb", start));
        app.handle_event(AppEvent::Key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT),
            start,
        ));
        assert_eq!(app.textarea.selection_range(), Some(((0, 1), (1, 1))));

        assert!(app.insert_paste("é", start));
        assert_eq!(app.input_text(), "aé");
    }
}
