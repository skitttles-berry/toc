# toc TUI Quiet Prism Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 현재 `Pipeline + Input + Output` 구조와 모든 키 바인딩을 유지하면서 변환 대기 중 이전 결과 보존, Quiet Prism 시각 위계, Grouped Command Dock과 Modal Dim·Shadow를 구현한다.

**Architecture:** `OutputStatus`가 실행 대상·시작 시각·장시간 안내 여부를 소유하고, `OutputState`의 현재 Artifact·Trace·출처는 최신 요청이 끝날 때까지 화면 Snapshot 역할을 한다. 렌더링은 기존 `src/tui/render.rs`의 패널과 Modal 함수를 유지하면서 팔레트·Footer Span·Modal Layer만 추가하고 새 모듈이나 의존성을 만들지 않는다.

**Tech Stack:** Rust 1.97.1, edition 2024, Ratatui 0.30.2, Crossterm 0.29.0, tui-textarea-2 0.12.1, 기존 Rust 단위 시험과 Ratatui `TestBackend`

## Global Constraints

- 기준 설계는 `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md`다.
- 현재 App Bar와 `Pipeline + Input + Output` 배치·크기·제목을 유지한다.
- Activity Ribbon, 노드 그래프, Segment Tab, Metric, Tree, Toast와 애니메이션을 추가하지 않는다.
- 기존 F3/F4, Output Enter, Ctrl+C를 포함한 모든 키 바인딩과 마우스 hitbox를 유지한다.
- 처리 시작 1초 전에는 안내를 표시하지 않고, 1초 이후에만 장시간 처리 문구를 표시한다.
- 대기 중 이전 결과는 표시만 하며 복사할 수 없다. 실패·취소 뒤에는 제거한다.
- 새 crate, 모듈, 사용자 테마 설정, trait와 registry를 추가하지 않는다.
- `NO_COLOR`에서 색상만 제거하고 포커스·상태·Keycap 문자 정보를 유지한다.
- ANSI·OSC·위험 제어 문자 차단, 출력 예산, 최신 요청 우선과 터미널 복구 계약을 유지한다.
- 코드 변경 커밋마다 `README.md`와 관련 기존 설계 문서를 같은 커밋에서 현행화한다.
- 셸 명령은 `rtk` 접두사를 사용하고 파일 검색·출력은 프로젝트 `AGENTS.md`의 안전 플래그를 따른다.

---

## File Map

| 파일 | 책임 |
|---|---|
| `src/tui/state.rs` | pending 실행 대상·시작 시각·1초 안내 전환, 이전 Output 보존, 복사·캐시 안전성 |
| `src/tui/render.rs` | 이전 Output 렌더링, 장시간 문구, Quiet Prism 팔레트, Command Dock, Modal Dim·Shadow |
| `README.md` | 사용자가 관찰하는 대기·Footer·Modal 동작과 검증 명령 |
| `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md` | 작업자·결과 보존·반응형 작업판의 현행 계약 |
| `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md` | Quiet Prism, Command Dock과 Modal 시각 계약 |
| `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md` | 구현 완료 상태와 최종 검증 결과 |

---

### Task 1: 변환 대기 중 이전 결과 보존

