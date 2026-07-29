use std::{
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, Paragraph},
};
use tui_textarea::{TextArea, WrapMode};

use crate::{
    MAX_STEPS, TUI_INPUT_LIMIT, TUI_OUTPUT_LIMIT,
    error::PipelineError,
    pipeline::{TransformStep, execute},
    transforms::{TransformDefinition, transform_by_id, transforms},
};

const DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Input,
    Preview,
    Chain,
}

pub enum PreviewState {
    Idle,
    Debouncing {
        deadline: Instant,
    },
    Running {
        generation: u64,
    },
    Ready {
        generation: u64,
        document: PreviewDocument,
    },
    Error {
        generation: u64,
        message: String,
    },
}

pub struct PreviewDocument {
    pub raw: Arc<str>,
    pub line_starts: Vec<usize>,
}

impl PreviewDocument {
    pub fn new(raw: String) -> Self {
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

pub enum Modal {
    TransformPicker { query: String, selected: usize },
    QuitConfirm,
    UnsafeCopyConfirm,
}

pub struct PreviewJob {
    pub generation: u64,
    pub input: Vec<u8>,
    pub steps: Vec<TransformStep>,
}

pub struct PreviewResult {
    pub generation: u64,
    pub result: Result<Vec<u8>, PipelineError>,
}

pub enum AppEvent {
    Key(KeyEvent, Instant),
    Paste(String, Instant),
    Tick(Instant),
    PreviewFinished(PreviewResult),
    ClipboardFinished(Result<(), String>),
}

pub enum Effect {
    Submit(PreviewJob),
    Copy(Arc<str>),
    Quit(i32),
}

pub struct App {
    pub textarea: TextArea<'static>,
    pub focus: Pane,
    pub steps: Vec<TransformStep>,
    pub selected_step: usize,
    pub preview: PreviewState,
    pub generation: u64,
    pub modal: Option<Modal>,
    pub status: Option<String>,
    pub preview_scroll: usize,
    pub no_color: bool,
    input_limit: usize,
}

impl App {
    pub fn new(now: Instant, no_color: bool) -> Self {
        Self::new_with_input_limit(now, no_color, TUI_INPUT_LIMIT)
    }

    fn new_with_input_limit(_: Instant, no_color: bool, input_limit: usize) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_wrap_mode(WrapMode::WordOrGlyph);
        textarea.set_cursor_line_style(Style::default());
        if no_color {
            textarea.set_cursor_style(Style::default());
            textarea.set_selection_style(Style::default());
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
        }
    }

    pub fn input_text(&self) -> String {
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

    fn changed(&mut self, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.preview = PreviewState::Debouncing {
            deadline: now + DEBOUNCE,
        };
        self.preview_scroll = 0;
    }

    pub fn insert_paste(&mut self, text: &str, now: Instant) -> bool {
        let retained = self.input_len().saturating_sub(self.selected_input_len());
        let remaining = self.input_limit.saturating_sub(retained);
        if text.len() > remaining.saturating_mul(2) {
            self.status = Some("Input limit reached".to_string());
            return false;
        }
        let (normalized, replaced) = normalize_paste(text);
        if normalized.len() > remaining {
            self.status = Some("Input limit reached".to_string());
            return false;
        }
        let before = self.textarea.clone();
        let modified = self.textarea.insert_str(normalized);
        if modified && self.input_len() > self.input_limit {
            self.textarea = before;
            self.status = Some("Input limit reached".to_string());
            return false;
        }
        if modified {
            self.status = (replaced > 0).then(|| format!("{replaced} control characters replaced"));
            self.changed(now);
        }
        modified
    }

    pub fn add_transform(&mut self, id: &str, now: Instant) -> bool {
        if self.steps.len() == MAX_STEPS {
            self.status = Some("Chain limit reached".to_string());
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

    pub fn toggle_selected(&mut self, now: Instant) {
        let Some(step) = self.steps.get_mut(self.selected_step) else {
            return;
        };
        step.enabled = !step.enabled;
        self.changed(now);
    }

    pub fn move_selected(&mut self, direction: i8, now: Instant) {
        let next = match direction {
            -1 if self.selected_step > 0 => self.selected_step - 1,
            1 if self.selected_step + 1 < self.steps.len() => self.selected_step + 1,
            _ => return,
        };
        self.steps.swap(self.selected_step, next);
        self.selected_step = next;
        self.changed(now);
    }

    pub fn delete_selected(&mut self, now: Instant) {
        if self.steps.get(self.selected_step).is_none() {
            return;
        }
        self.steps.remove(self.selected_step);
        self.selected_step = self.selected_step.min(self.steps.len().saturating_sub(1));
        self.changed(now);
    }

    pub fn can_copy(&self) -> bool {
        matches!(self.preview, PreviewState::Ready { .. })
    }

    pub fn open_picker(&mut self) {
        self.modal = Some(Modal::TransformPicker {
            query: String::new(),
            selected: 0,
        });
    }

    pub fn picker_insert(&mut self, character: char) {
        if let Some(Modal::TransformPicker { query, selected }) = &mut self.modal {
            query.push(character);
            *selected = 0;
        }
    }

    pub fn filtered_transforms(&self) -> Vec<&'static TransformDefinition> {
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

    pub fn confirm_picker(&mut self, now: Instant) {
        let selected = match self.modal {
            Some(Modal::TransformPicker { selected, .. }) => selected,
            _ => return,
        };
        let id = self
            .filtered_transforms()
            .get(selected)
            .map(|transform| transform.id);
        self.modal = None;
        if let Some(id) = id {
            self.add_transform(id, now);
        }
    }

    pub fn request_copy(&mut self) -> Vec<Effect> {
        let Some((raw, unsafe_raw)) = (match &self.preview {
            PreviewState::Ready { document, .. } => Some((
                Arc::clone(&document.raw),
                crate::error::contains_dangerous_control(&document.raw),
            )),
            _ => None,
        }) else {
            return Vec::new();
        };
        if unsafe_raw {
            self.modal = Some(Modal::UnsafeCopyConfirm);
            Vec::new()
        } else {
            vec![Effect::Copy(raw)]
        }
    }

    pub fn confirm_unsafe_copy(&mut self) -> Vec<Effect> {
        if !matches!(self.modal, Some(Modal::UnsafeCopyConfirm)) {
            return Vec::new();
        }
        self.modal = None;
        match &self.preview {
            PreviewState::Ready { document, .. } => {
                vec![Effect::Copy(Arc::clone(&document.raw))]
            }
            _ => Vec::new(),
        }
    }

    pub fn request_quit(&mut self) -> Vec<Effect> {
        if self.input_len() == 0 {
            vec![Effect::Quit(0)]
        } else {
            self.modal = Some(Modal::QuitConfirm);
            Vec::new()
        }
    }

    fn tick(&mut self, now: Instant) -> Vec<Effect> {
        let PreviewState::Debouncing { deadline } = &self.preview else {
            return Vec::new();
        };
        if now < *deadline {
            return Vec::new();
        }
        let generation = self.generation;
        self.preview = PreviewState::Running { generation };
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
                    generation: result.generation,
                    document: PreviewDocument::new(text),
                },
                Err(_) => PreviewState::Error {
                    generation: result.generation,
                    message: "Transform returned invalid UTF-8".to_string(),
                },
            },
            Err(error) => PreviewState::Error {
                generation: result.generation,
                message: crate::error::render_pipeline_error(&error),
            },
        };
    }

    fn rotate_focus(&mut self, backwards: bool) {
        self.focus = match (self.focus, backwards) {
            (Pane::Input, false) | (Pane::Chain, true) => Pane::Preview,
            (Pane::Preview, false) | (Pane::Input, true) => Pane::Chain,
            (Pane::Chain, false) | (Pane::Preview, true) => Pane::Input,
        };
    }

    fn handle_modal_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect> {
        match self.modal {
            Some(Modal::TransformPicker { .. }) => {
                let filtered_len = self.filtered_transforms().len();
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => self.modal = None,
                    (KeyCode::Enter, _) => self.confirm_picker(now),
                    (KeyCode::Backspace, _) => {
                        if let Some(Modal::TransformPicker { query, selected }) = &mut self.modal {
                            query.pop();
                            *selected = 0;
                        }
                    }
                    (KeyCode::Up, _) => {
                        if let Some(Modal::TransformPicker { selected, .. }) = &mut self.modal {
                            *selected = selected.saturating_sub(1);
                        }
                    }
                    (KeyCode::Down, _) => {
                        if let Some(Modal::TransformPicker { selected, .. }) = &mut self.modal {
                            *selected = selected
                                .saturating_add(1)
                                .min(filtered_len.saturating_sub(1));
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
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Some(Modal::QuitConfirm) => match key.code {
                KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                    self.modal = None;
                    vec![Effect::Quit(0)]
                }
                KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                    self.modal = None;
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
        let before = self.textarea.clone();
        if self.textarea.input(key) {
            if self.input_len() > self.input_limit {
                self.textarea = before;
                self.status = Some("Input limit reached".to_string());
            } else {
                self.changed(now);
            }
        }
    }

    fn handle_preview_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let line_count = match &self.preview {
            PreviewState::Ready { document, .. } => document.line_starts.len(),
            _ => 0,
        };
        let last_line = line_count.saturating_sub(1);
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
                self.selected_step = self.selected_step.saturating_sub(1);
            }
            (KeyCode::Down, _) => {
                self.selected_step = self
                    .selected_step
                    .saturating_add(1)
                    .min(self.steps.len().saturating_sub(1));
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

    pub fn handle_event(&mut self, event: AppEvent) -> Vec<Effect> {
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
                self.status = Some(match result {
                    Ok(()) => "Copied".to_string(),
                    Err(message) => crate::error::escape_external(&message, 512),
                });
                Vec::new()
            }
            AppEvent::Key(key, now) => self.handle_key(key, now),
        }
    }
}

fn visible_safe_text(
    document: &PreviewDocument,
    first_line: usize,
    rows: usize,
    columns: usize,
) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    for row in 0..rows {
        let Some(line) = document.line(first_line + row) else {
            break;
        };
        if row > 0 {
            output.push('\n');
        }
        let mut used = 0;
        for character in line.chars() {
            let width = if crate::error::is_dangerous_control(character) {
                4
            } else {
                1
            };
            if used + width > columns {
                break;
            }
            if width == 4 {
                write!(&mut output, "\\x{:02x}", character as u32)
                    .expect("writing to String cannot fail");
            } else {
                output.push(character);
            }
            used += width;
        }
    }
    output
}

fn pane_style(app: &App, focused: bool) -> Style {
    if app.no_color || !focused {
        Style::default()
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }
}

fn selection_style(app: &App) -> Style {
    if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    }
}

fn pane_block<'a>(app: &App, title: &'a str, focused: bool) -> Block<'a> {
    let style = pane_style(app, focused);
    Block::bordered()
        .title(title)
        .style(Style::default())
        .border_style(style)
        .title_style(style)
}

