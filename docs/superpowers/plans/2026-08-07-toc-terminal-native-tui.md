# toc Terminal-Native TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the TUI inherit the user's terminal theme while simplifying its keys, picker, titles, notifications, and data-column alignment.

**Architecture:** Keep the existing `App` state machine and Ratatui renderer boundaries. Change only the concrete key branches and rendering calculations in `src/tui/{state,render}.rs`; reuse current transform metadata, request flow, status deadline, rendering budget, and mouse-region structures.

**Tech Stack:** Rust 2024, Ratatui 0.30.2, Crossterm 0.29.0, tui-textarea-2, Unicode Width, Bash/Zsh Expect PTY smoke tests.

## Global Constraints

- Keep the App Bar exactly `>_ TOC`.
- Use terminal-default foreground/background and terminal-defined ANSI Cyan, Green, Yellow, and Red; do not query the terminal background.
- Preserve `NO_COLOR` without removing structural symbols, reverse video, bold focus, or dimmed secondary content.
- Keep global `Ctrl+p`/`Ctrl+ㅔ` Add and remove Pipeline `a`/`ㅁ` Add.
- Make Pipeline `Backspace` the only delete key and Pipeline `s`/`ㄴ` the only selected-Step key.
- Keep Picker character filtering and Backspace query editing; remove only the `Backspace Search` hint.
- Keep the full byte size in Ready Output titles and remove every `BYTE current/total` and `ROW current/total` counter.
- Preserve the transform engine, public CLI, clipboard worker, 64 MiB Output limit, 4 KiB rendering budget, dependencies, and responsive pane layout.
- Do not add a theme registry, configurable keymap, generic notification type, scrollbar, external icon font, or new dependency.
- Update affected README and historical TUI design notes in the same commit as each behavior change.
- Preserve the user's untracked `.codex/` directory.

---

## File Map

- `src/tui.rs`: terminal color role constants and TUI startup.
- `src/tui/state.rs`: textarea styles, real key dispatch, Step requests, delete status, and state tests.
- `src/tui/render.rs`: terminal-native styles, titles, Dock/Help, Picker, notification styling, Pipeline rows, Trace table, and render tests.
- `tests/shell-smoke.sh`: Bash/Zsh PTY contracts for real keys, Picker, and transient-status expiry.
- `README.md`: current user-visible TUI contract and latest verified results.
- `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md`: historical Output source/key contract superseded by the approved design.
- `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md`: historical theme note superseded by the approved design.
- `docs/superpowers/specs/2026-08-01-toc-tui-shortcuts-output-design.md`: historical title/key note superseded by the approved design.
- `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md`: historical Picker layout note superseded by the approved design.
- `docs/superpowers/specs/2026-07-31-toc-tui-mouse-design.md`: historical two-row Picker hitbox note superseded by the approved design.
- `docs/superpowers/specs/2026-08-07-toc-terminal-native-tui-design.md`: approved source of truth; mark implemented only after the final verification task.

---

### Task 1: Terminal-native styles and Output titles

**Files:**
- Modify: `src/tui.rs:31-39`
- Modify: `src/tui/state.rs:224-270`
- Modify: `src/tui/render.rs:61-168, 675-892, 1270-1320, 1400-1510, 2090-2185, 3500-3590`
- Modify: `README.md:55-97`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md:10-20, 120-135`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md:350-370`
- Modify: `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md:10-20`
- Modify: `docs/superpowers/specs/2026-08-01-toc-tui-shortcuts-output-design.md:125-160`

**Interfaces:**
- Consumes: `App::no_color: bool`, `OutputStatus`, `OutputSource`, `ViewMode`, and existing Ratatui `Style`/`Modifier` APIs.
- Produces: ANSI role constants `CYAN`, `GREEN`, `YELLOW`, `RED`; `output_title(app: &App, available_width: u16) -> String`; terminal-native `pane_style`, `selection_style`, modal shadow, cursor, and selection styles.

- [ ] **Step 1: Replace the fixed-palette assertions with failing terminal-native style tests**

In `src/tui/render.rs`, replace `colored_render_uses_only_the_quiet_prism_palette_and_no_ansi` and extend the existing no-color/modal tests with these assertions:

```rust
#[test]
fn colored_render_uses_terminal_defaults_and_ansi_role_colors() {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(now(), false);
    app.focus = Pane::Pipeline;
    app.steps.push(TransformStep {
        definition: transform_by_id("url-encode").unwrap(),
        enabled: true,
    });

    terminal.draw(|frame| render(frame, &mut app)).unwrap();

    let approved = [
        Color::Reset,
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Red,
    ];
    for cell in terminal.backend().buffer().content() {
        assert!(approved.contains(&cell.fg), "unexpected fg: {:?}", cell.fg);
        assert_eq!(cell.bg, Color::Reset);
        assert!(!matches!(cell.fg, Color::Rgb(..) | Color::Indexed(..)));
    }
    assert!(pane_style(&app, true).add_modifier.contains(Modifier::BOLD));
    assert!(selection_style(&app).add_modifier.contains(Modifier::REVERSED));
}
```

Update `assert_modal_depth` so both color modes require a dim shadow and Reset background:

```rust
let shadow = &buffer[(modal_area.right(), modal_area.y + 1)];
assert!(shadow.modifier.contains(Modifier::DIM));
assert_eq!(shadow.bg, Color::Reset);
```

Add to `no_color_uses_default_cell_styles_and_status_marks`:

```rust
assert_eq!(
    app.textarea.cursor_style(),
    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
);
assert_eq!(
    app.textarea.selection_style(),
    Style::default().add_modifier(Modifier::REVERSED)
);
assert!(pane_style(&app, true).add_modifier.contains(Modifier::BOLD));
assert!(pane_style(&app, false).add_modifier.contains(Modifier::DIM));
```

- [ ] **Step 2: Run the style tests and verify the fixed RGB contract fails**

Run:

```bash
rtk cargo test --locked colored_render_uses_terminal_defaults_and_ansi_role_colors
rtk cargo test --locked every_modal_dims_the_base_and_renders_a_one_cell_shadow
rtk cargo test --locked no_color_uses_default_cell_styles_and_status_marks
```

Expected: FAIL because the current frame, selection, cursor, and modal shadow still use Quiet Prism RGB values.

- [ ] **Step 3: Write failing Output title tests for bracketed Views without position counters**

Replace the counter expectations in `output_title_uses_byte_or_row_position_then_size_then_base_fallbacks` with:

```rust
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
```

Retain the App Bar regression assertion:

```rust
assert!(rendered_app(120, 20, &mut app).starts_with(">_ TOC"));
```

Update `output_titles_name_source_and_configured_view_for_text_hex_and_trace` and
`app_bar_omits_focus_and_output_title_shows_only_useful_source_and_size` to expect
the same bracketed title contract. Replace `resized_hex_title_uses_the_new_row_width`
with a regression asserting that resize still preserves the corrected row offset but
the title remains `» OUTPUT [HEX] · 80 B`; the title must no longer depend on row width.

- [ ] **Step 4: Run the title test and verify the current counter format fails**

Run:

```bash
rtk cargo test --locked output_title_brackets_the_view_and_keeps_only_total_size
```

Expected: FAIL with the current `» OUTPUT / VIEW · BYTE ...` or `ROW ...` title.

- [ ] **Step 5: Replace RGB constants and terminal-wide background styling**

In `src/tui.rs`, keep only terminal-defined ANSI role colors:

```rust
pub(super) const CYAN: Color = Color::Cyan;
pub(super) const GREEN: Color = Color::Green;
pub(super) const YELLOW: Color = Color::Yellow;
pub(super) const RED: Color = Color::Red;
```

Delete `BACKGROUND`, `SURFACE_HIGH`, `BORDER`, `TEXT`, and `MUTED`. In `render`, remove the full-buffer `fg(TEXT).bg(BACKGROUND)` assignment. Replace muted foreground uses with `Modifier::DIM`, selection/key-cap backgrounds with `REVERSED | BOLD`, and every modal shadow with `Modifier::DIM` and no background color.

Use one existing-boundary helper for repeated secondary text instead of retaining a
fake color role:

```rust
fn muted_style() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}
```

Call it for Hex/Trace headers, missing bytes, disabled/not-run states, modal secondary
text, and Dock separators. In `trace_status_style`, return `muted_style()` directly for
`Disabled | NotExecuted`; do not force `Color::Reset`, because that would override the
terminal foreground.

Use this style shape consistently:

```rust
fn pane_style(app: &App, focused: bool) -> Style {
    let style = if focused {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
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
```

In `App::new_with_input_limits`, make cursor and selection terminal-native in both color modes:

```rust
textarea.set_cursor_style(
    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
);
textarea.set_selection_style(Style::default().add_modifier(Modifier::REVERSED));
```

Continue gating ANSI status foregrounds on `app.no_color` in `hex_style`, `trace_status_style`, and failure rendering.