**Files:**
- Modify: `src/tui/state.rs:25-67,348-364,561-676,1084-1134,1235-1267`
- Test: `src/tui/state.rs:2221-2258,2469-2558,2877-2956`
- Modify: `src/tui/render.rs:109-190,194-288,341-366,565-645`
- Test: `src/tui/render.rs:1115-1155,1336-1367,1595-1677,2055-2155`
- Modify: `README.md:53-84`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md:164-200,226-243`

**Interfaces:**
- Consumes: `ExecutionTarget::{Final, Step}`, `AppEvent::Tick(Instant)`, `PreviewJob`, `request_id`
- Produces: `LONG_RUNNING_AFTER: Duration`, `OutputStatus::running(Instant, ExecutionTarget)`, `OutputStatus::running_target() -> Option<ExecutionTarget>`, `OutputStatus::long_running_notice() -> bool`
- Preserves: `OutputState::{active_artifact, traces, source, byte_offset, row_offset}` until the latest result finishes

- [ ] **Step 1: Replace the old hide-on-change test with a preservation RED test**

Replace `a_change_immediately_hides_the_previous_copyable_result` in `src/tui/state.rs` with:

```rust
#[test]
fn a_change_keeps_previous_result_visible_but_not_copyable() {
    let start = now();
    let mut app = App::new(start, true);
    let previous_trace = StepTrace {
        step: 1,
        transform_id: "base64-encode",
        input_bytes: Some(3),
        output_bytes: Some(4),
        elapsed: None,
        status: StepStatus::Succeeded,
        error: None,
    };
    app.output.source = OutputSource::Step(0);
    app.output.status = OutputStatus::Ready;
    app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
    app.output.final_traces = vec![previous_trace.clone()];
    app.output.active_artifact = Some(Artifact::new(b"old".to_vec()));
    app.output.traces = vec![previous_trace.clone()];
    app.output.byte_offset = 2;
    app.output.row_offset = 1;

    app.insert_paste("new", start);

    assert!(matches!(app.output.status, OutputStatus::Debouncing { .. }));
    assert_eq!(app.output.source, OutputSource::Step(0));
    assert_eq!(app.output.active_artifact.as_ref().unwrap().bytes(), b"old");
    assert_eq!(app.output.traces, vec![previous_trace]);
    assert_eq!((app.output.byte_offset, app.output.row_offset), (2, 1));
    assert!(app.output.final_artifact.is_none());
    assert!(app.output.final_traces.is_empty());
    assert!(!app.can_copy());
}
```

- [ ] **Step 2: Run the preservation test and verify RED**

Run:

```bash
rtk cargo test --locked --lib tui::state::tests::a_change_keeps_previous_result_visible_but_not_copyable -- --exact
```

Expected: `running 1 test`, then FAIL because `changed()` changes the source to Final and removes the active Artifact, Trace and offsets.

- [ ] **Step 3: Preserve only the visible state while invalidating the final cache**

Change `App::changed` to:

```rust
fn changed(&mut self, now: Instant) {
    self.request_id = self
        .request_id
        .checked_add(1)
        .expect("TUI request ID exhausted");
    self.output.status = OutputStatus::Debouncing {
        deadline: now + debounce_for(self.input_len()),
    };
    self.output.final_artifact = None;
    self.output.final_traces.clear();
    self.mark_dirty();
}
```

Do not clear or relabel `active_artifact`, `traces`, `source`, `byte_offset` or `row_offset` here.

- [ ] **Step 4: Run the preservation test and verify GREEN**

Run:

```bash
rtk cargo test --locked --lib tui::state::tests::a_change_keeps_previous_result_visible_but_not_copyable -- --exact
```

Expected: `running 1 test`, `1 passed`.

- [ ] **Step 5: Add deterministic running-target and one-second RED tests**

Add beside the debounce tests in `src/tui/state.rs`:

```rust
#[test]
fn running_notice_appears_once_at_one_second() {
    let start = now();
    let mut app = App::new(start, true);
    app.insert_paste("x", start);
    let submitted_at = start + debounce_for(1);

    let effects = app.handle_event(AppEvent::Tick(submitted_at));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Submit(PreviewJob {
            target: ExecutionTarget::Final,
            ..
        })]
    ));
    assert_eq!(
        app.output.status.running_target(),
        Some(ExecutionTarget::Final)
    );
    assert!(!app.output.status.long_running_notice());
    app.take_dirty();

    app.handle_event(AppEvent::Tick(
        submitted_at + LONG_RUNNING_AFTER - Duration::from_millis(1),
    ));
    assert!(!app.output.status.long_running_notice());
    assert!(!app.take_dirty());

    app.handle_event(AppEvent::Tick(submitted_at + LONG_RUNNING_AFTER));
    assert!(app.output.status.long_running_notice());
    assert!(app.take_dirty());

    app.handle_event(AppEvent::Tick(
        submitted_at + LONG_RUNNING_AFTER + Duration::from_millis(50),
    ));
    assert!(!app.take_dirty());
}
```

Update `selected_stage_request_is_immediate_and_keeps_cached_final` so it asserts the previous Final remains visible while Step 0 is pending:

```rust
assert_eq!(app.output.source, OutputSource::Final);
assert_eq!(
    app.output.status.running_target(),
    Some(ExecutionTarget::Step(0))
);
assert_eq!(app.output.active_artifact.as_ref().unwrap().bytes(), b"cached");
assert_eq!(app.output.final_artifact.as_ref().unwrap().bytes(), b"cached");
```

- [ ] **Step 6: Run the new state test and verify RED**

Run:

```bash
rtk cargo test --locked --lib tui::state::tests::running_notice_appears_once_at_one_second -- --exact
```

Expected: compilation failure because the new running state API and `LONG_RUNNING_AFTER` do not exist yet.

- [ ] **Step 7: Add the minimal pending execution state**

Replace the unit `Running` variant and add its helpers near `OutputStatus`:

```rust
pub(super) const LONG_RUNNING_AFTER: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OutputStatus {
    Idle,
    Debouncing { deadline: Instant },
    Running {
        started_at: Instant,
        target: ExecutionTarget,
        notice_visible: bool,
    },
    Ready,
    Failed(PipelineError),
    Cancelled,
}

impl OutputStatus {
    pub(super) fn running(started_at: Instant, target: ExecutionTarget) -> Self {
        Self::Running {
            started_at,
            target,
            notice_visible: false,
        }
    }

