use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Shadow, Table,
        Wrap,
    },
};
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use crate::{
    error::AppError,
    pipeline::{ExecutionTarget, StepStatus},
    transforms::transform_by_id,
};

use super::{
    CYAN, GREEN, RED, YELLOW,
    state::{App, CopyPhase, Modal, MouseRegions, OutputSource, OutputStatus, Pane},
    views::{
        EffectiveView, TEXT_VIEW_UNAVAILABLE_MESSAGE, VISIBLE_TEXT_BYTE_BUDGET, ViewMode,
        effective_view, render_pipeline_error_summary, render_text_window,
        render_transform_error_summary, trace_failure_detail_height, trace_start_row, trace_status,
        trace_visible_row_capacity, visible_hex_rows,
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
    let style = if focused {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        muted_style()
    };
    if focused && !app.no_color {
        style.fg(CYAN)
    } else {
        style
    }
}

fn selection_style(_app: &App) -> Style {
    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
}

fn muted_style() -> Style {
    Style::default().add_modifier(Modifier::DIM)
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

fn output_title(app: &App, available_width: u16) -> String {
    let view = match app.output.view {
        ViewMode::Smart => "SMART",
        ViewMode::Text => "TEXT",
        ViewMode::Hex => "HEX",
        ViewMode::Trace => "TRACE",
    };
    let base = match app.output.source {
        OutputSource::Final => format!("» OUTPUT [{view}]"),
        OutputSource::Step(index) => format!("» OUTPUT / STEP {:02} [{view}]", index + 1),
    };
    if !matches!(app.output.status, OutputStatus::Ready) {
        return base;
    }
    let Some(artifact) = app.output.active_artifact.as_ref() else {
        return base;
    };
    let with_size = format!("{base} · {} B", artifact.bytes().len());
    if with_size.width().saturating_add(2) <= available_width as usize {
        with_size
    } else {
        base
    }
}

fn render_input(
    frame: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    mouse_regions: &mut MouseRegions,
) {
    mouse_regions.input = Some(area);
    let block = pane_block(app, "> INPUT", app.focus == Pane::Input);
    app.textarea.set_block(block);
    frame.render_widget(&app.textarea, area);
}

fn hex_style(app: &App, color: Color) -> Style {
    if app.no_color {
        Style::default()
    } else {
        Style::default().fg(color)
    }
}

fn hex_bytes_cell(app: &App, bytes: &[u8], start: usize) -> Cell<'static> {
    let mut spans = Vec::with_capacity(15);
    for index in 0..8 {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        match bytes.get(start + index) {
            Some(byte) => spans.push(Span::styled(
                format!("{byte:02X}"),
                hex_style(
                    app,
                    if (0x20..=0x7e).contains(byte) {
                        Color::Reset
                    } else {
                        YELLOW
                    },
                ),
            )),
            None => spans.push(Span::styled("  ", muted_style())),
        }
    }
    Cell::from(Line::from(spans))
}

fn hex_ascii_cell(app: &App, bytes: &[u8]) -> Cell<'static> {
    let mut spans = Vec::with_capacity(16);
    for index in 0..16 {
        match bytes.get(index) {
            Some(byte) if (0x20..=0x7e).contains(byte) => spans.push(Span::styled(
                char::from(*byte).to_string(),
                hex_style(app, GREEN),
            )),
            Some(_) => spans.push(Span::styled(".", muted_style())),
            None => spans.push(Span::styled(" ", muted_style())),
        }
    }
    Cell::from(Line::from(spans))
}

fn render_hex_table(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let columns = area.width as usize;
    let rows = app
        .output
        .active_artifact
        .as_ref()
        .map_or_else(Vec::new, |artifact| {
            visible_hex_rows(
                artifact,
                app.output.row_offset,
                area.height.saturating_sub(1) as usize,
                columns,
            )
        });
    let header_style = muted_style();
    let offset = |row: &super::views::HexRow<'_>| {
        Cell::from(format!("{:08X}", row.offset)).style(hex_style(app, CYAN))
    };
    let table = match columns {
        78.. => Table::new(
            rows.iter().map(|row| {
                Row::new(vec![
                    offset(row),
                    hex_bytes_cell(app, row.bytes, 0),
                    hex_bytes_cell(app, row.bytes, 8),
                    hex_ascii_cell(app, row.bytes),
                ])
            }),
            [
                Constraint::Length(8),
                Constraint::Length(23),
                Constraint::Length(23),
                Constraint::Length(16),
            ],
        )
        .header(Row::new(vec![
            Cell::from("OFFSET").style(header_style),
            Cell::from("0–7").style(header_style),
            Cell::from("8–15").style(header_style),
            Cell::from("ASCII").style(header_style),
        ]))
        .column_spacing(2),
        60..=77 => Table::new(
            rows.iter().map(|row| {
                Row::new(vec![
                    offset(row),
                    hex_bytes_cell(app, row.bytes, 0),
                    hex_bytes_cell(app, row.bytes, 8),
                ])
            }),
            [
                Constraint::Length(8),
                Constraint::Length(23),
                Constraint::Length(23),
            ],
        )
        .header(Row::new(vec![
            Cell::from("OFFSET").style(header_style),
            Cell::from("0–7").style(header_style),
            Cell::from("8–15").style(header_style),
        ]))
        .column_spacing(2),
        _ => Table::new(
            rows.iter()
                .map(|row| Row::new(vec![offset(row), hex_bytes_cell(app, row.bytes, 0)])),
            [Constraint::Length(8), Constraint::Length(23)],
        )
        .header(Row::new(vec![
            Cell::from("OFFSET").style(header_style),
            Cell::from("0–7").style(header_style),
        ]))
        .column_spacing(2),
    };
    frame.render_widget(table, area);
}

fn trace_status_style(app: &App, status: StepStatus) -> Style {
    let color = match status {
        StepStatus::Succeeded => GREEN,
        StepStatus::Failed => RED,
        StepStatus::Cancelled => YELLOW,
        StepStatus::Disabled | StepStatus::NotExecuted => return muted_style(),
    };
    if app.no_color {
        Style::default()
    } else {
        Style::default().fg(color)
    }
}

fn failure_style(app: &App) -> Style {
    if app.no_color {
        Style::default()
    } else {
        Style::default().fg(RED)
    }
}

fn trace_prefix(text: &str, columns: usize, byte_budget: usize) -> String {
    let mut output = String::new();
    let mut used_width = 0usize;
    for grapheme in text.graphemes(true) {
        if output.len().saturating_add(grapheme.len()) > byte_budget
            || used_width.saturating_add(grapheme.width()) > columns
        {
            break;
        }
        output.push_str(grapheme);
        used_width += grapheme.width();
    }
    output
}

fn operation_name(transform_id: &str, width: usize) -> String {
    let name = transform_by_id(transform_id)
        .map(|transform| transform.display_name.to_string())
        .unwrap_or_else(|| crate::error::escape_external(transform_id, width));
    trace_prefix(&name, width, usize::MAX)
}

fn trace_cell(text: &str, width: usize, row_budget: &mut usize) -> Cell<'static> {
    let text = trace_prefix(text, width, *row_budget);
    *row_budget = row_budget.saturating_sub(text.len());
    Cell::from(text)
}

fn trace_status_cell(
    app: &App,
    status: StepStatus,
    width: usize,
    row_budget: &mut usize,
) -> Cell<'static> {
    trace_cell(trace_status(status), width, row_budget).style(trace_status_style(app, status))
}

fn render_trace_table(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let columns = area.width as usize;
    let wide = columns >= 70;
    let traces = &app.output.traces[..app.output.traces.len().min(32)];
    let fixed_columns = if wide { 47 } else { 29 };
    let maximum_operation = columns.saturating_sub(fixed_columns).max(1);
    let operation_width = traces
        .iter()
        .map(|trace| operation_name(trace.transform_id, maximum_operation).width())
        .chain(std::iter::once("OPERATION".width()))
        .max()
        .unwrap_or(1)
        .min(maximum_operation)
        .max(1);
    let (headers, widths) = if wide {
        (
            vec!["STEP", "OPERATION", "INPUT", "OUTPUT", "TIME", "STATUS"],
            vec![5, operation_width, 9, 9, 10, 9],
        )
    } else {
        (
            vec!["STEP", "OPERATION", "SIZE", "STATUS"],
            vec![5, operation_width, 12, 9],
        )
    };
    let header_cost = headers.iter().map(|header| header.len()).sum::<usize>();
    let failure_index = traces
        .iter()
        .position(|trace| trace.status == StepStatus::Failed);
    let failure = failure_index.and_then(|index| traces.get(index));
    let detail_height = trace_failure_detail_height(traces, area.height as usize) as u16;
    let (table_area, detail_area) = if detail_height == 0 {
        (area, None)
    } else if area.height >= 5 {
        let areas = Layout::vertical([Constraint::Min(2), Constraint::Length(3)]).split(area);
        (areas[0], Some(areas[1]))
    } else {
        let areas = Layout::vertical([Constraint::Length(2), Constraint::Length(detail_height)])
            .split(area);
        (areas[0], Some(areas[1]))
    };

    let mut remaining_budget = VISIBLE_TEXT_BYTE_BUDGET.saturating_sub(header_cost);
    let detail = failure.zip(detail_area).and_then(|(trace, detail_area)| {
        let error = trace.error.as_ref()?;
        let detail_width = detail_area.width.saturating_sub(1) as usize;
        if detail_area.height == 1 {
            let summary = trace_prefix(
                &render_transform_error_summary(error),
                detail_width,
                remaining_budget,
            );
            return Some((detail_area, summary, None));
        }
        let title = trace_prefix(
            &format!(
                "STEP {} · {}",
                trace.step,
                operation_name(trace.transform_id, detail_width)
            ),
            detail_width,
            remaining_budget,
        );
        remaining_budget = remaining_budget.saturating_sub(title.len());
        let summary = trace_prefix(
            &render_transform_error_summary(error),
            detail_width,
            remaining_budget,
        );
        Some((detail_area, title, Some(summary)))
    });

    let start =
        trace_start_row(traces, app.output.row_offset, area.height as usize).min(traces.len());
    let visible_rows = trace_visible_row_capacity(traces, area.height as usize, columns);
    let take = visible_rows.min(traces.len().saturating_sub(start));
    let rows = traces
        .iter()
        .skip(start)
        .take(take)
        .map(|trace| {
            let mut row_budget = columns;
            let operation = operation_name(trace.transform_id, widths[1]);
            let mut cells = vec![
                trace_cell(&format!("#{}", trace.step), widths[0], &mut row_budget),
                trace_cell(&operation, widths[1], &mut row_budget),
            ];
            if wide {
                cells.extend([
                    trace_cell(
                        &trace
                            .input_bytes
                            .map_or_else(|| "—".to_string(), |bytes| format!("{bytes} B")),
                        widths[2],
                        &mut row_budget,
                    ),
                    trace_cell(
                        &trace
                            .output_bytes
                            .map_or_else(|| "—".to_string(), |bytes| format!("{bytes} B")),
                        widths[3],
                        &mut row_budget,
                    ),
                    trace_cell(
                        &trace.elapsed.map_or_else(
                            || "—".to_string(),
                            |elapsed| format!("{} µs", elapsed.as_micros()),
                        ),
                        widths[4],
                        &mut row_budget,
                    ),
                    trace_status_cell(app, trace.status, widths[5], &mut row_budget),
                ]);
            } else {
                let input = trace
                    .input_bytes
                    .map_or_else(|| "—".to_string(), |bytes| bytes.to_string());
                let output = trace
                    .output_bytes
                    .map_or_else(|| "—".to_string(), |bytes| bytes.to_string());
                cells.extend([
                    trace_cell(&format!("{input}→{output} B"), widths[2], &mut row_budget),
                    trace_status_cell(app, trace.status, widths[3], &mut row_budget),
                ]);
            }
            let row = Row::new(cells);
            if trace.status == StepStatus::Failed {
                row.style(failure_style(app))
            } else {
                row
            }
        })
        .collect::<Vec<_>>();
    let header_style = muted_style();
    let header = Row::new(
        headers
            .into_iter()
            .map(|header| Cell::from(header).style(header_style)),
    );
    let constraints = widths
        .into_iter()
        .map(|width| Constraint::Length(width as u16))
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(rows, constraints)
            .header(header)
            .column_spacing(1),
        table_area,
    );

    if let Some((detail_area, title, summary)) = detail {
        let mut lines = vec![Line::styled(title, failure_style(app))];
        if let Some(summary) = summary {
            lines.push(Line::raw(summary));
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(failure_style(app)),
            ),
            detail_area,
        );
    }
}

