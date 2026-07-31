use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::{error::AppError, pipeline::StepStatus};

use super::{
    state::{App, Modal, OutputSource, OutputStatus, Pane},
    views::{
        EffectiveView, TEXT_VIEW_UNAVAILABLE_MESSAGE, ViewMode, effective_view, render_hex_window,
        render_pipeline_error_summary, render_text_window, render_trace_window,
        render_transform_error_summary, with_bounded_header,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WidthMode {
    Wide,
    Medium,
    Narrow,
    Tiny,
}

fn width_mode(area: Rect) -> WidthMode {
    if area.width < 40 || area.height < 10 {
        WidthMode::Tiny
    } else if area.width >= 120 {
        WidthMode::Wide
    } else if area.width >= 90 {
        WidthMode::Medium
    } else {
        WidthMode::Narrow
    }
}

fn pipeline_width(width: u16, mode: WidthMode) -> u16 {
    let proportional = width.saturating_mul(30) / 100;
    match mode {
        WidthMode::Wide => proportional.clamp(28, 42),
        WidthMode::Medium => proportional.clamp(28, 32),
        WidthMode::Narrow | WidthMode::Tiny => 0,
    }
}

fn pane_style(app: &App, focused: bool) -> Style {
    if app.no_color {
        Style::default()
    } else if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Green)
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
        .border_type(BorderType::Thick)
        .title(title)
        .border_style(style)
        .title_style(style)
}

fn stacked_pane_heights(height: u16) -> [u16; 3] {
    let remaining = height.saturating_sub(9);
    let pipeline = 3 + remaining.saturating_mul(3) / 10;
    let input = 3 + remaining.saturating_mul(3) / 10;
    let output = height.saturating_sub(pipeline).saturating_sub(input);
    [pipeline, input, output]
}

fn source_label(source: OutputSource) -> String {
    match source {
        OutputSource::Final => "FINAL".to_string(),
        OutputSource::Step(index) => format!("STEP {:02}", index + 1),
    }
}

fn render_input(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let block = pane_block(app, "> INPUT", app.focus == Pane::Input);
    app.textarea.set_block(block);
    frame.render_widget(&app.textarea, area);
}

fn render_output(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let source = source_label(app.output.source);
    let view = match app.output.view {
        ViewMode::Smart => "Smart",
        ViewMode::Text => "Text",
        ViewMode::Hex => "Hex",
        ViewMode::Trace => "Trace",
    };
    let title = format!("» OUTPUT / {source} / {}", view.to_ascii_uppercase());
    let block = pane_block(app, &title, app.focus == Pane::Output);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = inner.height as usize;
    let columns = inner.width as usize;
    let text = match &app.output.status {
        OutputStatus::Idle => String::new(),
        OutputStatus::Debouncing { .. } => "Waiting for changes…".to_string(),
        OutputStatus::Running => "Running…".to_string(),
        OutputStatus::Cancelled if app.output.traces.is_empty() => "Cancelled".to_string(),
        OutputStatus::Failed(error)
            if matches!(app.output.view, ViewMode::Text | ViewMode::Hex) =>
        {
            format!(
                "{}\nSwitch to Trace view",
                crate::error::escape_external(
                    &render_pipeline_error_summary(error),
                    columns.saturating_mul(rows).min(512),
                )
            )
        }
        status => match effective_view(
            app.output.view,
            app.output.active_artifact.as_ref(),
            matches!(status, OutputStatus::Failed(_)),
        ) {
            EffectiveView::Text => app
                .output
                .active_artifact
                .as_ref()
                .map(|artifact| {
                    render_text_window(artifact, app.output.byte_offset, rows, columns).text
                })
                .unwrap_or_default(),
            EffectiveView::Hex => {
                let body = app
                    .output
                    .active_artifact
                    .as_ref()
                    .map(|artifact| {
                        render_hex_window(artifact, app.output.row_offset, rows.saturating_sub(1))
                    })
                    .unwrap_or_default();
                with_bounded_header(
                    "OFFSET    HEX BYTES                                      ASCII",
                    body,
                )
            }
            EffectiveView::Trace => {
                let body = if app.output.traces.is_empty() {
                    match status {
                        OutputStatus::Failed(error) => crate::error::escape_external(
                            &render_pipeline_error_summary(error),
                            columns.saturating_mul(rows).min(512),
                        ),
                        _ => String::new(),
                    }
                } else {
                    render_trace_window(
                        &app.output.traces,
                        app.output.row_offset,
                        rows.saturating_sub(1),
                        columns,
                    )
                };
                with_bounded_header("STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS", body)
            }
            EffectiveView::Unavailable => TEXT_VIEW_UNAVAILABLE_MESSAGE.to_string(),
        },
    };
    frame.render_widget(Paragraph::new(text).style(Style::default()), inner);
}

fn render_pipeline(frame: &mut Frame<'_>, app: &App, area: Rect, show_sizes: bool) {
    let block = pane_block(app, "$ PIPELINE", app.focus == Pane::Pipeline);
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
            let enabled = if step.enabled { "ON" } else { "OFF" };
            let trace = app
                .output
                .traces
                .iter()
                .find(|trace| trace.step == index + 1);
            let status = if let Some(trace) = trace {
                trace.status
            } else if !step.enabled {
                StepStatus::Disabled
            } else if matches!(app.output.status, OutputStatus::Running)
                && matches!(app.output.source, OutputSource::Step(target) if target == index)
            {
                let text = format!("{prefix} [{enabled}]  › {}", step.definition.display_name);
                return ListItem::new(Span::styled(
                    text,
                    if selected {
                        selection_style(app)
                    } else if app.no_color {
                        Style::default()
                    } else {
                        Style::default().fg(Color::Yellow)
                    },
                ));
            } else {
                StepStatus::NotExecuted
            };
            let (mark, color) = match status {
                StepStatus::Succeeded => ("✓ ", Color::Green),
                StepStatus::Disabled => (" ", Color::DarkGray),
                StepStatus::Failed => ("× ", Color::Red),
                StepStatus::NotExecuted => ("· ", Color::DarkGray),
                StepStatus::Cancelled => ("− ", Color::Yellow),
            };
            let sizes = if show_sizes {
                trace
                    .and_then(|trace| Some((trace.input_bytes?, trace.output_bytes?)))
                    .map(|(input, output)| format!(" {input}B→{output}B"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let text = format!(
                "{prefix} [{enabled}]  {mark}{}{sizes}",
                step.definition.display_name
            );
            ListItem::new(Span::styled(
                text,
                if selected {
                    selection_style(app)
                } else if app.no_color {
                    Style::default()
                } else {
                    Style::default().fg(color)
                },
            ))
        })
        .collect();
    frame.render_widget(List::new(items).style(Style::default()), inner);
}

fn pane_label(pane: Pane) -> &'static str {
    match pane {
        Pane::Input => "Input",
        Pane::Output => "Output",
        Pane::Pipeline => "Pipeline",
    }
}

fn render_app_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let title_style = if app.no_color {
        Style::default()
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };
    let focus_style = if app.no_color {
        Style::default()
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    };
    let line = Line::from(vec![
        Span::styled(">_ DOOP", title_style),
        Span::raw("  │  FOCUS: "),
        Span::styled(pane_label(app.focus).to_ascii_uppercase(), focus_style),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn focused_help(app: &App) -> &'static str {
    match app.focus {
        Pane::Input => "[INPUT] Text editing · Esc Cancel",
        Pane::Pipeline => "[PIPELINE] ↑/↓ Select · Shift+↑/↓ Move · Space Toggle · Enter Inspect",
        Pane::Output if app.can_copy() => {
            "[OUTPUT] Enter Pretty · v/V View · p Step · f Final · z Zoom"
        }
        Pane::Output => "[OUTPUT] v/V View · p Step · f Final · z Zoom",
    }
}

fn common_help() -> &'static str {
    "[COMMON] Tab Focus · F3 Pretty · F4 Raw · Ctrl+P Add · F1 Help · Ctrl+Q Quit"
}

fn footer_first_line(app: &App, width: usize) -> String {
    match &app.output.status {
        OutputStatus::Failed(error) => {
            crate::error::escape_external(&render_pipeline_error_summary(error), width)
        }
        OutputStatus::Cancelled => "Cancelled".to_string(),
        _ => app
            .status
            .as_ref()
            .map(|status| crate::error::escape_external(status, width))
            .unwrap_or_else(|| focused_help(app).to_string()),
    }
}

fn render_footer(
    frame: &mut Frame<'_>,
    app: &App,
    focused_help_area: Rect,
    common_help_area: Rect,
) {
    frame.render_widget(
        Paragraph::new(footer_first_line(app, focused_help_area.width as usize)),
        focused_help_area,
    );
    frame.render_widget(Paragraph::new(common_help()), common_help_area);
}

fn render_focused_pane(
    frame: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    pane: Pane,
    mode: WidthMode,
) {
    match pane {
        Pane::Input => render_input(frame, app, area),
        Pane::Output => render_output(frame, app, area),
        Pane::Pipeline => render_pipeline(frame, app, area, mode == WidthMode::Wide),
    }
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

fn input_condition(accepts_binary: bool) -> &'static str {
    if accepts_binary {
        "Bytes accepted"
    } else {
        "Text input"
    }
}

fn separator(width: u16) -> String {
    "─".repeat(width as usize)
}

fn render_picker(frame: &mut Frame<'_>, app: &App, query: &str, selected: usize) {
    let area = centered(frame.area(), 72, 18);
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    };
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .title("Add transform")
        .style(Style::default())
        .border_style(style)
        .title_style(style);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let compact = inner.height < 14;
    let detail_rows = if compact { 0 } else { 4 };
    let detail_separator_rows = if compact { 0 } else { 1 };
    let [
        query_area,
        list_area,
        detail_separator,
        detail_area,
        hint_separator,
        hint_area,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(2),
        Constraint::Length(detail_separator_rows),
        Constraint::Length(detail_rows),
        Constraint::Length(1),
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
    let detail = filtered.get(selected).map_or_else(
        || "No matching transforms".to_string(),
        |transform| {
            format!(
                "INPUT     {}\nBEHAVIOR  {}\nTUI       Result remains bytes; Smart selects Text or Hex",
                input_condition(transform.accepts_binary),
                transform.behavior,
            )
        },
    );
    let available = (list_area.height as usize / 2).max(1);
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
            let is_selected = index == selected;
            let prefix = if is_selected { "> " } else { "  " };
            let text = format!(
                "{prefix}{}  [{}]\n  {}",
                transform.display_name, transform.id, transform.description,
            );
            ListItem::new(text).style(if is_selected {
                selection_style(app)
            } else {
                Style::default()
            })
        })
        .collect();
    frame.render_widget(List::new(items).style(Style::default()), list_area);
    if !compact {
        frame.render_widget(
            Paragraph::new(separator(detail_separator.width)).style(Style::default()),
            detail_separator,
        );
        frame.render_widget(
            Paragraph::new(detail)
                .style(Style::default())
                .wrap(Wrap { trim: false }),
            detail_area,
        );
    }
    frame.render_widget(
        Paragraph::new(separator(hint_separator.width)).style(Style::default()),
        hint_separator,
    );
    frame.render_widget(
        Paragraph::new(if compact {
            "Enter Add · Esc Cancel"
        } else {
            "↑/↓ Select · Enter Add · Backspace Search · Esc Cancel"
        })
        .style(Style::default()),
        hint_area,
    );
}