    pub(super) fn running_target(&self) -> Option<ExecutionTarget> {
        match self {
            Self::Running { target, .. } => Some(*target),
            _ => None,
        }
    }

    pub(super) fn long_running_notice(&self) -> bool {
        matches!(
            self,
            Self::Running {
                notice_visible: true,
                ..
            }
        )
    }
}
```

Change `tick` to submit Final once, then mark only the first Tick crossing one second:

```rust
fn tick(&mut self, now: Instant) -> Vec<Effect> {
    if let OutputStatus::Debouncing { deadline } = &self.output.status {
        if now < *deadline {
            return Vec::new();
        }
        self.output.status = OutputStatus::running(now, ExecutionTarget::Final);
        self.mark_dirty();
        return vec![Effect::Submit(PreviewJob {
            request_id: self.request_id,
            input: self.input_text().into_bytes(),
            steps: self.steps.clone(),
            target: ExecutionTarget::Final,
        })];
    }

    let show_notice = matches!(
        &self.output.status,
        OutputStatus::Running {
            started_at,
            notice_visible: false,
            ..
        } if now.saturating_duration_since(*started_at) >= LONG_RUNNING_AFTER
    );
    if show_notice {
        let OutputStatus::Running { notice_visible, .. } = &mut self.output.status else {
            unreachable!("running notice checked above")
        };
        *notice_visible = true;
        self.mark_dirty();
    }
    Vec::new()
}
```

Pass the event time through these exact signatures:

```rust
fn request_selected_step(&mut self, now: Instant) -> Vec<Effect>
fn handle_output_key(&mut self, key: KeyEvent, now: Instant) -> Vec<Effect>
```

Call `self.handle_output_key(key, now)` from `handle_key`. In `request_selected_step`, replace the source change and visible-state clearing with:

```rust
self.output.status =
    OutputStatus::running(now, ExecutionTarget::Step(self.selected_step));
self.mark_dirty();
```

Keep the existing request ID increment and `Effect::Submit`, and do not clear or relabel the current source, Artifact, Trace or offsets. Match `OutputStatus::Running { .. }` in cancellation and all existing state tests.

- [ ] **Step 8: Render the previous body and pending target instead of status strings**

In `render_output`, remove the dedicated Debouncing and Running strings so both states use the existing `effective_view` branch. Replace Pipeline and Inspector running checks with the pending target:

```rust
app.output.status.running_target() == Some(ExecutionTarget::Step(index))
```

and:

```rust
app.output.status.running_target()
    == Some(ExecutionTarget::Step(app.selected_step))
```

Add the long-running case before ordinary `App.status` in `footer_first_line`:

```rust
status if status.long_running_notice() => {
    if app.output.active_artifact.is_some() || !app.output.traces.is_empty() {
        "Still processing · Previous result shown · Esc Cancel".to_string()
    } else {
        "Still processing · Esc Cancel".to_string()
    }
}
```

Add this render regression test:

```rust
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
```

- [ ] **Step 9: Update affected fixtures and run focused state/render suites**

Mechanically replace unit-style test fixtures `OutputStatus::Running` with one of these exact constructors and preserve each test's intended target:

```rust
OutputStatus::running(start, ExecutionTarget::Final)
OutputStatus::running(now(), ExecutionTarget::Final)
OutputStatus::running(start, ExecutionTarget::Step(index))
OutputStatus::running(now(), ExecutionTarget::Step(index))
```

Selected-Step row tests use `ExecutionTarget::Step(index)` and Final tests use `ExecutionTarget::Final`. Add `LONG_RUNNING_AFTER` to the existing `super::super::state` import in render tests.

Update the three existing invalidation tests to the new visible-state contract:

```rust
// document_change_clears_owned_results_and_trace_and_disables_copy
assert_eq!(app.output.active_artifact.as_ref().unwrap().bytes(), b"active");
assert_eq!(app.output.traces.len(), 1);
assert!(app.output.final_artifact.is_none());
assert!(app.output.final_traces.is_empty());
assert!(!app.can_copy());

// pipeline_change_uses_the_same_invalidation_and_debounce_path
assert_eq!(app.output.active_artifact.as_ref().unwrap().bytes(), b"active");
assert!(app.output.final_artifact.is_none());
assert!(app.output.final_traces.is_empty());