fn render_output(
    frame: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    mouse_regions: &mut MouseRegions,
) {
    let inner = pane_block(app, "", app.focus == Pane::Output).inner(area);
    app.reflow_output_viewport(inner);
    let title = output_title(app, area.width);
    let block = pane_block(app, &title, app.focus == Pane::Output);
    mouse_regions.output = Some(area);
    mouse_regions.output_content = Some(inner);
    frame.render_widget(block, area);

    let rows = inner.height as usize;
    let columns = inner.width as usize;
    let text = match &app.output.status {
        OutputStatus::Idle => String::new(),
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
                render_hex_table(frame, app, inner);
                return;
            }
            EffectiveView::Trace => {
                render_trace_table(frame, app, inner);
                if app.output.traces.is_empty()
                    && let OutputStatus::Failed(error) = status
                {
                    let error_area = Rect::new(
                        inner.x,
                        inner.y.saturating_add(1),
                        inner.width,
                        inner.height.saturating_sub(1),
                    );
                    let error = crate::error::escape_external(
                        &render_pipeline_error_summary(error),
                        columns.saturating_mul(rows).min(512),
                    );
                    frame.render_widget(Paragraph::new(error), error_area);
                }
                return;
            }
            EffectiveView::Unavailable => TEXT_VIEW_UNAVAILABLE_MESSAGE.to_string(),
        },
    };
    frame.render_widget(Paragraph::new(text).style(Style::default()), inner);
}

fn render_pipeline(
    frame: &mut Frame<'_>,
    app: &App,
    area: Rect,
    show_sizes: bool,
    mouse_regions: &mut MouseRegions,
) {
    let block = pane_block(app, "$ PIPELINE", app.focus == Pane::Pipeline);
    let inner = block.inner(area);
    mouse_regions.pipeline = Some(area);
    mouse_regions.pipeline_content = Some(inner);
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
            let status = if !step.enabled {
                StepStatus::Disabled
            } else if app.output.status.running_target() == Some(ExecutionTarget::Step(index)) {
                let text = format!("{prefix} [{enabled}]  › {}", step.definition.display_name);
                return ListItem::new(Span::styled(
                    text,
                    if selected {
                        selection_style(app)
                    } else if app.no_color {
                        Style::default()
                    } else {
                        Style::default().fg(YELLOW)
                    },
                ));
            } else if let Some(trace) = trace {
                trace.status
            } else {
                StepStatus::NotExecuted
            };
            let (mark, color) = match status {
                StepStatus::Succeeded => ("✓ ", GREEN),
                StepStatus::Disabled => (" ", Color::Reset),
                StepStatus::Failed => ("× ", RED),
                StepStatus::NotExecuted => ("· ", Color::Reset),
                StepStatus::Cancelled => ("− ", YELLOW),
            };
            let left = format!(
                "{prefix} [{enabled}]  {mark}{}",
                step.definition.display_name
            );
            let text = trace
                .and_then(|trace| Some((trace.input_bytes?, trace.output_bytes?)))
                .filter(|_| show_sizes)
                .map(|(input, output)| format!("{input}B→{output}B"))
                .map_or_else(
                    || left.clone(),
                    |sizes| {
                        let needed = left.width().saturating_add(1).saturating_add(sizes.width());
                        if needed > inner.width as usize {
                            left.clone()
                        } else {
                            let padding = inner.width as usize - left.width() - sizes.width();
                            format!("{left}{}{sizes}", " ".repeat(padding))
                        }
                    },
                );
            ListItem::new(Span::styled(
                text,
                if selected {
                    selection_style(app)
                } else if matches!(status, StepStatus::Disabled | StepStatus::NotExecuted) {
                    muted_style()
                } else if app.no_color {
                    Style::default()
                } else {
                    Style::default().fg(color)
                },
            ))
        })
        .collect();
    mouse_regions
        .pipeline_rows
        .extend(
            (start..start + items.len())
                .enumerate()
                .map(|(row, index)| {
                    (
                        Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                        index,
                    )
                }),
        );
    frame.render_widget(List::new(items).style(Style::default()), inner);
}

fn render_app_bar(frame: &mut Frame<'_>, _app: &App, area: Rect) {
    let title_style = Style::default().add_modifier(Modifier::BOLD);
    let line = Line::from(Span::styled(">_ TOC", title_style));
    frame.render_widget(Paragraph::new(line), area);
}

#[derive(Clone, Copy)]
struct DockCommand {
    key: Option<&'static str>,
    label: &'static str,
    divider_before: bool,
}

const INPUT_COMMANDS: &[DockCommand] = &[
    DockCommand {
        key: None,
        label: "Text editing",
        divider_before: false,
    },
    DockCommand {
        key: Some("Esc"),
        label: "Cancel",
        divider_before: true,
    },
];
const PIPELINE_COMMANDS: &[DockCommand] = &[
    DockCommand {
        key: Some("↑/↓"),
        label: "Select",
        divider_before: false,
    },
    DockCommand {
        key: Some("Shift+↑/↓"),
        label: "Move",
        divider_before: false,
    },
    DockCommand {
        key: Some("Space"),
        label: "Toggle",
        divider_before: true,
    },
    DockCommand {
        key: Some("Backspace"),
        label: "Delete",
        divider_before: true,
    },
    DockCommand {
        key: Some("Enter"),
        label: "Inspect",
        divider_before: false,
    },
    DockCommand {
        key: Some("s"),
        label: "Step",
        divider_before: false,
    },
    DockCommand {
        key: Some("z"),
        label: "Zoom",
        divider_before: false,
    },
];
const OUTPUT_COMMANDS: &[DockCommand] = &[
    DockCommand {
        key: Some("Enter"),
        label: "Pretty",
        divider_before: false,
    },
    DockCommand {
        key: Some("Shift+Enter"),
        label: "Raw",
        divider_before: false,
    },
    DockCommand {
        key: Some("v"),
        label: "View",
        divider_before: false,
    },
    DockCommand {
        key: Some("f"),
        label: "Final",
        divider_before: true,
    },
    DockCommand {
        key: Some("z"),
        label: "Zoom",
        divider_before: true,
    },
];
const GLOBAL_COMMANDS: &[DockCommand] = &[
    DockCommand {
        key: Some("Tab"),
        label: "Focus",
        divider_before: false,
    },
    DockCommand {
        key: Some("Ctrl+p"),
        label: "Add",
        divider_before: true,
    },
    DockCommand {
        key: Some("F1"),
        label: "Help",
        divider_before: false,
    },
    DockCommand {
        key: Some("Ctrl+q"),
        label: "Quit",
        divider_before: false,
    },
];

fn dock_line(
    app: &App,
    scope: &'static str,
    commands: &[DockCommand],
    width: u16,
) -> Line<'static> {
    let scope_style = if app.no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
    };
    let key_style = if app.no_color {
        Style::default()
    } else {
        Style::default()
            .fg(CYAN)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    };
    let separator_style = muted_style();
    let mut line = Line::from(Span::styled(scope, scope_style));

    for (shown, command) in commands.iter().enumerate() {
        let separator = if shown == 0 || command.divider_before {
            " │ "
        } else {
            "  "
        };
        let key = command.key.map(|key| {
            if app.no_color {
                format!("[ {key} ]")
            } else {
                format!(" {key} ")
            }
        });
        let added_width = separator.width()
            + key.as_deref().map_or(0, |key| key.width())
            + usize::from(key.is_some())
            + command.label.width();
        if line.width().saturating_add(added_width) > width as usize {
            break;
        }
        line.push_span(Span::styled(separator, separator_style));
        if let Some(key) = key {
            line.push_span(Span::styled(key, key_style));
            line.push_span(Span::raw(" "));
        }
        line.push_span(Span::raw(command.label));
    }
    line
}

