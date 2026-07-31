use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, Paragraph},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChromeVisibility {
    navigation: bool,
    step_summary: bool,
    full_context: bool,
    all_panes: bool,
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

fn chrome_visibility(height: u16) -> ChromeVisibility {
    ChromeVisibility {
        navigation: height >= 14,
        step_summary: height >= 16,
        full_context: height >= 12,
        all_panes: height >= 12,
    }
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

fn source_label(source: OutputSource) -> String {
    match source {
        OutputSource::Final => "FINAL".to_string(),
        OutputSource::Step(index) => format!("STEP {:02}", index + 1),
    }
}

fn render_input(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let block = pane_block(app, "Input", app.focus == Pane::Input);
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
    let title = format!("Output — {source} — {view}");
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
    let block = pane_block(app, "Pipeline", app.focus == Pane::Pipeline);
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
                let mark = (!app.no_color).then_some("› ");
                let text = format!(
                    "{prefix} [{enabled}] {}RUNNING {}",
                    mark.unwrap_or_default(),
                    step.definition.display_name
                );
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
            let (mark, label, color) = match status {
                StepStatus::Succeeded => ("✓ ", "OK", Color::Green),
                StepStatus::Disabled => ("○ ", "OFF", Color::DarkGray),
                StepStatus::Failed => ("× ", "ERROR", Color::Red),
                StepStatus::NotExecuted => ("· ", "NOT RUN", Color::DarkGray),
                StepStatus::Cancelled => ("− ", "CANCELLED", Color::Yellow),
            };
            let sizes = if show_sizes {
                trace
                    .and_then(|trace| Some((trace.input_bytes?, trace.output_bytes?)))
                    .map(|(input, output)| format!(" {input}B→{output}B"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let mark = if app.no_color { "" } else { mark };
            let text = format!(
                "{prefix} [{enabled}] {mark}{label} {}{sizes}",
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

fn preview_label(status: &OutputStatus) -> &'static str {
    match status {
        OutputStatus::Idle => "Idle",
        OutputStatus::Debouncing { .. } => "Waiting",
        OutputStatus::Running => "Running",
        OutputStatus::Ready => "Ready",
        OutputStatus::Failed(_) => "Error",
        OutputStatus::Cancelled => "Cancelled",
    }
}

fn pane_label(pane: Pane) -> &'static str {
    match pane {
        Pane::Input => "Input",
        Pane::Output => "Output",
        Pane::Pipeline => "Pipeline",
    }
}

fn render_app_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let tabs = [Pane::Input, Pane::Output, Pane::Pipeline]
        .into_iter()
        .map(|pane| {
            if pane == app.focus {
                format!("[{}]", pane_label(pane))
            } else {
                pane_label(pane).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };
    frame.render_widget(Paragraph::new(format!("doop | {tabs}")).style(style), area);
}

fn render_navigation(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let text = match app.focus {
        Pane::Input => "Navigation | Tab focus | Ctrl+P add | Ctrl+Q quit",
        Pane::Output if app.can_copy() => "Navigation | Tab focus | Enter copy | p step | f final",
        Pane::Output => "Navigation | Tab focus | p step | f final",
        Pane::Pipeline => "Navigation | Tab focus | Space toggle | Shift+Up/Down move",
    };
    frame.render_widget(Paragraph::new(text).style(Style::default()), area);
}

fn render_step_summary(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let enabled = app.steps.iter().filter(|step| step.enabled).count();
    let selected = if app.steps.is_empty() {
        "-".to_string()
    } else {
        format!("{:02}", app.selected_step + 1)
    };
    frame.render_widget(
        Paragraph::new(format!(
            "Step Summary | selected {selected} | {enabled}/{} enabled | {}",
            app.steps.len(),
            preview_label(&app.output.status)
        ))
        .style(Style::default()),
        area,
    );
}

fn render_context(frame: &mut Frame<'_>, app: &App, area: Rect, full: bool) {
    let prefix = format!("{} | ", source_label(app.output.source));
    let body_width = (area.width as usize).saturating_sub(prefix.len());
    let body = match &app.output.status {
        OutputStatus::Failed(error) => {
            crate::error::escape_external(&render_pipeline_error_summary(error), body_width)
        }
        OutputStatus::Cancelled => "Cancelled".to_string(),
        _ => {
            if let Some(message) = &app.status {
                crate::error::escape_external(message, body_width)
            } else {
                match &app.output.status {
                    _ if app.can_copy()
                        && app
                            .output
                            .active_artifact
                            .as_ref()
                            .is_some_and(|artifact| !artifact.is_utf8()) =>
                    {
                        if full {
                            "Copy as Hex | Tab focus | Ctrl+Q quit".to_string()
                        } else {
                            "Copy as Hex".to_string()
                        }
                    }
                    _ if full => "Tab focus | Ctrl+P add | Ctrl+Q quit".to_string(),
                    _ => "Ctrl+Q quit".to_string(),
                }
            }
        }
    };
    let text = format!("{prefix}{body}");
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

fn input_condition(accepts_binary: bool) -> &'static str {
    if accepts_binary {
        "Bytes accepted"
    } else {
        "Text input"
    }
}

fn render_picker(frame: &mut Frame<'_>, app: &App, query: &str, selected: usize) {
    let area = centered(frame.area(), 72, 18);
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
    let [query_area, list_area, detail_area, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(8),
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
    let detail = filtered.get(selected).map_or_else(
        || "No matching transforms".to_string(),
        |transform| {
            format!(
                "ID: {}\n{}\nCLI description: {}\nCLI behavior: {}\nTUI result: bytes; Smart uses Text or Hex",
                transform.id,
                input_condition(transform.accepts_binary),
                transform.description,
                transform.behavior,
            )
        },
    );
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
    frame.render_widget(Paragraph::new(detail).style(Style::default()), detail_area);
    frame.render_widget(
        Paragraph::new("Enter add · Esc cancel").style(Style::default()),
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
            "Text editing: tui-textarea defaults\nTab / Shift+Tab  Next / previous pane\nCtrl+P  Add transform\nF1  Context help\nCtrl+Q  Quit\nCtrl+C  Force quit\nEsc  Close zoom or cancel request".to_string(),
        ),
        Pane::Pipeline => (
            "Pipeline Help",
            "Up/Down or j/k  Select step\nShift+Up/Down or J/K  Reorder\nSpace  Toggle step\nDelete or d  Delete step\nEnter  Inspect step\na / Ctrl+P  Add transform\nz  Toggle zoom\nTab / Shift+Tab  Change pane\n? / F1  Context help\nCtrl+Q  Quit\nCtrl+C  Force quit".to_string(),
        ),
        Pane::Output => (
            "Output Help",
            format!(
                "v/V  Next / previous view\np  Show selected step\nf  Restore final\nEnter/y  {}\nArrows / PageUp / PageDown / Home / End  Scroll\nz  Toggle zoom\nTab / Shift+Tab  Change pane\nCtrl+P  Add transform\n? / F1  Context help\nCtrl+Q  Quit\nCtrl+C  Force quit",
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
            Pane::Input => "Text edit · Tab focus\nCtrl+P Add · F1 Help\nCtrl+Q Quit · Ctrl+C Force\nEsc close".to_string(),
            Pane::Pipeline => "j/k Select · J/K Move\nSpace Toggle · d Delete\nEnter Inspect · a Add · z Zoom\nEsc close".to_string(),
            Pane::Output => format!(
                "v/V View · p Step · f Final\nEnter/y {}\nArrows/Page Scroll · z Zoom\nEsc close",
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

    let chrome = chrome_visibility(area.height);
    let mut top = area.y;
    let app_bar = Rect::new(area.x, top, area.width, 1);
    top += 1;
    render_app_bar(frame, app, app_bar);
    if chrome.navigation {
        let navigation = Rect::new(area.x, top, area.width, 1);
        top += 1;
        render_navigation(frame, app, navigation);
    }
    if chrome.step_summary {
        let step_summary = Rect::new(area.x, top, area.width, 1);
        top += 1;
        render_step_summary(frame, app, step_summary);
    }
    let context = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    let content = Rect::new(area.x, top, area.width, context.y.saturating_sub(top));

    let focused = app.zoom.unwrap_or(app.focus);
    if matches!(mode, WidthMode::Narrow) || !chrome.all_panes || app.zoom.is_some() {
        match focused {
            Pane::Input => render_input(frame, app, content),
            Pane::Output => render_output(frame, app, content),
            Pane::Pipeline => render_pipeline(frame, app, content, mode == WidthMode::Wide),
        }
    } else {
        let pipeline_columns = pipeline_width(area.width, mode);
        let pipeline = Rect::new(content.x, content.y, pipeline_columns, content.height);
        let right = Rect::new(
            content.x + pipeline_columns,
            content.y,
            content.width.saturating_sub(pipeline_columns),
            content.height,
        );
        let input_rows = (u32::from(right.height) * 42 / 100) as u16;
        let input_rows = input_rows.clamp(5, right.height.saturating_sub(5));
        let input = Rect::new(right.x, right.y, right.width, input_rows);
        let output = Rect::new(
            right.x,
            right.y + input_rows,
            right.width,
            right.height.saturating_sub(input_rows),
        );
        render_pipeline(frame, app, pipeline, mode == WidthMode::Wide);
        render_input(frame, app, input);
        render_output(frame, app, output);
    }
    render_context(frame, app, context, chrome.full_context);
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
    fn height_boundaries_hide_chrome_in_order() {
        assert_eq!(
            chrome_visibility(16),
            ChromeVisibility {
                navigation: true,
                step_summary: true,
                full_context: true,
                all_panes: true,
            }
        );
        assert_eq!(
            chrome_visibility(15),
            ChromeVisibility {
                navigation: true,
                step_summary: false,
                full_context: true,
                all_panes: true,
            }
        );
        assert_eq!(
            chrome_visibility(13),
            ChromeVisibility {
                navigation: false,
                step_summary: false,
                full_context: true,
                all_panes: true,
            }
        );
        assert_eq!(
            chrome_visibility(11),
            ChromeVisibility {
                navigation: false,
                step_summary: false,
                full_context: false,
                all_panes: false,
            }
        );
        assert_eq!(chrome_visibility(10), chrome_visibility(11));
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
            let pane_starts = lines[3]
                .chars()
                .enumerate()
                .filter_map(|(index, character)| (character == '┌').then_some(index))
                .collect::<Vec<_>>();

            assert_eq!(pane_starts, vec![0, expected_pipeline_width as usize]);
            assert!(lines[3].contains("Pipeline"));
            assert!(lines[3].contains("Input"));
            assert!(lines[8].contains("Output"));
        }
    }

    #[test]
    fn narrow_width_boundaries_show_only_the_focused_pane_and_text_tabs() {
        for width in [89, 40] {
            let screen = rendered(width, 16, Pane::Output);
            assert!(screen.contains("[Output]"));
            assert_eq!(screen.matches("Output").count(), 2);
            assert_eq!(screen.matches("Input").count(), 1);
            assert_eq!(screen.matches("Pipeline").count(), 1);
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
    fn height_boundaries_remove_summary_navigation_and_extra_panes_in_order() {
        let full = rendered(120, 16, Pane::Output);
        assert!(full.contains("Navigation"));
        assert!(full.contains("Step Summary"));
        assert!(full.contains("Pipeline"));
        assert!(full.contains("Input"));
        assert!(full.contains("Output"));

        for height in [15, 14] {
            let without_summary = rendered(120, height, Pane::Output);
            assert!(without_summary.contains("Navigation"));
            assert!(!without_summary.contains("Step Summary"));
        }

        for height in [13, 12] {
            let without_navigation = rendered(120, height, Pane::Output);
            assert!(!without_navigation.contains("Navigation"));
            assert!(!without_navigation.contains("Step Summary"));
            assert!(without_navigation.contains("Pipeline"));
            assert!(without_navigation.contains("Input"));
            assert!(
                without_navigation
                    .lines()
                    .last()
                    .unwrap()
                    .contains("Ctrl+P")
            );
        }

        for height in [11, 10] {
            let focused = rendered(120, height, Pane::Output);
            assert!(focused.contains("[Output]"));
            assert_eq!(focused.matches("Pipeline").count(), 1);
            assert_eq!(focused.matches("Input").count(), 1);
            assert_eq!(focused.matches("Output").count(), 2);
        }
    }

    #[test]
    fn twelve_row_layout_keeps_both_right_panes_at_five_bordered_rows() {
        let lines = rendered_lines(120, 12, Pane::Input);
        assert!(lines[1].contains("Input"));
        assert!(lines[6].contains("Output"));
        assert!(lines[5].contains("└"));
        assert!(lines[10].contains("└"));
    }

    #[test]
    fn large_height_keeps_the_right_split_at_forty_two_percent() {
        let lines = rendered_lines(120, 2_004, Pane::Input);

        assert!(lines[3].contains("Input"));
        assert!(lines[843].contains("Output"));
    }

    #[test]
    fn output_titles_name_source_and_configured_view_for_text_hex_and_trace() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"valid text".to_vec()));
        let text = rendered_app(89, 20, &mut app);
        assert!(text.contains("Output — FINAL — Smart"));
        assert!(text.contains("valid text"));

        app.output.source = OutputSource::Step(1);
        app.output.active_artifact = Some(Artifact::new(vec![0, 0xff]));
        let hex = rendered_app(89, 20, &mut app);
        assert!(hex.contains("Output — STEP 02 — Smart"));
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
        assert!(trace.contains("Output — STEP 02 — Trace"));
        assert!(trace.contains("STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS"));
        assert!(trace.contains("#2 hex-decode OK"));
    }

    #[test]
    fn palette_detail_uses_registry_metadata_and_binary_safe_tui_wording() {
        let mut app = App::new(now(), true);
        app.open_picker();

        let bytes = rendered_app(80, 20, &mut app);
        assert!(bytes.contains("base64-encode"));
        assert!(bytes.contains("Bytes accepted"));
        assert!(bytes.contains("CLI description"));
        assert!(bytes.contains("Encode bytes using padded RFC 4648 Base64"));
        assert!(bytes.contains("CLI behavior"));
        assert!(bytes.contains("canonical = padding"));
        assert!(bytes.contains("TUI result: bytes; Smart uses Text or Hex"));

        for character in "decode".chars() {
            key(
                &mut app,
                KeyCode::Char(character),
                KeyModifiers::NONE,
                now(),
            );
        }
        let text = rendered_app(80, 20, &mut app);
        assert!(text.contains("base64-decode"));
        assert!(text.contains("Text input"));
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
    fn forty_by_ten_palette_keeps_search_selection_and_close_hint_visible() {
        let start = now();
        let mut app = App::new(start, true);
        app.open_picker();
        for character in "hex".chars() {
            key(
                &mut app,
                KeyCode::Char(character),
                KeyModifiers::NONE,
                start,
            );
        }

        let screen = rendered_app(40, 10, &mut app);

        assert!(screen.contains("Search: hex"));
        assert!(screen.contains("Hex Encode"));
        assert!(screen.contains("Enter add"));
        assert!(screen.contains("Esc cancel"));
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
    fn forty_by_ten_help_keeps_context_keys_and_close_visible() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);

        let screen = rendered_app(40, 10, &mut app);

        for expected in ["Output Help", "v/V", "p", "Esc close"] {
            assert!(screen.contains(expected), "missing {expected}: {screen}");
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
        assert_eq!(screen.matches("Input").count(), 2);
        assert_eq!(screen.matches("Pipeline").count(), 1);
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
            assert!(!screen.contains("Enter copy"));
        }

        let mut copyable = App::new(start, true);
        copyable.focus = Pane::Output;
        copyable.output.status = OutputStatus::Ready;
        copyable.output.active_artifact = Some(Artifact::new(b"ready".to_vec()));
        key(&mut copyable, KeyCode::F(1), KeyModifiers::NONE, start);
        let screen = rendered_app(80, 20, &mut copyable);
        assert!(screen.contains("Copy whole result"));
        assert!(!screen.contains("Copy unavailable"));
        assert!(screen.contains("Enter copy"));
    }

    #[test]
    fn smart_failure_shows_trace_while_pinned_views_show_safe_guidance() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
        app.output.active_artifact = Some(Artifact::new(b"stale secret".to_vec()));

        let smart = rendered_app(89, 20, &mut app);
        assert!(smart.contains("Output — FINAL — Smart"));
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
            let failed_context = failed_screen.lines().last().unwrap();
            assert!(failed_context.contains("chain exceeds 32 steps"));
            assert!(!failed_context.contains("stale clipboard status"));

            let mut cancelled = App::new(now(), true);
            cancelled.status = Some("stale general status".to_string());
            cancelled.output.status = OutputStatus::Cancelled;
            let cancelled_screen = rendered_app(width, height, &mut cancelled);
            let cancelled_context = cancelled_screen.lines().last().unwrap();
            assert!(cancelled_context.contains("Cancelled"));
            assert!(!cancelled_context.contains("stale general status"));
        }
    }

    #[test]
    fn context_bar_prefixes_source_for_every_status_branch_and_usable_width() {
        for (source, prefix) in [
            (OutputSource::Final, "FINAL | "),
            (OutputSource::Step(1), "STEP 02 | "),
        ] {
            for (width, height) in [(120, 16), (90, 13), (40, 10)] {
                let mut normal = App::new(now(), true);
                normal.output.source = source;
                let context = rendered_app(width, height, &mut normal);
                assert!(context.lines().last().unwrap().starts_with(prefix));

                let mut status = App::new(now(), true);
                status.output.source = source;
                status.status = Some("Copied\u{1b}[2J".to_string());
                let context = rendered_app(width, height, &mut status);
                let context = context.lines().last().unwrap();
                assert!(context.starts_with(prefix));
                assert!(context.contains("Copied\\x1b[2J"));
                assert!(!context.contains('\u{1b}'));

                let mut failed = App::new(now(), true);
                failed.output.source = source;
                failed.status = Some("stale".to_string());
                failed.output.status =
                    OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
                let context = rendered_app(width, height, &mut failed);
                let context = context.lines().last().unwrap();
                assert!(context.starts_with(prefix));
                assert!(context.contains("chain exceeds 32 steps"));
                assert!(!context.contains("stale"));

                let mut cancelled = App::new(now(), true);
                cancelled.output.source = source;
                cancelled.status = Some("stale".to_string());
                cancelled.output.status = OutputStatus::Cancelled;
                let context = rendered_app(width, height, &mut cancelled);
                let context = context.lines().last().unwrap();
                assert!(context.starts_with(prefix));
                assert!(context.contains("Cancelled"));
                assert!(!context.contains("stale"));

                let mut copy_hint = App::new(now(), true);
                copy_hint.output.source = source;
                copy_hint.output.status = OutputStatus::Ready;
                copy_hint.output.active_artifact = Some(Artifact::new(vec![0xff]));
                let context = rendered_app(width, height, &mut copy_hint);
                let context = context.lines().last().unwrap();
                assert!(context.starts_with(prefix));
                assert!(context.contains("Copy as Hex"));
            }
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
        let context = screen.lines().last().unwrap();

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

        let screen = rendered_app(89, 20, &mut app);

        assert!(screen.contains("Output — FINAL — Text"));
        assert!(screen.contains("Switch to Hex view"));
        assert!(!screen.contains("sec"));
        assert_eq!(app.output.view, ViewMode::Text);
    }

    #[test]
    fn binary_artifact_context_offers_copy_as_hex_only_when_copyable() {
        for width in [120, 90, 40] {
            for mode in [ViewMode::Smart, ViewMode::Text, ViewMode::Hex] {
                let mut app = App::new(now(), true);
                app.focus = Pane::Output;
                app.output.status = OutputStatus::Ready;
                app.output.view = mode;
                app.output.active_artifact = Some(Artifact::new(vec![0xff]));

                let screen = rendered_app(width, 10, &mut app);

                assert!(screen.lines().last().unwrap().contains("Copy as Hex"));
            }

            let mut trace = App::new(now(), true);
            trace.focus = Pane::Output;
            trace.output.status = OutputStatus::Ready;
            trace.output.view = ViewMode::Trace;
            trace.output.active_artifact = Some(Artifact::new(vec![0xff]));
            let screen = rendered_app(width, 10, &mut trace);
            assert!(!screen.lines().last().unwrap().contains("Copy as Hex"));
        }
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

    #[test]
    fn pipeline_rows_keep_selection_enablement_and_every_trace_state_textual() {
        let mut app = App::new(now(), false);
        app.focus = Pane::Pipeline;
        app.selected_step = 1;
        app.steps = (0..5)
            .map(|index| TransformStep {
                definition: transform_by_id("url-encode").unwrap(),
                enabled: index != 2,
            })
            .collect();
        app.output.traces = vec![
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
        ];

        let screen = rendered_app(89, 20, &mut app);

        for status in ["OK", "ERROR", "OFF", "CANCELLED", "NOT RUN"] {
            assert!(screen.contains(status));
        }
        assert!(screen.contains("> [ON]"));
        assert!(screen.contains("[OFF]"));
        for mark in ["✓", "×", "○", "−", "·"] {
            assert!(screen.contains(mark));
        }
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

        assert!(disabled.contains("[OFF] OFF"));
        assert!(not_run.contains("[OFF] NOT RUN"));
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
        assert!(screen.contains("Output — FINAL — Trace"));
        assert!(screen.contains("STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS"));
        assert!(screen.contains("#2 hex-encode"));
        assert!(hex_row.contains("[ON] OK"));
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
        assert!(running.contains("› RUNNING"));

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

        assert!(target.contains("RUNNING"));
        assert!(!selected.contains("RUNNING"));
    }

    #[test]
    fn no_color_pipeline_uses_only_status_words_and_selection_markers() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.output.source = OutputSource::Step(0);
        app.output.status = OutputStatus::Running;

        let screen = rendered_app(89, 20, &mut app);

        assert!(screen.contains("> [ON] RUNNING"));
        for mark in ["✓", "×", "○", "›", "−", "·"] {
            assert!(!screen.contains(mark));
        }
    }

    #[test]
    fn zoomed_pipeline_or_output_uses_the_whole_content_area() {
        for pane in [Pane::Pipeline, Pane::Output] {
            let mut app = App::new(now(), true);
            app.focus = pane;
            app.zoom = Some(pane);

            let screen = rendered_app(120, 16, &mut app);

            assert_eq!(screen.matches(pane_label(pane)).count(), 2);
            for hidden in [Pane::Input, Pane::Output, Pane::Pipeline] {
                if hidden != pane {
                    assert_eq!(screen.matches(pane_label(hidden)).count(), 1);
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
            let context = screen.lines().last().unwrap();

            assert!(context.starts_with("FINAL | Clipboard unavailable"));
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
                .last()
                .unwrap()
                .starts_with("FINAL | chain exceeds 32 steps")
        );

        let mut cancelled = App::new(now(), true);
        cancelled.output.status = OutputStatus::Cancelled;
        let cancelled_screen = rendered_app(40, 10, &mut cancelled);
        assert!(
            cancelled_screen
                .lines()
                .last()
                .unwrap()
                .starts_with("FINAL | Cancelled")
        );
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