// pipeline_edits_schedule_final_but_selection_does_not
assert_eq!(app.output.source, OutputSource::Step(0));
assert_eq!(app.output.active_artifact.as_ref().unwrap().bytes(), b"owned");
assert_eq!(
    app.output.status.running_target(),
    Some(ExecutionTarget::Final)
);
```

Rename the first test to `document_change_preserves_visible_result_and_disables_copy`; keep the other two names because their high-level contracts remain accurate.

Run:

```bash
rtk cargo fmt --check
rtk cargo test --locked --lib tui::state::tests
rtk cargo test --locked --lib tui::render::tests
```

Expected: all state and render tests PASS; the two exact RED tests now report `running 1 test`, `1 passed` when run individually.

- [ ] **Step 10: Synchronize waiting-state documentation**

Add to `README.md` after the Output paragraph:

```markdown
변환 대기 중에는 이전 Output과 Trace를 그대로 표시하지만 복사는 비활성화됩니다.
처리가 시작된 뒤 1초를 넘으면 Footer에 이전 결과를 표시 중이라는 안내와 `Esc`
취소 키를 표시합니다. 실패하거나 취소되면 이전 결과를 현재 결과처럼 남기지 않습니다.
```

Replace `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md:175` with:

```markdown
* 새 최종 결과가 준비되기 전에는 현재 Output과 Trace를 이전 결과로 유지하되
  복사를 비활성화한다. Input 또는 Pipeline이 바뀌면 최종 결과 캐시는 즉시
  무효화하며, 최신 실행의 실패·취소 뒤에는 이전 표시를 제거한다. 실행 시작 뒤
  1초가 지나면 Footer 첫째 줄에 이전 결과 표시와 취소 키를 안내한다.
```

- [ ] **Step 11: Commit the waiting-state change**

```bash
rtk git add src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md
rtk git diff --cached --check
rtk git commit -m "fix(tui): 변환 대기 결과 보존"
```

---

### Task 2: Quiet Prism과 Grouped Command Dock

**Files:**
- Modify: `src/tui/state.rs:238-276`
- Modify: `src/tui/render.rs:1-80,194-366,760-824`
- Test: `src/tui/render.rs:1083-1155,1302-1317,1889-1915,2200-2274`
- Modify: `README.md:53-71`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md:101-169`

**Interfaces:**
- Consumes: Task 1의 `OutputStatus::long_running_notice()`와 기존 `WidthMode`
- Produces: Prism palette constants, `DockCommand`, `dock_line(&App, &str, &[DockCommand], u16) -> Line<'static>`
- Preserves: Footer 두 줄 높이, 상태가 첫째 줄만 대체하는 규칙, 기존 키 처리

- [ ] **Step 1: Replace the basic-color test with Quiet Prism RED assertions**

Replace `colored_render_uses_only_basic_colors_and_no_literal_ansi` with:

```rust
#[test]
fn colored_render_uses_only_the_quiet_prism_palette_and_no_ansi() {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(now(), false);
    app.steps.push(TransformStep {
        definition: transform_by_id("url-encode").unwrap(),
        enabled: true,
    });
    app.focus = Pane::Pipeline;
    terminal.draw(|frame| render(frame, &mut app)).unwrap();

    let approved = [
        Color::Reset,
        BACKGROUND,
        SURFACE_HIGH,
        BORDER,
        TEXT,
        MUTED,
        CYAN,
        GREEN,
        YELLOW,
        RED,
    ];
    for cell in terminal.backend().buffer().content() {
        assert!(approved.contains(&cell.fg), "unexpected fg: {:?}", cell.fg);
        assert!(approved.contains(&cell.bg), "unexpected bg: {:?}", cell.bg);
        assert!(!cell.symbol().contains('\u{1b}'));
    }
    assert_eq!(pane_style(&app, true).fg, Some(CYAN));
    assert_eq!(pane_style(&app, false).fg, Some(MUTED));
    assert_eq!(selection_style(&app).fg, Some(CYAN));
    assert_eq!(selection_style(&app).bg, Some(SURFACE_HIGH));
}
```

- [ ] **Step 2: Run the palette test and verify RED**

```bash
rtk cargo test --locked --lib tui::render::tests::colored_render_uses_only_the_quiet_prism_palette_and_no_ansi -- --exact
```

Expected: compilation failure because palette constants do not exist yet.

- [ ] **Step 3: Add the exact palette and replace existing basic colors**

Add at the top of `render.rs`:

```rust
const BACKGROUND: Color = Color::Rgb(0x11, 0x11, 0x1b);
const SURFACE_HIGH: Color = Color::Rgb(0x24, 0x24, 0x38);
const BORDER: Color = Color::Rgb(0x36, 0x3a, 0x4f);
const TEXT: Color = Color::Rgb(0xcd, 0xd6, 0xf4);
const MUTED: Color = Color::Rgb(0x6c, 0x70, 0x86);
const CYAN: Color = Color::Rgb(0x89, 0xdc, 0xeb);
const GREEN: Color = Color::Rgb(0xa6, 0xe3, 0xa1);
const YELLOW: Color = Color::Rgb(0xf9, 0xe2, 0xaf);
const RED: Color = Color::Rgb(0xf3, 0x8b, 0xa8);
```

Replace `pane_style` and `selection_style` with:

