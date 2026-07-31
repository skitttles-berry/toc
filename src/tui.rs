use std::{
    io::{self, Write as _},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::error::AppError;

mod render;
mod state;
mod views;
mod worker;

use render::draw_if_dirty;
use state::{App, AppEvent, Effect};
use worker::PreviewWorker;

pub fn check_terminal_entry(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> Result<(), AppError> {
    if stdin_is_terminal && stdout_is_terminal {
        Ok(())
    } else {
        Err(AppError::Tui(
            "doop tui requires terminal stdin and stdout".to_string(),
        ))
    }
}

fn execute_tracked<W: io::Write, C: crossterm::Command>(
    writer: &mut W,
    active: &mut bool,
    command: C,
) -> io::Result<()> {
    *active = true;
    execute!(writer, command)
}

struct TerminalSession {
    raw: bool,
    alternate: bool,
    paste: bool,
    mouse: bool,
    cursor_hidden: bool,
}

impl TerminalSession {
    fn enter() -> Result<Self, AppError> {
        let mut session = Self {
            raw: false,
            alternate: false,
            paste: false,
            mouse: false,
            cursor_hidden: false,
        };
        enable_raw_mode().map_err(|error| AppError::Tui(error.to_string()))?;
        session.raw = true;

        let mut stdout = io::stdout();
        execute_tracked(&mut stdout, &mut session.alternate, EnterAlternateScreen)
            .map_err(|error| AppError::Tui(error.to_string()))?;
        execute_tracked(&mut stdout, &mut session.paste, EnableBracketedPaste)
            .map_err(|error| AppError::Tui(error.to_string()))?;
        execute_tracked(&mut stdout, &mut session.mouse, EnableMouseCapture)
            .map_err(|error| AppError::Tui(error.to_string()))?;
        execute_tracked(&mut stdout, &mut session.cursor_hidden, Hide)
            .map_err(|error| AppError::Tui(error.to_string()))?;
        Ok(session)
    }

    fn restore(&mut self) {
        let mut stdout = io::stdout();
        if self.cursor_hidden {
            let _ = execute!(stdout, Show);
            self.cursor_hidden = false;
        }
        if self.mouse {
            let _ = execute!(stdout, DisableMouseCapture);
            self.mouse = false;
        }
        if self.paste {
            let _ = execute!(stdout, DisableBracketedPaste);
            self.paste = false;
        }
        if self.alternate {
            let _ = execute!(stdout, LeaveAlternateScreen);
            self.alternate = false;
        }
        if self.raw {
            let _ = disable_raw_mode();
            self.raw = false;
        }
        let _ = stdout.flush();
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.restore();
    }
}

fn set_clipboard_text(
    clipboard: &mut Option<arboard::Clipboard>,
    text: String,
) -> Result<(), String> {
    if clipboard.is_none() {
        *clipboard =
            Some(arboard::Clipboard::new().map_err(|_| "Clipboard unavailable".to_string())?);
    }
    let Some(clipboard) = clipboard.as_mut() else {
        return Err("Clipboard unavailable".to_string());
    };
    clipboard
        .set_text(text)
        .map_err(|_| "Clipboard unavailable".to_string())
}

fn is_force_interrupt(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    worker: &PreviewWorker,
    clipboard: &mut Option<arboard::Clipboard>,
) -> Result<i32, AppError> {
    loop {
        draw_if_dirty(terminal, app)?;
        let mut effects = Vec::new();
        if crossterm::event::poll(Duration::from_millis(50))
            .map_err(|error| AppError::Tui(error.to_string()))?
        {
            let event =
                crossterm::event::read().map_err(|error| AppError::Tui(error.to_string()))?;
            match event {
                crossterm::event::Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if is_force_interrupt(&key) {
                        effects.extend(app.force_interrupt());
                    } else {
                        effects.extend(app.handle_event(AppEvent::Key(key, Instant::now())));
                    }
                }
                crossterm::event::Event::Paste(text) => {
                    effects.extend(app.handle_event(AppEvent::Paste(text, Instant::now())));
                }
                crossterm::event::Event::Mouse(mouse) => {
                    effects.extend(app.handle_event(AppEvent::Mouse(mouse, Instant::now())));
                }
                crossterm::event::Event::Resize(_, _) => {
                    effects.extend(app.handle_event(AppEvent::Resize));
                }
                _ => {}
            }
        }

        loop {
            match worker.try_recv() {
                Ok(result) => effects.extend(app.handle_event(AppEvent::PreviewFinished(result))),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(AppError::Tui(
                        "Preview worker stopped unexpectedly".to_string(),
                    ));
                }
            }
        }
        effects.extend(app.handle_event(AppEvent::Tick(Instant::now())));

        for effect in effects {
            match effect {
                Effect::Submit(job) => worker.submit(job),
                Effect::Cancel(request_id) => worker.cancel(request_id),
                Effect::Copy(payload) => {
                    let kind = payload.kind;
                    let result = set_clipboard_text(clipboard, payload.text);
                    let _ = app.handle_event(AppEvent::ClipboardFinished { kind, result });
                }
                Effect::Quit(code) => return Ok(code),
            }
        }
    }
}

