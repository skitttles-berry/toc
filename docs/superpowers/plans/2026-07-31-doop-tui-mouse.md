# doop TUI Mouse Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `doop tui`에 렌더 좌표 기반 패널 포커스·목록 선택·영역별 휠·Modal 동작과 안전한 마우스 캡처 복구를 추가한다.

**Architecture:** 기존 Crossterm 이벤트 루프가 마우스 이벤트를 `AppEvent`로 전달하고, Ratatui 렌더러가 매 프레임 실제로 그린 `Rect`를 하나의 임시 `MouseRegions`에 기록한다. `App`은 이 좌표만 판정해 기존 키보드 상태 전이와 스크롤 함수를 재사용하며, 새 모듈·의존성·범용 이벤트 계층은 만들지 않는다.

**Tech Stack:** Rust 1.97.1, Crossterm 0.29.0, Ratatui 0.30.2, tui-textarea-2 0.12.1, unicode-width 0.2.2, 기존 Rust 단위 시험·Ratatui `TestBackend`·Expect PTY Smoke

## Global Constraints

- 기준 설계는 `docs/superpowers/specs/2026-07-31-doop-tui-mouse-design.md`다.
- `doop tui` 세션에서는 마우스 캡처를 항상 켜고 정상 종료·오류·패닉에서 역순으로 끈다.
- 입력 순서는 raw mode, alternate screen, bracketed paste, mouse capture, cursor hide이며 복구는 정확히 역순이다.
- 처리 이벤트는 modifier가 없는 `Down(MouseButton::Left)`, `ScrollUp`, `ScrollDown`뿐이다.
- `Moved`, `Drag`, `Up`, 오른쪽·가운데 버튼, modifier가 있는 이벤트, 수평 휠과 등록 영역 밖 좌표는 상태·효과·dirty를 바꾸지 않는다.
- 패널 클릭은 포커스만 바꾸며, Pipeline의 실제 표시 행과 Add Transform의 실제 표시 항목 클릭만 선택도 바꾼다.
- Input 클릭은 caret·selection을 바꾸지 않고 Output 클릭은 복사·View·원본을 바꾸지 않는다.
- Output 휠은 기존 스크롤 단위 3회, Pipeline과 Add Transform 휠은 선택 1개만 이동하며 휠은 포커스를 바꾸지 않는다.
- Modal이 열려 있으면 Modal 영역만 처리하고 바깥 클릭·휠과 아래 패널 영역은 무시한다.
- 클릭 가능한 Modal 동작은 `[Enter Add]`, `[Esc Cancel]`, `[Enter/y Confirm]`, `[n/Esc Cancel]`, `[Esc Close]`뿐이며 기존 키 처리 경로를 재사용한다.
- Hover 상태, 마우스 이동 redraw, Input 좌표 caret, 드래그 선택, Output 마우스 복사, Pipeline 직접 토글·이동·삭제, Footer 클릭과 사용자 마우스 설정은 추가하지 않는다.
- 기존 키보드 단축키, Copy 원본·안전성, Pipeline 실행, Worker, View, 반응형 배치, 입력·출력·단계 제한과 `NO_COLOR` 규약을 유지한다.
- 새 Cargo 의존성과 새 TUI 모듈을 추가하지 않는다.
- 제품 UI 문자열은 영어, 프로젝트 문서는 한국어를 사용한다.
- 관련 README와 기존 TUI 작업판 설계는 해당 코드 변경과 같은 커밋에서 현행화한다.
- 커밋은 한국어 Conventional Commits, 50자 이내, 명사형 종결을 사용한다.

## File Map

| 파일 | 책임 |
|---|---|
| `src/tui.rs` | 마우스 캡처 시작·역순 복구, Crossterm 마우스 이벤트 전달과 수명주기 단위 시험 |
| `src/tui/state.rs` | 프레임별 `MouseRegions`, 클릭·휠·Modal 상태 전이와 dirty/effect 단위 시험 |
| `src/tui/render.rs` | 실제 패널·목록·동작 라벨 좌표 기록, 마우스 도움말과 `TestBackend` 통합 시험 |
| `tests/shell-smoke.sh` | SGR 마우스 Sequence 파싱, 패널·목록·Modal·휠 동작과 캡처 해제 PTY 검증 |
| `README.md` | 지원 마우스 동작, 캡처 기간과 터미널 기본 드래그 선택 제한 안내 |
| `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md` | 이벤트 흐름·터미널 복구·화면 조작·시험 전략 현행화 |

---

### Task 1: 터미널 마우스 캡처 수명주기

**Files:**
- Modify: `src/tui.rs:8-16`
- Modify: `src/tui.rs:51-101`
- Modify: `src/tui.rs:177-183`
- Test: `src/tui.rs:235-267`

**Interfaces:**
- Consumes: 기존 `execute_tracked<W, C>(writer: &mut W, active: &mut bool, command: C) -> io::Result<()>`, `TerminalSession::restore(&mut self)`, `best_effort_restore_terminal()`
- Produces: `TerminalSession::mouse: bool`, Crossterm `EnableMouseCapture`·`DisableMouseCapture`의 추적 시작과 역순 복구

- [ ] **Step 1: 마우스 캡처 추적 실패 시험 작성**

`src/tui.rs` 시험 모듈에 다음 시험을 추가한다. 이 시험은 ANSI 출력 뒤 flush가 실패해도 복구 플래그가 먼저 설정되는 기존 규약을 마우스 캡처에도 고정한다.