```rust
fn pane_style(app: &App, focused: bool) -> Style {
    if app.no_color {
        Style::default()
    } else if focused {
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED).add_modifier(Modifier::BOLD)
    }
}

fn selection_style(app: &App) -> Style {
    if app.no_color {
        Style::default()
    } else {
        Style::default()
            .fg(CYAN)
            .bg(SURFACE_HIGH)
            .add_modifier(Modifier::BOLD)
    }
}
```

Use Text for ordinary content and Green/Red/Yellow for semantic status. At the beginning of `render`, before the Tiny branch, apply the base style only in color mode:

```rust
if !app.no_color {
    frame
        .buffer_mut()
        .set_style(area, Style::default().fg(TEXT).bg(BACKGROUND));
}
```

In `App::new`, replace the colored textarea styles with:

```rust
textarea.set_cursor_style(
    Style::default()
        .fg(Color::Rgb(0x11, 0x11, 0x1b))
        .bg(Color::Rgb(0x89, 0xdc, 0xeb))
        .add_modifier(Modifier::BOLD),
);
textarea.set_selection_style(
    Style::default()
        .fg(Color::Rgb(0x11, 0x11, 0x1b))
        .bg(Color::Rgb(0xf9, 0xe2, 0xaf)),
);
```

Do not introduce a shared theme module.

- [ ] **Step 4: Run the palette and NO_COLOR tests and verify GREEN**

```bash
rtk cargo test --locked --lib tui::render::tests::colored_render_uses_only_the_quiet_prism_palette_and_no_ansi -- --exact
rtk cargo test --locked --lib tui::render::tests::no_color_uses_default_cell_styles_and_status_marks -- --exact
```

Expected: each command reports `running 1 test`, `1 passed`; `NO_COLOR` cells retain Reset foreground/background.

- [ ] **Step 5: Add Grouped Command Dock RED tests**

Update existing Footer assertions with these exact label changes before adding the new test:

```rust
assert!(lines[14].starts_with("OUTPUT │"));
assert!(lines[15].starts_with("GLOBAL │"));
assert!(!lines[14].contains("INPUT │"));
assert!(screen.contains("GLOBAL │"));
```

This replaces the old `[OUTPUT]`, `[COMMON]` and `[INPUT]` expectations in the Wide/Medium/Footer status and `no_color_uses_default_cell_styles_and_status_marks` tests. Then add:

```rust
#[test]
fn grouped_command_dock_keeps_atomic_groups_at_wide_and_narrow_widths() {
    let mut app = App::new(now(), true);
    app.focus = Pane::Output;
    app.output.status = OutputStatus::Ready;
    app.output.active_artifact = Some(Artifact::new(b"copyable".to_vec()));

    let wide = rendered_app(120, 20, &mut app);
    let wide_lines = wide.lines().collect::<Vec<_>>();
    assert!(wide_lines[18].contains(
        "OUTPUT │ [ Enter ] Pretty  [ v/V ] View │ [ p ] Step  [ f ] Final │ [ z ] Zoom"
    ));
    assert!(wide_lines[19].contains(
        "GLOBAL │ [ Tab ] Focus │ [ F3 ] Pretty  [ F4 ] Raw │ [ Ctrl+P ] Add"
    ));
    assert!(wide_lines[19].contains("[ F1 ] Help  [ Ctrl+Q ] Quit"));

    let narrow = rendered_app(40, 10, &mut app);
    let narrow_lines = narrow.lines().collect::<Vec<_>>();
    assert!(narrow_lines[8].starts_with("OUTPUT │ [ Enter ] Pretty"));
    assert!(narrow_lines[8].contains("[ v/V ] View"));
    assert!(!narrow_lines[8].contains("[ p ]"));
    assert!(narrow_lines[9].starts_with("GLOBAL │ [ Tab ] Focus"));
    assert!(narrow_lines[9].contains("[ F3 ] Pretty"));
    assert!(!narrow_lines[9].contains("[ F4 ]"));
}
```

- [ ] **Step 6: Run the Command Dock test and verify RED**

```bash
rtk cargo test --locked --lib tui::render::tests::grouped_command_dock_keeps_atomic_groups_at_wide_and_narrow_widths -- --exact
```

Expected: `running 1 test`, then FAIL because the old Footer renders `[OUTPUT]` and `[COMMON]` plain strings.

- [ ] **Step 7: Implement one width-aware Span builder**

Add this minimal representation near the current Footer helpers:

```rust
#[derive(Clone, Copy)]
struct DockCommand {
    key: Option<&'static str>,
    label: &'static str,
    divider_before: bool,
}

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
            .bg(SURFACE_HIGH)
            .add_modifier(Modifier::BOLD)
    };
    let separator_style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(BORDER)
    };
    let mut line = Line::from(Span::styled(scope, scope_style));
    let mut shown = 0usize;

    for command in commands {
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
        shown += 1;
    }
    line
}
```