- [ ] **Step 6: Simplify `output_title` and update its caller**

Change the signature and base formatting:

```rust
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
```

In `render_output`, call `output_title(app, area.width)` and keep `reflow_output_viewport(inner)` unchanged.

- [ ] **Step 7: Run the focused theme and title tests**

Run:

```bash
rtk cargo test --locked tui::render::tests::colored_render_uses_terminal_defaults_and_ansi_role_colors
rtk cargo test --locked tui::render::tests::output_title_brackets_the_view_and_keeps_only_total_size
rtk cargo test --locked tui::render::tests::every_modal_dims_the_base_and_renders_a_one_cell_shadow
rtk cargo test --locked tui::render::tests::no_color_uses_default_cell_styles_and_status_marks
```

Expected: PASS.

- [ ] **Step 8: Update the theme and title documentation in the same change**

Update `README.md` to describe terminal-default foreground/background, terminal ANSI accents, `>_ TOC`, and `» OUTPUT [VIEW] · N B`. Remove the current `BYTE`/`ROW` wording.

Add a dated note at the top of both historical specs:

```markdown
> 2026-08-07 승인된 `2026-08-07-toc-terminal-native-tui-design.md`가
> 이 문서의 고정 RGB와 Output 위치 카운터 계약을 대체한다.
```

Add the same narrowly worded supersession note to the UX refresh and workbench specs,
whose historical bodies also describe the fixed palette or position counters. Do not
rewrite their historical implementation records.

- [ ] **Step 9: Run formatting and the TUI render suite**

Run:

```bash
rtk cargo fmt --all -- --check
rtk cargo test --locked tui::render::tests
rtk git diff --check
```

Expected: all commands succeed.

- [ ] **Step 10: Commit the terminal-native theme and title change**

Run:

```bash
rtk git add src/tui.rs src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md docs/superpowers/specs/2026-08-01-toc-tui-shortcuts-output-design.md
rtk git commit -m "feat(tui): 터미널 기본 테마 적용"
```

---

### Task 2: Pipeline key contract and transient delete feedback

**Files:**
- Modify: `src/tui/state.rs:700-740, 1290-1410, 2280-2335, 2880-3075, 3800-4060`
- Modify: `src/tui/render.rs:690-925, 1175-1225, 2000-2365, 3580-3650`
- Modify: `tests/shell-smoke.sh:190-235`
- Modify: `README.md:97-110`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md:350-370`
- Modify: `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md:125-140`
- Modify: `docs/superpowers/specs/2026-08-01-toc-tui-shortcuts-output-design.md:55-125, 160-190, 300-315`

**Interfaces:**
- Consumes: existing `App::delete_selected(&mut self, now: Instant)`, `App::request_selected_step(&mut self, now: Instant) -> Vec<Effect>`, `footer_status` priority, `TRANSIENT_STATUS_FOR`.
- Produces: Pipeline `Backspace` delete; Pipeline `s`/`ㄴ` Step; Output `p`/`ㅔ` no-op; a styled `footer_status_line(app: &App, width: usize) -> Option<Line<'static>>` preserving current priority.

- [ ] **Step 1: Write failing state tests for the new Pipeline-only keys**

Replace the delete-alias test and move the selected-Step test to Pipeline focus:

```rust
#[test]
fn pipeline_backspace_is_the_only_delete_key() {
    let start = now();
    let mut app = App::new(start, true);
    app.add_transform("base64-encode", start);
    app.add_transform("format-json", start);
    app.focus = Pane::Pipeline;
    app.selected_step = 1;

    for code in [
        KeyCode::Delete,
        KeyCode::Char('d'),
        KeyCode::Char('ㅇ'),
        KeyCode::Char('a'),
        KeyCode::Char('ㅁ'),
    ] {
        assert!(key(&mut app, code, KeyModifiers::NONE, start).is_empty());
        assert_eq!(app.steps.len(), 2);
        assert!(app.modal.is_none());
    }

    key(&mut app, KeyCode::Backspace, KeyModifiers::NONE, start);
    assert_eq!(app.steps.len(), 1);
    assert_eq!(app.status.as_deref(), Some("Removed JSON Prettify"));
}

#[test]
fn pipeline_s_and_hangul_alias_request_the_selected_step() {
    for code in [KeyCode::Char('s'), KeyCode::Char('ㄴ')] {
        let start = now();
        let mut app = App::new(start, true);
        app.focus = Pane::Pipeline;
        app.steps.push(TransformStep {
            definition: transform_by_id("base64-encode").unwrap(),
            enabled: true,
        });

        let effects = key(&mut app, code, KeyModifiers::NONE, start);

        assert_eq!(app.output.status.running_target(), Some(ExecutionTarget::Step(0)));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Cancel(1), Effect::Submit(PreviewJob {
                target: ExecutionTarget::Step(0),
                ..
            })]
        ));
    }
}

#[test]
fn output_p_and_hangul_alias_are_no_ops() {
    let start = now();
    let mut app = App::new(start, true);
    app.focus = Pane::Output;
    app.steps.push(TransformStep {
        definition: transform_by_id("base64-encode").unwrap(),
        enabled: true,
    });

    for code in [KeyCode::Char('p'), KeyCode::Char('ㅔ')] {
        assert!(key(&mut app, code, KeyModifiers::NONE, start).is_empty());
        assert_eq!(app.request_id, 0);
    }
}
```