```rust
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
```

- [ ] **Step 2: 새 시험이 실패하는지 확인**

Run: `cargo test --locked tui::tests::tracked_mouse_capture_marks_state_when_flush_fails_after_write -- --exact`

Expected: `EnableMouseCapture`가 import되지 않아 컴파일이 실패한다.

- [ ] **Step 3: 캡처 시작과 역순 복구 구현**

`src/tui.rs`의 Crossterm event import에 다음 두 명령을 추가한다.

```rust
DisableMouseCapture, EnableMouseCapture,
```

`TerminalSession`과 초기값에 `mouse`를 추가한다.

```rust
struct TerminalSession {
    raw: bool,
    alternate: bool,
    paste: bool,
    mouse: bool,
    cursor_hidden: bool,
}
```

```rust
mouse: false,
```

`EnableBracketedPaste` 다음, `Hide` 전에 추적 명령을 실행한다.

```rust
execute_tracked(&mut stdout, &mut session.mouse, EnableMouseCapture)
    .map_err(|error| AppError::Tui(error.to_string()))?;
```

`restore`에서는 `Show` 다음, `DisableBracketedPaste` 전에 마우스를 끈다.

```rust
if self.mouse {
    let _ = execute!(stdout, DisableMouseCapture);
    self.mouse = false;
}
```

패닉 복구 명령도 같은 순서로 바꾼다.

```rust
let _ = execute!(
    stdout,
    Show,
    DisableMouseCapture,
    DisableBracketedPaste,
    LeaveAlternateScreen
);
```

- [ ] **Step 4: 터미널 수명주기 단위 시험 통과 확인**

Run: `cargo test --locked tui::tests`

Expected: 기존 `tracked_command_marks_state_when_flush_fails_after_write`와 새 마우스 추적 시험을 포함한 `tui::tests`가 모두 통과한다.

- [ ] **Step 5: 첫 변경 커밋**

```bash
git add src/tui.rs
git commit -m "feat(tui): 마우스 캡처 수명주기"
```

---

### Task 2: 렌더 좌표와 패널·Pipeline 클릭

**Files:**
- Modify: `src/tui.rs:8-16,127-158`
- Modify: `src/tui/state.rs:1-8,44-194`
- Modify: `src/tui/state.rs:657-674,1001-1091`
- Test: `src/tui/state.rs:1113-1333`
- Modify: `src/tui/render.rs:1-18,94-264,354-405,648-731`
- Test: `src/tui/render.rs:751-874,945-1019,1860-1892`

**Interfaces:**
- Consumes: `Rect::contains(Position) -> bool`, `App::mark_dirty()`, 기존 패널 렌더 함수와 `draw_if_dirty`
- Produces: `MouseRegions`, `App::mouse_regions: MouseRegions`, `AppEvent::Mouse(MouseEvent, Instant)`, `App::handle_mouse(MouseEvent, Instant) -> Vec<Effect>`

- [ ] **Step 1: 패널·Pipeline 클릭 RED 시험 작성**

`src/tui/state.rs` 시험 import를 다음처럼 확장하고 helper를 추가한다.

```rust
use crossterm::event::{
    KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

fn mouse(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> AppEvent {
    AppEvent::Mouse(
        MouseEvent {
            kind,
            column,
            row,
            modifiers,
        },
        now(),
    )
}
```

다음 시험으로 테두리 포커스, 실제 행 선택, Input caret 불변과 Output 무동작을 고정한다.

```rust
#[test]
fn left_click_focuses_a_rendered_pane_and_pipeline_row_only() {
    let mut app = App::new(now(), true);
    app.steps = ["base64-encode", "base64-decode"]
        .into_iter()
        .map(|id| TransformStep {
            definition: transform_by_id(id).unwrap(),
            enabled: true,
        })
        .collect();
    app.selected_step = 0;
    app.mouse_regions.input = Some(Rect::new(20, 1, 20, 8));
    app.mouse_regions.output = Some(Rect::new(20, 9, 20, 10));
    app.mouse_regions.pipeline = Some(Rect::new(0, 1, 20, 18));
    app.mouse_regions.pipeline_rows = vec![
        (Rect::new(1, 2, 18, 1), 0),
        (Rect::new(1, 3, 18, 1), 1),
    ];
    let input_cursor = app.textarea.cursor();
    app.take_dirty();

    assert!(app
        .handle_event(mouse(
            MouseEventKind::Down(MouseButton::Left),
            5,
            3,
            KeyModifiers::NONE,
        ))
        .is_empty());
    assert_eq!(app.focus, Pane::Pipeline);
    assert_eq!(app.selected_step, 1);
    assert!(app.take_dirty());

    app.handle_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        20,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.focus, Pane::Input);
    assert_eq!(app.selected_step, 1);
    assert_eq!(app.textarea.cursor(), input_cursor);

    let source = app.output.source;
    let view = app.output.view;
    assert!(app
        .handle_event(mouse(
            MouseEventKind::Down(MouseButton::Left),
            21,
            10,
            KeyModifiers::NONE,
        ))
        .is_empty());
    assert_eq!(app.focus, Pane::Output);
    assert_eq!(app.output.source, source);
    assert_eq!(app.output.view, view);

    app.handle_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        0,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.focus, Pane::Pipeline);
    assert_eq!(app.selected_step, 1);
}
```