fn render_input(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let block = pane_block(app, "Input", app.focus == Pane::Input);
    app.textarea.set_block(block);
    frame.render_widget(&app.textarea, area);
}

fn render_preview(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = pane_block(app, "Preview", app.focus == Pane::Preview);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = match &app.preview {
        PreviewState::Idle => String::new(),
        PreviewState::Debouncing { .. } => "Waiting for changes…".to_string(),
        PreviewState::Running { .. } => "Running…".to_string(),
        PreviewState::Ready { document, .. } => visible_safe_text(
            document,
            app.preview_scroll,
            inner.height as usize,
            inner.width as usize,
        ),
        PreviewState::Error { message, .. } => crate::error::escape_external(
            message,
            (inner.width as usize)
                .saturating_mul(inner.height as usize)
                .min(512),
        ),
    };
    frame.render_widget(Paragraph::new(text).style(Style::default()), inner);
}

fn render_chain(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = pane_block(app, "Chain", app.focus == Pane::Chain);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let available = inner.height as usize;
    let start = if app.selected_step >= available {
        app.selected_step + 1 - available
    } else {
        0
    };
    let items: Vec<_> = app
        .steps
        .iter()
        .enumerate()
        .skip(start)
        .take(available)
        .map(|(index, step)| {
            let selected = index == app.selected_step;
            let prefix = if selected { ">" } else { " " };
            let enabled = if step.enabled { "on" } else { "off" };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{prefix} [{enabled}] "),
                    if selected {
                        selection_style(app)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    step.definition.display_name,
                    if selected {
                        selection_style(app)
                    } else {
                        Style::default()
                    },
                ),
            ]))
        })
        .collect();
    frame.render_widget(List::new(items).style(Style::default()), inner);
}