fn best_effort_restore_terminal() {
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        Show,
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
    let _ = stdout.flush();
}

pub fn run() -> Result<i32, AppError> {
    let mut session = TerminalSession::enter()?;
    let ui_thread = thread::current().id();
    let previous_hook = Arc::new(Mutex::new(Some(std::panic::take_hook())));
    let hook_state = Arc::clone(&previous_hook);
    std::panic::set_hook(Box::new(move |information| {
        if thread::current().id() == ui_thread {
            best_effort_restore_terminal();
        } else if let Ok(hook) = hook_state.lock()
            && let Some(previous) = hook.as_ref()
        {
            previous(information);
        }
    }));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal =
            Terminal::new(backend).map_err(|error| AppError::Tui(error.to_string()))?;
        let mut app = App::new(Instant::now(), std::env::var_os("NO_COLOR").is_some());
        let worker = PreviewWorker::new();
        let mut clipboard = None;
        run_loop(&mut terminal, &mut app, &worker, &mut clipboard)
    }));

    session.restore();
    let _temporary_hook = std::panic::take_hook();
    if let Ok(mut hook) = previous_hook.lock()
        && let Some(previous) = hook.take()
    {
        std::panic::set_hook(previous);
    }

    match result {
        Ok(result) => result,
        Err(_) => Err(AppError::Tui("TUI stopped unexpectedly".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FlushFailWriter {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl std::io::Write for FlushFailWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Err(io::Error::other("flush failed"))
        }
    }

    #[test]
    fn tracked_command_marks_state_when_flush_fails_after_write() {
        let mut writer = FlushFailWriter::default();
        let mut active = false;

        let result = execute_tracked(&mut writer, &mut active, EnterAlternateScreen);

        assert!(result.is_err());
        assert_eq!(writer.bytes, b"\x1b[?1049h");
        assert_eq!(writer.flushes, 1);
        assert!(active);
    }

    #[test]
    fn tracked_mouse_capture_marks_state_when_flush_fails_after_write() {
        let mut writer = FlushFailWriter::default();
        let mut active = false;

        let result = execute_tracked(&mut writer, &mut active, EnableMouseCapture);

        assert!(result.is_err());
        assert_eq!(
            writer.bytes,
            b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h"
        );
        assert_eq!(writer.flushes, 1);
        assert!(active);
    }

    #[test]
    fn tui_requires_both_standard_streams_to_be_terminals() {
        assert!(check_terminal_entry(true, true).is_ok());
        let error = check_terminal_entry(false, true).unwrap_err();
        assert_eq!(
            crate::error::render_app_error(&error),
            "TUI error: doop tui requires terminal stdin and stdout"
        );
        assert!(check_terminal_entry(true, false).is_err());
    }

    #[test]
    fn force_interrupt_accepts_only_exact_ctrl_c() {
        assert!(is_force_interrupt(&crossterm::event::KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        for key in [
            crossterm::event::KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            crossterm::event::KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            crossterm::event::KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL),
            crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        ] {
            assert!(!is_force_interrupt(&key));
        }
    }

    #[test]
    fn clipboard_boundary_consumes_an_owned_string() {
        let _: fn(&mut Option<arboard::Clipboard>, String) -> Result<(), String> =
            set_clipboard_text;
    }
}