- [ ] **Step 2: 무시 이벤트 no-dirty RED 시험 작성**

```rust

#[test]
fn unsupported_or_outside_mouse_events_are_true_no_ops() {
    let mut app = App::new(now(), true);
    app.mouse_regions.input = Some(Rect::new(0, 0, 10, 5));
    app.take_dirty();

    for event in [
        mouse(MouseEventKind::Moved, 1, 1, KeyModifiers::NONE),
        mouse(MouseEventKind::Up(MouseButton::Left), 1, 1, KeyModifiers::NONE),
        mouse(MouseEventKind::Drag(MouseButton::Left), 1, 1, KeyModifiers::NONE),
        mouse(
            MouseEventKind::Down(MouseButton::Right),
            1,
            1,
            KeyModifiers::NONE,
        ),
        mouse(
            MouseEventKind::Down(MouseButton::Middle),
            1,
            1,
            KeyModifiers::NONE,
        ),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            1,
            1,
            KeyModifiers::SHIFT,
        ),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            30,
            30,
            KeyModifiers::NONE,
        ),
        mouse(MouseEventKind::ScrollDown, 1, 1, KeyModifiers::SHIFT),
        mouse(MouseEventKind::ScrollDown, 30, 30, KeyModifiers::NONE),
        mouse(MouseEventKind::ScrollLeft, 1, 1, KeyModifiers::NONE),
    ] {
        assert!(app.handle_event(event).is_empty());
        assert!(!app.take_dirty());
    }
    assert_eq!(app.focus, Pane::Input);
}
```

- [ ] **Step 3: 렌더 영역 RED 시험 작성**

`src/tui/render.rs` 시험 모듈에 다음 시험을 추가한다.

```rust
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
    assert!(app
        .mouse_regions
        .pipeline_rows
        .windows(2)
        .all(|rows| rows[0].0.y + 1 == rows[1].0.y));

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
```

- [ ] **Step 4: 새 시험들이 실패하는지 확인**

Run: `cargo test --locked mouse -- --nocapture`

Expected: `MouseRegions`와 `AppEvent::Mouse`가 정의되지 않아 컴파일이 실패한다.

- [ ] **Step 5: `MouseRegions`와 이벤트 variant 추가**

`src/tui/state.rs` import를 확장한다.

```rust
use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};
```

`Modal` 앞에 전체 기능이 공유할 단일 좌표 묶음을 정의한다.

```rust
#[derive(Debug, Default)]
pub(super) struct MouseRegions {
    pub(super) pipeline: Option<Rect>,
    pub(super) input: Option<Rect>,
    pub(super) output: Option<Rect>,
    pub(super) pipeline_content: Option<Rect>,
    pub(super) output_content: Option<Rect>,
    pub(super) pipeline_rows: Vec<(Rect, usize)>,
    pub(super) picker_content: Option<Rect>,
    pub(super) picker_rows: Vec<(Rect, usize)>,
    pub(super) add_action: Option<Rect>,
    pub(super) confirm_action: Option<Rect>,
    pub(super) cancel_action: Option<Rect>,
    pub(super) close_action: Option<Rect>,
}
```

`AppEvent`, `App`와 `App::new_with_input_limits` 초기값에 다음 항목을 추가한다.

```rust
Mouse(MouseEvent, Instant),
```

```rust
pub(super) mouse_regions: MouseRegions,
```

```rust
mouse_regions: MouseRegions::default(),
```

- [ ] **Step 6: 패널 클릭 상태 전이와 이벤트 전달 구현**

`App`에 포커스와 클릭 처리를 추가한다. Modal 처리는 Task 3이 같은 함수 안에서 확장한다.

```rust
fn focus_pane(&mut self, pane: Pane) {
    if self.focus == pane {
        return;
    }
    self.focus = pane;
    if self.zoom.is_some() {
        self.zoom = Some(pane);
    }
    self.mark_dirty();
}

fn handle_mouse(&mut self, event: MouseEvent, _: Instant) -> Vec<Effect> {
    if event.modifiers != KeyModifiers::NONE || self.modal.is_some() {
        return Vec::new();
    }
    let MouseEventKind::Down(MouseButton::Left) = event.kind else {
        return Vec::new();
    };
    let position = Position::new(event.column, event.row);
    let pipeline_row = self
        .mouse_regions
        .pipeline_rows
        .iter()
        .find(|(area, _)| area.contains(position))
        .map(|(_, index)| *index);
    if self
        .mouse_regions
        .pipeline
        .is_some_and(|area| area.contains(position))
    {
        self.focus_pane(Pane::Pipeline);
        if let Some(index) = pipeline_row
            && self.selected_step != index
        {
            self.selected_step = index;
            self.mark_dirty();
        }
    } else if self
        .mouse_regions
        .input
        .is_some_and(|area| area.contains(position))
    {
        self.focus_pane(Pane::Input);
    } else if self
        .mouse_regions
        .output
        .is_some_and(|area| area.contains(position))
    {
        self.focus_pane(Pane::Output);
    }
    Vec::new()
}
```

`handle_event` match에 다음 arm을 추가한다.

```rust
AppEvent::Mouse(mouse, now) => self.handle_mouse(mouse, now),
```

`src/tui.rs` 이벤트 match의 `Resize` 앞에 다음 arm을 추가하고 `MouseEvent`를 그대로 소유 이동한다.