Update every existing state/render setup that invokes selected-Step through Output
`p`/`ㅔ` to focus Pipeline and send `s`/`ㄴ`. Specifically update
`latin_and_hangul_pane_shortcuts_share_state_transitions`,
`chain_keys_select_toggle_reorder_and_delete_steps`,
`pipeline_supports_arrow_selection_edit_inspect_palette_and_zoom_keys`,
`output_cycles_views_requests_sources_copy_and_zoom`, the selected-stage cache/stale
result tests, and the corresponding render setup. Rename the empty/delete tests so
their names describe Backspace rather than aliases.

Extend `input_keeps_all_pane_shortcut_characters_as_editor_input` with `s`, then send
Backspace and assert only the final input character is removed. This preserves the
Input editing side of the key contract.

- [ ] **Step 2: Run the key tests and verify the old key map fails**

Run:

```bash
rtk cargo test --locked pipeline_backspace_is_the_only_delete_key
rtk cargo test --locked pipeline_s_and_hangul_alias_request_the_selected_step
rtk cargo test --locked output_p_and_hangul_alias_are_no_ops
```

Expected: FAIL because Delete/d/ㅇ and a/ㅁ still act in Pipeline, s/ㄴ do nothing, and p/ㅔ still run Step from Output.

- [ ] **Step 3: Write failing Dock and Help tests for the real keys**

Update `dock_and_help_show_lowercase_current_keys_without_hangul_aliases` and `one_context_help_modal_lists_only_real_keys_for_each_pane`:

```rust
let pipeline = rendered_app(120, 20, &mut pipeline_app);
assert!(pipeline.contains("[ Backspace ] Delete"));
assert!(pipeline.contains("[ s ] Step"));
assert!(!pipeline.contains("Delete/d"));
assert!(!pipeline.contains("[ a ] Add"));

let output = rendered_app(120, 20, &mut output_app);
assert!(!output.contains("[ p ] Step"));
assert!(output.contains("[ f ] Final"));

for removed in ["Delete/d", "a  Add transform", "p  Show selected step", "ㅔ", "ㄴ"] {
    assert!(!help_screen.contains(removed), "unexpected {removed}: {help_screen}");
}
```

- [ ] **Step 4: Write a failing render test for highlighted Removed status**

Add:

```rust
#[test]
fn removed_status_is_reversed_and_bold_without_changing_footer_priority() {
    for no_color in [false, true] {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(now(), no_color);
        app.focus = Pane::Pipeline;
        app.set_status(Some("Removed URL Encode".to_string()), now());

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
```

The existing `transient_status_expires_at_exactly_two_seconds` remains the state-level lifetime contract.

- [ ] **Step 5: Run the Dock, Help, and status tests and verify they fail**

Run:

```bash
rtk cargo test --locked dock_and_help_show_lowercase_current_keys_without_hangul_aliases
rtk cargo test --locked one_context_help_modal_lists_only_real_keys_for_each_pane
rtk cargo test --locked removed_status_is_reversed_and_bold_without_changing_footer_priority
```

Expected: FAIL because the old strings and unstyled status are still rendered.

- [ ] **Step 6: Change only the concrete key branches**

In `handle_pipeline_key`, use these branches and return Step effects after the shared
match:

```rust
(KeyCode::Backspace, KeyModifiers::NONE) => self.delete_selected(now),
(KeyCode::Char('s' | 'ㄴ'), KeyModifiers::NONE) => {
    return self.request_selected_step(now);
}
```