fn step_status(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Succeeded => "OK",
        StepStatus::Disabled => "OFF",
        StepStatus::Failed => "ERROR",
        StepStatus::NotExecuted => "NOT RUN",
        StepStatus::Cancelled => "CANCELLED",
    }
}

fn render_inspector(frame: &mut Frame<'_>, app: &App) {
    let Some(step) = app.steps.get(app.selected_step) else {
        return;
    };
    let area = centered(frame.area(), 78, 13);
    let trace = app
        .output
        .traces
        .iter()
        .find(|trace| trace.step == app.selected_step + 1);
    let status = if let Some(trace) = trace {
        step_status(trace.status)
    } else if !step.enabled {
        "OFF"
    } else if matches!(app.output.status, OutputStatus::Running)
        && app.output.source == OutputSource::Step(app.selected_step)
    {
        "RUNNING"
    } else {
        "NOT RUN"
    };
    let input = trace
        .and_then(|trace| trace.input_bytes)
        .map_or_else(|| "—".to_string(), |bytes| format!("{bytes} B"));
    let output = trace
        .and_then(|trace| trace.output_bytes)
        .map_or_else(|| "—".to_string(), |bytes| format!("{bytes} B"));
    let elapsed = trace.and_then(|trace| trace.elapsed).map_or_else(
        || "—".to_string(),
        |elapsed| format!("{} µs", elapsed.as_micros()),
    );
    let error = trace
        .and_then(|trace| trace.error.as_ref())
        .map(|error| {
            render_pipeline_error_summary(&crate::error::PipelineError::Step {
                step: app.selected_step + 1,
                transform_id: step.definition.id,
                source: error.clone(),
            })
        })
        .unwrap_or_else(|| "—".to_string());
    let compact_error = trace
        .and_then(|trace| trace.error.as_ref())
        .map(render_transform_error_summary)
        .unwrap_or_else(|| "—".to_string());
    let compact = area.height <= 8;
    let text = if compact {
        format!(
            "{} ({})\nStatus: {status}\nOutput: {output}\nError: {compact_error}\nEsc close",
            step.definition.display_name, step.definition.id,
        )
    } else {
        format!(
            "{}\nID: {}\n{}\nStatus: {status}\nInput: {input}\nOutput: {output}\nElapsed: {elapsed}\nError: {error}\n\nEsc close",
            step.definition.display_name,
            step.definition.id,
            input_condition(step.definition.accepts_binary),
        )
    };
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    };
    let block = Block::bordered()
        .title("Step Inspector")
        .style(Style::default())
        .border_style(style)
        .title_style(style);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(text).style(Style::default()), inner);
}