Define these fixed command arrays in priority order:

```rust
const INPUT_COMMANDS: &[DockCommand] = &[
    DockCommand { key: None, label: "Text editing", divider_before: false },
    DockCommand { key: Some("Esc"), label: "Cancel", divider_before: true },
];
const PIPELINE_COMMANDS: &[DockCommand] = &[
    DockCommand { key: Some("j/k"), label: "Select", divider_before: false },
    DockCommand { key: Some("J/K"), label: "Move", divider_before: false },
    DockCommand { key: Some("Space"), label: "Toggle", divider_before: true },
    DockCommand { key: Some("Enter"), label: "Inspect", divider_before: false },
];
const OUTPUT_COMMANDS: &[DockCommand] = &[
    DockCommand { key: Some("Enter"), label: "Pretty", divider_before: false },
    DockCommand { key: Some("v/V"), label: "View", divider_before: false },
    DockCommand { key: Some("p"), label: "Step", divider_before: true },
    DockCommand { key: Some("f"), label: "Final", divider_before: false },
    DockCommand { key: Some("z"), label: "Zoom", divider_before: true },
];
const GLOBAL_COMMANDS: &[DockCommand] = &[
    DockCommand { key: Some("Tab"), label: "Focus", divider_before: false },
    DockCommand { key: Some("F3"), label: "Pretty", divider_before: true },
    DockCommand { key: Some("F4"), label: "Raw", divider_before: false },
    DockCommand { key: Some("Ctrl+P"), label: "Add", divider_before: true },
    DockCommand { key: Some("F1"), label: "Help", divider_before: false },
    DockCommand { key: Some("Ctrl+Q"), label: "Quit", divider_before: false },
];
```

Pass `&OUTPUT_COMMANDS[1..]` directly when Output is not copyable; do not duplicate the array. Replace the old `focused_help`, `common_help` and String-only Footer path with:

```rust
fn footer_status(app: &App, width: usize) -> Option<String> {
    match &app.output.status {
        OutputStatus::Failed(error) => Some(crate::error::escape_external(
            &render_pipeline_error_summary(error),
            width,
        )),
        OutputStatus::Cancelled => Some("Cancelled".to_string()),
        status if status.long_running_notice() => Some(
            if app.output.active_artifact.is_some() || !app.output.traces.is_empty() {
                "Still processing · Previous result shown · Esc Cancel"
            } else {
                "Still processing · Esc Cancel"
            }
            .to_string(),
        ),
        _ => app
            .status
            .as_ref()
            .map(|status| crate::error::escape_external(status, width)),
    }
}

fn footer_first_line(app: &App, width: u16) -> Line<'static> {
    if let Some(status) = footer_status(app, width as usize) {
        return Line::raw(status);
    }
    match app.focus {
        Pane::Input => dock_line(app, "INPUT", INPUT_COMMANDS, width),
        Pane::Pipeline => dock_line(app, "PIPELINE", PIPELINE_COMMANDS, width),
        Pane::Output if app.can_copy() => dock_line(app, "OUTPUT", OUTPUT_COMMANDS, width),
        Pane::Output => dock_line(app, "OUTPUT", &OUTPUT_COMMANDS[1..], width),
    }
}
```

Render `dock_line(app, "GLOBAL", GLOBAL_COMMANDS, common_help_area.width)` on the second row. This keeps Failed, Cancelled, Task 1's long-running notice and `App.status` precedence unchanged.

- [ ] **Step 8: Assert colored Keycap cells and NO_COLOR brackets**

Add this `TestBackend` style assertion:

```rust
#[test]
fn colored_keycap_uses_surface_high_while_no_color_uses_brackets() {
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
    assert_eq!(enter.bg, SURFACE_HIGH);
    assert!(enter.modifier.contains(Modifier::BOLD));

    let mut no_color = App::new(now(), true);
    no_color.focus = Pane::Output;
    no_color.output.status = OutputStatus::Ready;
    no_color.output.active_artifact = Some(Artifact::new(b"copyable".to_vec()));
    let screen = rendered_app(120, 20, &mut no_color);
    assert!(screen.lines().nth(18).unwrap().contains("[ Enter ] Pretty"));
}
```

Run:

```bash
rtk cargo fmt --check
rtk cargo test --locked --lib tui::render::tests
```

Expected: all render tests PASS, including Wide/Medium/Narrow/Tiny, Zoom and two-line Footer regressions.

- [ ] **Step 9: Synchronize Quiet Prism and Footer documentation**

Update `README.md` to call the bottom two rows a Grouped Command Dock and explain that complete commands are omitted by width without changing keys. In `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md`, replace the obsolete Yellow/Green basic-color policy and plain Footer example with:

```text
OUTPUT │  Enter  Pretty   v/V  View │  p  Step   f  Final │  z  Zoom
GLOBAL │  Tab  Focus │  F3  Pretty   F4  Raw │  Ctrl+P  Add   F1  Help   Ctrl+Q  Quit
```