```rust
crossterm::event::Event::Mouse(mouse) => {
    effects.extend(app.handle_event(AppEvent::Mouse(mouse, Instant::now())));
}
```

- [ ] **Step 7: 렌더가 실제 좌표를 매 프레임 교체하도록 구현**

`src/tui/render.rs`의 state import에 `MouseRegions`를 추가한다.

```rust
state::{App, Modal, MouseRegions, OutputSource, OutputStatus, Pane},
```

패널 함수와 `render_focused_pane`에 `mouse_regions: &mut MouseRegions` 인자를 추가한다. 각 패널의 기존 렌더 본문 앞이나 `inner` 계산 직후에 다음 좌표를 기록한다.

```rust
mouse_regions.input = Some(area);
```

```rust
mouse_regions.output = Some(area);
mouse_regions.output_content = Some(inner);
```

```rust
mouse_regions.pipeline = Some(area);
mouse_regions.pipeline_content = Some(inner);
```

`render_pipeline`에서 `items`를 만든 뒤 `frame.render_widget` 전에 실제 표시 index와 한 줄 높이를 기록한다.

```rust
mouse_regions.pipeline_rows.extend(
    (start..start + items.len()).enumerate().map(|(row, index)| {
        (
            Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
            index,
        )
    }),
);
```

`render`의 첫 줄에서 빈 좌표를 만들고 모든 패널 호출에 전달한다.

```rust
let mut mouse_regions = MouseRegions::default();
```

Tiny 조기 반환 직전과 일반 렌더의 `render_modal` 직후에 각각 새 좌표를 저장한다.

```rust
app.mouse_regions = mouse_regions;
```

Tiny에서는 저장 뒤 `return`하고, 일반 화면에서는 저장을 함수의 마지막 문장으로 둔다. 따라서 resize·zoom·modal 전환 뒤 다음 이벤트보다 앞선 redraw가 이전 좌표를 제거한다.

- [ ] **Step 8: 클릭과 좌표 시험 통과 확인**

Run: `cargo test --locked mouse -- --nocapture`

Expected: 패널 클릭·무시 이벤트·반응형 좌표 시험이 모두 통과한다.

- [ ] **Step 9: 기존 zoom·focus 회귀 확인**

Run: `cargo test --locked tui::render::tests::zoomed_tab_keeps_the_visible_pane_equal_to_focus -- --exact`

Expected: 기존 zoom·focus 연동 시험이 통과한다.

- [ ] **Step 10: 두 번째 변경 커밋**

```bash
git add src/tui.rs src/tui/state.rs src/tui/render.rs
git commit -m "feat(tui): 마우스 패널 탐색"
```

---

### Task 3: 영역별 휠과 Modal 동작

**Files:**
- Modify: `src/tui/state.rs:674-752,825-997`
- Test: `src/tui/state.rs:1302-1333,1723-1921`
- Modify: `src/tui/render.rs:354-665`
- Test: `src/tui/render.rs:1087-1305,2023-2074`

**Interfaces:**
- Consumes: Task 2의 `MouseRegions`·`App::handle_mouse(MouseEvent, Instant)`, 기존 `App::handle_modal_key(KeyEvent, Instant)`, `App::handle_pipeline_key(KeyEvent, Instant)`, `App::scroll_output(i8, usize)`
- Produces: `action_rect(Rect, &str, &str, bool) -> Rect`, Modal item·동작 좌표, Output 3단위·Pipeline/Picker 1단위 휠, Modal 우선순위

- [ ] **Step 1: 휠 범위·단위·포커스 RED 시험 작성**

`src/tui/state.rs` 시험 모듈에 다음 시험을 추가한다.

```rust
#[test]
fn wheel_uses_scoped_units_without_changing_focus() {
    let mut app = App::new(now(), true);
    app.steps = ["base64-encode", "base64-decode"]
        .into_iter()
        .map(|id| TransformStep {
            definition: transform_by_id(id).unwrap(),
            enabled: true,
        })
        .collect();
    let artifact = Artifact::new(b"0\n1\n2\n3\n4".to_vec());
    let mut expected = 0;
    for _ in 0..3 {
        expected = next_text_offset(&artifact, expected);
    }
    app.output.status = OutputStatus::Ready;
    app.output.active_artifact = Some(artifact);
    app.mouse_regions.pipeline_content = Some(Rect::new(0, 0, 10, 10));
    app.mouse_regions.output_content = Some(Rect::new(20, 0, 10, 10));
    app.focus = Pane::Input;

    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        21,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.output.byte_offset, expected);
    assert_eq!(app.focus, Pane::Input);

    app.handle_event(mouse(
        MouseEventKind::ScrollUp,
        21,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.output.byte_offset, 0);
    assert_eq!(app.focus, Pane::Input);

    for _ in 0..32 {
        app.handle_event(mouse(
            MouseEventKind::ScrollDown,
            21,
            1,
            KeyModifiers::NONE,
        ));
    }
    assert_eq!(
        app.output.byte_offset,
        last_text_offset(app.output.active_artifact.as_ref().unwrap())
    );
    app.take_dirty();
    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        21,
        1,
        KeyModifiers::NONE,
    ));
    assert!(!app.take_dirty());

    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        1,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.selected_step, 1);
    assert_eq!(app.focus, Pane::Input);

    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        1,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.selected_step, 1);

    app.handle_event(mouse(
        MouseEventKind::ScrollUp,
        1,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.selected_step, 0);

    app.steps.clear();
    app.take_dirty();
    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        1,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.selected_step, 0);
    assert!(!app.take_dirty());
}
```