fn render_help(frame: &mut Frame<'_>, app: &App) {
    let (title, body) = match app.focus {
        Pane::Input => (
            "Input Help",
            "Text editing: tui-textarea defaults\nTab / Shift+Tab  Next / previous pane\nCtrl+P  Add transform · F3/F4 Pretty/Raw Copy\nF1  Context help\nCtrl+Q  Quit\nCtrl+C  Force quit\nEsc  Close zoom or cancel request".to_string(),
        ),
        Pane::Pipeline => (
            "Pipeline Help",
            "Up/Down or j/k  Select step\nShift+Up/Down or J/K  Reorder\nSpace  Toggle step\nDelete or d  Delete step\nEnter  Inspect step\na / Ctrl+P  Add transform · F3/F4 Pretty/Raw Copy\nz  Toggle zoom\nTab / Shift+Tab  Change pane\n? / F1  Context help\nCtrl+Q  Quit\nCtrl+C  Force quit".to_string(),
        ),
        Pane::Output => (
            "Output Help",
            format!(
                "v/V  Next / previous view\np  Show selected step\nf  Restore final\nEnter/y  {} · F3/F4 Pretty/Raw Copy\nArrows / PageUp / PageDown / Home / End  Scroll\nz  Toggle zoom\nTab / Shift+Tab  Change pane\nCtrl+P  Add transform\n? / F1  Context help\nCtrl+Q  Quit\nCtrl+C  Force quit",
                if app.can_copy() {
                    "Copy whole result"
                } else {
                    "Copy unavailable"
                }
            ),
        ),
    };
    let area = centered(frame.area(), 68, 16);
    let compact = area.height <= 8;
    let body = if compact {
        match app.focus {
            Pane::Input => "Text edit · Tab focus\nF3/F4 Pretty/Raw\nCtrl+P Add · F1 Help\nCtrl+Q Quit · Ctrl+C Force\nEsc close".to_string(),
            Pane::Pipeline => "j/k Select · J/K Move\nSpace Toggle · d Delete\nF3/F4 Pretty/Raw\nEnter Inspect · a Add · z Zoom\nEsc close".to_string(),
            Pane::Output => format!(
                "v/V View · p Step · f Final\nF3/F4 Pretty/Raw\nEnter/y {}\nArrows/Page Scroll · z Zoom\nEsc close",
                if app.can_copy() {
                    "Copy"
                } else {
                    "Copy unavailable"
                }
            ),
        }
    } else {
        format!("{body}\n\nEsc close")
    };
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    };
    let block = Block::bordered()
        .title(title)
        .style(Style::default())
        .border_style(style)
        .title_style(style);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(body).style(Style::default()), inner);
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
        Some(Modal::StepInspector) => render_inspector(frame, app),
        Some(Modal::Help) => render_help(frame, app),
        Some(Modal::UnsafeCopyConfirm { .. }) => {
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
    let mode = width_mode(area);
    if mode == WidthMode::Tiny {
        frame.render_widget(
            Paragraph::new("Increase terminal size to at least 40×10").alignment(Alignment::Center),
            area,
        );
        if matches!(
            app.modal.as_ref(),
            Some(Modal::UnsafeCopyConfirm { .. }) | Some(Modal::QuitConfirm)
        ) {
            render_modal(frame, app);
        }
        return;
    }

    let [app_bar, content, focused_help, common_help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    render_app_bar(frame, app, app_bar);

    let focused = app.zoom.unwrap_or(app.focus);
    if area.height < 12 || app.zoom.is_some() {
        render_focused_pane(frame, app, content, focused, mode);
    } else if mode == WidthMode::Narrow {
        let [pipeline_rows, input_rows, output_rows] = stacked_pane_heights(content.height);
        let [pipeline, input, output] = Layout::vertical([
            Constraint::Length(pipeline_rows),
            Constraint::Length(input_rows),
            Constraint::Length(output_rows),
        ])
        .areas(content);
        render_pipeline(frame, app, pipeline, false);
        render_input(frame, app, input);
        render_output(frame, app, output);
    } else {
        let pipeline_columns = pipeline_width(area.width, mode);
        let [pipeline, right] =
            Layout::horizontal([Constraint::Length(pipeline_columns), Constraint::Min(0)])
                .areas(content);
        let input_rows = (u32::from(right.height) * 42 / 100) as u16;
        let input_rows = input_rows.clamp(3, right.height.saturating_sub(3));
        let [input, output] =
            Layout::vertical([Constraint::Length(input_rows), Constraint::Min(3)]).areas(right);
        render_pipeline(frame, app, pipeline, mode == WidthMode::Wide);
        render_input(frame, app, input);
        render_output(frame, app, output);
    }
    render_footer(frame, app, focused_help, common_help);
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
        state::{
            AppEvent, ClipboardPayload, CopyKind, Modal, OutputSource, OutputStatus, Pane,
            debounce_for,
        },
        views::{Artifact, ViewMode},
        worker::PreviewResult,
    };
    use super::*;
    use crate::{
        error::{PipelineError, TransformError},
        pipeline::{
            ExecutionOutcome, ExecutionReport, ExecutionTarget, StepStatus, StepTrace,
            TransformStep,
        },
        transforms::transform_by_id,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{
        backend::TestBackend,
        style::{Color, Modifier},
    };
    use std::time::{Duration, Instant};

    fn now() -> Instant {
        Instant::now()
    }

    fn rendered_lines(width: u16, height: u16, focus: Pane) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.focus = focus;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    fn rendered(width: u16, height: u16, focus: Pane) -> String {
        rendered_lines(width, height, focus).join("\n")
    }

    fn rendered_app(width: u16, height: u16, app: &mut App) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers, now: Instant) {
        app.handle_event(AppEvent::Key(KeyEvent::new(code, modifiers), now));
    }

    #[test]
    fn width_boundaries_choose_four_deterministic_modes() {
        for (width, expected) in [
            (120, WidthMode::Wide),
            (119, WidthMode::Medium),
            (90, WidthMode::Medium),
            (89, WidthMode::Narrow),
            (40, WidthMode::Narrow),
            (39, WidthMode::Tiny),
        ] {
            assert_eq!(width_mode(Rect::new(0, 0, width, 16)), expected);
        }
        assert_eq!(width_mode(Rect::new(0, 0, 120, 9)), WidthMode::Tiny);
    }

    #[test]
    fn pipeline_width_respects_wide_and_medium_bounds() {
        assert_eq!(pipeline_width(120, WidthMode::Wide), 36);
        assert_eq!(pipeline_width(200, WidthMode::Wide), 42);
        assert_eq!(pipeline_width(90, WidthMode::Medium), 28);
        assert_eq!(pipeline_width(119, WidthMode::Medium), 32);
        assert_eq!(pipeline_width(89, WidthMode::Narrow), 0);
        assert_eq!(pipeline_width(39, WidthMode::Tiny), 0);
    }

    #[test]
    fn stacked_pane_heights_keep_minimums_and_give_remainder_to_output() {
        assert_eq!(stacked_pane_heights(9), [3, 3, 3]);
        assert_eq!(stacked_pane_heights(13), [4, 4, 5]);
        assert_eq!(stacked_pane_heights(25), [7, 7, 11]);
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

        app.handle_event(AppEvent::Tick(start + debounce_for(1)));
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            report: ExecutionReport {
                request_id: 1,
                target: ExecutionTarget::Final,
                outcome: ExecutionOutcome::Success(b"x".to_vec()),
                traces: Vec::new(),
            },
        }));
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::ClipboardFinished {
            kind: CopyKind::Pretty,
            result: Err("Clipboard unavailable".to_string()),
        });
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());

        app.handle_event(AppEvent::Resize);
        assert!(draw_if_dirty(&mut terminal, &mut app).unwrap());
    }

    fn measure_dirty_preview_render(iterations: usize) -> (Duration, Duration) {
        let baseline_backend = TestBackend::new(80, 20);
        let mut baseline_terminal = Terminal::new(baseline_backend).unwrap();
        let mut baseline_app = App::new(now(), true);
        let baseline_start = Instant::now();
        for _ in 0..iterations {
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
        for _ in 0..iterations {
            redraws += usize::from(draw_if_dirty(&mut dirty_terminal, &mut dirty_app).unwrap());
        }
        let dirty = dirty_start.elapsed();
        assert_eq!(redraws, 1);

        (baseline, dirty)
    }

    fn measure_dirty_preview_render_samples(
        warmups: usize,
        samples: usize,
        iterations: usize,
    ) -> Vec<(Duration, Duration)> {
        for _ in 0..warmups {
            std::hint::black_box(measure_dirty_preview_render(iterations));
        }
        (0..samples)
            .map(|_| measure_dirty_preview_render(iterations))
            .collect()
    }

    #[test]
    fn dirty_preview_render_measurement_collects_requested_samples() {
        let samples = measure_dirty_preview_render_samples(1, 3, 2);

        assert_eq!(samples.len(), 3);
    }

    #[test]
    #[ignore = "release-only rendering measurement"]
    fn dirty_redraw_release_measurement() {
        const WARMUPS: usize = 5;
        const SAMPLES: usize = 30;
        const ITERATIONS: usize = 500;

        let samples = measure_dirty_preview_render_samples(WARMUPS, SAMPLES, ITERATIONS);
        let mut unconditional = samples.iter().map(|sample| sample.0).collect::<Vec<_>>();
        unconditional.sort_unstable();
        let mut dirty = samples.iter().map(|sample| sample.1).collect::<Vec<_>>();
        dirty.sort_unstable();

        eprintln!(
            "dirty redraw release measurement: warmups={WARMUPS}, samples={SAMPLES}, iterations={ITERATIONS}, unconditional_min={:?}, unconditional_median={:?}, unconditional_max={:?}, dirty_min={:?}, dirty_median={:?}, dirty_max={:?}, redraws=1",
            unconditional[0],
            unconditional[SAMPLES / 2],
            unconditional[SAMPLES - 1],
            dirty[0],
            dirty[SAMPLES / 2],
            dirty[SAMPLES - 1]
        );
    }

    #[test]
    fn wide_and_medium_layouts_put_bounded_pipeline_left_of_split_content() {
        for (width, expected_pipeline_width) in [(120, 36), (119, 32), (90, 28)] {
            let lines = rendered_lines(width, 16, Pane::Input);
            let pane_starts = lines[1]
                .chars()
                .enumerate()
                .filter_map(|(index, character)| (character == '┏').then_some(index))
                .collect::<Vec<_>>();

            assert_eq!(pane_starts, vec![0, expected_pipeline_width as usize]);
            assert!(lines[1].contains("$ PIPELINE"));
            assert!(lines[1].contains("> INPUT"));
            assert!(lines[6].contains("» OUTPUT"));
        }
    }

    #[test]
    fn narrow_layout_stacks_pipeline_input_and_output_in_order() {
        for width in [89, 40] {
            let lines = rendered_lines(width, 16, Pane::Output);
            let pipeline = lines
                .iter()
                .position(|line| line.contains("$ PIPELINE"))
                .unwrap();
            let input = lines
                .iter()
                .position(|line| line.contains("> INPUT"))
                .unwrap();
            let output = lines
                .iter()
                .position(|line| line.contains("» OUTPUT"))
                .unwrap();

            assert!(pipeline < input && input < output);
            assert!(lines[0].contains(">_ DOOP"));
            assert!(lines[0].contains("FOCUS: OUTPUT"));
            assert!(lines[14].contains("[OUTPUT]"));
            assert!(lines[15].contains("[COMMON]"));
        }
    }

    #[test]
    fn tiny_width_or_height_shows_only_resize_guidance() {
        for (width, height) in [(39, 16), (120, 9)] {
            let screen = rendered(width, height, Pane::Input);
            assert!(screen.contains("Increase terminal size"));
            assert!(!screen.contains("doop"));
            assert!(!screen.contains("Input"));
            assert!(!screen.contains("Ctrl+P"));
        }
    }

    #[test]
    fn ten_and_eleven_rows_show_only_the_focused_pane() {
        for height in [10, 11] {
            let screen = rendered(120, height, Pane::Output);
            assert!(screen.contains("» OUTPUT"));
            assert!(!screen.contains("$ PIPELINE"));
            assert!(!screen.contains("> INPUT"));
        }
    }

    #[test]
    fn app_bar_is_unboxed_and_footer_has_exactly_two_roles() {
        let lines = rendered_lines(120, 16, Pane::Output);
        assert!(lines[0].starts_with(">_ DOOP"));
        assert!(lines[0].contains("│  FOCUS: OUTPUT"));
        assert!(!lines[0].contains('┏'));
        assert!(lines[14].starts_with("[OUTPUT]"));
        assert!(lines[15].starts_with("[COMMON]"));
        assert!(!lines.iter().any(|line| line.contains("Navigation")));
        assert!(!lines.iter().any(|line| line.contains("Step Summary")));
    }

    #[test]
    fn status_replaces_only_the_focused_help_line() {
        let mut app = App::new(now(), true);
        app.handle_event(AppEvent::ClipboardFinished {
            kind: CopyKind::Pretty,
            result: Err("Clipboard unavailable".to_string()),
        });
        let screen = rendered_app(80, 16, &mut app);
        let lines: Vec<_> = screen.lines().collect();

        assert!(lines[14].contains("Clipboard unavailable"));
        assert!(!lines[14].contains("[INPUT]"));
        assert!(lines[15].starts_with("[COMMON]"));
        assert!(lines[15].contains("F3 Pretty"));
        assert!(lines[15].contains("F4 Raw"));
    }

    #[test]
    fn twelve_row_layout_keeps_both_right_panes_at_three_bordered_rows() {
        let lines = rendered_lines(120, 12, Pane::Input);
        assert!(lines[1].contains("> INPUT"));
        assert!(lines[4].contains("» OUTPUT"));
        assert!(lines[3].contains('┛'));
        assert!(lines[9].contains('┛'));
    }

    #[test]
    fn large_height_keeps_the_right_split_at_forty_two_percent() {
        let lines = rendered_lines(120, 2_004, Pane::Input);

        assert!(lines[1].contains("> INPUT"));
        assert!(lines[841].contains("» OUTPUT"));
    }

    #[test]
    fn output_titles_name_source_and_configured_view_for_text_hex_and_trace() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"valid text".to_vec()));
        let text = rendered_app(89, 20, &mut app);
        assert!(text.contains("» OUTPUT / FINAL / SMART"));
        assert!(text.contains("valid text"));

        app.output.source = OutputSource::Step(1);
        app.output.active_artifact = Some(Artifact::new(vec![0, 0xff]));
        let hex = rendered_app(89, 20, &mut app);
        assert!(hex.contains("» OUTPUT / STEP 02 / SMART"));
        assert!(hex.contains("OFFSET"));
        assert!(hex.contains("ASCII"));

        app.output.view = ViewMode::Trace;
        app.output.traces = vec![StepTrace {
            step: 2,
            transform_id: "hex-decode",
            input_bytes: Some(4),
            output_bytes: Some(2),
            elapsed: Some(Duration::from_millis(1)),
            status: StepStatus::Succeeded,
            error: None,
        }];
        let trace = rendered_app(89, 20, &mut app);
        assert!(trace.contains("» OUTPUT / STEP 02 / TRACE"));
        assert!(trace.contains("STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS"));
        assert!(trace.contains("#2 hex-decode OK"));
    }

    #[test]
    fn add_transform_separates_item_description_detail_and_key_help() {
        let mut app = App::new(now(), true);
        app.open_picker();

        let screen = rendered_app(80, 20, &mut app);
        assert!(screen.contains("> Base64 Encode  [base64-encode]"));
        assert!(screen.contains("  Encode bytes using padded RFC 4648 Base64"));
        assert!(screen.contains("INPUT"));
        assert!(screen.contains("BEHAVIOR"));
        assert!(screen.contains("TUI"));
        assert!(
            screen
                .lines()
                .filter(|line| line.contains("────────"))
                .count()
                >= 2
        );
        assert!(screen.contains("Enter Add"));
        assert!(screen.contains("Esc Cancel"));
        assert!(!screen.contains("CLI description"));
        assert!(!screen.contains("CLI behavior"));
    }

    #[test]
    fn inspector_is_read_only_and_shows_only_safe_trace_metadata() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("base64-decode").unwrap(),
            enabled: true,
        });
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "base64-decode",
            input_bytes: Some(4),
            output_bytes: None,
            elapsed: Some(Duration::from_micros(6)),
            status: StepStatus::Failed,
            error: Some(TransformError::InvalidUtf8Output {
                preview_hex: "736563726574".to_string(),
                total_bytes: 6,
            }),
        });

        key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);
        let screen = rendered_app(80, 20, &mut app);

        for expected in [
            "Step Inspector",
            "Base64 Decode",
            "base64-decode",
            "Text input",
            "ERROR",
            "Input: 4 B",
            "Output: —",
            "Elapsed: 6 µs",
            "output is not valid UTF-8 (6 bytes)",
        ] {
            assert!(screen.contains(expected), "missing {expected}: {screen}");
        }
        assert!(!screen.contains("736563726574"));
        assert!(!screen.contains("secret"));
        assert!(!screen.contains("Option"));

        app.output.status = OutputStatus::Running;
        app.output.source = OutputSource::Step(0);
        app.output.traces.clear();
        let running = rendered_app(80, 20, &mut app);
        assert!(running.contains("Status: RUNNING"));
    }

    #[test]
    fn one_context_help_modal_lists_only_real_keys_for_each_pane() {
        let start = now();
        for (pane, expected) in [
            (
                Pane::Input,
                &[
                    "Input Help",
                    "Tab",
                    "Ctrl+P",
                    "F1",
                    "Ctrl+Q",
                    "Ctrl+C",
                    "Esc",
                ][..],
            ),
            (
                Pane::Pipeline,
                &[
                    "Pipeline Help",
                    "j/k",
                    "J/K",
                    "Enter",
                    "a",
                    "z",
                    "? / F1",
                    "Ctrl+P",
                    "Ctrl+C",
                ][..],
            ),
            (
                Pane::Output,
                &[
                    "Output Help",
                    "v/V",
                    "p",
                    "f",
                    "Enter/y",
                    "z",
                    "? / F1",
                    "Ctrl+P",
                    "Ctrl+C",
                ][..],
            ),
        ] {
            let mut app = App::new(start, true);
            app.focus = pane;
            key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);
            let screen = rendered_app(80, 20, &mut app);
            assert!(screen.contains("Help"));
            for key_name in expected {
                assert!(
                    screen.contains(key_name),
                    "missing {key_name} for {pane:?}: {screen}"
                );
            }
            assert!(screen.contains("F1"));
            assert!(screen.contains("Esc close"));
        }
    }

    #[test]
    fn compact_add_transform_keeps_selected_description_and_close_keys() {
        let mut app = App::new(now(), true);
        app.open_picker();

        let screen = rendered_app(40, 10, &mut app);

        assert!(screen.contains("Search:"));
        assert!(screen.contains("Base64 Encode"));
        assert!(screen.contains("Encode bytes"));
        assert!(screen.contains("Enter Add"));
        assert!(screen.contains("Esc Cancel"));
    }

    #[test]
    fn forty_by_ten_inspector_keeps_status_output_error_and_close_visible() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("base64-decode").unwrap(),
            enabled: true,
        });
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "base64-decode",
            input_bytes: Some(4),
            output_bytes: None,
            elapsed: None,
            status: StepStatus::Failed,
            error: Some(TransformError::InvalidBase64 { position: Some(2) }),
        });
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);

        let screen = rendered_app(40, 10, &mut app);

        for expected in [
            "Status: ERROR",
            "Output: —",
            "Error: invalid Base64 at byte 2",
            "Esc close",
        ] {
            assert!(screen.contains(expected), "missing {expected}: {screen}");
        }
        assert!(!screen.contains("736563726574"));
        assert!(!screen.contains("secret"));
        assert!(!screen.contains('\u{1b}'));
    }

    #[test]
    fn forty_by_ten_help_keeps_copy_keys_and_close_visible_for_every_pane() {
        let start = now();
        for (pane, title) in [
            (Pane::Input, "Input Help"),
            (Pane::Pipeline, "Pipeline Help"),
            (Pane::Output, "Output Help"),
        ] {
            let mut app = App::new(start, true);
            app.focus = pane;
            key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);

            let screen = rendered_app(40, 10, &mut app);

            for expected in [title, "F3/F4 Pretty/Raw", "Esc close"] {
                assert!(screen.contains(expected), "missing {expected}: {screen}");
            }
        }
    }

    #[test]
    fn zoomed_tab_keeps_the_visible_pane_equal_to_focus() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Pipeline;
        app.zoom = Some(Pane::Pipeline);

        key(&mut app, KeyCode::Tab, KeyModifiers::NONE, start);
        let screen = rendered_app(40, 10, &mut app);

        assert_eq!(app.focus, Pane::Input);
        assert_eq!(app.zoom, Some(Pane::Input));
        assert_eq!(screen.matches("> INPUT").count(), 1);
        assert_eq!(screen.matches("$ PIPELINE").count(), 0);
    }

    #[test]
    fn output_help_never_claims_copy_when_the_current_result_is_not_copyable() {
        let start = now();
        let mut apps = Vec::new();

        let mut trace = App::new(start, true);
        trace.focus = Pane::Output;
        trace.output.view = ViewMode::Trace;
        trace.output.status = OutputStatus::Ready;
        trace.output.active_artifact = Some(Artifact::new(b"hidden".to_vec()));
        apps.push(trace);

        let mut failed = App::new(start, true);
        failed.focus = Pane::Output;
        failed.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
        apps.push(failed);

        let mut missing = App::new(start, true);
        missing.focus = Pane::Output;
        missing.output.status = OutputStatus::Ready;
        apps.push(missing);

        for mut app in apps {
            key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);
            let screen = rendered_app(80, 20, &mut app);
            assert!(!screen.contains("Copy whole result"));
            assert!(screen.contains("Copy unavailable"));
            assert!(!screen.contains("Enter/y  Copy whole result"));
        }

        let mut copyable = App::new(start, true);
        copyable.focus = Pane::Output;
        copyable.output.status = OutputStatus::Ready;
        copyable.output.active_artifact = Some(Artifact::new(b"ready".to_vec()));
        key(&mut copyable, KeyCode::F(1), KeyModifiers::NONE, start);
        let screen = rendered_app(80, 20, &mut copyable);
        assert!(screen.contains("Copy whole result"));
        assert!(!screen.contains("Copy unavailable"));
        assert!(screen.contains("Enter/y"));
    }

    #[test]
    fn smart_failure_shows_trace_while_pinned_views_show_safe_guidance() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
        app.output.active_artifact = Some(Artifact::new(b"stale secret".to_vec()));

        let smart = rendered_app(89, 20, &mut app);
        assert!(smart.contains("» OUTPUT / FINAL / SMART"));
        assert!(smart.contains("STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS"));
        assert!(smart.contains("chain exceeds 32 steps"));
        assert!(!smart.contains("stale secret"));

        for mode in [ViewMode::Text, ViewMode::Hex] {
            app.output.view = mode;
            let pinned = rendered_app(89, 20, &mut app);
            assert!(pinned.contains("chain exceeds 32 steps"));
            assert!(pinned.contains("Switch to Trace view"));
            assert!(!pinned.contains("stale secret"));
            assert_eq!(app.output.view, mode);
        }
    }

    #[test]
    fn failed_and_cancelled_output_override_stale_status_at_every_usable_width() {
        for (width, height) in [(120, 16), (90, 13), (40, 10)] {
            let mut failed = App::new(now(), true);
            failed.status = Some("stale clipboard status".to_string());
            failed.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
            let failed_screen = rendered_app(width, height, &mut failed);
            let failed_context = failed_screen.lines().rev().nth(1).unwrap();
            assert!(failed_context.contains("chain exceeds 32 steps"));
            assert!(!failed_context.contains("stale clipboard status"));

            let mut cancelled = App::new(now(), true);
            cancelled.status = Some("stale general status".to_string());
            cancelled.output.status = OutputStatus::Cancelled;
            let cancelled_screen = rendered_app(width, height, &mut cancelled);
            let cancelled_context = cancelled_screen.lines().rev().nth(1).unwrap();
            assert!(cancelled_context.contains("Cancelled"));
            assert!(!cancelled_context.contains("stale general status"));
        }
    }

    #[test]
    fn invalid_utf8_failure_output_never_exposes_the_hex_preview() {
        for view in [ViewMode::Smart, ViewMode::Text, ViewMode::Hex] {
            let mut app = App::new(now(), true);
            app.focus = Pane::Output;
            app.output.view = view;
            app.output.status = OutputStatus::Failed(PipelineError::Step {
                step: 1,
                transform_id: "hex-decode",
                source: TransformError::InvalidUtf8Output {
                    preview_hex: "736563726574".to_string(),
                    total_bytes: 6,
                },
            });

            let screen = rendered_app(89, 20, &mut app);

            assert!(screen.contains("output is not valid UTF-8 (6 bytes)"));
            assert!(!screen.contains("736563726574"));
            assert!(!screen.contains("secret"));
        }
    }

    #[test]
    fn invalid_utf8_failure_context_never_exposes_the_hex_preview() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Input;
        app.output.status = OutputStatus::Failed(PipelineError::Step {
            step: 1,
            transform_id: "hex-decode",
            source: TransformError::InvalidUtf8Output {
                preview_hex: "736563726574".to_string(),
                total_bytes: 6,
            },
        });

        let screen = rendered_app(120, 10, &mut app);
        let context = screen.lines().rev().nth(1).unwrap();

        assert!(context.contains("output is not valid UTF-8 (6 bytes)"));
        assert!(!context.contains("736563726574"));
        assert!(!context.contains("secret"));
    }

    #[test]
    fn pinned_text_over_binary_guides_to_hex_without_partial_bytes() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.view = ViewMode::Text;
        app.output.active_artifact = Some(Artifact::new(vec![0xff, b's', b'e', b'c']));

        let screen = rendered_app(120, 20, &mut app);

        assert!(screen.contains("» OUTPUT / FINAL / TEXT"));
        assert!(screen.contains("Switch to Hex view"));
        assert!(!screen.contains("sec"));
        assert_eq!(app.output.view, ViewMode::Text);
    }

    #[test]
    fn newline_dense_output_stays_within_the_four_kibibyte_view_budget() {
        let artifact = Artifact::new(vec![b'\n'; 8 * 1024]);

        let window = render_text_window(&artifact, 0, 10_000, 80);

        assert!(window.text.len() <= 4 * 1024);
        assert!(window.inspected_bytes <= 4 * 1024);
    }

    #[test]
    fn composed_hex_and_trace_views_reserve_header_space_inside_the_budget() {
        let hex = with_bounded_header(
            "OFFSET    HEX BYTES                                      ASCII",
            render_hex_window(&Artifact::new(vec![0xff; 4 * 1024]), 0, 1_000),
        );
        assert!(hex.starts_with("OFFSET"));
        assert!(hex.len() <= 4 * 1024);

        let traces = (1..=32)
            .map(|step| StepTrace {
                step,
                transform_id: "very-long-transform-operation-name-used-to-fill-the-bounded-trace-view-without-rendering-any-input-or-output-body",
                input_bytes: Some(step),
                output_bytes: Some(step),
                elapsed: Some(Duration::from_millis(1)),
                status: StepStatus::Succeeded,
                error: None,
            })
            .collect::<Vec<_>>();
        let trace = with_bounded_header(
            "STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS",
            render_trace_window(&traces, 0, 1_000, 1_000),
        );
        assert!(trace.starts_with("STEP  OPERATION"));
        assert!(trace.len() <= 4 * 1024);
    }

    #[test]
    fn output_controls_are_rendered_only_as_inert_escape_text() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(
            "ansi:\u{1b}[2J osc:\u{1b}]52;c;secret\u{7} nul:\0 c1:\u{85}"
                .as_bytes()
                .to_vec(),
        ));

        let screen = rendered_app(89, 20, &mut app);

        assert!(screen.contains("\\x1b[2J"));
        assert!(screen.contains("\\x1b]52;c;secret\\x07"));
        assert!(screen.contains("\\x00"));
        assert!(screen.contains("\\x85"));
        assert!(!screen.contains('\u{1b}'));
        assert!(!screen.contains('\0'));
        assert!(!screen.contains('\u{85}'));
    }

    fn traces_for_all_five_states() -> Vec<StepTrace> {
        vec![
            StepTrace {
                step: 1,
                transform_id: "url-encode",
                input_bytes: Some(3),
                output_bytes: Some(4),
                elapsed: Some(Duration::from_millis(1)),
                status: StepStatus::Succeeded,
                error: None,
            },
            StepTrace {
                step: 2,
                transform_id: "url-encode",
                input_bytes: Some(4),
                output_bytes: None,
                elapsed: None,
                status: StepStatus::Failed,
                error: Some(TransformError::InvalidUtf8Input),
            },
            StepTrace {
                step: 3,
                transform_id: "url-encode",
                input_bytes: None,
                output_bytes: None,
                elapsed: None,
                status: StepStatus::Disabled,
                error: None,
            },
            StepTrace {
                step: 4,
                transform_id: "url-encode",
                input_bytes: Some(4),
                output_bytes: None,
                elapsed: None,
                status: StepStatus::Cancelled,
                error: None,
            },
            StepTrace {
                step: 5,
                transform_id: "url-encode",
                input_bytes: None,
                output_bytes: None,
                elapsed: None,
                status: StepStatus::NotExecuted,
                error: None,
            },
        ]
    }

    #[test]
    fn pipeline_rows_show_enablement_once_and_one_runtime_mark() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.zoom = Some(Pane::Pipeline);
        app.steps = (0..5)
            .map(|index| TransformStep {
                definition: transform_by_id("url-encode").unwrap(),
                enabled: index != 2,
            })
            .collect();
        app.output.traces = traces_for_all_five_states();

        let screen = rendered_app(80, 20, &mut app);

        for expected in [
            "[ON]  ✓ URL Encode",
            "[ON]  × URL Encode",
            "[OFF]   URL Encode",
            "[ON]  − URL Encode",
            "[ON]  · URL Encode",
        ] {
            assert!(screen.contains(expected), "missing {expected}: {screen}");
        }
        for duplicate in [
            " OK ",
            " ERROR ",
            " RUNNING ",
            " NOT RUN ",
            " CANCELLED ",
            "○",
        ] {
            assert!(
                !screen.contains(duplicate),
                "unexpected {duplicate}: {screen}"
            );
        }
    }

    #[test]
    fn running_pipeline_row_uses_the_same_compact_shape_without_color() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;

        let screen = rendered_app(80, 16, &mut app);
        assert!(screen.contains("[ON]  › URL Encode"));
        assert!(!screen.contains("RUNNING"));
    }

    #[test]
    fn disabled_pipeline_rows_show_trace_status_before_current_enablement() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.steps = ["base64-encode", "hex-encode"]
            .into_iter()
            .map(|id| TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: false,
            })
            .collect();
        app.output.traces = vec![
            StepTrace {
                step: 1,
                transform_id: "base64-encode",
                input_bytes: Some(1),
                output_bytes: Some(1),
                elapsed: None,
                status: StepStatus::Disabled,
                error: None,
            },
            StepTrace {
                step: 2,
                transform_id: "hex-encode",
                input_bytes: None,
                output_bytes: None,
                elapsed: None,
                status: StepStatus::NotExecuted,
                error: None,
            },
        ];

        let screen = rendered_app(89, 20, &mut app);
        let disabled = screen
            .lines()
            .find(|line| line.contains("Base64 Encode"))
            .unwrap();
        let not_run = screen
            .lines()
            .find(|line| line.contains("Hex Encode"))
            .unwrap();

        assert!(disabled.contains("[OFF]   Base64 Encode"));
        assert!(not_run.contains("[OFF]  · Hex Encode"));
    }

    #[test]
    fn disabled_step_inspector_shows_trace_status_before_current_enablement() {
        let mut app = App::new(now(), true);
        app.steps.push(TransformStep {
            definition: transform_by_id("hex-encode").unwrap(),
            enabled: false,
        });
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "hex-encode",
            input_bytes: None,
            output_bytes: None,
            elapsed: None,
            status: StepStatus::NotExecuted,
            error: None,
        });
        app.modal = Some(Modal::StepInspector);

        let screen = rendered_app(80, 20, &mut app);

        assert!(screen.contains("Status: NOT RUN"));
        assert!(!screen.contains("Status: OFF"));
    }

    #[test]
    fn restoring_final_renders_cached_pipeline_and_trace_without_rerun() {
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
        key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE, start);
        app.handle_event(AppEvent::PreviewFinished(PreviewResult {
            report: ExecutionReport {
                request_id: 1,
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
        key(&mut app, KeyCode::Char('f'), KeyModifiers::NONE, start);
        app.output.view = ViewMode::Trace;
        app.focus = Pane::Pipeline;

        let screen = rendered_app(120, 20, &mut app);
        let hex_row = screen
            .lines()
            .find(|line| line.contains("Hex Encode"))
            .unwrap();

        assert_eq!(app.output.source, OutputSource::Final);
        assert_eq!(app.output.status, OutputStatus::Ready);
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"final"
        );
        assert_eq!(app.output.traces, final_traces);
        assert!(screen.contains("» OUTPUT / FINAL / TRACE"));
        assert!(screen.contains("STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS"));
        assert!(screen.contains("#2 hex-encode"));
        assert!(hex_row.contains("[ON]  ✓ Hex Encode"));
        assert!(!hex_row.contains("NOT RUN"));
    }

    #[test]
    fn running_pipeline_state_and_byte_sizes_follow_color_and_width_policy() {
        let mut app = App::new(now(), false);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;
        let running = rendered_app(89, 20, &mut app);
        assert!(running.contains("[ON]  › URL Encode"));

        app.output.status = OutputStatus::Ready;
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "url-encode",
            input_bytes: Some(3),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });
        let wide = rendered_app(120, 20, &mut app);
        let medium = rendered_app(119, 20, &mut app);
        assert!(wide.contains("3B→4B"));
        assert!(!medium.contains("3B→4B"));
    }

    #[test]
    fn final_running_does_not_guess_a_running_pipeline_row() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.selected_step = 2;
        app.steps = ["url-encode", "base64-encode", "format-json"]
            .into_iter()
            .map(|id| TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: true,
            })
            .collect();
        app.output.source = OutputSource::Final;
        app.output.status = OutputStatus::Running;

        let screen = rendered_app(89, 20, &mut app);

        assert!(!screen.contains("RUNNING"));
    }

    #[test]
    fn selected_step_running_marker_stays_on_the_fixed_output_target() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.selected_step = 2;
        app.steps = ["url-encode", "base64-encode", "format-json"]
            .into_iter()
            .map(|id| TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: true,
            })
            .collect();
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;

        let screen = rendered_app(89, 20, &mut app);
        let target = screen
            .lines()
            .find(|line| line.contains("URL Encode"))
            .unwrap();
        let selected = screen
            .lines()
            .find(|line| line.contains("JSON Prettify"))
            .unwrap();

        assert!(target.contains("[ON]  › URL Encode"));
        assert!(!selected.contains("›"));
    }

    #[test]
    fn no_color_pipeline_keeps_status_and_selection_markers() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;

        let screen = rendered_app(89, 20, &mut app);

        assert!(screen.contains("> [ON]  › URL Encode"));
    }

    #[test]
    fn zoomed_pipeline_or_output_uses_the_whole_content_area() {
        for pane in [Pane::Pipeline, Pane::Output] {
            let mut app = App::new(now(), true);
            app.focus = pane;
            app.zoom = Some(pane);

            let screen = rendered_app(120, 16, &mut app);

            assert!(screen.contains(&format!(
                "{} {}",
                match pane {
                    Pane::Input => ">",
                    Pane::Output => "»",
                    Pane::Pipeline => "$",
                },
                pane_label(pane).to_ascii_uppercase()
            )));
            for hidden in [Pane::Input, Pane::Output, Pane::Pipeline] {
                if hidden != pane {
                    assert!(!screen.contains(&format!(
                        "{} {}",
                        match hidden {
                            Pane::Input => ">",
                            Pane::Output => "»",
                            Pane::Pipeline => "$",
                        },
                        pane_label(hidden).to_ascii_uppercase()
                    )));
                }
            }
        }
    }
    #[test]
    fn clipboard_failure_replaces_contextual_help_at_every_usable_width() {
        for (width, height) in [(120, 16), (90, 13), (40, 10)] {
            let mut app = App::new(now(), true);
            app.handle_event(AppEvent::ClipboardFinished {
                kind: CopyKind::Pretty,
                result: Err("Clipboard unavailable".to_string()),
            });

            let screen = rendered_app(width, height, &mut app);
            let context = screen.lines().rev().nth(1).unwrap();

            assert!(context.starts_with("Clipboard unavailable"));
            assert!(!context.contains("Ctrl+P"));
        }
    }

    #[test]
    fn failure_and_cancellation_replace_minimal_context_help() {
        let mut failed = App::new(now(), true);
        failed.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
        let failed_screen = rendered_app(40, 10, &mut failed);
        assert!(
            failed_screen
                .lines()
                .rev()
                .nth(1)
                .unwrap()
                .starts_with("chain exceeds 32 steps")
        );

        let mut cancelled = App::new(now(), true);
        cancelled.output.status = OutputStatus::Cancelled;
        let cancelled_screen = rendered_app(40, 10, &mut cancelled);
        assert!(
            cancelled_screen
                .lines()
                .rev()
                .nth(1)
                .unwrap()
                .starts_with("Cancelled")
        );
    }
    #[test]
    fn no_color_uses_default_cell_styles_and_status_marks() {
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
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(screen.contains("[ON]  › URL Encode"));
        assert!(screen.contains(">_ DOOP"));
        assert!(screen.contains("[COMMON]"));
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
        app.handle_event(AppEvent::ClipboardFinished {
            kind: CopyKind::Pretty,
            result: Err("clipboard\n\u{1b}[2J".to_string()),
        });
        app.focus = Pane::Output;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let status_screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!status_screen.contains('\n'));
        assert!(!status_screen.contains('\u{1b}'));
        assert!(status_screen.contains("\\x0a\\x1b[2J"));

        app.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let error_screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!error_screen.contains('\n'));
        assert!(!error_screen.contains('\u{1b}'));
        assert!(error_screen.contains("chain exceeds 32 steps"));
        assert!(!error_screen.contains("\\x0a\\x1b[2J"));
    }
    #[test]
    fn confirmation_modals_render_explicit_warning_text() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.modal = Some(Modal::UnsafeCopyConfirm {
            payload: ClipboardPayload {
                text: "\u{1b}".to_string(),
                kind: CopyKind::Pretty,
            },
        });
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
    fn tiny_layout_keeps_destructive_confirmations_visible_without_payload_text() {
        for (width, height) in [(39, 16), (120, 9)] {
            let mut unsafe_copy = App::new(now(), true);
            unsafe_copy.modal = Some(Modal::UnsafeCopyConfirm {
                payload: ClipboardPayload {
                    text: "hidden-payload\u{1b}".to_string(),
                    kind: CopyKind::Pretty,
                },
            });
            let screen = rendered_app(width, height, &mut unsafe_copy);

            assert!(screen.contains("Copy raw control characters?"));
            assert!(screen.contains("Enter/y confirm"));
            assert!(screen.contains("n/Esc cancel"));
            assert!(!screen.contains("hidden-payload"));

            let mut quit = App::new(now(), true);
            quit.modal = Some(Modal::QuitConfirm);
            let screen = rendered_app(width, height, &mut quit);

            assert!(screen.contains("Discard input and quit?"));
            assert!(screen.contains("Enter/y confirm"));
            assert!(screen.contains("n/Esc cancel"));
        }
    }
}
