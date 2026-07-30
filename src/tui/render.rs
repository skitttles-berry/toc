use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, Paragraph},
};

use crate::error::AppError;

use super::{
    state::{App, Modal, Pane, PreviewState},
    views::visible_safe_text,
};

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
        PreviewState::Running => "Running…".to_string(),
        PreviewState::Ready { document } => visible_safe_text(
            document,
            app.preview_scroll,
            inner.height as usize,
            inner.width as usize,
        ),
        PreviewState::Error { message } => crate::error::escape_external(
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
        PreviewState::Running => "Running",
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
    let help = format!(
        "{location} | {} | {enabled} enabled | Ctrl+P add | Ctrl+Q quit",
        preview_label(&app.preview)
    );
    let text = match &app.status {
        Some(message) if narrow => format!(
            "{} | {help}",
            crate::error::escape_external(message, area.width as usize)
        ),
        Some(message) => format!(
            "{help} | {}",
            crate::error::escape_external(message, area.width as usize)
        ),
        None => help,
    };
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

fn render(frame: &mut Frame<'_>, app: &mut App) {
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

pub(super) fn draw_if_dirty<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<bool, AppError> {
    if !app.take_dirty() {
        return Ok(false);
    }
    terminal
        .draw(|frame| render(frame, app))
        .map_err(|error| AppError::Tui(error.to_string()))?;
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::super::{
        state::{AppEvent, DEBOUNCE, Modal, Pane, PreviewState},
        worker::PreviewResult,
    };
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{
        backend::TestBackend,
        style::{Color, Modifier},
    };
    use std::time::{Duration, Instant};

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

    fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers, now: Instant) {
        app.handle_event(AppEvent::Key(KeyEvent::new(code, modifiers), now));
    }

    #[test]
    fn redraws_initial_and_changed_state_but_not_idle_polls() {
        let start = now();
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(start, true);

        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());
        assert!(!draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::Tick(start + Duration::from_millis(1)));
        assert!(!draw_if_dirty(&mut terminal, &mut app).unwrap());

        key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE, start);
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        key(&mut app, KeyCode::Tab, KeyModifiers::NONE, start);
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::Tick(start + DEBOUNCE));
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            generation: 1,
            result: Ok(b"x".to_vec()),
        }));
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::ClipboardFinished(Err(
            "Clipboard unavailable".to_string()
        )));
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::Resize);
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());
    }
    #[test]
    #[ignore = "release-only rendering measurement"]
    fn dirty_redraw_release_measurement() {
        const ITERATIONS: usize = 500;

        let baseline_backend = TestBackend::new(80, 20);
        let mut baseline_terminal = Terminal::new(baseline_backend).unwrap();
        let mut baseline_app = App::new(now(), true);
        let baseline_start = Instant::now();
        for _ in 0..ITERATIONS {
            baseline_terminal
                .draw(|frame| render(frame, &mut baseline_app))
                .unwrap();
        }
        let baseline = baseline_start.elapsed();

        let dirty_backend = TestBackend::new(80, 20);
        let mut dirty_terminal = Terminal::new(dirty_backend).unwrap();
        let mut dirty_app = App::new(now(), true);
        let dirty_start = Instant::now();
        let mut redraws = 0;
        for _ in 0..ITERATIONS {
            redraws += usize::from(draw_if_dirty(&mut dirty_terminal, &mut dirty_app).unwrap());
        }
        let dirty = dirty_start.elapsed();

        eprintln!(
            "dirty redraw release measurement: iterations={ITERATIONS}, unconditional={baseline:?}, dirty={dirty:?}, redraws={redraws}"
        );
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
    fn narrow_status_prioritizes_clipboard_failure_over_help() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.handle_event(AppEvent::ClipboardFinished(Err(
            "Clipboard unavailable".to_string()
        )));

        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let status: String = terminal.backend().buffer().content()[60 * 11..]
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(status.starts_with("Clipboard unavailable | [Input]"));
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
        assert_eq!(
            app.textarea.cursor_style(),
            Style::default().add_modifier(Modifier::REVERSED)
        );
        assert_eq!(
            app.textarea.selection_style(),
            Style::default().add_modifier(Modifier::REVERSED)
        );
        app.open_picker();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(screen.contains("Idle"));
        assert!(screen.contains("0 enabled"));
        assert!(screen.contains("Add transform"));
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| { cell.fg == Color::Reset && cell.bg == Color::Reset })
        );
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
    fn external_status_and_error_text_cannot_reach_the_render_buffer_as_controls() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.handle_event(AppEvent::ClipboardFinished(Err(
            "clipboard\n\u{1b}[2J".to_string()
        )));
        app.preview = PreviewState::Error {
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
}