fn footer_status_line(app: &App, width: usize) -> Option<Line<'static>> {
    match &app.output.status {
        OutputStatus::Failed(error) => Some(Line::raw(crate::error::escape_external(
            &render_pipeline_error_summary(error),
            width,
        ))),
        OutputStatus::Cancelled => Some(Line::raw("Cancelled")),
        status if status.long_running_notice() => Some(Line::raw(
            if app.output.active_artifact.is_some() || !app.output.traces.is_empty() {
                "Still processing · Previous result shown · Esc Cancel"
            } else {
                "Still processing · Esc Cancel"
            },
        )),
        _ if matches!(app.copy_phase, CopyPhase::Preparing { .. }) => {
            Some(Line::raw("Preparing copy…"))
        }
        _ if matches!(app.copy_phase, CopyPhase::Writing { .. }) => {
            Some(Line::raw("Writing clipboard…"))
        }
        _ => app.status.as_ref().map(|status| {
            let style = if status.starts_with("Removed ") {
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::styled(crate::error::escape_external(status, width), style)
        }),
    }
}

fn footer_first_line(app: &App, width: u16) -> Line<'static> {
    if let Some(status) = footer_status_line(app, width as usize) {
        return status;
    }
    match app.focus {
        Pane::Input => dock_line(app, "INPUT", INPUT_COMMANDS, width),
        Pane::Pipeline => dock_line(app, "PIPELINE", PIPELINE_COMMANDS, width),
        Pane::Output if app.can_copy() => dock_line(app, "OUTPUT", OUTPUT_COMMANDS, width),
        Pane::Output => dock_line(app, "OUTPUT", &OUTPUT_COMMANDS[2..], width),
    }
}

fn render_footer(
    frame: &mut Frame<'_>,
    app: &App,
    focused_help_area: Rect,
    common_help_area: Rect,
) {
    frame.render_widget(
        Paragraph::new(footer_first_line(app, focused_help_area.width)),
        focused_help_area,
    );
    frame.render_widget(
        Paragraph::new(dock_line(
            app,
            "GLOBAL",
            GLOBAL_COMMANDS,
            common_help_area.width,
        )),
        common_help_area,
    );
}

fn render_focused_pane(
    frame: &mut Frame<'_>,
    app: &mut App,
    area: Rect,
    pane: Pane,
    mode: WidthMode,
    mouse_regions: &mut MouseRegions,
) {
    match pane {
        Pane::Input => render_input(frame, app, area, mouse_regions),
        Pane::Output => render_output(frame, app, area, mouse_regions),
        Pane::Pipeline => render_pipeline(frame, app, area, mode == WidthMode::Wide, mouse_regions),
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

fn modal_block<'a>(app: &App, title: &'a str) -> Block<'a> {
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
    };
    let shadow = Shadow::overlay().style(muted_style());
    Block::bordered()
        .title(title)
        .style(Style::default())
        .border_style(style)
        .title_style(style)
        .shadow(shadow)
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

fn action_rect(area: Rect, line: &str, label: &str, centered: bool) -> Rect {
    let label_start = line
        .find(label)
        .expect("rendered action line contains its label");
    let line_width = line.width().min(area.width as usize) as u16;
    let base_x = if centered {
        area.x + area.width.saturating_sub(line_width) / 2
    } else {
        area.x
    };
    let offset = line[..label_start].width().min(area.width as usize) as u16;
    let width = label
        .width()
        .min(area.width.saturating_sub(offset) as usize) as u16;
    Rect::new(base_x.saturating_add(offset), area.y, width, 1)
}

fn render_picker(
    frame: &mut Frame<'_>,
    app: &App,
    query: &str,
    selected: usize,
    mouse_regions: &mut MouseRegions,
) {
    let area = centered(frame.area(), 72, 18);
    let block = modal_block(app, "Add transform");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let compact = inner.height < 14;
    let detail_rows = if compact {
        inner.height.saturating_sub(4).clamp(1, 2)
    } else {
        6
    };
    let separator_rows = u16::from(!compact);
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
        Constraint::Length(separator_rows),
        Constraint::Length(detail_rows),
        Constraint::Length(separator_rows),
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
            if compact {
                transform.description.to_string()
            } else {
                format!(
                    "ID        {}\nABOUT     {}\nINPUT     {}\nBEHAVIOR  {}\nTUI       Result remains bytes; Smart selects Text or Hex",
                    transform.id,
                    transform.description,
                    input_condition(transform.accepts_binary),
                    transform.behavior,
                )
            }
        },
    );
    let available = (list_area.height as usize).max(1);
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
            let prefix = if index == selected { "> " } else { "  " };
            ListItem::new(format!("{prefix}{}", transform.display_name)).style(
                if index == selected {
                    selection_style(app)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    mouse_regions.picker_content = Some(list_area);
    mouse_regions
        .picker_rows
        .extend(
            (start..start + items.len())
                .enumerate()
                .map(|(row, index)| {
                    (
                        Rect::new(list_area.x, list_area.y + row as u16, list_area.width, 1),
                        index,
                    )
                }),
        );
    frame.render_widget(List::new(items).style(Style::default()), list_area);
    if separator_rows > 0 {
        frame.render_widget(
            Paragraph::new(separator(detail_separator.width)).style(muted_style()),
            detail_separator,
        );
    }
    frame.render_widget(
        Paragraph::new(detail)
            .style(muted_style())
            .wrap(Wrap { trim: false }),
        detail_area,
    );
    if separator_rows > 0 {
        frame.render_widget(
            Paragraph::new(separator(hint_separator.width)).style(muted_style()),
            hint_separator,
        );
    }
    let hint = if compact {
        "[Enter Add] · [Esc Cancel]"
    } else {
        "↑/↓ Select · [Enter Add] · [Esc Cancel]"
    };
    frame.render_widget(Paragraph::new(hint).style(muted_style()), hint_area);
    mouse_regions.add_action = Some(action_rect(hint_area, hint, "[Enter Add]", false));
    mouse_regions.cancel_action = Some(action_rect(hint_area, hint, "[Esc Cancel]", false));
}

fn render_inspector(frame: &mut Frame<'_>, app: &App, mouse_regions: &mut MouseRegions) {
    let Some(step) = app.steps.get(app.selected_step) else {
        return;
    };
    let area = centered(frame.area(), 78, 13);
    let trace = app
        .output
        .traces
        .iter()
        .find(|trace| trace.step == app.selected_step + 1);
    let status = if !step.enabled {
        "OFF"
    } else if app.output.status.running_target() == Some(ExecutionTarget::Step(app.selected_step)) {
        "RUNNING"
    } else if let Some(trace) = trace {
        trace_status(trace.status)
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
            "{} ({})\nStatus: {status}\nOutput: {output}\nError: {compact_error}\n[Esc Close]",
            step.definition.display_name, step.definition.id,
        )
    } else {
        format!(
            "{}\nID: {}\n{}\nStatus: {status}\nInput: {input}\nOutput: {output}\nElapsed: {elapsed}\nError: {error}\n\n[Esc Close]",
            step.definition.display_name,
            step.definition.id,
            input_condition(step.definition.accepts_binary),
        )
    };
    let block = modal_block(app, "Step Inspector");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let close_line = Rect::new(
        inner.x,
        inner.y + text.lines().count().saturating_sub(1) as u16,
        inner.width,
        1,
    );
    mouse_regions.close_action = Some(action_rect(close_line, "[Esc Close]", "[Esc Close]", false));
    frame.render_widget(Paragraph::new(text).style(Style::default()), inner);
}

fn render_help(frame: &mut Frame<'_>, app: &App, mouse_regions: &mut MouseRegions) {
    let (title, body) = match app.focus {
        Pane::Input => (
            "Input Help",
            "Text editing: tui-textarea defaults\nTab / Shift+Tab  Next / previous pane\nCtrl+p  Add transform\nF1  Context help\nCtrl+q  Quit\nCtrl+c  Force quit\nEsc  Close zoom or cancel request\nMouse Click  Focus only".to_string(),
        ),
        Pane::Pipeline => (
            "Pipeline Help",
            "↑/↓  Select step\nShift+↑/↓  Reorder\nSpace  Toggle step\nBackspace  Delete step\nEnter  Inspect step\ns  Show selected step\nz  Toggle zoom\nTab / Shift+Tab  Change pane\n? / F1  Context help\nCtrl+p  Add transform\nCtrl+q  Quit\nCtrl+c  Force quit · Esc  Close zoom or cancel request\nMouse Click  Focus/select · Wheel  Move selection".to_string(),
        ),
        Pane::Output => (
            "Output Help",
            format!(
                "v  Next view\nf  Restore final\n{}\nArrows / PageUp / PageDown / Home / End  Scroll\nz  Toggle zoom\nTab / Shift+Tab  Change pane\nCtrl+p  Add transform\n? / F1  Context help\nCtrl+q  Quit\nCtrl+c  Force quit · Esc  Close zoom or cancel request\nMouse Click  Focus only · Wheel  Scroll",
                if app.can_copy() {
                    "Enter  Pretty copy\nShift+Enter  Raw copy"
                } else {
                    "Enter / Shift+Enter  Copy unavailable"
                }
            ),
        ),
    };
    let area = centered(frame.area(), 68, 17);
    let compact = area.height < 17;
    let body = if compact {
        match app.focus {
            Pane::Input => "Text edit · Tab focus\nCtrl+p Add · F1 Help\nCtrl+q Quit · Ctrl+c Force\n[Esc Close]".to_string(),
            Pane::Pipeline => "↑/↓ Select · Shift+↑/↓ Move\nSpace Toggle · Backspace Delete\nEnter Inspect · s Step · z Zoom\n[Esc Close]".to_string(),
            Pane::Output => format!(
                "v View · f Final\n{}\nArrows/Page Scroll · z Zoom\n[Esc Close]",
                if app.can_copy() {
                    "Enter Pretty · Shift+Enter Raw"
                } else {
                    "Enter / Shift+Enter unavailable"
                }
            ),
        }
    } else {
        format!("{body}\n\n[Esc Close]")
    };
    let block = modal_block(app, title);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let close_line = Rect::new(
        inner.x,
        inner.y + body.lines().count().saturating_sub(1) as u16,
        inner.width,
        1,
    );
    mouse_regions.close_action = Some(action_rect(close_line, "[Esc Close]", "[Esc Close]", false));
    frame.render_widget(Paragraph::new(body).style(Style::default()), inner);
}

