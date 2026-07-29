use std::{
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use crossterm::event::KeyEvent;
use tui_textarea::TextArea;

use crate::{
    MAX_STEPS, TUI_INPUT_LIMIT, TUI_OUTPUT_LIMIT,
    error::PipelineError,
    pipeline::{TransformStep, execute},
    transforms::transform_by_id,
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
        Self {
            textarea: TextArea::default(),
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

    fn changed(&mut self, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.preview = PreviewState::Debouncing {
            deadline: now + DEBOUNCE,
        };
        self.preview_scroll = 0;
    }

    pub fn insert_paste(&mut self, text: &str, now: Instant) -> bool {
        let remaining = self.input_limit.saturating_sub(self.input_len());
        if text.len() > remaining.saturating_mul(2) {
            self.status = Some("Input limit reached".to_string());
            return false;
        }
        let (normalized, replaced) = normalize_paste(text);
        if normalized.len() > remaining {
            self.status = Some("Input limit reached".to_string());
            return false;
        }
        let modified = self.textarea.insert_str(normalized);
        if modified && self.input_len() > self.input_limit {
            self.textarea.undo();
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
                    Err(message) => message,
                });
                Vec::new()
            }
            AppEvent::Key(key, now) if self.modal.is_none() && self.focus == Pane::Input => {
                if self.textarea.input(key) {
                    if self.input_len() > self.input_limit {
                        self.textarea.undo();
                        self.status = Some("Input limit reached".to_string());
                    } else {
                        self.changed(now);
                    }
                }
                Vec::new()
            }
            AppEvent::Key(_, _) => Vec::new(),
        }
    }
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

    fn now() -> Instant {
        Instant::now()
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
}