- [ ] **Step 2: Input·Modal 배경 무시 RED 시험 작성**

```rust

#[test]
fn input_wheel_and_modal_background_wheel_are_true_no_ops() {
    let mut app = App::new(now(), true);
    app.output.status = OutputStatus::Ready;
    app.output.active_artifact = Some(Artifact::new(b"0\n1\n2".to_vec()));
    app.mouse_regions.input = Some(Rect::new(0, 0, 10, 10));
    app.mouse_regions.output_content = Some(Rect::new(20, 0, 10, 10));
    app.take_dirty();

    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        1,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.output.byte_offset, 0);
    assert!(!app.take_dirty());

    app.modal = Some(Modal::Help);
    app.mouse_regions.output_content = Some(Rect::new(0, 0, 10, 10));
    app.mouse_regions.close_action = Some(Rect::new(20, 20, 5, 1));
    app.handle_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        1,
        1,
        KeyModifiers::NONE,
    ));
    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        1,
        1,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.output.byte_offset, 0);
    assert!(matches!(app.modal, Some(Modal::Help)));
    assert!(!app.take_dirty());
}
```

- [ ] **Step 3: Picker 휠·빈 결과 경계 RED 시험 작성**

```rust

#[test]
fn picker_wheel_moves_one_item_and_clamps() {
    let mut app = App::new(now(), true);
    app.open_picker();
    app.mouse_regions.picker_content = Some(Rect::new(10, 10, 20, 8));

    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        11,
        11,
        KeyModifiers::NONE,
    ));
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 1, .. })
    ));

    for _ in 0..32 {
        app.handle_event(mouse(
            MouseEventKind::ScrollDown,
            11,
            11,
            KeyModifiers::NONE,
        ));
    }
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 7, .. })
    ));

    for _ in 0..32 {
        app.handle_event(mouse(
            MouseEventKind::ScrollUp,
            11,
            11,
            KeyModifiers::NONE,
        ));
    }
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 0, .. })
    ));

    if let Some(Modal::TransformPicker { query, .. }) = &mut app.modal {
        *query = "no-such-transform".to_string();
    }
    app.take_dirty();
    app.handle_event(mouse(
        MouseEventKind::ScrollDown,
        11,
        11,
        KeyModifiers::NONE,
    ));
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 0, .. })
    ));
    assert!(!app.take_dirty());
}
```

- [ ] **Step 4: Modal 항목·동작 RED 통합 시험 작성**

`src/tui/render.rs` 시험 모듈에 다음 helper와 시험을 추가한다.

```rust
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
fn picker_click_selects_then_explicit_add_and_cancel_regions_act() {
    let start = now();
    let mut app = App::new(start, true);
    app.open_picker();
    let screen = rendered_app(120, 24, &mut app);
    assert!(screen.contains("[Enter Add]"));
    assert!(screen.contains("[Esc Cancel]"));
    let second = app.mouse_regions.picker_rows[1].0;
    assert!(click(&mut app, second, start).is_empty());
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 1, .. })
    ));
    let add = app.mouse_regions.add_action.unwrap();
    assert!(click(&mut app, add, start).is_empty());
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
    assert_eq!(app.mouse_regions.picker_rows.first().unwrap().1, 4);
    assert_eq!(app.mouse_regions.picker_rows.last().unwrap().1, 7);
    let first_visible = app.mouse_regions.picker_rows.first().unwrap().0;
    assert!(click(&mut app, first_visible, start).is_empty());
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 4, .. })
    ));
}
```

- [ ] **Step 5: Confirm·Cancel·Close와 Tiny Modal RED 시험 작성**

```rust

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
    rendered_app(120, 24, &mut app);
    let confirm = app.mouse_regions.confirm_action.unwrap();
    let effects = click(&mut app, confirm, start);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Copy(ClipboardPayload { text, .. })] if text == "exact\u{1b}payload"
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
```

시험 import에 다음 타입을 추가한다.

```rust
use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
```

기존 `super::super::state` import 목록에는 `Effect`를 추가한다.

```rust
state::{
    AppEvent, ClipboardPayload, CopyKind, Effect, Modal, OutputSource, OutputStatus, Pane,
    debounce_for,
},
```

기존 Help·Inspector 렌더 시험의 close 기대는 모두 `[Esc Close]`로 바꾼다. `one_context_help_modal_lists_only_real_keys_for_each_pane`의 case에 마우스 기대 문자열을 함께 넣어 일반 Help만 확장되는 규약을 고정한다.

```rust
for (pane, mouse_help) in [
    (Pane::Input, "Mouse Click  Focus only"),
    (
        Pane::Pipeline,
        "Mouse Click  Focus/select · Wheel  Move selection",
    ),
    (
        Pane::Output,
        "Mouse Click  Focus only · Wheel  Scroll",
    ),
] {
    let mut app = App::new(start, true);
    app.focus = pane;
    key(&mut app, KeyCode::F(1), KeyModifiers::NONE, start);
    let screen = rendered_app(80, 20, &mut app);
    assert!(screen.contains(mouse_help), "missing {mouse_help}: {screen}");
    assert!(screen.contains("[Esc Close]"));
}
```

40×10 compact Help·Inspector 시험에서는 기존 key·status 기대를 유지하고 마지막 기대만 다음 문자열로 교체한다.