fn preview_label(preview: &PreviewState) -> &'static str {
    match preview {
        PreviewState::Idle => "Idle",
        PreviewState::Debouncing { .. } => "Waiting",
        PreviewState::Running { .. } => "Running",
        PreviewState::Ready { .. } => "Ready",
        PreviewState::Error { .. } => "Error",
    }
}

fn pane_label(pane: Pane) -> &'static str {
    match pane {
        Pane::Input => "Input",
        Pane::Preview => "Preview",
        Pane::Chain => "Chain",
    }
}

fn render_status(frame: &mut Frame<'_>, app: &App, area: Rect, narrow: bool) {
    let enabled = app.steps.iter().filter(|step| step.enabled).count();
    let location = if narrow {
        [Pane::Input, Pane::Preview, Pane::Chain]
            .into_iter()
            .map(|pane| {
                if pane == app.focus {
                    format!("[{}]", pane_label(pane))
                } else {
                    pane_label(pane).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        format!("Focus: {}", pane_label(app.focus))
    };
    let mut text = format!(
        "{location} | {} | {enabled} enabled | Ctrl+P add | Ctrl+Q quit",
        preview_label(&app.preview)
    );
    if let Some(message) = &app.status {
        text.push_str(" | ");
        text.push_str(&crate::error::escape_external(message, area.width as usize));
    }
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Cyan)
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn render_picker(frame: &mut Frame<'_>, app: &App, query: &str, selected: usize) {
    let area = centered(frame.area(), 60, 12);
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    };
    let block = Block::bordered()
        .title("Add transform")
        .style(Style::default())
        .border_style(style)
        .title_style(style);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let [query_area, list_area, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(format!(
            "Search: {}",
            crate::error::escape_external(query, query_area.width as usize)
        ))
        .style(Style::default()),
        query_area,
    );

    let filtered = app.filtered_transforms();
    let available = list_area.height as usize;
    let start = if selected >= available {
        selected + 1 - available
    } else {
        0
    };
    let items: Vec<_> = filtered
        .into_iter()
        .enumerate()
        .skip(start)
        .take(available)
        .map(|(index, transform)| {
            let selected = index == selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "> " } else { "  " },
                    if selected {
                        selection_style(app)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    format!("{} ({})", transform.display_name, transform.id),
                    if selected {
                        selection_style(app)
                    } else {
                        Style::default()
                    },
                ),
            ]))
        })
        .collect();
    frame.render_widget(List::new(items).style(Style::default()), list_area);
    frame.render_widget(
        Paragraph::new("Enter add · Esc cancel").style(Style::default()),
        hint_area,
    );
}