Document Cyan focus, Muted inactive borders, Surface High selection/Keycaps and the `NO_COLOR` `[ Key ]` fallback. Preserve the current panel layout and titles.

- [ ] **Step 10: Commit the Quiet Prism and Dock change**

```bash
rtk git add src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md
rtk git diff --cached --check
rtk git commit -m "feat(tui): Quiet Prism 명령 Dock 적용"
```

---

### Task 3: 모든 Modal에 Dim과 한 셀 Shadow 적용

**Files:**
- Modify: `src/tui/render.rs:423-758,760-824`
- Test: `src/tui/render.rs:974-1081,1369-1558,2225-2343`
- Modify: `README.md:73-79`
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md:132-169`

**Interfaces:**
- Consumes: Task 2의 `SURFACE_HIGH`, `CYAN`과 기존 `centered`
- Produces: `modal_block(&App, &str) -> Block`, `render_modal_layer(&mut Frame, &App, &mut MouseRegions)`
- Preserves: 각 Modal의 area, content, action rectangle과 입력 우선순위

- [ ] **Step 1: Add a modal depth RED test for every variant**

Add a test helper and test in `src/tui/render.rs`:

```rust
fn assert_modal_depth(app: &mut App, modal_area: Rect) {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();

    assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
    assert_eq!(buffer[(modal_area.right(), modal_area.y + 1)].bg, SURFACE_HIGH);
    assert!(!buffer[(modal_area.x + 1, modal_area.y + 1)]
        .modifier
        .contains(Modifier::DIM));
}

#[test]
fn every_modal_dims_the_base_and_renders_a_one_cell_shadow() {
    let start = now();

    let mut picker = App::new(start, false);
    picker.open_picker();
    assert_modal_depth(&mut picker, centered(Rect::new(0, 0, 120, 30), 72, 18));

    let mut inspector = App::new(start, false);
    inspector.steps.push(TransformStep {
        definition: transform_by_id("base64-encode").unwrap(),
        enabled: true,
    });
    inspector.modal = Some(Modal::StepInspector);
    assert_modal_depth(
        &mut inspector,
        centered(Rect::new(0, 0, 120, 30), 78, 13),
    );

    let mut help = App::new(start, false);
    help.modal = Some(Modal::Help);
    assert_modal_depth(&mut help, centered(Rect::new(0, 0, 120, 30), 68, 17));

    let mut confirm = App::new(start, false);
    confirm.modal = Some(Modal::QuitConfirm);
    assert_modal_depth(
        &mut confirm,
        centered(Rect::new(0, 0, 120, 30), 42, 5),
    );

    let mut unsafe_confirm = App::new(start, false);
    unsafe_confirm.modal = Some(Modal::UnsafeCopyConfirm {
        payload: ClipboardPayload {
            text: "safe fixture".to_string(),
            kind: CopyKind::Pretty,
        },
    });
    assert_modal_depth(
        &mut unsafe_confirm,
        centered(Rect::new(0, 0, 120, 30), 42, 5),
    );
}
```

- [ ] **Step 2: Run the modal depth test and verify RED**

```bash
rtk cargo test --locked --lib tui::render::tests::every_modal_dims_the_base_and_renders_a_one_cell_shadow -- --exact
```

Expected: `running 1 test`, then FAIL because the background lacks DIM and the shadow cell lacks Surface High.

- [ ] **Step 3: Reuse one modal block and one modal layer**

Import `Shadow` and replace the four duplicated Modal block builders with:

```rust
fn modal_block<'a>(app: &App, title: &'a str) -> Block<'a> {
    let style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
    };
    let shadow = if app.no_color {
        Shadow::overlay().style(Style::default().add_modifier(Modifier::DIM))
    } else {
        Shadow::overlay().style(Style::default().bg(SURFACE_HIGH))
    };
    Block::bordered()
        .title(title)
        .style(Style::default())
        .border_style(style)
        .title_style(style)
        .shadow(shadow)
}

