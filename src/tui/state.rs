use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use tui_textarea::{TextArea, WrapMode};

use crate::{
    MAX_STEPS, TUI_INPUT_LIMIT, TUI_INPUT_LINE_LIMIT, TUI_UNDO_HISTORY_LIMIT,
    error::PipelineError,
    pipeline::{ExecutionOutcome, ExecutionTarget, StepTrace, TransformStep},
    transforms::{TransformDefinition, transform_by_id, transforms},
};

use super::{
    views::{
        Artifact, EffectiveView, ViewMode, effective_view, last_text_offset, next_text_offset,
        previous_text_offset,
    },
    worker::{PreviewJob, PreviewResult},
};

// ponytail: fixed page stride; use viewport rows if terminal-sized paging becomes necessary.
const OUTPUT_PAGE_SCROLL: usize = 10;

pub(super) fn debounce_for(input_bytes: usize) -> Duration {
    if input_bytes <= 256 * 1024 {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(200)
    }
}

fn confirmation_choice(key: &KeyEvent) -> Option<bool> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter | KeyCode::Char('y'), KeyModifiers::NONE)
        | (KeyCode::Char('Y'), KeyModifiers::SHIFT) => Some(true),
        (KeyCode::Esc | KeyCode::Char('n'), KeyModifiers::NONE)
        | (KeyCode::Char('N'), KeyModifiers::SHIFT) => Some(false),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pane {
    Input,
    Output,
    Pipeline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OutputSource {
    Final,
    Step(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OutputStatus {
    Idle,
    Debouncing { deadline: Instant },
    Running,
    Ready,
    Failed(PipelineError),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyKind {
    Text,
    Hex,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ClipboardPayload {
    pub(super) text: String,
    pub(super) kind: CopyKind,
}

fn checked_hex_len(byte_len: usize) -> Option<usize> {
    byte_len.checked_mul(2)
}

fn binary_hex(bytes: &[u8]) -> Result<String, ()> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let capacity = checked_hex_len(bytes.len()).ok_or(())?;
    let mut output = String::new();
    output.try_reserve_exact(capacity).map_err(|_| ())?;
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(output)
}

fn clipboard_payload(artifact: &Artifact) -> Result<ClipboardPayload, ()> {
    match std::str::from_utf8(artifact.bytes()) {
        Ok(raw) => {
            let mut text = String::new();
            text.try_reserve_exact(raw.len()).map_err(|_| ())?;
            text.push_str(raw);
            Ok(ClipboardPayload {
                text,
                kind: CopyKind::Text,
            })
        }
        Err(_) => Ok(ClipboardPayload {
            text: binary_hex(artifact.bytes())?,
            kind: CopyKind::Hex,
        }),
    }
}

pub(super) struct OutputState {
    pub(super) source: OutputSource,
    pub(super) view: ViewMode,
    pub(super) status: OutputStatus,
    pub(super) final_artifact: Option<Artifact>,
    pub(super) final_traces: Vec<StepTrace>,
    pub(super) active_artifact: Option<Artifact>,
    pub(super) traces: Vec<StepTrace>,
    pub(super) byte_offset: usize,
    pub(super) row_offset: usize,
}

pub(super) enum Modal {
    TransformPicker { query: String, selected: usize },
    StepInspector,
    Help,
    QuitConfirm,
    UnsafeCopyConfirm { payload: ClipboardPayload },
}

pub(super) enum AppEvent {
    Key(KeyEvent, Instant),
    Paste(String, Instant),
    Tick(Instant),
    PreviewFinished(PreviewResult),
    ClipboardFinished {
        kind: CopyKind,
        result: Result<(), String>,
    },
    Resize,
}

pub(super) enum Effect {
    Submit(PreviewJob),
    Cancel(u64),
    Copy(ClipboardPayload),
    Quit(i32),
}

pub(super) struct App {
    pub(super) textarea: TextArea<'static>,
    pub(super) focus: Pane,
    pub(super) zoom: Option<Pane>,
    pub(super) steps: Vec<TransformStep>,
    pub(super) selected_step: usize,
    pub(super) output: OutputState,
    pub(super) request_id: u64,
    pub(super) modal: Option<Modal>,
    suspended_modal: Option<Modal>,
    pub(super) status: Option<String>,
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
            zoom: None,
            steps: Vec::new(),
            selected_step: 0,
            output: OutputState {
                source: OutputSource::Final,
                view: ViewMode::Smart,
                status: OutputStatus::Idle,
                final_artifact: None,
                final_traces: Vec::new(),
                active_artifact: None,
                traces: Vec::new(),
                byte_offset: 0,
                row_offset: 0,
            },
            request_id: 0,
            modal: None,
            suspended_modal: None,
            status: None,
            no_color,
            input_limit,
            input_line_limit,
            dirty: true,
        }
    }

    fn input_text(&self) -> String {
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
        self.request_id = self
            .request_id
            .checked_add(1)
            .expect("TUI request ID exhausted");
        self.output.source = OutputSource::Final;
        self.output.status = OutputStatus::Debouncing {
            deadline: now + debounce_for(self.input_len()),
        };
        self.output.final_artifact = None;
        self.output.final_traces.clear();
        self.output.active_artifact = None;
        self.output.traces.clear();
        self.output.byte_offset = 0;
        self.output.row_offset = 0;
        self.mark_dirty();
    }

    fn insert_paste(&mut self, text: &str, now: Instant) -> bool {
        let retained = self.input_len().saturating_sub(self.selected_input_len());
        let remaining = self.input_limit.saturating_sub(retained);
        // ponytail: the 1 MiB cap and 2x precheck bound normalization output to
        // about 8 MiB; use a one-pass validator only if this ceiling measures poorly.
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

    fn add_transform(&mut self, id: &str, now: Instant) -> bool {
        if self.steps.len() == MAX_STEPS {
            self.set_status(Some("Pipeline limit reached".to_string()));
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

    pub(super) fn can_copy(&self) -> bool {
        matches!(self.output.status, OutputStatus::Ready)
            && self.output.view != ViewMode::Trace
            && self.output.active_artifact.is_some()
    }

    pub(super) fn open_picker(&mut self) {
        self.modal = Some(Modal::TransformPicker {
            query: String::new(),
            selected: 0,
        });
        self.mark_dirty();
    }

    fn open_inspector(&mut self) {
        if self.steps.get(self.selected_step).is_some() {
            self.modal = Some(Modal::StepInspector);
            self.mark_dirty();
        }
    }

    fn open_help(&mut self) {
        if matches!(self.modal, Some(Modal::Help)) {
            return;
        }
        let modal = self.modal.take();
        if matches!(modal.as_ref(), Some(Modal::UnsafeCopyConfirm { .. })) {
            self.suspended_modal = None;
        } else {
            self.suspended_modal = modal;
        }
        self.modal = Some(Modal::Help);
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
        if !self.can_copy() {
            return Vec::new();
        }
        let Some(artifact) = self.output.active_artifact.as_ref() else {
            return Vec::new();
        };
        let Ok(payload) = clipboard_payload(artifact) else {
            self.set_status(Some("Copy unavailable".to_string()));
            return Vec::new();
        };
        if payload.kind == CopyKind::Text && crate::error::contains_dangerous_control(&payload.text)
        {
            self.modal = Some(Modal::UnsafeCopyConfirm { payload });
            self.mark_dirty();
            Vec::new()
        } else {
            vec![Effect::Copy(payload)]
        }
    }

    fn confirm_unsafe_copy(&mut self) -> Vec<Effect> {
        if !matches!(self.modal, Some(Modal::UnsafeCopyConfirm { .. })) {
            return Vec::new();
        }
        let Some(Modal::UnsafeCopyConfirm { payload }) = self.modal.take() else {
            unreachable!("unsafe copy modal checked above")
        };
        self.mark_dirty();
        vec![Effect::Copy(payload)]
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
        self.suspended_modal = None;
        self.mark_dirty();
        vec![Effect::Quit(130)]
    }

    fn tick(&mut self, now: Instant) -> Vec<Effect> {
        let OutputStatus::Debouncing { deadline } = &self.output.status else {
            return Vec::new();
        };
        if now < *deadline {
            return Vec::new();
        }
        self.output.status = OutputStatus::Running;
        self.mark_dirty();
        vec![Effect::Submit(PreviewJob {
            request_id: self.request_id,
            input: self.input_text().into_bytes(),
            steps: self.steps.clone(),
            target: ExecutionTarget::Final,
        })]
    }

    fn finish_preview(&mut self, result: PreviewResult) {
        let report = result.report;
        if report.request_id != self.request_id {
            return;
        }
        self.output.source = match report.target {
            ExecutionTarget::Final => OutputSource::Final,
            ExecutionTarget::Step(index) => OutputSource::Step(index),
        };
        self.output.traces = report.traces.into_iter().take(MAX_STEPS).collect();
        self.output.byte_offset = 0;
        self.output.row_offset = 0;
        match report.outcome {
            ExecutionOutcome::Success(bytes) => {
                let artifact = Artifact::new(bytes);
                if report.target == ExecutionTarget::Final {
                    self.output.final_artifact = Some(artifact.clone());
                    self.output.final_traces = self.output.traces.clone();
                }
                self.output.active_artifact = Some(artifact);
                self.output.status = OutputStatus::Ready;
            }
            ExecutionOutcome::Failed(error) => {
                self.output.active_artifact = None;
                if report.target == ExecutionTarget::Final {
                    self.output.final_artifact = None;
                    self.output.final_traces.clear();
                }
                self.output.status = OutputStatus::Failed(error);
            }
            ExecutionOutcome::Cancelled => {
                self.output.active_artifact = None;
                if report.target == ExecutionTarget::Final {
                    self.output.final_artifact = None;
                    self.output.final_traces.clear();
                }
                self.output.status = OutputStatus::Cancelled;
            }
        }
        self.mark_dirty();
    }

    fn request_selected_step(&mut self) -> Vec<Effect> {
        if self.steps.get(self.selected_step).is_none() {
            self.set_status(Some("No pipeline step selected".to_string()));
            return Vec::new();
        }
        self.request_id = self
            .request_id
            .checked_add(1)
            .expect("TUI request ID exhausted");
        self.output.source = OutputSource::Step(self.selected_step);
        self.output.status = OutputStatus::Running;
        self.output.active_artifact = None;
        self.output.traces.clear();
        self.output.byte_offset = 0;
        self.output.row_offset = 0;
        self.mark_dirty();
        vec![Effect::Submit(PreviewJob {
            request_id: self.request_id,
            input: self.input_text().into_bytes(),
            steps: self.steps.clone(),
            target: ExecutionTarget::Step(self.selected_step),
        })]
    }

    fn restore_final(&mut self) -> Vec<Effect> {
        let Some(final_artifact) = self.output.final_artifact.clone() else {
            self.set_status(Some("Final output unavailable".to_string()));
            return Vec::new();
        };
        self.request_id = self
            .request_id
            .checked_add(1)
            .expect("TUI request ID exhausted");
        self.output.source = OutputSource::Final;
        self.output.status = OutputStatus::Ready;
        self.output.active_artifact = Some(final_artifact);
        self.output.traces.clone_from(&self.output.final_traces);
        self.output.byte_offset = 0;
        self.output.row_offset = 0;
        self.mark_dirty();
        Vec::new()
    }

    fn cancel_active(&mut self) {
        if !matches!(
            self.output.status,
            OutputStatus::Debouncing { .. } | OutputStatus::Running
        ) {
            return;
        }
        self.request_id = self
            .request_id
            .checked_add(1)
            .expect("TUI request ID exhausted");
        self.output.status = OutputStatus::Cancelled;
        self.output.active_artifact = None;
        self.output.traces.clear();
        self.mark_dirty();
    }

    fn rotate_focus(&mut self, backwards: bool) {
        self.focus = match (self.focus, backwards) {
            (Pane::Input, false) | (Pane::Pipeline, true) => Pane::Output,
            (Pane::Output, false) | (Pane::Input, true) => Pane::Pipeline,
            (Pane::Pipeline, false) | (Pane::Output, true) => Pane::Input,
        };
        if self.zoom.is_some() {
            self.zoom = Some(self.focus);
        }
        self.mark_dirty();
    }

    fn toggle_zoom(&mut self, pane: Pane) {
        self.zoom = (self.zoom != Some(pane)).then_some(pane);
        self.mark_dirty();
    }

    fn handle_modal_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        match self.modal {
            Some(Modal::TransformPicker { .. }) => {
                let filtered_len = self.filtered_transforms().len();
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, KeyModifiers::NONE) => {
                        self.modal = None;
                        self.mark_dirty();
                    }
                    (KeyCode::Enter, KeyModifiers::NONE) => self.confirm_picker(now),
                    (KeyCode::Backspace, KeyModifiers::NONE) => {
                        let mut changed = false;
                        if let Some(Modal::TransformPicker { query, selected }) = &mut self.modal {
                            changed = query.pop().is_some() || *selected != 0;
                            *selected = 0;
                        }
                        if changed {
                            self.mark_dirty();
                        }
                    }
                    (KeyCode::Up, KeyModifiers::NONE) => {
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
                    (KeyCode::Down, KeyModifiers::NONE) => {
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
            Some(Modal::UnsafeCopyConfirm { .. }) => match confirmation_choice(&key) {
                Some(true) => self.confirm_unsafe_copy(),
                Some(false) => {
                    self.modal = None;
                    self.mark_dirty();
                    Vec::new()
                }
                None => Vec::new(),
            },
            Some(Modal::QuitConfirm) => match confirmation_choice(&key) {
                Some(true) => {
                    self.modal = None;
                    self.mark_dirty();
                    vec![Effect::Quit(0)]
                }
                Some(false) => {
                    self.modal = None;
                    self.mark_dirty();
                    Vec::new()
                }
                None => Vec::new(),
            },
            Some(Modal::Help) => {
                if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
                    self.modal = self.suspended_modal.take();
                    self.mark_dirty();
                }
                Vec::new()
            }
            Some(Modal::StepInspector) => {
                if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
                    self.modal = None;
                    self.mark_dirty();
                }
                Vec::new()
            }
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

    fn cycle_view(&mut self, backwards: bool) {
        self.output.view = match (self.output.view, backwards) {
            (ViewMode::Smart, false) | (ViewMode::Hex, true) => ViewMode::Text,
            (ViewMode::Text, false) | (ViewMode::Trace, true) => ViewMode::Hex,
            (ViewMode::Hex, false) | (ViewMode::Smart, true) => ViewMode::Trace,
            (ViewMode::Trace, false) | (ViewMode::Text, true) => ViewMode::Smart,
        };
        self.output.byte_offset = 0;
        self.output.row_offset = 0;
        self.mark_dirty();
    }

    fn output_max_offset(&self) -> (bool, usize) {
        match effective_view(
            self.output.view,
            self.output.active_artifact.as_ref(),
            matches!(self.output.status, OutputStatus::Failed(_)),
        ) {
            EffectiveView::Text => (
                true,
                self.output
                    .active_artifact
                    .as_ref()
                    .map_or(0, last_text_offset),
            ),
            EffectiveView::Unavailable => (true, 0),
            EffectiveView::Hex => (
                false,
                self.output
                    .active_artifact
                    .as_ref()
                    .map_or(0, |artifact| artifact.bytes().len().saturating_sub(1) / 16),
            ),
            EffectiveView::Trace => (false, self.output.traces.len().saturating_sub(1)),
        }
    }

    fn scroll_output(&mut self, direction: i8, amount: usize) {
        let (bytes, maximum) = self.output_max_offset();
        if bytes
            && let Some(artifact) = self.output.active_artifact.clone()
            && effective_view(
                self.output.view,
                Some(&artifact),
                matches!(self.output.status, OutputStatus::Failed(_)),
            ) == EffectiveView::Text
        {
            let mut next = self.output.byte_offset;
            for _ in 0..amount {
                let moved = if direction < 0 {
                    previous_text_offset(&artifact, next)
                } else {
                    next_text_offset(&artifact, next)
                }
                .min(maximum);
                if moved == next {
                    break;
                }
                next = moved;
            }
            if self.output.byte_offset != next {
                self.output.byte_offset = next;
                self.mark_dirty();
            }
            return;
        }
        let offset = if bytes {
            &mut self.output.byte_offset
        } else {
            &mut self.output.row_offset
        };
        let next = if direction < 0 {
            offset.saturating_sub(amount)
        } else {
            offset.saturating_add(amount).min(maximum)
        };
        if *offset != next {
            *offset = next;
            self.mark_dirty();
        }
    }

    fn output_home_or_end(&mut self, end: bool) {
        let (bytes, maximum) = self.output_max_offset();
        let offset = if bytes {
            &mut self.output.byte_offset
        } else {
            &mut self.output.row_offset
        };
        let next = if end { maximum } else { 0 };
        if *offset != next {
            *offset = next;
            self.mark_dirty();
        }
    }

    fn handle_output_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match (key.code, key.modifiers) {
            (KeyCode::Enter | KeyCode::Char('y'), KeyModifiers::NONE) => self.request_copy(),
            (KeyCode::Char('p'), KeyModifiers::NONE) => self.request_selected_step(),
            (KeyCode::Char('f'), KeyModifiers::NONE) => self.restore_final(),
            (KeyCode::Char('v'), KeyModifiers::NONE) => {
                self.cycle_view(false);
                Vec::new()
            }
            (KeyCode::Char('V'), KeyModifiers::SHIFT) => {
                self.cycle_view(true);
                Vec::new()
            }
            (KeyCode::Up | KeyCode::Left, KeyModifiers::NONE) => {
                self.scroll_output(-1, 1);
                Vec::new()
            }
            (KeyCode::Down | KeyCode::Right, KeyModifiers::NONE) => {
                self.scroll_output(1, 1);
                Vec::new()
            }
            (KeyCode::PageUp, KeyModifiers::NONE) => {
                self.scroll_output(-1, OUTPUT_PAGE_SCROLL);
                Vec::new()
            }
            (KeyCode::PageDown, KeyModifiers::NONE) => {
                self.scroll_output(1, OUTPUT_PAGE_SCROLL);
                Vec::new()
            }
            (KeyCode::Home, KeyModifiers::NONE) => {
                self.output_home_or_end(false);
                Vec::new()
            }
            (KeyCode::End, KeyModifiers::NONE) => {
                self.output_home_or_end(true);
                Vec::new()
            }
            (KeyCode::Char('z'), KeyModifiers::NONE) => {
                self.toggle_zoom(Pane::Output);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn handle_pipeline_key(&mut self, key: KeyEvent, now: Instant) {
        match (key.code, key.modifiers) {
            (KeyCode::Up, modifiers) if modifiers == KeyModifiers::SHIFT => {
                self.move_selected(-1, now);
            }
            (KeyCode::Down, modifiers) if modifiers == KeyModifiers::SHIFT => {
                self.move_selected(1, now);
            }
            (KeyCode::Char('K'), KeyModifiers::SHIFT) => self.move_selected(-1, now),
            (KeyCode::Char('J'), KeyModifiers::SHIFT) => self.move_selected(1, now),
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                let next = self.selected_step.saturating_sub(1);
                if self.selected_step != next {
                    self.selected_step = next;
                    self.mark_dirty();
                }
            }
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                let next = self
                    .selected_step
                    .saturating_add(1)
                    .min(self.steps.len().saturating_sub(1));
                if self.selected_step != next {
                    self.selected_step = next;
                    self.mark_dirty();
                }
            }
            (KeyCode::Char(' '), KeyModifiers::NONE) => self.toggle_selected(now),
            (KeyCode::Delete | KeyCode::Char('d'), KeyModifiers::NONE) => {
                self.delete_selected(now);
            }
            (KeyCode::Enter, KeyModifiers::NONE) => self.open_inspector(),
            (KeyCode::Char('a'), KeyModifiers::NONE) => self.open_picker(),
            (KeyCode::Char('z'), KeyModifiers::NONE) => self.toggle_zoom(Pane::Pipeline),
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        if key.code == KeyCode::F(1) && key.modifiers == KeyModifiers::NONE {
            self.open_help();
            return Vec::new();
        }
        if self.modal.is_some() {
            return self.handle_modal_key(key, now);
        }
        if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
            if self.zoom.take().is_some() {
                self.mark_dirty();
            } else {
                self.cancel_active();
            }
            return Vec::new();
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.open_picker();
                return Vec::new();
            }
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                return self.request_quit();
            }
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.rotate_focus(false);
                return Vec::new();
            }
            (KeyCode::BackTab | KeyCode::Tab, KeyModifiers::SHIFT) => {
                self.rotate_focus(true);
                return Vec::new();
            }
            _ => {}
        }
        if key.code == KeyCode::Char('?') {
            if !matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) {
                return Vec::new();
            }
            if self.focus != Pane::Input {
                self.open_help();
                return Vec::new();
            }
        }
        match self.focus {
            Pane::Input => {
                self.handle_input_key(key, now);
                Vec::new()
            }
            Pane::Output => self.handle_output_key(key),
            Pane::Pipeline => {
                self.handle_pipeline_key(key, now);
                Vec::new()
            }
        }
    }

    pub(super) fn handle_event(&mut self, event: AppEvent) -> Vec<Effect> {
        let request_id = self.request_id;
        let mut effects = match event {
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
            AppEvent::ClipboardFinished { kind, result } => {
                self.set_status(Some(match result {
                    Ok(()) if kind == CopyKind::Hex => "Copied as Hex".to_string(),
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
        };
        if self.request_id != request_id {
            effects.insert(0, Effect::Cancel(self.request_id));
        }
        effects
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
    use crate::{
        TUI_OUTPUT_LIMIT,
        error::PipelineError,
        pipeline::{ExecutionOutcome, ExecutionReport, ExecutionTarget, StepStatus, execute},
        tui::views::{EffectiveView, effective_view},
    };
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
    fn input_keeps_all_pane_shortcut_characters_as_editor_input() {
        let start = now();
        let mut app = App::new(start, true);

        for character in "1234?zadjkpfvy".chars() {
            key(
                &mut app,
                KeyCode::Char(character),
                KeyModifiers::NONE,
                start,
            );
        }

        assert_eq!(app.input_text(), "1234?zadjkpfvy");
        assert_eq!(app.focus, Pane::Input);
        assert!(app.modal.is_none());
        assert!(app.zoom.is_none());
    }
    #[test]
    fn global_keys_cycle_focus_and_open_palette_help_or_quit() {
        let start = now();
        let mut app = App::new(start, true);

        key(&mut app, KeyCode::Tab, KeyModifiers::NONE, start);
        assert_eq!(app.focus, Pane::Output);
        key(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT, start);
        assert_eq!(app.focus, Pane::Input);

        key(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL, start);
        assert!(app.modal.is_some());
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);

        key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);
        assert!(app.modal.is_some());
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);

        assert!(matches!(
            key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL, start).as_slice(),
            [Effect::Quit(0)]
        ));
    }
    #[test]
    fn question_mark_opens_help_only_outside_input() {
        let start = now();
        let mut app = App::new(start, true);

        key(&mut app, KeyCode::Char('?'), KeyModifiers::NONE, start);
        assert_eq!(app.input_text(), "?");
        assert!(app.modal.is_none());

        app.focus = Pane::Pipeline;
        key(&mut app, KeyCode::Char('?'), KeyModifiers::NONE, start);
        assert!(app.modal.is_some());
    }
    #[test]
    fn escape_closes_only_the_highest_priority_transient_state() {
        let start = now();
        let mut app = App::new(start, true);
        app.output.status = OutputStatus::Running;
        app.zoom = Some(Pane::Output);
        app.open_picker();

        let request_id = app.request_id;
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        assert!(app.modal.is_none());
        assert_eq!(app.zoom, Some(Pane::Output));
        assert!(matches!(app.output.status, OutputStatus::Running));
        assert_eq!(app.request_id, request_id);

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        assert!(app.zoom.is_none());
        assert!(matches!(app.output.status, OutputStatus::Running));
        assert_eq!(app.request_id, request_id);

        let effects = key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(_)]));
        assert!(matches!(app.output.status, OutputStatus::Cancelled));

        let request_id = app.request_id;
        assert!(key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start).is_empty());
        assert_eq!(app.request_id, request_id);
        assert!(matches!(app.output.status, OutputStatus::Cancelled));
    }
    #[test]
    fn ctrl_c_is_owned_only_by_the_run_loop_interrupt_path() {
        let start = now();
        let mut app = App::new(start, true);

        assert!(key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL, start).is_empty());
        assert_eq!(app.input_text(), "");
        assert!(matches!(
            app.force_interrupt().as_slice(),
            [Effect::Quit(130)]
        ));
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
        assert_eq!(app.focus, Pane::Output);
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
    fn utf8_artifact_copy_keeps_the_exact_original_text() {
        let original = "한글 e\u{301}\n\t\\u0061";
        let payload = clipboard_payload(&Artifact::new(original.as_bytes().to_vec())).unwrap();

        assert_eq!(payload.text, original);
        assert_eq!(payload.kind, CopyKind::Text);
    }
    #[test]
    fn binary_artifact_copies_as_lowercase_hex() {
        let payload = clipboard_payload(&Artifact::new(vec![0x00, 0xab, 0xff])).unwrap();

        assert_eq!(payload.text, "00abff");
        assert_eq!(payload.kind, CopyKind::Hex);
    }
    #[test]
    fn binary_copy_length_rejects_arithmetic_overflow() {
        assert_eq!(checked_hex_len(usize::MAX), None);
        assert_eq!(
            checked_hex_len(TUI_OUTPUT_LIMIT),
            Some(TUI_OUTPUT_LIMIT * 2)
        );
    }
    #[test]
    fn copy_format_depends_on_artifact_validity_in_every_non_trace_view() {
        for (bytes, expected_text, expected_kind) in [
            (b"plain".to_vec(), "plain", CopyKind::Text),
            (vec![0x00, 0xab, 0xff], "00abff", CopyKind::Hex),
        ] {
            for view in [ViewMode::Smart, ViewMode::Text, ViewMode::Hex] {
                let mut app = App::new(now(), true);
                app.output.status = OutputStatus::Ready;
                app.output.view = view;
                app.output.active_artifact = Some(Artifact::new(bytes.clone()));

                assert!(matches!(
                    app.request_copy().as_slice(),
                    [Effect::Copy(ClipboardPayload { text, kind })]
                        if text == expected_text && *kind == expected_kind
                ));
            }
        }

        for bytes in [b"plain".to_vec(), vec![0xff]] {
            let mut app = App::new(now(), true);
            app.output.status = OutputStatus::Ready;
            app.output.view = ViewMode::Trace;
            app.output.active_artifact = Some(Artifact::new(bytes));
            assert!(app.request_copy().is_empty());
        }
    }
    #[test]
    fn unsafe_preview_requires_confirmation_before_copy_effect() {
        let mut app = App::new(now(), true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"x\x1b[2J".to_vec()));
        assert!(app.request_copy().is_empty());
        assert!(matches!(app.modal, Some(Modal::UnsafeCopyConfirm { .. })));
        assert!(matches!(
            app.confirm_unsafe_copy().as_slice(),
            [Effect::Copy(_)]
        ));
    }
    #[test]
    fn unsafe_confirmation_owns_the_original_payload_until_approval() {
        let mut app = App::new(now(), true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"old\x1b".to_vec()));

        assert!(app.request_copy().is_empty());
        app.output.active_artifact = Some(Artifact::new(b"new".to_vec()));

        assert!(matches!(
            app.confirm_unsafe_copy().as_slice(),
            [Effect::Copy(ClipboardPayload {
                text,
                kind: CopyKind::Text,
            })] if text == "old\x1b"
        ));
    }
    #[test]
    fn unsafe_copy_cancel_or_modal_transition_discards_the_payload() {
        let start = now();
        for key_code in [KeyCode::Esc, KeyCode::Char('n')] {
            let mut app = App::new(start, true);
            app.output.status = OutputStatus::Ready;
            app.output.active_artifact = Some(Artifact::new(b"secret\x1b".to_vec()));
            app.request_copy();

            assert!(key(&mut app, key_code, KeyModifiers::NONE, start).is_empty());
            assert!(app.confirm_unsafe_copy().is_empty());
        }

        let mut app = App::new(start, true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"secret\x1b".to_vec()));
        app.request_copy();
        key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);

        assert!(app.modal.is_none());
        assert!(app.confirm_unsafe_copy().is_empty());
    }
    #[test]
    fn binary_copy_never_requires_unsafe_text_confirmation() {
        let mut app = App::new(now(), true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(vec![0x00, 0x1b, 0xff]));

        assert!(matches!(
            app.request_copy().as_slice(),
            [Effect::Copy(ClipboardPayload {
                text,
                kind: CopyKind::Hex,
            })] if text == "001bff"
        ));
        assert!(app.modal.is_none());
    }
    #[test]
    fn only_ready_preview_can_be_copied() {
        let start = now();
        let mut app = App::new(start, true);
        app.output.active_artifact = Some(Artifact::new(b"stale".to_vec()));
        assert!(app.request_copy().is_empty());
        app.output.status = OutputStatus::Debouncing {
            deadline: start + debounce_for(0),
        };
        assert!(app.request_copy().is_empty());
        app.output.status = OutputStatus::Running;
        assert!(app.request_copy().is_empty());
        app.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
        assert!(app.request_copy().is_empty());
        app.output.status = OutputStatus::Cancelled;
        assert!(app.request_copy().is_empty());
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = None;
        assert!(app.request_copy().is_empty());
        app.output.active_artifact = Some(Artifact::new(b"safe".to_vec()));
        assert!(matches!(app.request_copy().as_slice(), [Effect::Copy(_)]));
    }
    #[test]
    fn confirmation_modal_keys_accept_or_cancel_explicit_actions() {
        let start = now();
        let mut app = App::new(start, true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(vec![0x1b]));

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
    fn unsafe_confirmation_rejects_modified_keys_without_losing_the_payload() {
        let start = now();
        let mut app = App::new(start, true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"secret\x1b".to_vec()));
        app.request_copy();
        assert!(matches!(app.modal, Some(Modal::UnsafeCopyConfirm { .. })));

        for (code, modifiers) in [
            (KeyCode::Char('y'), KeyModifiers::CONTROL),
            (KeyCode::Enter, KeyModifiers::ALT),
            (KeyCode::Char('n'), KeyModifiers::META),
            (KeyCode::Esc, KeyModifiers::CONTROL),
        ] {
            assert!(key(&mut app, code, modifiers, start).is_empty());
            assert!(matches!(app.modal, Some(Modal::UnsafeCopyConfirm { .. })));
        }

        assert!(matches!(
            key(
                &mut app,
                KeyCode::Char('Y'),
                KeyModifiers::SHIFT,
                start
            )
            .as_slice(),
            [Effect::Copy(ClipboardPayload { text, .. })] if text == "secret\x1b"
        ));
    }
    #[test]
    fn quit_confirmation_rejects_modified_keys_without_discarding_input() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("keep", start);
        app.request_quit();
        assert!(matches!(app.modal, Some(Modal::QuitConfirm)));

        for (code, modifiers) in [
            (KeyCode::Char('y'), KeyModifiers::CONTROL),
            (KeyCode::Enter, KeyModifiers::ALT),
            (KeyCode::Char('n'), KeyModifiers::META),
            (KeyCode::Esc, KeyModifiers::CONTROL),
        ] {
            assert!(key(&mut app, code, modifiers, start).is_empty());
            assert!(matches!(app.modal, Some(Modal::QuitConfirm)));
            assert_eq!(app.input_text(), "keep");
        }

        assert!(key(&mut app, KeyCode::Char('N'), KeyModifiers::SHIFT, start).is_empty());
        assert!(app.modal.is_none());
        assert_eq!(app.input_text(), "keep");
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
    fn preview_enter_requests_copy_without_editing_input() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("source", start);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"result".to_vec()));
        app.focus = Pane::Output;

        let effects = key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Copy(ClipboardPayload {
                text,
                kind: CopyKind::Text,
            })] if text == "result"
        ));
        assert_eq!(app.input_text(), "source");
    }
    #[test]
    fn chain_keys_select_toggle_reorder_and_delete_steps() {
        let start = now();
        let mut app = App::new(start, true);
        app.add_transform("base64-encode", start);
        app.add_transform("url-encode", start);
        app.add_transform("format-json", start);
        app.focus = Pane::Pipeline;
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
    fn pipeline_supports_all_selection_edit_inspect_palette_and_zoom_keys() {
        let start = now();
        let mut app = App::new(start, true);
        app.steps = ["base64-encode", "url-encode", "format-json"]
            .into_iter()
            .map(|id| TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: true,
            })
            .collect();
        app.focus = Pane::Pipeline;

        key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE, start);
        assert_eq!(app.selected_step, 1);
        key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE, start);
        assert_eq!(app.selected_step, 0);
        key(&mut app, KeyCode::Down, KeyModifiers::SHIFT, start);
        assert_eq!(app.selected_step, 1);
        assert_eq!(app.steps[1].definition.id, "base64-encode");
        key(&mut app, KeyCode::Up, KeyModifiers::SHIFT, start);
        assert_eq!(app.selected_step, 0);
        assert_eq!(app.steps[0].definition.id, "base64-encode");
        key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);
        assert_eq!(app.selected_step, 1);
        key(&mut app, KeyCode::Char('J'), KeyModifiers::SHIFT, start);
        assert_eq!(app.selected_step, 2);
        assert_eq!(app.steps[2].definition.id, "url-encode");
        key(&mut app, KeyCode::Char('K'), KeyModifiers::SHIFT, start);
        assert_eq!(app.selected_step, 1);
        assert_eq!(app.steps[1].definition.id, "url-encode");

        key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE, start);
        assert!(!app.steps[1].enabled);
        key(&mut app, KeyCode::Char('d'), KeyModifiers::NONE, start);
        assert_eq!(app.steps.len(), 2);

        key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);
        assert!(app.modal.is_some());
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE, start);
        assert!(app.modal.is_some());
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);

        key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE, start);
        assert_eq!(app.zoom, Some(Pane::Pipeline));
        key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE, start);
        assert!(app.zoom.is_none());
    }
    #[test]
    fn pipeline_edits_schedule_final_but_selection_does_not() {
        let start = now();
        let mut app = App::new(start, true);
        app.steps = ["base64-encode", "url-encode"]
            .into_iter()
            .map(|id| TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: true,
            })
            .collect();
        app.focus = Pane::Pipeline;
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"owned".to_vec()));

        assert!(key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE, start).is_empty());
        assert_eq!(app.request_id, 0);
        assert_eq!(app.output.source, OutputSource::Step(0));
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"owned"
        );

        assert!(matches!(
            key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE, start).as_slice(),
            [Effect::Cancel(_)]
        ));
        assert_eq!(app.output.source, OutputSource::Final);
        assert!(matches!(app.output.status, OutputStatus::Debouncing { .. }));
        assert!(matches!(
            app.handle_event(AppEvent::Tick(start + debounce_for(0)))
                .as_slice(),
            [Effect::Submit(PreviewJob {
                target: ExecutionTarget::Final,
                ..
            })]
        ));
    }
    #[test]
    fn output_cycles_views_requests_sources_copy_and_zoom() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        app.steps.push(TransformStep {
            definition: transform_by_id("base64-encode").unwrap(),
            enabled: true,
        });
        app.output.status = OutputStatus::Ready;
        app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
        app.output.active_artifact = app.output.final_artifact.clone();

        for expected in [
            ViewMode::Text,
            ViewMode::Hex,
            ViewMode::Trace,
            ViewMode::Smart,
        ] {
            key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE, start);
            assert_eq!(app.output.view, expected);
        }
        key(&mut app, KeyCode::Char('V'), KeyModifiers::SHIFT, start);
        assert_eq!(app.output.view, ViewMode::Trace);

        assert!(matches!(
            key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE, start).as_slice(),
            [Effect::Cancel(_), Effect::Submit(_)]
        ));
        assert!(
            key(&mut app, KeyCode::Char('f'), KeyModifiers::NONE, start)
                .iter()
                .all(|effect| !matches!(effect, Effect::Submit(_)))
        );
        assert_eq!(app.output.source, OutputSource::Final);
        app.output.view = ViewMode::Smart;
        assert!(matches!(
            key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE, start).as_slice(),
            [Effect::Copy(ClipboardPayload {
                text,
                kind: CopyKind::Text,
            })] if text == "final"
        ));
        assert!(matches!(
            key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start).as_slice(),
            [Effect::Copy(ClipboardPayload {
                text,
                kind: CopyKind::Text,
            })] if text == "final"
        ));

        key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE, start);
        assert_eq!(app.zoom, Some(Pane::Output));
        key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE, start);
        assert!(app.zoom.is_none());
    }
    #[test]
    fn output_scroll_keys_change_only_bounded_offsets() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(vec![b'x'; 40]));

        app.output.view = ViewMode::Text;
        for code in [
            KeyCode::Down,
            KeyCode::Right,
            KeyCode::PageDown,
            KeyCode::End,
        ] {
            key(&mut app, code, KeyModifiers::NONE, start);
        }
        assert!(app.output.byte_offset <= 40);
        assert!(
            std::str::from_utf8(app.output.active_artifact.as_ref().unwrap().bytes())
                .unwrap()
                .is_char_boundary(app.output.byte_offset)
        );
        let request_id = app.request_id;
        key(&mut app, KeyCode::Home, KeyModifiers::NONE, start);
        assert_eq!(app.output.byte_offset, 0);
        key(&mut app, KeyCode::Right, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Left, KeyModifiers::NONE, start);
        assert_eq!(app.output.byte_offset, 0);

        app.output.view = ViewMode::Hex;
        key(&mut app, KeyCode::End, KeyModifiers::NONE, start);
        assert_eq!(app.output.row_offset, 2);
        key(&mut app, KeyCode::PageUp, KeyModifiers::NONE, start);
        assert!(app.output.row_offset <= 2);

        app.output.view = ViewMode::Trace;
        app.output.traces = (1..=3)
            .map(|step| StepTrace {
                step,
                transform_id: "base64-encode",
                input_bytes: None,
                output_bytes: None,
                elapsed: None,
                status: StepStatus::NotExecuted,
                error: None,
            })
            .collect();
        key(&mut app, KeyCode::End, KeyModifiers::NONE, start);
        assert_eq!(app.output.row_offset, 2);
        key(&mut app, KeyCode::Up, KeyModifiers::NONE, start);
        assert_eq!(app.output.row_offset, 1);
        assert_eq!(app.request_id, request_id);
        assert!(matches!(app.output.status, OutputStatus::Ready));
    }
    #[test]
    fn trace_view_never_copies_an_underlying_artifact() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.view = ViewMode::Trace;
        app.output.active_artifact = Some(Artifact::new(b"hidden result".to_vec()));

        assert!(key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start).is_empty());
        assert!(key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE, start).is_empty());
        assert!(app.modal.is_none());
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
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"result".to_vec()));
        app.handle_event(AppEvent::ClipboardFinished {
            kind: CopyKind::Text,
            result: Err("Clipboard unavailable".to_string()),
        });
        assert!(matches!(app.output.status, OutputStatus::Ready));
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"result"
        );
        assert_eq!(app.status.as_deref(), Some("Clipboard unavailable"));
    }
    #[test]
    fn clipboard_success_message_preserves_the_copy_kind() {
        for (kind, expected) in [(CopyKind::Text, "Copied"), (CopyKind::Hex, "Copied as Hex")] {
            let mut app = App::new(now(), true);

            app.handle_event(AppEvent::ClipboardFinished {
                kind,
                result: Ok(()),
            });

            assert_eq!(app.status.as_deref(), Some(expected));
        }
    }
    #[test]
    fn clipboard_failure_is_safe_and_preserves_workbench_state() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("source", start);
        app.steps.push(TransformStep {
            definition: transform_by_id("base64-encode").unwrap(),
            enabled: true,
        });
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"result".to_vec()));
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "base64-encode",
            input_bytes: Some(6),
            output_bytes: Some(8),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });

        app.handle_event(AppEvent::ClipboardFinished {
            kind: CopyKind::Hex,
            result: Err("clipboard\n\u{1b}[2J".to_string()),
        });

        assert_eq!(app.status.as_deref(), Some("clipboard\\x0a\\x1b[2J"));
        assert_eq!(app.input_text(), "source");
        assert_eq!(app.steps.len(), 1);
        assert_eq!(app.output.source, OutputSource::Step(0));
        assert!(matches!(app.output.status, OutputStatus::Ready));
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"result"
        );
        assert_eq!(app.output.traces.len(), 1);
    }
    #[test]
    fn starts_as_an_empty_non_destructive_workbench() {
        let app = App::new(now(), true);
        assert_eq!(app.input_text(), "");
        assert!(app.steps.is_empty());
        assert!(matches!(app.output.status, OutputStatus::Idle));
    }
    #[test]
    fn small_change_debounces_for_50_milliseconds() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("x", start);

        assert!(
            app.handle_event(AppEvent::Tick(start + Duration::from_millis(49)))
                .is_empty()
        );
        let effects = app.handle_event(AppEvent::Tick(start + Duration::from_millis(50)));
        assert!(matches!(effects.as_slice(), [Effect::Submit(_)]));
        assert!(matches!(app.output.status, OutputStatus::Running));
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
        app.handle_event(AppEvent::PreviewFinished(preview_result(
            job.request_id,
            job.target,
            match result {
                Ok(bytes) => ExecutionOutcome::Success(bytes),
                Err(error) => ExecutionOutcome::Failed(error),
            },
        )));
        assert_eq!(app.input_text(), "plain");
        assert!(matches!(app.output.status, OutputStatus::Ready));
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"plain"
        );
    }
    #[test]
    fn a_change_immediately_hides_the_previous_copyable_result() {
        let start = now();
        let mut app = App::new(start, true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"old".to_vec()));
        app.insert_paste("new", start);
        assert!(matches!(app.output.status, OutputStatus::Debouncing { .. }));
        assert!(!app.can_copy());
    }
    #[test]
    fn stale_worker_result_never_replaces_current_preview() {
        let start = now();
        let mut app = App::new(start, true);
        app.request_id = 2;
        app.output.status = OutputStatus::Running;
        app.handle_event(AppEvent::PreviewFinished(preview_result(
            1,
            ExecutionTarget::Final,
            ExecutionOutcome::Success(b"old".to_vec()),
        )));
        assert!(matches!(app.output.status, OutputStatus::Running));
        assert!(app.output.active_artifact.is_none());
        assert!(app.request_copy().is_empty());
    }
    #[test]
    fn an_error_hides_previous_result_and_disables_copy() {
        let start = now();
        let mut app = App::new(start, true);
        app.request_id = 2;
        app.output.status = OutputStatus::Ready;
        app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
        app.output.final_traces.push(StepTrace {
            step: 1,
            transform_id: "hex-encode",
            input_bytes: Some(3),
            output_bytes: Some(6),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });
        app.output.active_artifact = Some(Artifact::new(b"old".to_vec()));
        app.handle_event(AppEvent::PreviewFinished(preview_result(
            2,
            ExecutionTarget::Final,
            ExecutionOutcome::Failed(PipelineError::TooManySteps { max: 32 }),
        )));
        assert!(matches!(app.output.status, OutputStatus::Failed(_)));
        assert!(app.output.final_artifact.is_none());
        assert!(app.output.final_traces.is_empty());
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
        let before = (app.input_text(), app.request_id);

        assert!(!app.insert_paste("\nc", start));
        assert_eq!((app.input_text(), app.request_id), before);
        assert!(matches!(app.output.status, OutputStatus::Debouncing { .. }));
        assert_eq!(app.status.as_deref(), Some("Input limit reached"));
    }
    #[test]
    fn enter_past_line_limit_preserves_input_and_preview_state() {
        let start = now();
        let mut app = App::new_with_input_limits(start, true, 8, 1);
        assert!(app.insert_paste("a", start));
        let before = (app.input_text(), app.request_id);

        app.handle_event(AppEvent::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            start,
        ));

        assert_eq!((app.input_text(), app.request_id), before);
        assert!(matches!(app.output.status, OutputStatus::Debouncing { .. }));
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
        let before = (app.input_text(), app.request_id);

        key(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL, start);

        assert_eq!((app.input_text(), app.request_id), before);
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
        let before = (app.input_text(), app.request_id);

        key(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL, start);

        assert_eq!((app.input_text(), app.request_id), before);
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
    fn cursor_and_selection_only_edits_keep_preview_ownership_and_cache() {
        let start = now();
        let mut app = App::new(start, true);
        assert!(app.insert_paste("text", start));
        let trace = StepTrace {
            step: 1,
            transform_id: "hex-encode",
            input_bytes: Some(4),
            output_bytes: Some(8),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        };
        app.request_id = 7;
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Ready;
        app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
        app.output.final_traces = vec![trace.clone()];
        app.output.active_artifact = Some(Artifact::new(b"active".to_vec()));
        app.output.traces = vec![trace.clone()];
        app.status = Some("keep".to_string());

        key(&mut app, KeyCode::Left, KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Left, KeyModifiers::SHIFT, start);

        assert_eq!(app.input_text(), "text");
        assert!(app.textarea.selection_range().is_some());
        assert_eq!(app.request_id, 7);
        assert_eq!(app.output.source, OutputSource::Step(0));
        assert_eq!(app.output.status, OutputStatus::Ready);
        assert_eq!(
            app.output.final_artifact.as_ref().unwrap().bytes(),
            b"final"
        );
        assert_eq!(
            app.output.final_traces.as_slice(),
            std::slice::from_ref(&trace)
        );
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"active"
        );
        assert_eq!(app.output.traces, [trace]);
        assert_eq!(app.status.as_deref(), Some("keep"));
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

    #[test]
    fn debounce_is_50_ms_through_256_kib_and_200_ms_above_it() {
        assert_eq!(debounce_for(256 * 1024), Duration::from_millis(50));
        assert_eq!(debounce_for(256 * 1024 + 1), Duration::from_millis(200));
    }

    #[test]
    fn one_paste_invalidates_once_and_schedules_one_final_job() {
        let start = now();
        let mut app = App::new(start, true);

        let effects = app.handle_event(AppEvent::Paste("pasted".to_string(), start));

        assert_eq!(app.request_id, 1);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(1)]));
        assert!(matches!(
            app.output.status,
            OutputStatus::Debouncing { deadline }
                if deadline == start + Duration::from_millis(50)
        ));
        let effects = app.handle_event(AppEvent::Tick(start + Duration::from_millis(50)));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Submit(PreviewJob {
                request_id: 1,
                target: ExecutionTarget::Final,
                ..
            })]
        ));
    }

    fn preview_result(
        request_id: u64,
        target: ExecutionTarget,
        outcome: ExecutionOutcome,
    ) -> PreviewResult {
        PreviewResult {
            report: ExecutionReport {
                request_id,
                target,
                outcome,
                traces: Vec::new(),
            },
        }
    }

    #[test]
    fn document_change_clears_owned_results_and_trace_and_disables_copy() {
        let start = now();
        let mut app = App::new(start, true);
        app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
        app.output.active_artifact = Some(Artifact::new(b"active".to_vec()));
        app.output.status = OutputStatus::Ready;
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "base64-encode",
            input_bytes: Some(3),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });
        app.output.final_traces = app.output.traces.clone();
        assert!(app.can_copy());

        let effects = app.handle_event(AppEvent::Paste("x".to_string(), start));

        assert_eq!(app.request_id, 1);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(1)]));
        assert_eq!(app.output.source, OutputSource::Final);
        assert!(app.output.final_artifact.is_none());
        assert!(app.output.final_traces.is_empty());
        assert!(app.output.active_artifact.is_none());
        assert!(app.output.traces.is_empty());
        assert!(!app.can_copy());
    }

    #[test]
    fn pipeline_change_uses_the_same_invalidation_and_debounce_path() {
        let start = now();
        let mut app = App::new(start, true);
        app.steps.push(TransformStep {
            definition: transform_by_id("base64-encode").unwrap(),
            enabled: true,
        });
        app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
        app.output.final_traces.push(StepTrace {
            step: 1,
            transform_id: "base64-encode",
            input_bytes: Some(3),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });
        app.output.active_artifact = Some(Artifact::new(b"active".to_vec()));
        app.output.status = OutputStatus::Ready;
        app.focus = Pane::Pipeline;

        let effects = key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE, start);

        assert_eq!(app.request_id, 1);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(1)]));
        assert_eq!(app.output.source, OutputSource::Final);
        assert!(app.output.final_artifact.is_none());
        assert!(app.output.final_traces.is_empty());
        assert!(app.output.active_artifact.is_none());
        assert!(matches!(
            app.output.status,
            OutputStatus::Debouncing { deadline }
                if deadline == start + Duration::from_millis(50)
        ));
    }

    #[test]
    fn selected_stage_request_is_immediate_and_keeps_cached_final() {
        let start = now();
        let mut app = App::new(start, true);
        app.steps.push(TransformStep {
            definition: transform_by_id("base64-encode").unwrap(),
            enabled: true,
        });
        app.output.final_artifact = Some(Artifact::new(b"cached".to_vec()));
        app.output.final_traces.push(StepTrace {
            step: 1,
            transform_id: "base64-encode",
            input_bytes: Some(3),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });
        app.output.active_artifact = app.output.final_artifact.clone();
        app.output.status = OutputStatus::Ready;
        app.focus = Pane::Output;

        let effects = key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE, start);

        assert_eq!(app.request_id, 1);
        assert_eq!(app.output.source, OutputSource::Step(0));
        assert!(matches!(app.output.status, OutputStatus::Running));
        assert_eq!(
            app.output.final_artifact.as_ref().unwrap().bytes(),
            b"cached"
        );
        assert_eq!(app.output.final_traces.len(), 1);
        assert!(app.output.active_artifact.is_none());
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cancel(1),
                Effect::Submit(PreviewJob {
                    request_id: 1,
                    target: ExecutionTarget::Step(0),
                    ..
                })
            ]
        ));
    }

    #[test]
    fn selected_stage_without_pipeline_is_a_true_no_op() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;

        let effects = key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE, start);

        assert!(effects.is_empty());
        assert_eq!(app.request_id, 0);
        assert_eq!(app.output.source, OutputSource::Final);
        assert_eq!(app.status.as_deref(), Some("No pipeline step selected"));
    }

    #[test]
    fn final_key_cancels_selected_stage_and_restores_cached_artifact() {
        let start = now();
        let mut app = App::new(start, true);
        app.request_id = 4;
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;
        app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
        app.focus = Pane::Output;

        let effects = key(&mut app, KeyCode::Char('f'), KeyModifiers::NONE, start);

        assert_eq!(app.request_id, 5);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(5)]));
        assert_eq!(app.output.source, OutputSource::Final);
        assert!(matches!(app.output.status, OutputStatus::Ready));
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"final"
        );
    }

    #[test]
    fn final_key_restores_cached_artifact_and_traces_after_completed_step_preview() {
        let start = now();
        let mut app = App::new(start, true);
        app.steps = ["base64-encode", "hex-encode"]
            .into_iter()
            .map(|id| TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: true,
            })
            .collect();
        let final_traces = vec![
            StepTrace {
                step: 1,
                transform_id: "base64-encode",
                input_bytes: Some(1),
                output_bytes: Some(4),
                elapsed: None,
                status: StepStatus::Succeeded,
                error: None,
            },
            StepTrace {
                step: 2,
                transform_id: "hex-encode",
                input_bytes: Some(4),
                output_bytes: Some(8),
                elapsed: None,
                status: StepStatus::Succeeded,
                error: None,
            },
        ];
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            report: ExecutionReport {
                request_id: 0,
                target: ExecutionTarget::Final,
                outcome: ExecutionOutcome::Success(b"final".to_vec()),
                traces: final_traces.clone(),
            },
        }));
        app.focus = Pane::Output;

        let effects = key(&mut app, KeyCode::Char('f'), KeyModifiers::NONE, start);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(1)]));
        assert_eq!(app.output.source, OutputSource::Final);
        assert_eq!(app.output.status, OutputStatus::Ready);
        assert_eq!(app.output.traces, final_traces);

        key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE, start);
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            report: ExecutionReport {
                request_id: 2,
                target: ExecutionTarget::Step(0),
                outcome: ExecutionOutcome::Success(b"step".to_vec()),
                traces: vec![
                    final_traces[0].clone(),
                    StepTrace {
                        step: 2,
                        transform_id: "hex-encode",
                        input_bytes: None,
                        output_bytes: None,
                        elapsed: None,
                        status: StepStatus::NotExecuted,
                        error: None,
                    },
                ],
            },
        }));
        app.output.view = ViewMode::Trace;

        let effects = key(&mut app, KeyCode::Char('f'), KeyModifiers::NONE, start);

        assert!(matches!(effects.as_slice(), [Effect::Cancel(3)]));
        assert_eq!(app.output.source, OutputSource::Final);
        assert_eq!(app.output.status, OutputStatus::Ready);
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"final"
        );
        assert_eq!(app.output.traces, final_traces);
        assert_eq!(
            effective_view(app.output.view, app.output.active_artifact.as_ref(), false),
            EffectiveView::Trace
        );
    }

    #[test]
    fn stale_or_cancelled_step_preview_does_not_change_cached_final() {
        let start = now();
        let mut app = App::new(start, true);
        app.steps.push(TransformStep {
            definition: transform_by_id("hex-encode").unwrap(),
            enabled: true,
        });
        let final_trace = StepTrace {
            step: 1,
            transform_id: "hex-encode",
            input_bytes: Some(1),
            output_bytes: Some(2),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        };
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            report: ExecutionReport {
                request_id: 0,
                target: ExecutionTarget::Final,
                outcome: ExecutionOutcome::Success(b"61".to_vec()),
                traces: vec![final_trace.clone()],
            },
        }));
        app.focus = Pane::Output;
        key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE, start);
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            report: ExecutionReport {
                request_id: 1,
                target: ExecutionTarget::Step(0),
                outcome: ExecutionOutcome::Success(b"stale".to_vec()),
                traces: Vec::new(),
            },
        }));

        key(&mut app, KeyCode::Char('f'), KeyModifiers::NONE, start);

        assert_eq!(app.output.source, OutputSource::Final);
        assert_eq!(app.output.status, OutputStatus::Ready);
        assert_eq!(app.output.active_artifact.as_ref().unwrap().bytes(), b"61");
        assert_eq!(app.output.traces, [final_trace]);
    }

    #[test]
    fn final_key_without_cached_artifact_submits_nothing() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;

        let effects = key(&mut app, KeyCode::Char('f'), KeyModifiers::NONE, start);

        assert!(effects.is_empty());
        assert_eq!(app.request_id, 0);
        assert_eq!(app.status.as_deref(), Some("Final output unavailable"));
    }

    #[test]
    fn pipeline_selection_movement_does_not_change_result_ownership() {
        let start = now();
        let mut app = App::new(start, true);
        for id in ["base64-encode", "hex-encode"] {
            app.steps.push(TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: true,
            });
        }
        app.output.source = OutputSource::Step(0);
        app.output.active_artifact = Some(Artifact::new(b"active".to_vec()));
        app.output.status = OutputStatus::Ready;
        app.focus = Pane::Pipeline;
        let request_id = app.request_id;

        let effects = key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);

        assert!(effects.is_empty());
        assert_eq!(app.selected_step, 1);
        assert_eq!(app.request_id, request_id);
        assert_eq!(app.output.source, OutputSource::Step(0));
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"active"
        );
    }

    #[test]
    fn manual_view_stays_pinned_and_smart_is_recomputed_from_reports() {
        let start = now();
        let mut app = App::new(start, true);
        app.output.view = ViewMode::Text;
        app.handle_event(AppEvent::Paste("x".to_string(), start));
        assert_eq!(app.output.view, ViewMode::Text);

        app.output.view = ViewMode::Smart;
        app.handle_event(AppEvent::PreviewFinished(preview_result(
            app.request_id,
            ExecutionTarget::Final,
            ExecutionOutcome::Success(vec![0xff]),
        )));
        assert_eq!(
            effective_view(
                app.output.view,
                app.output.active_artifact.as_ref(),
                matches!(app.output.status, OutputStatus::Failed(_))
            ),
            EffectiveView::Hex
        );

        app.request_id += 1;
        app.handle_event(AppEvent::PreviewFinished(preview_result(
            app.request_id,
            ExecutionTarget::Final,
            ExecutionOutcome::Failed(PipelineError::TooManySteps { max: 32 }),
        )));
        assert_eq!(
            effective_view(
                app.output.view,
                app.output.active_artifact.as_ref(),
                matches!(app.output.status, OutputStatus::Failed(_))
            ),
            EffectiveView::Trace
        );
    }

    #[test]
    fn every_stale_report_is_inert() {
        let start = now();
        let outcomes = [
            ExecutionOutcome::Success(b"stale".to_vec()),
            ExecutionOutcome::Failed(PipelineError::TooManySteps { max: 32 }),
            ExecutionOutcome::Cancelled,
        ];
        for outcome in outcomes {
            let mut app = App::new(start, true);
            app.request_id = 9;
            app.output.source = OutputSource::Step(2);
            app.output.status = OutputStatus::Ready;
            app.output.final_artifact = Some(Artifact::new(b"final-current".to_vec()));
            app.output.active_artifact = Some(Artifact::new(b"current".to_vec()));
            app.output.traces.push(StepTrace {
                step: 1,
                transform_id: "base64-encode",
                input_bytes: Some(3),
                output_bytes: Some(4),
                elapsed: None,
                status: StepStatus::Succeeded,
                error: None,
            });
            app.output.byte_offset = 7;
            app.output.row_offset = 3;
            app.status = Some("keep".to_string());
            assert!(app.can_copy());

            let active_before = app
                .output
                .active_artifact
                .as_ref()
                .unwrap()
                .bytes()
                .to_vec();
            let final_before = app.output.final_artifact.as_ref().unwrap().bytes().to_vec();
            let traces_before = app.output.traces.clone();
            let source_before = app.output.source;
            let byte_offset_before = app.output.byte_offset;
            let row_offset_before = app.output.row_offset;
            let output_status_before = app.output.status.clone();
            let user_status_before = app.status.clone();
            let was_copyable = app.can_copy();

            app.handle_event(AppEvent::PreviewFinished(preview_result(
                8,
                ExecutionTarget::Final,
                outcome,
            )));

            assert_eq!(
                app.output.active_artifact.as_ref().unwrap().bytes(),
                active_before
            );
            assert_eq!(
                app.output.final_artifact.as_ref().unwrap().bytes(),
                final_before
            );
            assert_eq!(app.output.traces, traces_before);
            assert_eq!(app.output.source, source_before);
            assert_eq!(app.output.byte_offset, byte_offset_before);
            assert_eq!(app.output.row_offset, row_offset_before);
            assert_eq!(app.output.status, output_status_before);
            assert_eq!(app.status, user_status_before);
            assert_eq!(app.can_copy(), was_copyable);
            assert_eq!(app.request_id, 9);
        }
    }

    #[test]
    fn escape_cancels_active_request_and_a_later_change_schedules_final() {
        let start = now();
        let mut app = App::new(start, true);
        app.request_id = 3;
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;
        app.focus = Pane::Output;

        let effects = key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
        assert_eq!(app.request_id, 4);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(4)]));
        assert!(matches!(app.output.status, OutputStatus::Cancelled));

        app.focus = Pane::Input;
        let effects = app.handle_event(AppEvent::Paste("fresh".to_string(), start));
        assert_eq!(app.request_id, 5);
        assert!(matches!(effects.as_slice(), [Effect::Cancel(5)]));
        assert_eq!(app.output.source, OutputSource::Final);
        assert!(matches!(app.output.status, OutputStatus::Debouncing { .. }));
    }

    #[test]
    fn text_arrows_advance_on_utf8_boundaries_without_repeating_the_same_window() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        app.output.view = ViewMode::Text;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new("界a".as_bytes().to_vec()));

        key(&mut app, KeyCode::Right, KeyModifiers::NONE, start);
        assert_eq!(app.output.byte_offset, "界".len());
        assert!("界a".is_char_boundary(app.output.byte_offset));
        let first = crate::tui::views::render_text_window(
            app.output.active_artifact.as_ref().unwrap(),
            app.output.byte_offset,
            1,
            80,
        );
        assert_eq!(first.text, "a");

        key(&mut app, KeyCode::Right, KeyModifiers::NONE, start);
        assert_eq!(app.output.byte_offset, "界".len());
        let second = crate::tui::views::render_text_window(
            app.output.active_artifact.as_ref().unwrap(),
            app.output.byte_offset,
            1,
            80,
        );
        assert_eq!(second.text, "a");
        key(&mut app, KeyCode::Left, KeyModifiers::NONE, start);
        assert_eq!(app.output.byte_offset, 0);
        assert!("界a".is_char_boundary(app.output.byte_offset));
    }

    #[test]
    fn text_end_and_page_down_stop_at_the_last_visible_start() {
        let start = now();
        for code in [KeyCode::End, KeyCode::PageDown] {
            let mut app = App::new(start, true);
            app.focus = Pane::Output;
            app.output.view = ViewMode::Text;
            app.output.status = OutputStatus::Ready;
            app.output.active_artifact = Some(Artifact::new(b"abc".to_vec()));

            key(&mut app, code, KeyModifiers::NONE, start);

            assert_eq!(app.output.byte_offset, 2);
            let window = crate::tui::views::render_text_window(
                app.output.active_artifact.as_ref().unwrap(),
                app.output.byte_offset,
                1,
                80,
            );
            assert_eq!(window.text, "c");
        }
    }

    #[test]
    fn text_pages_stay_bounded_and_long_graphemes_render_without_scalar_crawl() {
        let start = now();
        let text = format!("a{}b", "\u{301}".repeat(3_000));
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        app.output.view = ViewMode::Text;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(text.as_bytes().to_vec()));

        for _ in 0..2 {
            let before = app.output.byte_offset;
            key(&mut app, KeyCode::Right, KeyModifiers::NONE, start);
            assert!(app.output.byte_offset > before);
            assert!(app.output.byte_offset > before.saturating_add(1));
            assert!(text.is_char_boundary(app.output.byte_offset));
            let window = crate::tui::views::render_text_window(
                app.output.active_artifact.as_ref().unwrap(),
                app.output.byte_offset,
                1,
                80,
            );
            assert!(!window.text.is_empty());
        }
        assert_eq!(app.output.byte_offset, text.len() - 1);
        let maximum = app.output.byte_offset;
        key(&mut app, KeyCode::Right, KeyModifiers::NONE, start);
        assert_eq!(app.output.byte_offset, maximum);

        app.output.byte_offset = 0;
        key(&mut app, KeyCode::PageDown, KeyModifiers::NONE, start);
        assert_eq!(app.output.byte_offset, maximum);
        assert!(text.is_char_boundary(app.output.byte_offset));
        let page = crate::tui::views::render_text_window(
            app.output.active_artifact.as_ref().unwrap(),
            app.output.byte_offset,
            1,
            80,
        );
        assert_eq!(page.text, "b");

        key(&mut app, KeyCode::PageUp, KeyModifiers::NONE, start);
        assert!(app.output.byte_offset < maximum);
        assert!(text.is_char_boundary(app.output.byte_offset));
    }

    #[test]
    fn zoom_focus_moves_with_exact_tab_and_shift_tab() {
        let start = now();
        for (focus, code, modifiers, expected) in [
            (Pane::Input, KeyCode::Tab, KeyModifiers::NONE, Pane::Output),
            (
                Pane::Output,
                KeyCode::Tab,
                KeyModifiers::NONE,
                Pane::Pipeline,
            ),
            (
                Pane::Pipeline,
                KeyCode::Tab,
                KeyModifiers::NONE,
                Pane::Input,
            ),
            (
                Pane::Input,
                KeyCode::Tab,
                KeyModifiers::SHIFT,
                Pane::Pipeline,
            ),
            (Pane::Output, KeyCode::Tab, KeyModifiers::SHIFT, Pane::Input),
            (
                Pane::Pipeline,
                KeyCode::Tab,
                KeyModifiers::SHIFT,
                Pane::Output,
            ),
            (
                Pane::Pipeline,
                KeyCode::BackTab,
                KeyModifiers::SHIFT,
                Pane::Output,
            ),
        ] {
            let mut app = App::new(start, true);
            app.focus = focus;
            app.zoom = Some(focus);

            key(&mut app, code, modifiers, start);

            assert_eq!(app.focus, expected);
            assert_eq!(app.zoom, Some(expected));
        }
    }

    #[test]
    fn global_shortcuts_reject_extra_modifiers() {
        let start = now();
        for (code, modifiers) in [
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            (
                KeyCode::Char('q'),
                KeyModifiers::CONTROL | KeyModifiers::META,
            ),
            (KeyCode::Char('P'), KeyModifiers::CONTROL),
            (KeyCode::Char('Q'), KeyModifiers::CONTROL),
        ] {
            let mut app = App::new(start, true);
            let effects = key(&mut app, code, modifiers, start);
            assert!(effects.is_empty());
            assert!(app.modal.is_none());
        }

        for (code, modifiers) in [
            (KeyCode::Tab, KeyModifiers::ALT),
            (KeyCode::Tab, KeyModifiers::SHIFT | KeyModifiers::CONTROL),
            (KeyCode::BackTab, KeyModifiers::NONE),
            (KeyCode::BackTab, KeyModifiers::SHIFT | KeyModifiers::ALT),
        ] {
            let mut app = App::new(start, true);
            key(&mut app, code, modifiers, start);
            assert_eq!(app.focus, Pane::Input);
        }
    }

    #[test]
    fn transient_keys_accept_only_their_documented_modifiers() {
        let start = now();
        let mut running = App::new(start, true);
        running.focus = Pane::Output;
        running.output.status = OutputStatus::Running;
        let request_id = running.request_id;
        assert!(key(&mut running, KeyCode::Esc, KeyModifiers::ALT, start).is_empty());
        assert_eq!(running.request_id, request_id);
        assert_eq!(running.output.status, OutputStatus::Running);

        for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT, KeyModifiers::META] {
            let mut app = App::new(start, true);
            app.focus = Pane::Pipeline;
            key(&mut app, KeyCode::Char('?'), modifiers, start);
            assert!(app.modal.is_none());

            let mut input = App::new(start, true);
            key(&mut input, KeyCode::Char('?'), modifiers, start);
            assert_eq!(input.input_text(), "");
            assert!(input.modal.is_none());
        }
        let mut shifted_question = App::new(start, true);
        shifted_question.focus = Pane::Pipeline;
        key(
            &mut shifted_question,
            KeyCode::Char('?'),
            KeyModifiers::SHIFT,
            start,
        );
        assert!(matches!(shifted_question.modal, Some(Modal::Help)));

        for (code, modifiers) in [
            (KeyCode::Esc, KeyModifiers::ALT),
            (KeyCode::Enter, KeyModifiers::CONTROL),
            (KeyCode::Backspace, KeyModifiers::META),
            (KeyCode::Down, KeyModifiers::SHIFT),
            (KeyCode::Up, KeyModifiers::ALT),
        ] {
            let mut picker = App::new(start, true);
            picker.open_picker();
            key(&mut picker, KeyCode::Char('z'), KeyModifiers::NONE, start);
            key(&mut picker, code, modifiers, start);
            assert!(matches!(
                picker.modal,
                Some(Modal::TransformPicker {
                    ref query,
                    selected: 0,
                }) if query == "z"
            ));
            assert!(picker.steps.is_empty());
        }

        let mut help = App::new(start, true);
        help.open_help();
        key(&mut help, KeyCode::Esc, KeyModifiers::CONTROL, start);
        assert!(matches!(help.modal, Some(Modal::Help)));

        let mut inspector = App::new(start, true);
        inspector.steps.push(TransformStep {
            definition: transform_by_id("hex-encode").unwrap(),
            enabled: true,
        });
        inspector.open_inspector();
        key(&mut inspector, KeyCode::Esc, KeyModifiers::ALT, start);
        assert!(matches!(inspector.modal, Some(Modal::StepInspector)));
    }

    #[test]
    fn help_restores_the_suspended_picker_query_and_selection() {
        let start = now();
        let mut app = App::new(start, true);
        app.open_picker();
        for character in "decode".chars() {
            key(
                &mut app,
                KeyCode::Char(character),
                KeyModifiers::NONE,
                start,
            );
        }
        key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);

        key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);
        assert!(matches!(app.modal, Some(Modal::Help)));
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);

        assert!(matches!(
            app.modal,
            Some(Modal::TransformPicker {
                ref query,
                selected: 1,
            }) if query == "decode"
        ));
    }
}