Change `handle_pipeline_key` to return `Vec<Effect>` so Step effects propagate; keep
the existing state-changing match arms as statements and return `Vec::new()` once
after the match. Remove the Delete/d/ㅇ and a/ㅁ branches. In `handle_output_key`, remove
the p/ㅔ branch. Keep the global Ctrl+p/Ctrl+ㅔ branch in `handle_key` unchanged.

Update the Pipeline arm in `handle_key`:

```rust
Pane::Pipeline => self.handle_pipeline_key(key, now),
```

For Pipeline mouse wheel, call `handle_pipeline_key` and intentionally ignore its empty selection-move result:

```rust
let _ = self.handle_pipeline_key(KeyEvent::new(code, KeyModifiers::NONE), now);
```

- [ ] **Step 7: Update Dock, full Help, and compact Help strings**

Set the Pipeline commands to Backspace Delete, Enter Inspect, s Step, z Zoom. Remove p Step from `OUTPUT_COMMANDS`. Keep Ctrl+p Add in `GLOBAL_COMMANDS`.

The full Pipeline Help must include:

```text
Backspace  Delete step
Enter  Inspect step
s  Show selected step
```

The full Output Help must begin with `v  Next view` and `f  Restore final` and contain no p Step line. Apply the same contract to compact Help.

- [ ] **Step 8: Render only the Removed state with reverse and bold**

Replace the raw-string status helper with a Line-returning helper while preserving the current priority order:

```rust
fn footer_status_line(app: &App, width: usize) -> Option<Line<'static>> {
    match &app.output.status {
        OutputStatus::Failed(error) => Some(Line::raw(crate::error::escape_external(
            &render_pipeline_error_summary(error),
            width,
        ))),
        OutputStatus::Cancelled => Some(Line::raw("Cancelled")),
        status if status.long_running_notice() => Some(Line::raw(if
            app.output.active_artifact.is_some() || !app.output.traces.is_empty()
        {
            "Still processing · Previous result shown · Esc Cancel"
        } else {
            "Still processing · Esc Cancel"
        })),
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
```

Make `footer_first_line` return this Line before the Dock. Do not add state fields or change `TRANSIENT_STATUS_FOR`.

- [ ] **Step 9: Update the PTY flow for Ctrl+p, Backspace delete, and status expiry**

In `tests/shell-smoke.sh`, replace the Pipeline literal `a` used to open Picker with Ctrl+p:

```tcl
send -- "\020"
expect_exact "Search:" 150 151
```

After returning to Pipeline with at least one step, add a semantic expiry check:

```tcl
send -- "\177"
expect_exact "Removed" 166 167
after 2200
expect_exact "Backspace" 168 169
```

The unchanged Pipeline title is not guaranteed to be emitted by incremental redraw, so
the returning `Backspace` Dock label is the expiry assertion. Do not assert brittle fixed
ANSI adjacency; keep the existing semantic/order assertion style.

- [ ] **Step 10: Run state, render, and shell checks**

Run:

```bash
rtk cargo test --locked tui::state::tests
rtk cargo test --locked tui::render::tests
rtk bash tests/shell-smoke.sh
rtk zsh tests/shell-smoke.sh
```

Expected: all commands succeed and both shells observe the Dock after the Removed status expires.

- [ ] **Step 11: Update key documentation in the same change**

Update the README key lists and the historical shortcut spec with a dated supersession note and the exact new Pipeline/Output tables. Add narrow current-contract notes to the workbench and Quiet Prism specs where their historical bodies still show Output `p`. State explicitly that Picker Backspace still edits Search and global Ctrl+p Add remains.

- [ ] **Step 12: Commit the key and delete-feedback change**

Run:

```bash
rtk git add src/tui/state.rs src/tui/render.rs tests/shell-smoke.sh README.md docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md docs/superpowers/specs/2026-08-01-toc-tui-shortcuts-output-design.md
rtk git commit -m "feat(tui): 파이프라인 단축키 정돈"
```

---

### Task 3: One-line Add Transform picker with exact details

**Files:**
- Modify: `src/tui/render.rs:950-1110, 1660-1735, 2190-2375`
- Test: `src/tui/state.rs:2015-2055`
- Modify: `tests/shell-smoke.sh:115-145, 205-225`
- Modify: `README.md:55-110`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md:180-215`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-mouse-design.md:105-125, 155-170, 265-280`

**Interfaces:**
- Consumes: `App::filtered_transforms() -> Vec<&'static TransformDefinition>`, `TransformDefinition::{id, display_name, description, behavior, accepts_binary}`, existing `picker_insert`, Backspace query editing, and `MouseRegions::picker_rows`.
- Produces: one-row Picker items and one-row click regions; normal six-row exact details; compact one- or two-row selected description; unchanged Search and Enter/Esc actions.