fn render_confirmation(
    frame: &mut Frame<'_>,
    app: &App,
    message: &'static str,
    mouse_regions: &mut MouseRegions,
) {
    let area = centered(frame.area(), 42, 5);
    let block = modal_block(app, "Confirm");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let actions = "[Enter/y Confirm] · [n/Esc Cancel]";
    frame.render_widget(
        Paragraph::new(format!("{message}\n{actions}"))
            .alignment(Alignment::Center)
            .style(Style::default()),
        inner,
    );
    let action_line = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    mouse_regions.confirm_action =
        Some(action_rect(action_line, actions, "[Enter/y Confirm]", true));
    mouse_regions.cancel_action = Some(action_rect(action_line, actions, "[n/Esc Cancel]", true));
}

fn render_modal(frame: &mut Frame<'_>, app: &App, mouse_regions: &mut MouseRegions) {
    match &app.modal {
        Some(Modal::TransformPicker { query, selected }) => {
            render_picker(frame, app, query, *selected, mouse_regions);
        }
        Some(Modal::StepInspector) => render_inspector(frame, app, mouse_regions),
        Some(Modal::Help) => render_help(frame, app, mouse_regions),
        Some(Modal::UnsafeCopyConfirm { .. }) => {
            render_confirmation(frame, app, "Copy raw control characters?", mouse_regions);
        }
        Some(Modal::QuitConfirm) => {
            render_confirmation(frame, app, "Discard input and quit?", mouse_regions);
        }
        None => {}
    }
}

fn render_modal_layer(frame: &mut Frame<'_>, app: &App, mouse_regions: &mut MouseRegions) {
    if app.modal.is_none() {
        return;
    }
    let area = frame.area();
    frame
        .buffer_mut()
        .set_style(area, Style::default().add_modifier(Modifier::DIM));
    render_modal(frame, app, mouse_regions);
}