```rust
"[Esc Close]"
```

- [ ] **Step 6: 휠과 Modal 시험이 실패하는지 확인**

Run: `cargo test --locked mouse -- --nocapture`

Expected: 휠이 상태를 바꾸지 않고 Modal 좌표가 비어 있어 새 시험이 실패한다.

- [ ] **Step 7: Modal 마우스 상태 전이 구현**

`src/tui/state.rs`에서 Modal 전용 함수를 Task 2의 `handle_mouse` 바로 앞에 추가한다.

```rust
fn handle_modal_mouse(&mut self, event: MouseEvent, now: Instant) -> Vec<Effect> {
    let position = Position::new(event.column, event.row);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let picker_row = self
                .mouse_regions
                .picker_rows
                .iter()
                .find(|(area, _)| area.contains(position))
                .map(|(_, index)| *index);
            if let Some(index) = picker_row {
                if let Some(Modal::TransformPicker { selected, .. }) = &mut self.modal
                    && *selected != index
                {
                    *selected = index;
                    self.mark_dirty();
                }
                return Vec::new();
            }
            let key = if self
                .mouse_regions
                .add_action
                .is_some_and(|area| area.contains(position))
                || self
                    .mouse_regions
                    .confirm_action
                    .is_some_and(|area| area.contains(position))
            {
                Some(KeyCode::Enter)
            } else if self
                .mouse_regions
                .cancel_action
                .is_some_and(|area| area.contains(position))
                || self
                    .mouse_regions
                    .close_action
                    .is_some_and(|area| area.contains(position))
            {
                Some(KeyCode::Esc)
            } else {
                None
            };
            key.map_or_else(Vec::new, |code| {
                self.handle_modal_key(KeyEvent::new(code, KeyModifiers::NONE), now)
            })
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if matches!(self.modal, Some(Modal::TransformPicker { .. }))
                && self
                    .mouse_regions
                    .picker_content
                    .is_some_and(|area| area.contains(position)) =>
        {
            let code = if event.kind == MouseEventKind::ScrollUp {
                KeyCode::Up
            } else {
                KeyCode::Down
            };
            self.handle_modal_key(KeyEvent::new(code, KeyModifiers::NONE), now)
        }
        _ => Vec::new(),
    }
}
```

- [ ] **Step 8: 패널 클릭과 영역별 휠 상태 전이 구현**

Task 2의 `handle_mouse`를 다음 최종 형태로 교체한다.

```rust

fn handle_mouse(&mut self, event: MouseEvent, now: Instant) -> Vec<Effect> {
    if event.modifiers != KeyModifiers::NONE {
        return Vec::new();
    }
    if self.modal.is_some() {
        return self.handle_modal_mouse(event, now);
    }
    let position = Position::new(event.column, event.row);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let pipeline_row = self
                .mouse_regions
                .pipeline_rows
                .iter()
                .find(|(area, _)| area.contains(position))
                .map(|(_, index)| *index);
            if self
                .mouse_regions
                .pipeline
                .is_some_and(|area| area.contains(position))
            {
                self.focus_pane(Pane::Pipeline);
                if let Some(index) = pipeline_row
                    && self.selected_step != index
                {
                    self.selected_step = index;
                    self.mark_dirty();
                }
            } else if self
                .mouse_regions
                .input
                .is_some_and(|area| area.contains(position))
            {
                self.focus_pane(Pane::Input);
            } else if self
                .mouse_regions
                .output
                .is_some_and(|area| area.contains(position))
            {
                self.focus_pane(Pane::Output);
            }
            Vec::new()
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if self
                .mouse_regions
                .output_content
                .is_some_and(|area| area.contains(position)) =>
        {
            let direction = if event.kind == MouseEventKind::ScrollUp {
                -1
            } else {
                1
            };
            self.scroll_output(direction, 3);
            Vec::new()
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if self
                .mouse_regions
                .pipeline_content
                .is_some_and(|area| area.contains(position)) =>
        {
            let code = if event.kind == MouseEventKind::ScrollUp {
                KeyCode::Up
            } else {
                KeyCode::Down
            };
            self.handle_pipeline_key(KeyEvent::new(code, KeyModifiers::NONE), now);
            Vec::new()
        }
        _ => Vec::new(),
    }
}
```

이 구현은 Output의 기존 UTF-8·Hex·Trace 최대 경계, Pipeline과 Picker의 기존 포화 로직을 그대로 사용한다.

- [ ] **Step 9: Picker 항목과 동작 라벨 좌표 구현**

`src/tui/render.rs`에 기존 `unicode-width` 의존성을 import하고 동작 라벨 좌표 helper를 추가한다.

```rust
use unicode_width::UnicodeWidthStr as _;

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
```

`render_picker`와 `render_modal`에 `mouse_regions: &mut MouseRegions` 인자를 추가하고 Picker branch와 `render` 호출을 갱신한다. Picker 목록을 만든 뒤 각 2행 항목과 목록 휠 영역을 기록한다.

```rust
mouse_regions.picker_content = Some(list_area);
mouse_regions.picker_rows.extend(
    (start..start + items.len()).enumerate().map(|(row, index)| {
        let y = list_area.y + row as u16 * 2;
        (
            Rect::new(
                list_area.x,
                y,
                list_area.width,
                list_area.bottom().saturating_sub(y).min(2),
            ),
            index,
        )
    }),
);
```

Picker hint는 다음 문자열을 사용하고 왼쪽 정렬 좌표를 기록한다.