- [ ] **Step 1: Write failing normal and compact Picker render tests**

Replace `add_transform_separates_item_description_detail_and_key_help` with:

```rust
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
```

Replace the compact test with:

```rust
#[test]
fn compact_add_transform_keeps_a_separate_selected_description() {
    let mut app = App::new(now(), true);
    app.open_picker();

    let screen = rendered_app(40, 10, &mut app);

    assert!(screen.contains("Search:"));
    assert!(screen.contains("> Base64 Encode"));
    assert!(screen.contains("Encode bytes"));
    assert!(screen.contains("Enter Add"));
    assert!(screen.contains("Esc Cancel"));
    assert!(!screen.contains("Backspace Search"));
}
```

- [ ] **Step 2: Run both Picker tests and verify the two-row list fails**

Run:

```bash
rtk cargo test --locked add_transform_uses_one_line_names_and_exact_detail_metadata
rtk cargo test --locked compact_add_transform_keeps_a_separate_selected_description
```

Expected: FAIL because ID/description are still in the two-row list, exact ABOUT metadata is absent from the detail area, and the hint still names Backspace.

- [ ] **Step 3: Update the Picker mouse test for one-row hitboxes**

In the existing Picker click/visible-row test, assert each stored row is one cell high and adjacent:

```rust
assert!(app.mouse_regions.picker_rows.iter().all(|(area, _)| area.height == 1));
for rows in app.mouse_regions.picker_rows.windows(2) {
    assert_eq!(rows[1].0.y, rows[0].0.y + 1);
}
```

Keep the click behavior assertion that selecting an item does not add it until Enter or the explicit Add action.

- [ ] **Step 4: Run the Picker mouse test and verify the two-row geometry fails**

Run:

```bash
rtk cargo test --locked picker_click_selects_then_explicit_add_and_cancel_regions_act
```

Expected: FAIL because current Picker hitboxes are two rows high and advance by two.

- [ ] **Step 5: Change the Picker vertical layout and detail content**

Use compact-aware rows without separators on small screens:

```rust
let compact = inner.height < 14;
let detail_rows = if compact {
    inner.height.saturating_sub(4).clamp(1, 2)
} else {
    6
};
let separator_rows = u16::from(!compact);
```

Use `separator_rows` for both `detail_separator` and `hint_separator` constraints, so
the 40×10 inner height resolves to query 1 + list 2 + detail 2 + hint 1 without hidden
overflow. Six normal detail rows allow the longest registered behavior line to wrap
once while keeping the TUI field visible.

Build details from the selected transform:

```rust
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
```

Render the detail in both modes with existing wrapping and safe frame clipping.

- [ ] **Step 6: Render one-row list items and one-row mouse regions**

Replace the item body and row math with:

```rust
let available = (list_area.height as usize).max(1);
let items = filtered
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
    .collect::<Vec<_>>();

mouse_regions.picker_rows.extend(
    (start..start + items.len()).enumerate().map(|(row, index)| {
        (Rect::new(list_area.x, list_area.y + row as u16, list_area.width, 1), index)
    }),
);
```

Change the noncompact hint to the same text as compact mode:

```rust
let hint = "↑/↓ Select · [Enter Add] · [Esc Cancel]";
```

Do not change the Picker Backspace branch in `state.rs`; retain its existing query pop and selected-index reset.

- [ ] **Step 7: Run Picker render, state, and mouse tests**

Run:

```bash
rtk cargo test --locked add_transform_uses_one_line_names_and_exact_detail_metadata
rtk cargo test --locked compact_add_transform_keeps_a_separate_selected_description
rtk cargo test --locked picker_click_selects_then_explicit_add_and_cancel_regions_act
rtk cargo test --locked picker_key_selection_clamps_and_backspace_edits_query
```

Expected: PASS.

- [ ] **Step 8: Update PTY Picker coordinates and semantic expectations**

Keep `Search:` and Backspace query-edit behavior. In the 120×24 flow, change the second
item click from SGR y=8 to y=7 and replace the old cursor-adjacency expectation with
`Decode canonical padded Base64 into UTF-8 text`; this proves the one-line selection and
separate detail changed together. Keep the explicit Add click on the hint row. Avoid
literal border adjacency assertions.

Run:

```bash
rtk bash tests/shell-smoke.sh
rtk zsh tests/shell-smoke.sh
```

Expected: both scripts pass.

- [ ] **Step 9: Update Picker and mouse documentation in the same change**