fn render_confirmation(frame: &mut Frame<'_>, app: &App, message: &'static str) {
    let area = centered(frame.area(), 42, 5);
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    };
    let block = Block::bordered()
        .title("Confirm")
        .style(Style::default())
        .border_style(style)
        .title_style(style);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(format!("{message}\nEnter/y confirm · n/Esc cancel"))
            .alignment(Alignment::Center)
            .style(Style::default()),
        inner,
    );
}

fn render_modal(frame: &mut Frame<'_>, app: &App) {
    match &app.modal {
        Some(Modal::TransformPicker { query, selected }) => {
            render_picker(frame, app, query, *selected);
        }
        Some(Modal::UnsafeCopyConfirm) => {
            render_confirmation(frame, app, "Copy raw control characters?");
        }
        Some(Modal::QuitConfirm) => {
            render_confirmation(frame, app, "Discard input and quit?");
        }
        None => {}
    }
}

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    if area.width < 40 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Increase terminal size to at least 40×10").alignment(Alignment::Center),
            area,
        );
        return;
    }

    let [main, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    if area.width >= 120 {
        let [input, preview, chain] = Layout::horizontal([
            Constraint::Percentage(38),
            Constraint::Percentage(38),
            Constraint::Percentage(24),
        ])
        .areas(main);
        render_input(frame, app, input);
        render_preview(frame, app, preview);
        render_chain(frame, app, chain);
    } else {
        match app.focus {
            Pane::Input => render_input(frame, app, main),
            Pane::Preview => render_preview(frame, app, main),
            Pane::Chain => render_chain(frame, app, main),
        }
    }
    render_status(frame, app, status, area.width < 120);
    render_modal(frame, app);
}