```rust
let hint = if compact {
    "[Enter Add] · [Esc Cancel]"
} else {
    "↑/↓ Select · [Enter Add] · Backspace Search · [Esc Cancel]"
};
frame.render_widget(Paragraph::new(hint).style(Style::default()), hint_area);
mouse_regions.add_action = Some(action_rect(hint_area, hint, "[Enter Add]", false));
mouse_regions.cancel_action = Some(action_rect(hint_area, hint, "[Esc Cancel]", false));
```

- [ ] **Step 10: Close·Confirmation 좌표와 일반 Help 구현**

`render_inspector`, `render_help`, `render_confirmation`에도 `mouse_regions: &mut MouseRegions`를 전달하고 `render_modal`의 해당 branch를 갱신한다.

Inspector와 Help의 마지막 줄을 `[Esc Close]`로 바꾸고, 최종 문자열의 마지막 행을 기록한다.

```rust
let close_line = Rect::new(
    inner.x,
    inner.y + text.lines().count().saturating_sub(1) as u16,
    inner.width,
    1,
);
mouse_regions.close_action = Some(action_rect(
    close_line,
    "[Esc Close]",
    "[Esc Close]",
    false,
));
```

Help에서는 위 코드의 `text`를 최종 `body` 문자열로 바꿔 적용한다. 일반 Help 높이를 17로 하고 `area.height < 17`일 때 기존 compact 키 요약을 사용한다. 일반 Help의 패널별 본문에는 다음 한 줄을 각각 추가한다.

```text
Mouse Click  Focus only
Mouse Click  Focus/select · Wheel  Move selection
Mouse Click  Focus only · Wheel  Scroll
```

Confirmation의 둘째 줄을 바꾸고 가운데 정렬 좌표를 기록한다.

```rust
let actions = "[Enter/y Confirm] · [n/Esc Cancel]";
frame.render_widget(
    Paragraph::new(format!("{message}\n{actions}"))
        .alignment(Alignment::Center)
        .style(Style::default()),
    inner,
);
let action_line = Rect::new(inner.x, inner.y + 1, inner.width, 1);
mouse_regions.confirm_action = Some(action_rect(
    action_line,
    actions,
    "[Enter/y Confirm]",
    true,
));
mouse_regions.cancel_action = Some(action_rect(
    action_line,
    actions,
    "[n/Esc Cancel]",
    true,
));
```

`render`는 Task 2의 지역 `mouse_regions`를 `render_modal`에도 전달한다. Modal 아래 패널 좌표가 남아 있어도 `handle_mouse`의 Modal 조기 분기가 아래 영역 사용을 차단한다.

- [ ] **Step 11: 휠·Modal 시험 통과 확인**

Run: `cargo test --locked mouse -- --nocapture`

Expected: 모든 `mouse` 이름 시험이 통과한다.

- [ ] **Step 12: 도움말 회귀 시험 통과 확인**

Run: `cargo test --locked tui::render::tests::one_context_help_modal_lists_only_real_keys_for_each_pane -- --exact`

Expected: 일반 Help에 승인된 마우스 한 줄이 포함되고 compact Help의 기존 키 정보가 유지된 상태로 통과한다.

Run: `cargo test --locked tui::render::tests::forty_by_ten_help_keeps_copy_keys_and_close_visible_for_every_pane -- --exact`

Expected: `[Esc Close]`를 포함한 40×10 Help가 통과한다.

- [ ] **Step 13: 세 번째 변경 커밋**

```bash
git add src/tui/state.rs src/tui/render.rs
git commit -m "feat(tui): 마우스 모달과 스크롤"
```

---

### Task 4: PTY 통합 검증과 사용자 문서