Update README Picker wording. Add a dated supersession note to the UX refresh and mouse design specs stating that the 2026-08-07 design replaces two-row items, inline descriptions, two-row hitboxes, and the Backspace hint while keeping Search editing.

- [ ] **Step 10: Run formatting and the related TUI suites**

Run:

```bash
rtk cargo fmt --all -- --check
rtk cargo test --locked tui::state::tests
rtk cargo test --locked tui::render::tests
rtk git diff --check
```

Expected: all commands succeed.

- [ ] **Step 11: Commit the Picker information-architecture change**

Run:

```bash
rtk git add src/tui/render.rs src/tui/state.rs tests/shell-smoke.sh README.md docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md docs/superpowers/specs/2026-07-31-toc-tui-mouse-design.md
rtk git commit -m "refactor(tui): 변환 선택 정보 구조 정돈"
```

---

### Task 4: Pipeline and Trace column alignment

**Files:**
- Modify: `src/tui/render.rs:320-480, 576-675, 2570-2625, 3260-3345`
- Modify: `README.md:70-97`

**Interfaces:**
- Consumes: `inner.width`, `UnicodeWidthStr`, existing safe `operation_name(transform_id, width) -> String`, maximum 32 traces, current Wide/Medium/Narrow layout thresholds.
- Produces: right-aligned Pipeline size text on Wide screens and a stable Trace Operation width based on all current traces.

- [ ] **Step 1: Extend the Pipeline size test with a failing right-edge assertion**

In `running_pipeline_state_and_byte_sizes_follow_color_and_width_policy`, add:

```rust
let size = "3B→4B";
let row = wide.lines().find(|line| line.contains(size)).unwrap();
let start = row.find(size).unwrap();
assert_eq!(
    row[..start].width(),
    pipeline_width(120, WidthMode::Wide) as usize - 1 - size.width()
);
```

Keep the current assertion that Medium mode does not show sizes.

- [ ] **Step 2: Run the Pipeline alignment test and verify it fails**

Run:

```bash
rtk cargo test --locked running_pipeline_state_and_byte_sizes_follow_color_and_width_policy
```

Expected: FAIL because `3B→4B` currently follows the operation name instead of the Pipeline content edge.

- [ ] **Step 3: Add a failing Trace header-position test**

Add to the existing Trace table test or as a focused test:

```rust
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
    let header = screen.lines().find(|line| line.contains("OPERATION")).unwrap();
    let operation = header.find("OPERATION").unwrap();
    let input = header.find("INPUT").unwrap();
    assert_eq!(header[operation..input].width(), "Base64 Decode".width() + 1);
}
```

- [ ] **Step 4: Run the Trace alignment test and verify the elastic Operation column fails**

Run:

```bash
rtk cargo test --locked trace_operation_width_follows_the_longest_registered_name
```

Expected: FAIL because the current Operation column consumes all remaining width.

- [ ] **Step 5: Right-align Wide Pipeline sizes with display-width padding**

In the normal Pipeline row branch, build the left and optional right segments separately:

```rust
let left = format!("{prefix} [{enabled}]  {mark}{}", step.definition.display_name);
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
```

Leave running, disabled, failed, and no-size status selection unchanged apart from terminal-native styles established in Task 1.

- [ ] **Step 6: Compute Trace Operation width from all current traces**

Move the bounded `traces` slice before width construction and calculate:

```rust
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
```

Use `operation_width` in the existing Wide and Narrow `widths` vectors. Keep the fixed data widths, one-column spacing, failure detail, and row byte budget unchanged.

- [ ] **Step 7: Run Pipeline, Trace, budget, and resize tests**

Run:

```bash
rtk cargo test --locked running_pipeline_state_and_byte_sizes_follow_color_and_width_policy
rtk cargo test --locked trace_operation_width_follows_the_longest_registered_name
rtk cargo test --locked composed_hex_and_trace_views_reserve_header_space_inside_the_budget
rtk cargo test --locked trace_end_obeys_the_render_byte_budget
```

Expected: PASS.

- [ ] **Step 8: Update README alignment wording and run render tests**

Describe right-aligned Wide Pipeline byte deltas and compact Trace Operation width. Then run:

```bash
rtk cargo fmt --all -- --check
rtk cargo test --locked tui::render::tests
rtk git diff --check
```

Expected: all commands succeed.

- [ ] **Step 9: Commit the alignment change**

Run:

```bash
rtk git add src/tui/render.rs README.md
rtk git commit -m "fix(tui): 파이프라인과 Trace 열 정렬"
```