fn render_modal_layer(
    frame: &mut Frame<'_>,
    app: &App,
    mouse_regions: &mut MouseRegions,
) {
    if app.modal.is_none() {
        return;
    }
    let area = frame.area();
    frame
        .buffer_mut()
        .set_style(area, Style::default().add_modifier(Modifier::DIM));
    render_modal(frame, app, mouse_regions);
}
```

Keep each existing `Clear` call before rendering its `modal_block`. Replace both normal and Tiny calls to `render_modal` with `render_modal_layer`. Ratatui `Shadow::overlay()` already uses `Offset::new(1, 1)`, so do not add custom geometry.

- [ ] **Step 4: Verify NO_COLOR, mouse regions and modal content**

Run:

```bash
rtk cargo fmt --check
rtk cargo test --locked --lib tui::render::tests::every_modal_dims_the_base_and_renders_a_one_cell_shadow -- --exact
rtk cargo test --locked --lib tui::render::tests::no_color_uses_default_cell_styles_and_status_marks -- --exact
rtk cargo test --locked --lib tui::render::tests::picker_click_selects_then_explicit_add_and_cancel_regions_act -- --exact
rtk cargo test --locked --lib tui::render::tests::confirmation_and_close_regions_reuse_the_keyboard_actions -- --exact
rtk cargo test --locked --lib tui::render::tests::confirmation_modals_render_explicit_warning_text -- --exact
```

Expected: every command reports `running 1 test`, `1 passed`; colored shadows use one-cell Surface High, `NO_COLOR` uses no RGB, and action regions/content remain unchanged.

- [ ] **Step 5: Synchronize Modal documentation**

Extend the README Modal paragraph with:

```markdown
Modal이 열리면 기존 화면은 어둡게 표시되고 Popup 오른쪽·아래에 한 셀 Shadow를
그립니다. 이 효과는 Modal 크기, 키 처리와 마우스 클릭 영역을 바꾸지 않습니다.
```

Update `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md` to record the base → Dim → Shadow → Clear → Modal render order and the no-animation decision.

- [ ] **Step 6: Commit the Modal depth change**

```bash
rtk git add src/tui/render.rs README.md docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md
rtk git diff --cached --check
rtk git commit -m "feat(tui): Modal 깊이감 적용"
```

---

### Task 4: 전체 회귀 검증과 구현 상태 확정

**Files:**
- Modify: `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md:1-8,238-320`
- Conditional Modify: `README.md:86-139` only when verification commands or counts differ from the current documented contract

**Interfaces:**
- Consumes: Tasks 1-3의 committed implementation and documentation
- Produces: passing format, lint, test, rustdoc, performance, package and PTY evidence; design status `사용자 승인·구현 완료`

- [ ] **Step 1: Run formatting, lint, tests and rustdoc from a clean index**

```bash
rtk git status --short --branch
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets --all-features -- -D warnings
rtk cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' rtk cargo doc --no-deps --all-features
```

Expected: only user-owned `.codex/` may remain untracked; format, Clippy, all tests and rustdoc PASS with no warnings. If any command changes a file or fails, return to the owning task instead of weakening the check.

- [ ] **Step 2: Run the redraw measurement**

```bash
rtk cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
```

Expected: the ignored measurement runs once, reports `redraws=1`, and does not act as a timing pass/fail threshold.

- [ ] **Step 3: Run package, temporary offline install and shell smoke checks**

```bash
rtk cargo package --locked
toc_install_root=$(rtk mktemp -d "${TMPDIR:-/tmp}/toc-install.XXXXXX")
rtk cargo install --locked --offline --path . --root "$toc_install_root"
rtk "$toc_install_root/bin/toc" --version
rtk bash tests/shell-smoke.sh
rtk zsh tests/shell-smoke.sh
TOC_SMOKE_CLIPBOARD_MODE=macos rtk bash tests/shell-smoke.sh
rtk pbpaste
TOC_SMOKE_CLIPBOARD_MODE=macos rtk zsh tests/shell-smoke.sh
rtk pbpaste
case "$toc_install_root" in
  "${TMPDIR:-/tmp}"/toc-install.*) rtk rm -rf -- "$toc_install_root" ;;
  *) exit 1 ;;
esac
```

Expected: package and offline install succeed, version prints `toc 0.2.0`, both normal PTY suites pass, both macOS clipboard suites pass, each `pbpaste` output is lowercase `ff`, and only the validated temporary install directory is removed.

- [ ] **Step 4: Mark the approved design implemented**

Change the design header to:

```markdown
* **상태:** 사용자 승인·구현 완료
```

Append this verification statement without inventing counts or timings:

```markdown
2026-08-01 전체 형식, Clippy, 잠금 전체 시험, rustdoc, release redraw 측정,
잠금 패키징, 임시 경로 오프라인 설치, Bash·Zsh PTY와 macOS 클립보드 Smoke를
실행해 통과했다. redraw 측정은 1회의 변경 시 렌더만 확인하며 시간값을 성공
기준으로 사용하지 않는다.
```

If actual command scope differs, record only the commands that really ran and passed.

- [ ] **Step 5: Commit verification documentation and confirm final scope**

```bash
rtk git add docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md README.md
rtk git diff --cached --check
rtk git diff --cached --name-status
rtk git commit -m "docs(tui): Quiet Prism 검증 결과"
rtk git status --short --branch
rtk git log -4 --oneline --decorate
```

Expected: the final four commits are the three logical implementation commits plus the verification documentation commit. `.codex/` remains untracked and unmodified; no build output, `.env` or unrelated file is staged.