fn render(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let mode = width_mode(area);
    let mut mouse_regions = MouseRegions::default();
    if mode == WidthMode::Tiny {
        frame.render_widget(
            Paragraph::new("Increase terminal size to at least 40×10").alignment(Alignment::Center),
            area,
        );
        if matches!(
            app.modal.as_ref(),
            Some(Modal::UnsafeCopyConfirm { .. }) | Some(Modal::QuitConfirm)
        ) {
            render_modal_layer(frame, app, &mut mouse_regions);
        }
        app.mouse_regions = mouse_regions;
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
        render_focused_pane(frame, app, content, focused, mode, &mut mouse_regions);
    } else if mode == WidthMode::Narrow {
        let [pipeline_rows, input_rows, output_rows] = stacked_pane_heights(content.height);
        let [pipeline, input, output] = Layout::vertical([
            Constraint::Length(pipeline_rows),
            Constraint::Length(input_rows),
            Constraint::Length(output_rows),
        ])
        .areas(content);
        render_pipeline(frame, app, pipeline, false, &mut mouse_regions);
        render_input(frame, app, input, &mut mouse_regions);
        render_output(frame, app, output, &mut mouse_regions);
    } else {
        let pipeline_columns = pipeline_width(area.width, mode);
        let [pipeline, right] =
            Layout::horizontal([Constraint::Length(pipeline_columns), Constraint::Min(0)])
                .areas(content);
        let input_rows = (u32::from(right.height) * 42 / 100) as u16;
        let input_rows = input_rows.clamp(3, right.height.saturating_sub(3));
        let [input, output] =
            Layout::vertical([Constraint::Length(input_rows), Constraint::Min(3)]).areas(right);
        render_pipeline(
            frame,
            app,
            pipeline,
            mode == WidthMode::Wide,
            &mut mouse_regions,
        );
        render_input(frame, app, input, &mut mouse_regions);
        render_output(frame, app, output, &mut mouse_regions);
    }
    render_footer(frame, app, focused_help, common_help);
    render_modal_layer(frame, app, &mut mouse_regions);
    app.mouse_regions = mouse_regions;
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
        clipboard::{ClipboardPayload, CopyKind},
        state::{
            AppEvent, CopyPhase, Effect, LONG_RUNNING_AFTER, Modal, OutputSource, OutputStatus,
            Pane, debounce_for,
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
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
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

    fn assert_modal_depth(app: &mut App, frame_area: Rect, modal_area: Rect) {
        let backend = TestBackend::new(frame_area.width, frame_area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();

        assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
        let shadow = &buffer[(modal_area.right(), modal_area.y + 1)];
        assert!(shadow.modifier.contains(Modifier::DIM));
        assert_eq!(shadow.bg, Color::Reset);
        if app.no_color {
            assert!(
                buffer
                    .content()
                    .iter()
                    .all(|cell| { cell.fg == Color::Reset && cell.bg == Color::Reset })
            );
        }
        assert!(
            !buffer[(modal_area.x + 1, modal_area.y + 1)]
                .modifier
                .contains(Modifier::DIM)
        );
    }

    #[test]
    fn every_modal_dims_the_base_and_renders_a_one_cell_shadow() {
        let start = now();
        let frame_area = Rect::new(0, 0, 120, 30);

        let mut picker = App::new(start, false);
        picker.open_picker();
        assert_modal_depth(&mut picker, frame_area, centered(frame_area, 72, 18));

        let mut inspector = App::new(start, false);
        inspector.steps.push(TransformStep {
            definition: transform_by_id("base64-encode").unwrap(),
            enabled: true,
        });
        inspector.modal = Some(Modal::StepInspector);
        assert_modal_depth(&mut inspector, frame_area, centered(frame_area, 78, 13));

        let mut help = App::new(start, false);
        help.modal = Some(Modal::Help);
        assert_modal_depth(&mut help, frame_area, centered(frame_area, 68, 17));

        let mut confirm = App::new(start, false);
        confirm.modal = Some(Modal::QuitConfirm);
        assert_modal_depth(&mut confirm, frame_area, centered(frame_area, 42, 5));

        let mut unsafe_confirm = App::new(start, false);
        unsafe_confirm.modal = Some(Modal::UnsafeCopyConfirm {
            payload: ClipboardPayload {
                text: "safe fixture".to_string(),
                kind: CopyKind::Pretty,
            },
        });
        assert_modal_depth(&mut unsafe_confirm, frame_area, centered(frame_area, 42, 5));
    }

    #[test]
    fn no_color_modal_dims_without_rgb_and_uses_a_dim_shadow() {
        let frame_area = Rect::new(0, 0, 120, 30);
        let mut app = App::new(now(), true);
        app.modal = Some(Modal::Help);

        assert_modal_depth(&mut app, frame_area, centered(frame_area, 68, 17));
    }

    #[test]
    fn tiny_confirmations_dim_the_base_and_render_a_one_cell_shadow() {
        for frame_area in [Rect::new(0, 0, 39, 16), Rect::new(0, 0, 120, 9)] {
            let mut app = App::new(now(), true);
            app.modal = Some(Modal::QuitConfirm);

            assert_modal_depth(&mut app, frame_area, centered(frame_area, 42, 5));
        }
    }

    #[test]
    fn pending_preview_keeps_previous_body_and_delays_the_notice() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        app.output.source = OutputSource::Step(0);
        app.output.active_artifact = Some(Artifact::new(b"previous result".to_vec()));
        app.output.status = OutputStatus::running(start, ExecutionTarget::Final);

        let pending = rendered_app(80, 20, &mut app);
        assert!(pending.contains("previous result"));
        assert!(!pending.contains("Waiting for changes"));
        assert!(!pending.contains("Running"));
        assert!(!pending.contains("Still processing"));

        app.handle_event(AppEvent::Tick(start + LONG_RUNNING_AFTER));
        let delayed = rendered_app(80, 20, &mut app);
        assert!(delayed.contains("previous result"));
        assert!(delayed.contains("Still processing · Previous result shown"));
    }

    #[test]
    fn footer_prioritizes_preview_state_copy_progress_and_transient_status() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Output;
        app.status = Some("Copied Pretty".to_string());
        app.copy_phase = CopyPhase::Preparing { request_id: 1 };

        let preparing = rendered_app(80, 16, &mut app);
        assert!(
            preparing
                .lines()
                .nth(14)
                .unwrap()
                .contains("Preparing copy…")
        );

        app.copy_phase = CopyPhase::Writing {
            request_id: 1,
            kind: CopyKind::Pretty,
        };
        let writing = rendered_app(80, 16, &mut app);
        assert!(
            writing
                .lines()
                .nth(14)
                .unwrap()
                .contains("Writing clipboard…")
        );

        app.output.status = OutputStatus::running(start, ExecutionTarget::Final);
        app.handle_event(AppEvent::Tick(start + LONG_RUNNING_AFTER));
        let running = rendered_app(80, 16, &mut app);
        assert!(
            running
                .lines()
                .nth(14)
                .unwrap()
                .contains("Still processing")
        );
        assert!(
            !running
                .lines()
                .nth(14)
                .unwrap()
                .contains("Writing clipboard")
        );

        app.output.status = OutputStatus::Cancelled;
        let cancelled = rendered_app(80, 16, &mut app);
        assert!(cancelled.lines().nth(14).unwrap().contains("Cancelled"));
    }

    #[test]
    fn pipeline_and_output_help_include_global_escape_on_the_ctrl_c_row() {
        let start = now();
        for pane in [Pane::Pipeline, Pane::Output] {
            let mut app = App::new(start, true);
            app.focus = pane;
            key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);

            let screen = rendered_app(80, 20, &mut app);

            assert!(screen.contains("Ctrl+c  Force quit · Esc  Close zoom or cancel request"));
        }
    }

    fn click(app: &mut App, area: Rect, now: Instant) -> Vec<Effect> {
        app.handle_event(AppEvent::Mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: area.x + area.width.saturating_sub(1) / 2,
                row: area.y,
                modifiers: KeyModifiers::NONE,
            },
            now,
        ))
    }

    #[test]
    fn render_records_only_visible_panes_and_exact_pipeline_rows() {
        let mut app = App::new(now(), true);
        app.steps = (0..12)
            .map(|_| TransformStep {
                definition: transform_by_id("base64-encode").unwrap(),
                enabled: true,
            })
            .collect();
        app.selected_step = 11;

        rendered_app(80, 24, &mut app);
        let pipeline = app.mouse_regions.pipeline.unwrap();
        let input = app.mouse_regions.input.unwrap();
        let output = app.mouse_regions.output.unwrap();
        assert!(pipeline.y < input.y);
        assert!(input.y < output.y);
        assert_eq!(app.mouse_regions.pipeline_rows.last().unwrap().1, 11);
        let visible_indices = app
            .mouse_regions
            .pipeline_rows
            .iter()
            .map(|(_, index)| *index)
            .collect::<Vec<_>>();
        assert_eq!(visible_indices.last(), Some(&11));
        assert_eq!(
            visible_indices[visible_indices.len() / 2],
            visible_indices[0] + visible_indices.len() / 2
        );
        assert!(
            app.mouse_regions
                .pipeline_rows
                .windows(2)
                .all(|rows| rows[0].0.y + 1 == rows[1].0.y)
        );

        for width in [90, 120] {
            rendered_app(width, 24, &mut app);
            let pipeline = app.mouse_regions.pipeline.unwrap();
            let input = app.mouse_regions.input.unwrap();
            let output = app.mouse_regions.output.unwrap();
            assert!(pipeline.x < input.x);
            assert_eq!(input.x, output.x);
        }

        app.zoom = Some(Pane::Output);
        rendered_app(120, 24, &mut app);
        assert!(app.mouse_regions.output.is_some());
        assert!(app.mouse_regions.input.is_none());
        assert!(app.mouse_regions.pipeline.is_none());
        assert!(app.mouse_regions.pipeline_rows.is_empty());

        app.zoom = None;
        rendered_app(39, 24, &mut app);
        assert!(app.mouse_regions.input.is_none());
        assert!(app.mouse_regions.output.is_none());
        assert!(app.mouse_regions.pipeline.is_none());
    }

    #[test]
    fn picker_click_selects_then_explicit_add_and_cancel_regions_act() {
        let start = now();
        let mut app = App::new(start, true);
        app.open_picker();
        let screen = rendered_app(120, 24, &mut app);
        assert!(screen.contains("[Enter Add]"));
        assert!(screen.contains("[Esc Cancel]"));
        assert!(
            app.mouse_regions
                .picker_rows
                .iter()
                .all(|(area, _)| area.height == 1)
        );
        for rows in app.mouse_regions.picker_rows.windows(2) {
            assert_eq!(rows[1].0.y, rows[0].0.y + 1);
        }
        let second = app.mouse_regions.picker_rows[1].0;
        assert!(click(&mut app, second, start).is_empty());
        assert!(matches!(
            app.modal,
            Some(Modal::TransformPicker { selected: 1, .. })
        ));
        let selected_screen = rendered_app(120, 24, &mut app);
        assert!(selected_screen.contains("Decode canonical padded Base64 into UTF-8 text"));
        let add = app.mouse_regions.add_action.unwrap();
        assert!(matches!(
            click(&mut app, add, start).as_slice(),
            [Effect::Cancel(_)]
        ));
        assert_eq!(app.steps[0].definition.id, "base64-decode");
        assert!(app.modal.is_none());

        app.open_picker();
        rendered_app(120, 24, &mut app);
        let cancel = app.mouse_regions.cancel_action.unwrap();
        assert!(click(&mut app, cancel, start).is_empty());
        assert!(app.modal.is_none());

        app.open_picker();
        if let Some(Modal::TransformPicker { selected, .. }) = &mut app.modal {
            *selected = 7;
        }
        rendered_app(120, 24, &mut app);
        assert_eq!(app.mouse_regions.picker_rows.first().unwrap().1, 2);
        assert_eq!(app.mouse_regions.picker_rows.last().unwrap().1, 7);
        let first_visible = app.mouse_regions.picker_rows.first().unwrap().0;
        assert!(click(&mut app, first_visible, start).is_empty());
        assert!(matches!(
            app.modal,
            Some(Modal::TransformPicker { selected: 2, .. })
        ));
    }

    #[test]
    fn confirmation_and_close_regions_reuse_the_keyboard_actions() {
        let start = now();
        let mut app = App::new(start, true);
        app.modal = Some(Modal::QuitConfirm);
        let screen = rendered_app(120, 24, &mut app);
        assert!(screen.contains("[Enter/y Confirm] · [n/Esc Cancel]"));
        let confirm = app.mouse_regions.confirm_action.unwrap();
        let effects = click(&mut app, confirm, start);
        assert!(matches!(effects.as_slice(), [Effect::Quit(0)]));

        app.modal = Some(Modal::QuitConfirm);
        rendered_app(120, 24, &mut app);
        let cancel = app.mouse_regions.cancel_action.unwrap();
        assert!(click(&mut app, cancel, start).is_empty());
        assert!(app.modal.is_none());

        app.modal = Some(Modal::UnsafeCopyConfirm {
            payload: ClipboardPayload {
                text: "exact\u{1b}payload".to_string(),
                kind: CopyKind::Pretty,
            },
        });
        app.copy_phase = CopyPhase::AwaitingConfirmation { request_id: 1 };
        rendered_app(120, 24, &mut app);
        let confirm = app.mouse_regions.confirm_action.unwrap();
        let effects = click(&mut app, confirm, start);
        assert!(matches!(
            effects.as_slice(),
            [Effect::WriteClipboard {
                request_id: 1,
                payload: ClipboardPayload { text, .. },
            }] if text == "exact\u{1b}payload"
        ));

        app.modal = Some(Modal::Help);
        let screen = rendered_app(120, 24, &mut app);
        assert!(screen.contains("[Esc Close]"));
        let close = app.mouse_regions.close_action.unwrap();
        assert!(click(&mut app, close, start).is_empty());
        assert!(app.modal.is_none());

        app.steps.push(TransformStep {
            definition: transform_by_id("base64-encode").unwrap(),
            enabled: true,
        });
        app.modal = Some(Modal::StepInspector);
        rendered_app(120, 24, &mut app);
        let close = app.mouse_regions.close_action.unwrap();
        assert!(click(&mut app, close, start).is_empty());
        assert!(app.modal.is_none());
    }

    #[test]
    fn tiny_confirmation_records_only_modal_actions() {
        let start = now();
        let mut app = App::new(start, true);
        app.modal = Some(Modal::QuitConfirm);
        rendered_app(39, 9, &mut app);
        assert!(app.mouse_regions.pipeline.is_none());
        assert!(app.mouse_regions.input.is_none());
        assert!(app.mouse_regions.output.is_none());
        assert!(app.mouse_regions.confirm_action.is_some());
        assert!(app.mouse_regions.cancel_action.is_some());
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

        app.copy_phase = CopyPhase::Writing {
            request_id: 1,
            kind: CopyKind::Pretty,
        };
        app.handle_event(AppEvent::ClipboardWriteFinished {
            request_id: 1,
            kind: CopyKind::Pretty,
            result: Err(()),
            now: start,
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
            assert!(lines[0].contains(">_ TOC"));
            assert!(!lines[0].contains("FOCUS:"));
            assert!(lines[14].starts_with("OUTPUT │"));
            assert!(lines[15].starts_with("GLOBAL │"));
        }
    }

    #[test]
    fn tiny_width_or_height_shows_only_resize_guidance() {
        for (width, height) in [(39, 16), (120, 9)] {
            let screen = rendered(width, height, Pane::Input);
            assert!(screen.contains("Increase terminal size"));
            assert!(!screen.contains("TOC"));
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
        assert!(lines[0].starts_with(">_ TOC"));
        assert!(!lines[0].contains("FOCUS:"));
        assert!(!lines[0].contains('┏'));
        assert!(lines[14].starts_with("OUTPUT │"));
        assert!(lines[15].starts_with("GLOBAL │"));
        assert!(!lines.iter().any(|line| line.contains("Navigation")));
        assert!(!lines.iter().any(|line| line.contains("Step Summary")));
    }

    #[test]
    fn status_replaces_only_the_focused_help_line() {
        let start = now();
        let mut app = App::new(start, true);
        app.copy_phase = CopyPhase::Writing {
            request_id: 1,
            kind: CopyKind::Pretty,
        };
        app.handle_event(AppEvent::ClipboardWriteFinished {
            request_id: 1,
            kind: CopyKind::Pretty,
            result: Err(()),
            now: start,
        });
        let screen = rendered_app(80, 16, &mut app);
        let lines: Vec<_> = screen.lines().collect();

        assert!(lines[14].contains("Copy unavailable"));
        assert!(!lines[14].contains("INPUT │"));
        assert!(lines[15].starts_with("GLOBAL │"));
        assert!(lines[15].contains("[ Ctrl+p ] Add"));
        assert!(lines[15].contains("[ Ctrl+q ] Quit"));
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
        assert!(text.contains("» OUTPUT [SMART]"));
        assert!(text.contains("valid text"));

        app.output.source = OutputSource::Step(1);
        app.output.active_artifact = Some(Artifact::new(vec![0, 0xff]));
        let hex = rendered_app(89, 20, &mut app);
        assert!(hex.contains("» OUTPUT / STEP 02 [SMART]"));
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
        assert!(trace.contains("» OUTPUT / STEP 02 [TRACE]"));
        for expected in [
            "STEP",
            "OPERATION",
            "INPUT",
            "OUTPUT",
            "TIME",
            "STATUS",
            "#2",
            "Hex Decode",
            "OK",
        ] {
            assert!(trace.contains(expected), "missing {expected}: {trace}");
        }
    }

    #[test]
    fn app_bar_omits_focus_and_output_title_shows_only_useful_source_and_size() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"valid text".to_vec()));

        let final_screen = rendered_app(120, 20, &mut app);
        assert!(final_screen.lines().next().unwrap().starts_with(">_ TOC"));
        assert!(!final_screen.contains("FOCUS:"));
        assert!(final_screen.contains("» OUTPUT [SMART] · 10 B"));
        assert!(!final_screen.contains("/ FINAL"));

        app.output.source = OutputSource::Step(1);
        let step_screen = rendered_app(120, 20, &mut app);
        assert!(step_screen.contains("» OUTPUT / STEP 02 [SMART] · 10 B"));

        app.output.status = OutputStatus::Debouncing { deadline: now() };
        let pending = rendered_app(120, 20, &mut app);
        assert!(pending.contains("» OUTPUT / STEP 02 [SMART]"));
        assert!(!pending.contains("BYTE"));
        assert!(!pending.contains("SMART · 10 B"));
    }

    #[test]
    fn output_title_brackets_the_view_and_keeps_only_total_size() {
        let mut app = App::new(now(), true);
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(vec![b'x'; 100]));

        app.output.view = ViewMode::Text;
        app.output.byte_offset = 12;
        assert_eq!(output_title(&app, 120), "» OUTPUT [TEXT] · 100 B");

        app.output.view = ViewMode::Hex;
        app.output.row_offset = 2;
        assert_eq!(output_title(&app, 120), "» OUTPUT [HEX] · 100 B");

        app.output.view = ViewMode::Trace;
        app.output.source = OutputSource::Step(1);
        assert_eq!(
            output_title(&app, 120),
            "» OUTPUT / STEP 02 [TRACE] · 100 B"
        );

        app.output.source = OutputSource::Final;
        assert_eq!(output_title(&app, 20), "» OUTPUT [TRACE]");

        app.output.status = OutputStatus::running(now(), ExecutionTarget::Final);
        assert_eq!(output_title(&app, 120), "» OUTPUT [TRACE]");
    }

    #[test]
    fn resized_hex_title_keeps_corrected_row_offset_without_a_counter() {
        let mut app = App::new(now(), true);
        app.output.status = OutputStatus::Ready;
        app.output.view = ViewMode::Hex;
        app.output.active_artifact = Some(Artifact::new(vec![0xff; 80]));
        app.output.row_offset = 7;
        app.reflow_output_viewport(Rect::new(0, 0, 59, 4));

        app.reflow_output_viewport(Rect::new(0, 0, 78, 4));

        assert_eq!(app.output.row_offset, 2);
        assert_eq!(output_title(&app, 120), "» OUTPUT [HEX] · 80 B");
    }

    #[test]
    fn dock_and_help_show_lowercase_current_keys_without_hangul_aliases() {
        let mut pipeline_app = App::new(now(), true);
        pipeline_app.focus = Pane::Pipeline;
        let pipeline = rendered_app(120, 20, &mut pipeline_app);
        assert!(pipeline.contains("[ Backspace ] Delete"));
        assert!(pipeline.contains("[ s ] Step"));
        assert!(!pipeline.contains("Delete/d"));
        assert!(!pipeline.contains("[ a ] Add"));

        let mut output_app = App::new(now(), true);
        output_app.focus = Pane::Output;
        output_app.output.status = OutputStatus::Ready;
        output_app.output.active_artifact = Some(Artifact::new(b"copyable".to_vec()));
        let output = rendered_app(120, 20, &mut output_app);
        assert!(!output.contains("[ p ] Step"));
        assert!(output.contains("[ f ] Final"));
        for removed in ["F3", "F4", "v/V", "Enter/y", "ㅔ", "ㅂ"] {
            assert!(!output.contains(removed), "unexpected {removed}: {output}");
        }
    }

    #[test]
    fn add_transform_uses_one_line_names_and_exact_detail_metadata() {
        let mut app = App::new(now(), true);
        app.open_picker();

        let screen = rendered_app(80, 20, &mut app);
        let selected = screen
            .lines()
            .find(|line| line.contains("> Base64 Encode"))
            .unwrap();
        assert!(!selected.contains("[base64-encode]"));
        assert!(!selected.contains("Encode bytes"));
        for expected in [
            "ID        base64-encode",
            "ABOUT     Encode bytes using padded RFC 4648 Base64",
            "INPUT     Bytes accepted",
            "BEHAVIOR",
            "TUI       Result remains bytes; Smart selects Text or Hex",
        ] {
            assert!(screen.contains(expected), "missing {expected}: {screen}");
        }
        assert!(!screen.contains("Backspace Search"));
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

        app.output.status = OutputStatus::running(start, ExecutionTarget::Step(0));
        app.output.source = OutputSource::Step(0);
        app.output.traces.clear();
        let running = rendered_app(80, 20, &mut app);
        assert!(running.contains("Status: RUNNING"));
    }

    #[test]
    fn one_context_help_modal_lists_only_real_keys_for_each_pane() {
        let start = now();
        for (pane, expected, mouse_help) in [
            (
                Pane::Input,
                &[
                    "Input Help",
                    "Tab",
                    "Ctrl+p",
                    "F1",
                    "Ctrl+q",
                    "Ctrl+c",
                    "Esc",
                ][..],
                "Mouse Click  Focus only",
            ),
            (
                Pane::Pipeline,
                &[
                    "Pipeline Help",
                    "↑/↓",
                    "Shift+↑/↓",
                    "Backspace",
                    "Enter",
                    "s",
                    "z",
                    "? / F1",
                    "Ctrl+p",
                    "Ctrl+c",
                ][..],
                "Mouse Click  Focus/select · Wheel  Move selection",
            ),
            (
                Pane::Output,
                &[
                    "Output Help",
                    "v",
                    "f",
                    "Enter",
                    "Shift+Enter",
                    "z",
                    "? / F1",
                    "Ctrl+p",
                    "Ctrl+c",
                ][..],
                "Mouse Click  Focus only · Wheel  Scroll",
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
            assert!(
                screen.contains(mouse_help),
                "missing {mouse_help}: {screen}"
            );
            assert!(screen.contains("F1"));
            assert!(screen.contains("[Esc Close]"));
            for removed in [
                "Delete/d",
                "a  Add transform",
                "p  Show selected step",
                "ㅔ",
                "ㄴ",
            ] {
                assert!(!screen.contains(removed), "unexpected {removed}: {screen}");
            }
        }
    }

    #[test]
    fn compact_add_transform_keeps_a_separate_selected_description() {
        let mut app = App::new(now(), true);
        app.open_picker();

        let screen = rendered_app(40, 10, &mut app);

        assert!(screen.contains("Search:"));
        let mut lines = screen.lines();
        let selected = lines.find(|line| line.contains("> Base64 Encode")).unwrap();
        assert!(!selected.contains("[base64-encode]"));
        assert!(!selected.contains("Encode bytes"));
        assert!(!lines.next().unwrap().contains("Encode bytes"));
        assert!(screen.contains("Encode bytes"));
        assert!(screen.contains("Enter Add"));
        assert!(screen.contains("Esc Cancel"));
        assert!(!screen.contains("Backspace Search"));
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
            "[Esc Close]",
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

            for expected in [title, "[Esc Close]"] {
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
            assert!(screen.contains("Enter / Shift+Enter  Copy unavailable"));
        }

        let mut copyable = App::new(start, true);
        copyable.focus = Pane::Output;
        copyable.output.status = OutputStatus::Ready;
        copyable.output.active_artifact = Some(Artifact::new(b"ready".to_vec()));
        key(&mut copyable, KeyCode::F(1), KeyModifiers::NONE, start);
        let screen = rendered_app(80, 20, &mut copyable);
        assert!(screen.contains("Enter  Pretty copy"));
        assert!(screen.contains("Shift+Enter  Raw copy"));
        assert!(!screen.contains("Copy unavailable"));
        assert!(!screen.contains("Enter/y"));
    }

    #[test]
    fn smart_failure_shows_trace_while_pinned_views_show_safe_guidance() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Failed(PipelineError::TooManySteps { max: 32 });
        app.output.active_artifact = Some(Artifact::new(b"stale secret".to_vec()));

        let smart = rendered_app(89, 20, &mut app);
        assert!(smart.contains("» OUTPUT [SMART]"));
        for header in ["STEP", "OPERATION", "INPUT", "OUTPUT", "TIME", "STATUS"] {
            assert!(smart.contains(header), "missing {header}: {smart}");
        }
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

        assert!(screen.contains("» OUTPUT [TEXT]"));
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
    fn trace_table_uses_display_names_columns_and_safe_first_failure_detail() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.zoom = Some(Pane::Output);
        app.output.status = OutputStatus::Failed(PipelineError::Step {
            step: 2,
            transform_id: "format-json",
            source: TransformError::InvalidUtf8Output {
                preview_hex: "736563726574".to_string(),
                total_bytes: 6,
            },
        });
        app.output.view = ViewMode::Trace;
        app.output.traces = vec![
            StepTrace {
                step: 1,
                transform_id: "base64-decode",
                input_bytes: Some(24),
                output_bytes: Some(17),
                elapsed: Some(Duration::from_micros(80)),
                status: StepStatus::Succeeded,
                error: None,
            },
            StepTrace {
                step: 2,
                transform_id: "format-json",
                input_bytes: Some(17),
                output_bytes: None,
                elapsed: None,
                status: StepStatus::Failed,
                error: Some(TransformError::InvalidUtf8Output {
                    preview_hex: "736563726574".to_string(),
                    total_bytes: 6,
                }),
            },
        ];

        let wide = rendered_app(120, 12, &mut app);
        for expected in [
            "STEP",
            "OPERATION",
            "INPUT",
            "OUTPUT",
            "TIME",
            "STATUS",
            "Base64 Decode",
            "JSON Prettify",
            "OK",
            "ERROR",
            "STEP 2 · JSON Prettify",
            "output is not valid UTF-8 (6 bytes)",
        ] {
            assert!(wide.contains(expected), "missing {expected}: {wide}");
        }
        assert!(!wide.contains("736563726574"));
        assert!(!wide.contains("secret"));

        let compact = rendered_app(69, 12, &mut app);
        assert!(compact.contains("SIZE"));
        assert!(compact.contains("24→17 B"));
        assert!(!compact.contains("TIME"));
    }

    #[test]
    fn trace_operation_width_follows_the_longest_registered_name() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.view = ViewMode::Trace;
        app.output.status = OutputStatus::Ready;
        app.output.traces = ["url-encode", "base64-decode"]
            .into_iter()
            .enumerate()
            .map(|(index, transform_id)| StepTrace {
                step: index + 1,
                transform_id,
                input_bytes: Some(3),
                output_bytes: Some(4),
                elapsed: None,
                status: StepStatus::Succeeded,
                error: None,
            })
            .collect();

        let screen = rendered_app(120, 20, &mut app);
        let header = screen
            .lines()
            .find(|line| line.contains("OPERATION"))
            .unwrap();
        let operation = header.find("OPERATION").unwrap();
        let input = header.find("INPUT").unwrap();
        assert_eq!(
            header[operation..input].width(),
            "Base64 Decode".width() + 1
        );
    }

    #[test]
    fn trace_status_cells_keep_color_and_no_color_meaning() {
        let statuses = [
            (StepStatus::Succeeded, "OK", Some(GREEN)),
            (StepStatus::Disabled, "OFF", None),
            (StepStatus::Failed, "ERROR", Some(RED)),
            (StepStatus::NotExecuted, "NOT RUN", None),
            (StepStatus::Cancelled, "CANCELLED", Some(YELLOW)),
        ];
        let traces = statuses
            .iter()
            .enumerate()
            .map(|(index, (status, _, _))| StepTrace {
                step: index + 1,
                transform_id: "base64-encode",
                input_bytes: None,
                output_bytes: None,
                elapsed: None,
                status: *status,
                error: None,
            })
            .collect::<Vec<_>>();

        let mut colored = App::new(now(), false);
        colored.output.traces = traces.clone();
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_trace_table(frame, &colored, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        for (status, label, color) in statuses {
            assert_eq!(trace_status(status), label);
            let symbols = label
                .chars()
                .map(|symbol| symbol.to_string())
                .collect::<Vec<_>>();
            let cell = buffer
                .content()
                .chunks(80)
                .find_map(|row| {
                    row.windows(symbols.len()).find_map(|cells| {
                        cells
                            .iter()
                            .map(|cell| cell.symbol())
                            .eq(symbols.iter().map(String::as_str))
                            .then_some(&cells[0])
                    })
                })
                .unwrap();
            assert_eq!(
                cell.fg,
                color.unwrap_or(Color::Reset),
                "wrong color for {label}"
            );
            assert_eq!(cell.bg, Color::Reset, "wrong background for {label}");
            if color.is_none() {
                assert!(cell.modifier.contains(Modifier::DIM));
            }
        }

        let mut no_color = App::new(now(), true);
        no_color.output.traces = traces;
        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_trace_table(frame, &no_color, frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for (_, label, _) in statuses {
            assert!(screen.contains(label));
        }
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
        );
    }

    #[test]
    fn short_trace_prioritizes_failure_and_uses_remaining_detail_space() {
        let mut app = App::new(now(), true);
        app.output.traces = vec![
            StepTrace {
                step: 1,
                transform_id: "base64-decode",
                input_bytes: Some(24),
                output_bytes: Some(17),
                elapsed: None,
                status: StepStatus::Succeeded,
                error: None,
            },
            StepTrace {
                step: 2,
                transform_id: "url-decode",
                input_bytes: Some(17),
                output_bytes: Some(17),
                elapsed: None,
                status: StepStatus::Succeeded,
                error: None,
            },
            StepTrace {
                step: 3,
                transform_id: "format-json",
                input_bytes: Some(17),
                output_bytes: None,
                elapsed: None,
                status: StepStatus::Failed,
                error: Some(TransformError::InvalidUtf8Output {
                    preview_hex: "736563726574".to_string(),
                    total_bytes: 6,
                }),
            },
        ];
        let backend = TestBackend::new(69, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_trace_table(frame, &app, frame.area()))
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .chunks(69)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(screen.contains("#3"));
        assert!(screen.contains("ERROR"));
        assert!(screen.contains("STEP 3 · JSON Prettify"));
        assert!(screen.contains("output is not valid UTF-8 (6 bytes)"));
        assert!(!screen.contains("#1"));

        let backend = TestBackend::new(69, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_trace_table(frame, &app, frame.area()))
            .unwrap();
        let shortest = terminal
            .backend()
            .buffer()
            .content()
            .chunks(69)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(shortest.contains("#3"));
        assert!(shortest.contains("ERROR"));
        assert!(shortest.contains("output is not valid UTF-8 (6 bytes)"));
        assert!(!shortest.contains("STEP 3 · JSON Prettify"));
        assert!(!shortest.contains("736563726574"));
        assert!(!shortest.contains("secret"));
    }

    #[test]
    fn trace_table_never_exposes_invalid_utf8_preview() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.zoom = Some(Pane::Output);
        app.output.status = OutputStatus::Failed(PipelineError::Step {
            step: 1,
            transform_id: "format-json",
            source: TransformError::InvalidUtf8Output {
                preview_hex: "736563726574".to_string(),
                total_bytes: 6,
            },
        });
        app.output.view = ViewMode::Trace;
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "format-json",
            input_bytes: Some(6),
            output_bytes: None,
            elapsed: None,
            status: StepStatus::Failed,
            error: Some(TransformError::InvalidUtf8Output {
                preview_hex: "736563726574".to_string(),
                total_bytes: 6,
            }),
        });

        let screen = rendered_app(120, 12, &mut app);

        assert!(screen.contains("output is not valid UTF-8 (6 bytes)"));
        assert!(!screen.contains("736563726574"));
        assert!(!screen.contains("secret"));
        assert!(!screen.contains('\u{1b}'));
    }

    #[test]
    fn composed_hex_and_trace_views_reserve_header_space_inside_the_budget() {
        const OPERATION: &str = "very-long-transform-operation-name-used-to-fill-the-bounded-trace-view-without-rendering-any-input-or-output-body";
        let mut app = App::new(now(), true);
        app.output.traces = (1..=32)
            .map(|step| StepTrace {
                step,
                transform_id: OPERATION,
                input_bytes: Some(step),
                output_bytes: Some(step),
                elapsed: Some(Duration::from_micros(1)),
                status: if step == 32 {
                    StepStatus::Failed
                } else {
                    StepStatus::Succeeded
                },
                error: (step == 32).then_some(TransformError::InvalidUtf8Output {
                    preview_hex: "736563726574".to_string(),
                    total_bytes: 6,
                }),
            })
            .collect::<Vec<_>>();
        let backend = TestBackend::new(1_000, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_trace_table(frame, &app, frame.area()))
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .chunks(1_000)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        let rendered_rows = screen
            .lines()
            .filter(|line| line.contains(OPERATION))
            .count()
            - 1;
        let detail_title = format!("STEP 32 · {OPERATION}");
        let detail_summary = "output is not valid UTF-8 (6 bytes)";
        let header_cost = ["STEP", "OPERATION", "INPUT", "OUTPUT", "TIME", "STATUS"]
            .into_iter()
            .map(str::len)
            .sum::<usize>();

        assert!(screen.contains("STEP"));
        assert!(screen.contains(&detail_title));
        assert!(screen.contains(detail_summary));
        assert!(
            header_cost + detail_title.len() + detail_summary.len() + rendered_rows * 1_000
                <= VISIBLE_TEXT_BYTE_BUDGET
        );
    }

    #[test]
    fn hex_table_adapts_columns_and_keeps_no_color_structure() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.zoom = Some(Pane::Output);
        app.output.status = OutputStatus::Ready;
        app.output.view = ViewMode::Hex;
        app.output.active_artifact = Some(Artifact::new((0..32).collect()));

        let full = rendered_app(80, 10, &mut app);
        assert!(full.contains("00 01 02 03 04 05 06 07"));
        assert!(full.contains("08 09 0A 0B 0C 0D 0E 0F"));
        assert!(full.contains("ASCII"));

        let middle = rendered_app(62, 10, &mut app);
        assert!(middle.contains("08 09 0A 0B 0C 0D 0E 0F"));
        assert!(!middle.contains("ASCII"));

        let narrow = rendered_app(40, 10, &mut app);
        assert!(narrow.contains("00000008"));
        assert!(!narrow.contains("8–15"));
    }

    #[test]
    fn hex_table_styles_every_cell_kind_and_disables_all_colors() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), false);
        app.focus = Pane::Output;
        app.zoom = Some(Pane::Output);
        app.output.status = OutputStatus::Ready;
        app.output.view = ViewMode::Hex;
        app.output.active_artifact = Some(Artifact::new(vec![0x00, b'A']));
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let buffer = terminal.backend().buffer();
        let (header_row, header) = buffer
            .content()
            .chunks(80)
            .enumerate()
            .find_map(|(row, cells)| {
                let header = cells.windows(6).position(|cells| {
                    cells
                        .iter()
                        .map(|cell| cell.symbol())
                        .eq(["O", "F", "F", "S", "E", "T"])
                })?;
                Some((row as u16, header as u16))
            })
            .unwrap();
        let (row, offset, hex) = buffer
            .content()
            .chunks(80)
            .enumerate()
            .find_map(|(row, cells)| {
                let offset = cells.windows(8).position(|cells| {
                    cells
                        .iter()
                        .map(|cell| cell.symbol())
                        .eq(["0", "0", "0", "0", "0", "0", "0", "0"])
                })?;
                let hex = cells.windows(5).position(|cells| {
                    cells
                        .iter()
                        .map(|cell| cell.symbol())
                        .eq(["0", "0", " ", "4", "1"])
                })?;
                Some((row as u16, offset as u16, hex as u16))
            })
            .unwrap();
        let cells = buffer.content().chunks(80).nth(row as usize).unwrap();
        let ascii = cells
            .iter()
            .enumerate()
            .skip((hex + 5) as usize)
            .find_map(|(column, cell)| (cell.symbol() == ".").then_some(column as u16))
            .unwrap();

        assert_eq!(buffer[(header, header_row)].fg, Color::Reset);
        assert!(
            buffer[(header, header_row)]
                .modifier
                .contains(Modifier::DIM)
        );
        assert_eq!(buffer[(offset, row)].fg, CYAN);
        assert_eq!(buffer[(hex, row)].fg, YELLOW);
        assert_eq!(buffer[(hex + 3, row)].fg, Color::Reset);
        assert_eq!(buffer[(hex + 6, row)].fg, Color::Reset);
        assert!(buffer[(hex + 6, row)].modifier.contains(Modifier::DIM));
        assert_eq!(buffer[(ascii, row)].fg, Color::Reset);
        assert!(buffer[(ascii, row)].modifier.contains(Modifier::DIM));
        assert_eq!(buffer[(ascii + 1, row)].fg, GREEN);
        assert_eq!(buffer[(ascii + 2, row)].fg, Color::Reset);
        assert!(buffer[(ascii + 2, row)].modifier.contains(Modifier::DIM));

        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.zoom = Some(Pane::Output);
        app.output.status = OutputStatus::Ready;
        app.output.view = ViewMode::Hex;
        app.output.active_artifact = Some(Artifact::new(vec![0x00, b'A']));
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|cell| cell.fg == Color::Reset)
        );
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
        app.output.status = OutputStatus::running(now(), ExecutionTarget::Step(0));

        let screen = rendered_app(80, 16, &mut app);
        assert!(screen.contains("[ON]  › URL Encode"));
        assert!(!screen.contains("RUNNING"));
    }

    #[test]
    fn disabled_pipeline_row_after_failed_predecessor_has_blank_runtime_mark() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Pipeline;
        app.steps = ["base64-encode", "hex-encode"]
            .into_iter()
            .enumerate()
            .map(|(index, id)| TransformStep {
                definition: transform_by_id(id).unwrap(),
                enabled: index == 0,
            })
            .collect();
        app.output.traces = vec![
            StepTrace {
                step: 1,
                transform_id: "base64-encode",
                input_bytes: Some(1),
                output_bytes: None,
                elapsed: None,
                status: StepStatus::Failed,
                error: Some(TransformError::InvalidBase64 { position: None }),
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
        let not_run = screen
            .lines()
            .find(|line| line.contains("Hex Encode"))
            .unwrap();

        assert!(not_run.contains("[OFF]   Hex Encode"));
    }

    #[test]
    fn disabled_step_inspector_uses_current_enablement_before_trace_status() {
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

        assert!(screen.contains("Status: OFF"));
        assert!(!screen.contains("Status: NOT RUN"));
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
        app.focus = Pane::Pipeline;
        key(&mut app, KeyCode::Char('s'), KeyModifiers::NONE, start);
        app.focus = Pane::Output;
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
        assert!(screen.contains("» OUTPUT [TRACE]"));
        for expected in ["STEP", "OPERATION", "INPUT", "OUTPUT", "TIME", "STATUS"] {
            assert!(screen.contains(expected), "missing {expected}: {screen}");
        }
        assert!(screen.contains("#2"));
        assert!(screen.contains("Hex Encode"));
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
        app.output.status = OutputStatus::running(now(), ExecutionTarget::Step(0));
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
        let size = "3B→4B";
        let row = wide.lines().find(|line| line.contains(size)).unwrap();
        let start = row.find(size).unwrap();
        assert_eq!(
            row[..start].width(),
            pipeline_width(120, WidthMode::Wide) as usize - 1 - size.width()
        );
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
        app.output.status = OutputStatus::running(now(), ExecutionTarget::Final);

        let screen = rendered_app(89, 20, &mut app);

        assert!(!screen.contains("RUNNING"));
    }

    #[test]
    fn pending_step_overrides_preserved_trace_status_in_pipeline() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "url-encode",
            input_bytes: Some(3),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });
        app.output.status = OutputStatus::running(start, ExecutionTarget::Step(0));

        let screen = rendered_app(80, 16, &mut app);
        let row = screen
            .lines()
            .find(|line| line.contains("URL Encode"))
            .unwrap();

        assert!(row.contains("[ON]  › URL Encode"));
    }

    #[test]
    fn pending_step_overrides_preserved_trace_status_in_inspector() {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.output.traces.push(StepTrace {
            step: 1,
            transform_id: "url-encode",
            input_bytes: Some(3),
            output_bytes: Some(4),
            elapsed: None,
            status: StepStatus::Succeeded,
            error: None,
        });
        app.output.status = OutputStatus::running(start, ExecutionTarget::Step(0));
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);

        let screen = rendered_app(80, 20, &mut app);

        assert!(screen.contains("Status: RUNNING"));
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
        app.output.status = OutputStatus::running(now(), ExecutionTarget::Step(0));

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
        app.output.status = OutputStatus::running(now(), ExecutionTarget::Step(0));

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
            let title = match pane {
                Pane::Input => "> INPUT",
                Pane::Output => "» OUTPUT",
                Pane::Pipeline => "$ PIPELINE",
            };

            assert!(screen.contains(title));
            for hidden in [Pane::Input, Pane::Output, Pane::Pipeline] {
                if hidden != pane {
                    let hidden_title = match hidden {
                        Pane::Input => "> INPUT",
                        Pane::Output => "» OUTPUT",
                        Pane::Pipeline => "$ PIPELINE",
                    };
                    assert!(!screen.contains(hidden_title));
                }
            }
        }
    }
    #[test]
    fn clipboard_failure_replaces_contextual_help_at_every_usable_width() {
        for (width, height) in [(120, 16), (90, 13), (40, 10)] {
            let start = now();
            let mut app = App::new(start, true);
            app.copy_phase = CopyPhase::Writing {
                request_id: 1,
                kind: CopyKind::Pretty,
            };
            app.handle_event(AppEvent::ClipboardWriteFinished {
                request_id: 1,
                kind: CopyKind::Pretty,
                result: Err(()),
                now: start,
            });

            let screen = rendered_app(width, height, &mut app);
            let context = screen.lines().rev().nth(1).unwrap();

            assert!(context.starts_with("Copy unavailable"));
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
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
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
        app.output.status = OutputStatus::running(now(), ExecutionTarget::Step(0));
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(screen.contains("[ON]  › URL Encode"));
        assert!(screen.contains(">_ TOC"));
        assert!(screen.contains("GLOBAL │"));
        assert!(pane_style(&app, true).add_modifier.contains(Modifier::BOLD));
        assert!(pane_style(&app, false).add_modifier.contains(Modifier::DIM));
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| { cell.fg == Color::Reset && cell.bg == Color::Reset })
        );
    }
    #[test]
    fn colored_render_uses_terminal_defaults_and_ansi_role_colors() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), false);
        app.steps.push(TransformStep {
            definition: transform_by_id("url-encode").unwrap(),
            enabled: true,
        });
        app.focus = Pane::Pipeline;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let approved = [Color::Reset, CYAN, GREEN, YELLOW, RED];
        for cell in terminal.backend().buffer().content() {
            assert!(approved.contains(&cell.fg), "unexpected fg: {:?}", cell.fg);
            assert_eq!(cell.bg, Color::Reset);
            assert!(!matches!(cell.fg, Color::Rgb(..) | Color::Indexed(..)));
        }
        assert!(pane_style(&app, true).add_modifier.contains(Modifier::BOLD));
        assert!(
            selection_style(&app)
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn grouped_command_dock_keeps_atomic_groups_at_wide_and_narrow_widths() {
        let mut app = App::new(now(), true);
        app.focus = Pane::Output;
        app.output.status = OutputStatus::Ready;
        app.output.active_artifact = Some(Artifact::new(b"copyable".to_vec()));

        let wide = rendered_app(120, 20, &mut app);
        let wide_lines = wide.lines().collect::<Vec<_>>();
        assert!(wide_lines[18].contains(
            "OUTPUT │ [ Enter ] Pretty  [ Shift+Enter ] Raw  [ v ] View │ [ f ] Final │ [ z ] Zoom"
        ));
        assert!(
            wide_lines[19]
                .contains("GLOBAL │ [ Tab ] Focus │ [ Ctrl+p ] Add  [ F1 ] Help  [ Ctrl+q ] Quit")
        );

        let narrow = rendered_app(40, 10, &mut app);
        let narrow_lines = narrow.lines().collect::<Vec<_>>();
        assert!(narrow_lines[8].starts_with("OUTPUT │ [ Enter ] Pretty"));
        assert!(!narrow_lines[8].contains("[ Shift+Enter ]"));
        assert!(!narrow_lines[8].contains("[ v ] View"));
        assert!(!narrow_lines[8].contains("[ p ]"));
        assert!(narrow_lines[9].starts_with("GLOBAL │ [ Tab ] Focus"));
        assert!(narrow_lines[9].contains("[ Ctrl+p ] Add"));
        assert!(!narrow_lines[9].contains("[ F1 ]"));
    }

    #[test]
    fn colored_keycap_uses_reverse_while_no_color_uses_brackets() {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut colored = App::new(now(), false);
        colored.focus = Pane::Output;
        colored.output.status = OutputStatus::Ready;
        colored.output.active_artifact = Some(Artifact::new(b"copyable".to_vec()));
        terminal.draw(|frame| render(frame, &mut colored)).unwrap();

        let buffer = terminal.backend().buffer();
        let footer: String = (0..120).map(|x| buffer[(x, 18)].symbol()).collect();
        let enter_x = footer.find("Enter").unwrap() as u16;
        let enter = &buffer[(enter_x, 18)];
        assert_eq!(enter.fg, CYAN);
        assert_eq!(enter.bg, Color::Reset);
        assert!(enter.modifier.contains(Modifier::BOLD));
        assert!(enter.modifier.contains(Modifier::REVERSED));

        let mut no_color = App::new(now(), true);
        no_color.focus = Pane::Output;
        no_color.output.status = OutputStatus::Ready;
        no_color.output.active_artifact = Some(Artifact::new(b"copyable".to_vec()));
        let screen = rendered_app(120, 20, &mut no_color);
        assert!(screen.lines().nth(18).unwrap().contains("[ Enter ] Pretty"));
    }

    #[test]
    fn removed_status_is_reversed_and_bold_without_changing_footer_priority() {
        for no_color in [false, true] {
            let backend = TestBackend::new(120, 20);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = App::new(now(), no_color);
            app.focus = Pane::Pipeline;
            app.status = Some("Removed URL Encode".to_string());

            terminal.draw(|frame| render(frame, &mut app)).unwrap();

            let buffer = terminal.backend().buffer();
            let row = 18_u16;
            let line = buffer.content()[row as usize * 120..(row as usize + 1) * 120]
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            let start = line.find("Removed URL Encode").unwrap() as u16;
            for x in start..start + "Removed URL Encode".len() as u16 {
                assert!(buffer[(x, row)].modifier.contains(Modifier::REVERSED));
                assert!(buffer[(x, row)].modifier.contains(Modifier::BOLD));
            }
        }
    }

    #[test]
    fn external_status_and_error_text_cannot_reach_the_render_buffer_as_controls() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), true);
        app.status = Some("clipboard\n\u{1b}[2J".to_string());
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
            assert!(screen.contains("[Enter/y Confirm]"));
            assert!(screen.contains("[n/Esc Cancel]"));
            assert!(!screen.contains("hidden-payload"));

            let mut quit = App::new(now(), true);
            quit.modal = Some(Modal::QuitConfirm);
            let screen = rendered_app(width, height, &mut quit);

            assert!(screen.contains("Discard input and quit?"));
            assert!(screen.contains("[Enter/y Confirm]"));
            assert!(screen.contains("[n/Esc Cancel]"));
        }
    }
}