---

### Task 5: Full verification and implementation documentation

**Files:**
- Modify: `README.md:125-175`
- Modify: `docs/superpowers/specs/2026-08-07-toc-terminal-native-tui-design.md:1-8, 242-280`
- Verify: `src/tui.rs`, `src/tui/state.rs`, `src/tui/render.rs`, `src/tui/views.rs`, `tests/shell-smoke.sh`

**Interfaces:**
- Consumes: the completed behavior from Tasks 1-4 and existing ignored release measurements.
- Produces: an implementation-complete design status, current test totals, current performance measurements, and verified packaging/PTY evidence.

- [ ] **Step 1: Run format, warning-free Clippy, complete tests, and rustdoc**

Run:

```bash
rtk cargo fmt --all -- --check
rtk cargo clippy --locked --all-targets --all-features -- -D warnings
rtk cargo test --locked --all-targets --all-features
rtk cargo rustdoc --locked --lib -- -D warnings
```

Expected: all commands succeed. Record the exact library, integration, ignored, and total passed counts from the full test output.

- [ ] **Step 2: Run the three existing release measurements**

Run:

```bash
rtk proxy cargo test --release --locked dirty_redraw_release_measurement -- --ignored --nocapture
rtk proxy cargo test --release --locked max_input_edit_release_measurement -- --ignored --nocapture
rtk proxy cargo test --release --locked utf8_validation_release_measurement -- --ignored --nocapture
```

Expected: all three ignored measurements pass. Record min/median/max values and redraw count exactly; timing remains observational rather than a pass threshold.

- [ ] **Step 3: Verify packaging and a temporary offline installation**

Run:

```bash
rtk cargo package --locked --allow-dirty
rtk mktemp -d /private/tmp/toc-install.XXXXXX
```

Capture the exact directory printed by `mktemp`, then run `rtk cargo install --path . --locked --offline --root` with that exact directory and run its `bin/toc --version`. Expected version: `toc 0.2.0`.

After validation, confirm the directory begins with `/private/tmp/toc-install.` and remove that exact directory only.

- [ ] **Step 4: Run Bash, Zsh, and real macOS clipboard PTY checks**

Run:

```bash
rtk bash tests/shell-smoke.sh
rtk zsh tests/shell-smoke.sh
rtk env TOC_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
rtk env TOC_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
rtk pbpaste
```

Expected: all shell-smoke runs pass and `pbpaste` returns the expected lowercase Hex payload `ff`. If the host is not Darwin or has no Pasteboard access, state that exact environmental limitation instead of substituting a fake result. Run X11/Wayland only on a host where those display servers are available; otherwise record them as not run.

- [ ] **Step 5: Update the verification record with actual results**

In README, replace the newest local-verification summary with the exact test totals, install version, three performance measurement ranges, Bash/Zsh PTY outcome, macOS Pasteboard outcome, and X11/Wayland status observed in Steps 1-4.

Change the new design metadata to:

```markdown
**상태:** 사용자 승인·구현 완료
```

Append a concise implementation-verification note under its test section without rewriting the approved decisions.

- [ ] **Step 6: Re-run verification on the final documented tree**

Run:

```bash
rtk cargo fmt --all -- --check
rtk cargo clippy --locked --all-targets --all-features -- -D warnings
rtk cargo test --locked --all-targets --all-features
rtk cargo rustdoc --locked --lib -- -D warnings
rtk git diff --check
```

Expected: all commands succeed and the test totals match the README.

- [ ] **Step 7: Confirm only intended files changed and no debug markers remain**

Run:

```bash
rtk git status --short
rtk rg --color=never -n "TO[D]O|FIX[M]E|log_user 1" src tests README.md docs/superpowers/specs
```

Expected: status lists only the final README/design verification edits; the search returns no new production markers. Existing ignored measurement `eprintln!` calls are permitted.

- [ ] **Step 8: Commit the final verification record**

Run:

```bash
rtk git add README.md docs/superpowers/specs/2026-08-07-toc-terminal-native-tui-design.md
rtk git commit -m "docs(tui): 터미널 기본 테마 검증 결과"
```

- [ ] **Step 9: Request final code review and use the branch-finishing workflow**

Run a read-only final review against the approved design and this plan. Resolve any validated Critical or Important findings with a failing regression test, rerun the affected suite, and create a focused corrective commit. Then invoke `superpowers:verification-before-completion` and `superpowers:finishing-a-development-branch`; do not merge, push, or delete the branch without the user's integration choice.