pub fn normalize_paste(input: &str) -> (String, usize) {
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

struct WorkerState {
    pending: Option<PreviewJob>,
    shutdown: bool,
}

pub struct PreviewWorker {
    shared: Arc<(Mutex<WorkerState>, Condvar)>,
    results: mpsc::Receiver<PreviewResult>,
}

impl Default for PreviewWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewWorker {
    pub fn new() -> Self {
        let shared = Arc::new((
            Mutex::new(WorkerState {
                pending: None,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let worker_shared = Arc::clone(&shared);
        let (sender, results) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let job = {
                    let (lock, condition) = &*worker_shared;
                    let mut state = lock.lock().expect("preview worker lock poisoned");
                    while state.pending.is_none() && !state.shutdown {
                        state = condition.wait(state).expect("preview worker lock poisoned");
                    }
                    if state.shutdown {
                        return;
                    }
                    state.pending.take().expect("pending job checked")
                };
                let result = execute(job.input, &job.steps, TUI_OUTPUT_LIMIT);
                if sender
                    .send(PreviewResult {
                        generation: job.generation,
                        result,
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
        Self { shared, results }
    }

    pub fn submit(&self, job: PreviewJob) {
        let (lock, condition) = &*self.shared;
        let mut state = lock.lock().expect("preview worker lock poisoned");
        state.pending = Some(job);
        condition.notify_one();
    }

    pub fn try_recv(&self) -> Option<PreviewResult> {
        self.results.try_recv().ok()
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        let (lock, condition) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.pending = None;
            condition.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Barrier, OnceLock};

    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };
    use tui_textarea::WrapMode;

    use crate::{error::TransformError, transforms::TransformDefinition};

    struct BlockingTransformControl {
        started: mpsc::Sender<Vec<u8>>,
        release: Arc<Barrier>,
    }

    static LATEST_ONLY_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();
    static DROP_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();

    fn block(
        control: &OnceLock<BlockingTransformControl>,
        input: &[u8],
    ) -> Result<Vec<u8>, TransformError> {
        let control = control.get().expect("blocking transform configured");
        control
            .started
            .send(input.to_vec())
            .expect("blocking transform observer available");
        control.release.wait();
        Ok(input.to_vec())
    }

    fn block_latest_only(input: &[u8], _: usize) -> Result<Vec<u8>, TransformError> {
        block(&LATEST_ONLY_CONTROL, input)
    }

    fn block_during_drop(input: &[u8], _: usize) -> Result<Vec<u8>, TransformError> {
        block(&DROP_CONTROL, input)
    }

    static LATEST_ONLY_TRANSFORM: TransformDefinition = TransformDefinition {
        id: "test-latest-only",
        display_name: "Test latest only",
        description: "Test-only blocking transform",
        accepts_binary: true,
        apply: block_latest_only,
    };

    static DROP_TRANSFORM: TransformDefinition = TransformDefinition {
        id: "test-drop",
        display_name: "Test drop",
        description: "Test-only blocking transform",
        accepts_binary: true,
        apply: block_during_drop,
    };

    fn blocking_job(
        generation: u64,
        input: &[u8],
        definition: &'static TransformDefinition,
    ) -> PreviewJob {
        PreviewJob {
            generation,
            input: input.to_vec(),
            steps: vec![TransformStep {
                definition,
                enabled: true,
            }],
        }
    }

    fn now() -> Instant {
        Instant::now()
    }

    fn rendered(width: u16, height: u16, focus: Pane) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.focus = focus;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers, now: Instant) -> Vec<Effect> {
        app.handle_event(AppEvent::Key(KeyEvent::new(code, modifiers), now))
    }

    #[test]
    fn wide_layout_shows_input_preview_and_chain() {
        let screen = rendered(120, 30, Pane::Input);
        let pane_starts = screen
            .chars()
            .take(120)
            .enumerate()
            .filter_map(|(index, character)| (character == '┌').then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            pane_starts,
            vec![0, 46, 91],
            "120 columns must split into 38/38/24 percent panes"
        );
        assert!(screen.contains("Input"));
        assert!(screen.contains("Preview"));
        assert!(screen.contains("Chain"));
    }

    #[test]
    fn narrow_layout_shows_one_focused_pane_and_text_tabs() {
        let screen = rendered(119, 30, Pane::Preview);
        assert!(screen.contains("[Preview]"));
        assert_eq!(screen.matches("Chain").count(), 1);
    }

    #[test]
    fn tiny_layout_only_shows_resize_guidance() {
        let screen = rendered(39, 9, Pane::Input);
        assert!(screen.contains("Increase terminal size"));
        assert!(!screen.contains("Ctrl+P"));
        assert!(!screen.contains("Input"));
    }

    #[test]
    fn no_color_uses_default_cell_styles_and_textual_state() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.open_picker();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(screen.contains("Idle"));
        assert!(screen.contains("0 enabled"));
        assert!(screen.contains("Add transform"));
        assert!(buffer.content().iter().all(|cell| {
            cell.fg == Color::Reset && cell.bg == Color::Reset && cell.modifier == Modifier::empty()
        }));
    }

    #[test]
    fn colored_render_uses_only_basic_colors_and_no_literal_ansi() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), false);
        app.open_picker();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        for cell in terminal.backend().buffer().content() {
            assert!(
                !matches!(cell.fg, Color::Indexed(_) | Color::Rgb(_, _, _))
                    && !matches!(cell.bg, Color::Indexed(_) | Color::Rgb(_, _, _))
            );
            assert!(!cell.symbol().contains('\u{1b}'));
        }
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
            generation: 1,
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
        app.preview = PreviewState::Running { generation: 1 };
        assert!(app.request_copy().is_empty());
        app.preview = PreviewState::Error {
            generation: 1,
            message: "failed".to_string(),
        };
        assert!(app.request_copy().is_empty());
        app.preview = PreviewState::Ready {
            generation: 1,
            document: PreviewDocument::new("safe".to_string()),
        };
        assert!(matches!(app.request_copy().as_slice(), [Effect::Copy(_)]));
    }

    #[test]
    fn confirmation_modal_keys_accept_or_cancel_explicit_actions() {
        let start = now();
        let mut app = App::new(start, true);
        app.preview = PreviewState::Ready {
            generation: 1,
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
    fn preview_escapes_only_visible_dangerous_controls() {
        let document = PreviewDocument::new("safe\u{1b}[2J\nnext".to_string());
        assert_eq!(visible_safe_text(&document, 0, 1, 12), "safe\\x1b[2J");
        assert_eq!(visible_safe_text(&document, 1, 1, 12), "next");
        assert_eq!(&*document.raw, "safe\u{1b}[2J\nnext");
    }

    #[test]
    fn preview_scroll_is_bounded_and_does_not_mutate_input_or_generation() {
        let start = now();
        let mut app = App::new(start, true);
        app.insert_paste("source", start);
        app.generation = 7;
        app.preview = PreviewState::Ready {
            generation: 7,
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
            generation: 1,
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
    fn external_status_and_error_text_cannot_reach_the_render_buffer_as_controls() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.handle_event(AppEvent::ClipboardFinished(Err(
            "clipboard\n\u{1b}[2J".to_string()
        )));
        app.preview = PreviewState::Error {
            generation: 1,
            message: "preview\n\u{1b}[2J".to_string(),
        };
        app.focus = Pane::Preview;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!screen.contains('\n'));
        assert!(!screen.contains('\u{1b}'));
        assert!(screen.contains("\\x0a\\x1b[2J"));
    }

    #[test]
    fn confirmation_modals_render_explicit_warning_text() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.modal = Some(Modal::UnsafeCopyConfirm);
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let unsafe_screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(unsafe_screen.contains("Copy raw control characters?"));

        app.modal = Some(Modal::QuitConfirm);
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let quit_screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(quit_screen.contains("Discard input and quit?"));
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
        assert!(matches!(
            app.preview,
            PreviewState::Running { generation: 1 }
        ));
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
            generation: 0,
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
        app.preview = PreviewState::Running { generation: 2 };
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            generation: 1,
            result: Ok(b"old".to_vec()),
        }));
        assert!(matches!(
            app.preview,
            PreviewState::Running { generation: 2 }
        ));
    }

    #[test]
    fn an_error_hides_previous_result_and_disables_copy() {
        let start = now();
        let mut app = App::new(start, true);
        app.generation = 2;
        app.preview = PreviewState::Ready {
            generation: 1,
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

    #[test]
    fn worker_returns_a_bounded_pipeline_result() {
        let worker = PreviewWorker::new();
        worker.submit(PreviewJob {
            generation: 7,
            input: b"foo".to_vec(),
            steps: vec![TransformStep {
                definition: transform_by_id("base64-encode").unwrap(),
                enabled: true,
            }],
        });
        let result = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.generation, 7);
        assert_eq!(result.result.unwrap(), b"Zm9v");
    }

    #[test]
    fn worker_runs_current_job_and_only_the_latest_pending_job() {
        let (started_sender, started) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        assert!(
            LATEST_ONLY_CONTROL
                .set(BlockingTransformControl {
                    started: started_sender,
                    release: Arc::clone(&release),
                })
                .is_ok()
        );
        let worker = PreviewWorker::new();

        worker.submit(blocking_job(1, b"first", &LATEST_ONLY_TRANSFORM));
        assert_eq!(
            started.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"first"
        );
        worker.submit(blocking_job(2, b"middle", &LATEST_ONLY_TRANSFORM));
        worker.submit(blocking_job(3, b"last", &LATEST_ONLY_TRANSFORM));
        release.wait();

        let first = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(first.result.unwrap(), b"first");
        assert_eq!(
            started.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"last"
        );
        release.wait();

        let last = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(last.generation, 3);
        assert_eq!(last.result.unwrap(), b"last");
        assert!(matches!(
            worker.results.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(matches!(started.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }

    #[test]
    fn dropping_running_worker_does_not_wait_and_worker_exits_after_release() {
        let (started_sender, started) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        assert!(
            DROP_CONTROL
                .set(BlockingTransformControl {
                    started: started_sender,
                    release: Arc::clone(&release),
                })
                .is_ok()
        );
        let mut worker = PreviewWorker::new();
        let (dummy_sender, dummy_results) = mpsc::channel();
        let results = std::mem::replace(&mut worker.results, dummy_results);
        drop(dummy_sender);

        worker.submit(blocking_job(1, b"running", &DROP_TRANSFORM));
        assert_eq!(
            started.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"running"
        );
        let (drop_sender, drop_returned) = mpsc::channel();
        thread::spawn(move || {
            drop(worker);
            drop_sender.send(()).expect("drop observer available");
        });

        let returned_before_release = drop_returned.recv_timeout(Duration::from_secs(1)).is_ok();
        release.wait();
        if !returned_before_release {
            drop_returned
                .recv_timeout(Duration::from_secs(1))
                .expect("drop returns after transform release");
        }
        assert!(returned_before_release);

        let result = results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.generation, 1);
        assert_eq!(result.result.unwrap(), b"running");
        assert!(matches!(
            results.recv_timeout(Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}