**Files:**
- Modify: `tests/shell-smoke.sh:175-305`
- Modify: `README.md:50-75,102-115`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:134-185,204-277,340-380,397-408`

**Interfaces:**
- Consumes: Task 1의 캡처 수명주기, Task 2의 SGR click 전달, Task 3의 Modal·wheel 처리와 기존 Expect `expect_exact` helper
- Produces: 실제 PTY에서 캡처 enable/disable·패널 focus·Picker item/Add·Pipeline wheel이 연결된 회귀 검증과 사용자 안내

- [ ] **Step 1: PTY 마우스 통합 검증 추가**

`tests/shell-smoke.sh`의 TUI spawn 직후 첫 화면 기대보다 앞에 캡처 enable Sequence를 추가한다.

```tcl
expect_exact "\033\[?1000h\033\[?1002h\033\[?1003h\033\[?1015h\033\[?1006h" 148 149
```

normal mode의 `prepare_text_preview` 직후, 기존 Tab 검증 앞에 다음 흐름을 추가한다. 좌표는 120×24 Wide 렌더의 Pipeline 첫 행, 72×18 Picker 둘째 항목과 동작 줄을 사용한다.

```tcl
send -- "\033\[<0;5;3M"
send -- "a"
expect_exact "Search:" 150 151
send -- "\033\[<0;27;8M"
send -- "\033\[<0;40;20M"
send -- " "
expect_exact "\[OFF\]   Base64 Decode" 152 153
send -- "\033\[<64;5;3M"
send -- " "
expect_exact "\[OFF\]   Hex Encode" 154 155
send -- " "
send -- "\033\[<0;50;3M"
```

마지막 Input 클릭 뒤 기존 Tab 세 번은 Input→Output→Pipeline→Input 순서를 그대로 검증한다. 종료 복구 기대에는 cursor show 다음, bracketed paste disable 전에 다음 Sequence를 추가한다.

```tcl
expect_exact "\033\[?1006l\033\[?1015l\033\[?1003l\033\[?1002l\033\[?1000l" 158 159
```

- [ ] **Step 2: PTY 시험 통과 확인**

Run: `bash tests/shell-smoke.sh`

Expected: 캡처 enable Sequence, 패널 클릭, Picker 둘째 항목·Add 클릭, Pipeline 휠과 역순 disable Sequence가 모두 관찰되고 종료 코드 0으로 끝난다.

- [ ] **Step 3: README 사용자 안내 현행화**

`README.md`의 TUI 단축키 목록 다음에 아래 문단을 추가한다.

```markdown
마우스는 `doop tui` 실행 중 항상 활성화됩니다. 패널 클릭은 포커스를 바꾸고,
Pipeline과 Add Transform 항목 클릭은 표시된 항목을 선택합니다. Output 휠은
결과를 스크롤하고 Pipeline·Add Transform 휠은 선택을 한 항목씩 이동합니다.
Modal에서는 대괄호로 표시된 Add·Confirm·Cancel·Close만 클릭할 수 있습니다.
Input caret 이동, 드래그 선택, Output 마우스 복사와 Pipeline 직접 변경은 지원하지
않습니다. 마우스 캡처 중에는 터미널의 일반 드래그 텍스트 선택이 제한될 수 있으며,
키보드 조작은 마우스를 보고하지 않는 터미널에서도 그대로 사용할 수 있습니다.
```

`최신 로컬 검증 요약` 끝에 최종 명령이 통과한 뒤 아래 문단을 추가한다.

```markdown
같은 환경의 마우스 고도화 검증에서 Crossterm 캡처 활성화·역순 해제, SGR 패널
클릭·Add Transform 항목과 Add 동작·Pipeline 휠을 Bash와 Zsh PTY에서 확인했다.
Ratatui TestBackend 시험은 Wide·Medium·Narrow·Tiny·Zoom·Modal 좌표와 Output
3단위, Pipeline·Add Transform 1단위 휠 경계를 검증했다.
```

- [ ] **Step 4: 작업판 설계 문서 현행화**

`docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md`에 다음 내용을 기존 구조에 맞춰 삽입한다.

```markdown
`tui.rs`는 Crossterm 마우스 이벤트를 `AppEvent::Mouse`로 전달한다. 렌더러는 매
프레임 실제로 표시한 패널·목록·Modal 동작의 `Rect`만 `MouseRegions`에 저장하며,
resize·zoom·Modal 전환 뒤 이전 좌표를 유지하지 않는다. Modal이 열려 있으면
아래 패널의 좌표는 입력 판정에서 제외한다.

마우스 캡처는 raw mode, alternate screen, bracketed paste 다음에 활성화하고
cursor hide 전에 완료한다. 정상 종료·오류·패닉 복구는 cursor show, mouse
capture disable, bracketed paste disable, alternate screen leave, raw mode
disable 순서다.

패널 클릭은 포커스만 바꾸고 Pipeline·Add Transform의 실제 표시 행 클릭은
선택도 바꾼다. Output 휠은 기존 스크롤 3단위, Pipeline·Add Transform 휠은
선택 1개를 이동하며 포커스는 유지한다. Modal은 표시된 Add·Confirm·Cancel·Close
동작만 클릭으로 실행한다. Input caret, 드래그 선택, Output 마우스 복사,
Pipeline 직접 편집, Hover와 Footer 클릭은 지원하지 않는다.
```

회귀 시험 목록의 터미널 복구 항목을 다음 문장으로 바꾼다.

```markdown
* raw mode, 대체 화면, bracketed paste와 mouse capture의 정상·인터럽트·패닉 복구
* SGR 마우스로 패널 포커스, Pipeline·Add Transform 선택, Output·목록 휠과 Modal 동작
```

- [ ] **Step 5: 형식과 정적 분석 실행**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: 형식 차이와 Clippy 경고 없이 두 명령이 종료 코드 0으로 끝난다.

- [ ] **Step 6: 전체 시험·문서·패키지 검증 실행**

Run:

```bash
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
cargo package --locked
```

Expected: 전체 시험 실패와 rustdoc 경고가 없고 잠금 패키지가 생성된다.

- [ ] **Step 7: Bash·Zsh PTY 검증 실행**

Run:

```bash
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
```

Expected: 두 셸 모두 마우스 enable/disable Sequence와 실제 조작을 확인하고 종료 코드 0으로 끝난다.

- [ ] **Step 8: 검색 index와 diff 위생 확인**

Run:

```bash
ccc index
git diff --check
```

Expected: `ccc index`가 변경된 Rust·Shell·Markdown 파일을 오류 없이 반영하고 diff 공백 오류가 없다.

- [ ] **Step 9: 최종 변경 커밋**

```bash
git add tests/shell-smoke.sh README.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "test(tui): 마우스 통합 검증"
```

- [ ] **Step 10: 커밋 후 깨끗한 상태 재확인**

Run:

```bash
git status --short --branch
git log -4 --oneline
```

Expected: status는 현재 branch 한 줄만 표시하고, 최신 네 커밋은 Task 4부터 Task 1까지 계획의 네 커밋 순서로 보인다.
