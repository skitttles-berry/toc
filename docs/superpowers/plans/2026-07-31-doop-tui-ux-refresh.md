# doop TUI UX Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 승인된 Neon Console 화면, 좁은 화면 세로 배치, 정돈된 Pipeline·Add Transform 표시와 Pretty/Raw 복사를 `doop tui`에 구현한다.

**Architecture:** 기존 `App` 상태와 단일 `clipboard_payload` 경로를 `CopyMode`로 확장하고, 등록된 JSON 변환을 재사용해 안전한 Pretty/Raw 결과를 만든다. 렌더링은 `src/tui/render.rs` 안에서 기존 패널 함수를 유지하되 상단·하단 Chrome과 좁은 화면 배치만 교체하며, 새 모듈이나 의존성은 추가하지 않는다.

**Tech Stack:** Rust 1.97.1, Crossterm 0.29.0, Ratatui 0.30.2, tui-textarea-2 0.12.1, 기존 Rust 단위 시험과 Ratatui `TestBackend`

## Global Constraints

- 기준 설계는 `docs/superpowers/specs/2026-07-31-doop-tui-ux-refresh-design.md`다.
- 제품 UI 문자열은 영어, 프로젝트 문서는 한국어를 사용한다.
- CLI 명령, 공개 변환 ID, Pipeline 실행 정책, 작업자와 View 종류는 변경하지 않는다.
- `NO_COLOR`에서는 색상만 제거하고 타이틀, 포커스, 테두리와 상태 문자를 유지한다.
- Pretty/Raw는 현재 `OutputState.active_artifact`를 View와 무관하게 처리하고 Trace에서는 복사하지 않는다.
- 비 JSON UTF-8은 원문, 비 UTF-8은 공백 없는 소문자 Hex를 유지한다.
- 새 parser, formatter registry, trait, 사용자 키맵, 외부 아이콘 글꼴과 Cargo 의존성을 추가하지 않는다.
- 위험 제어 문자 확인, 할당 실패, 클립보드 오류와 외부 문자열 escape 규약을 약화하지 않는다.
- 관련 README와 기존 TUI 설계 설명은 해당 코드 변경과 같은 커밋에서 현행화한다.
- 커밋은 한국어 Conventional Commits, 50자 이내, 명사형 종결을 사용한다.

## File Map

| 파일 | 책임 |
|---|---|
| `src/tui/state.rs` | Copy 모드·payload·단축키·완료 상태와 상태 단위 시험 |
| `src/tui/render.rs` | 상단·패널·하단·반응형 배치·Pipeline·Add Transform과 렌더 시험 |
| `README.md` | 사용자가 보는 TUI 배치·복사·키 설명과 최종 검증 근거 |
| `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md` | 기존 구현 설계의 화면·키·복사 규약 현행화 |
| `docs/superpowers/specs/2026-07-31-doop-tui-ux-refresh-design.md` | 구현 상태와 최종 검증 상태 |

---

### Task 1: Pretty/Raw 중앙 복사 경로와 단축키

**Files:**
- Modify: `src/tui/state.rs:62-110`
- Modify: `src/tui/state.rs:389-482`
- Modify: `src/tui/state.rs:890-1050`
- Test: `src/tui/state.rs:1300-1885`
- Test fixture update: `src/tui/render.rs:750-760, 884-924, 1971-2151`
- Modify: `README.md:56-64`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:226-265,318-324`

**Interfaces:**
- Consumes: `transform_by_id(id) -> Option<&'static TransformDefinition>`, `TransformDefinition::apply: fn(&[u8], usize) -> Result<Vec<u8>, TransformError>`, `crate::TUI_OUTPUT_LIMIT`, `Artifact::bytes() -> &[u8]`
- Produces: `CopyMode::{Pretty, Raw}`, `CopyKind::{Pretty, Raw, Hex}`, `clipboard_payload(&Artifact, CopyMode) -> Result<ClipboardPayload, ()>`, `App::request_copy(CopyMode) -> Vec<Effect>`

- [ ] **Step 1: Pretty/Raw 포맷 시험 작성**

기존 `utf8_artifact_copy_keeps_the_exact_original_text`를 다음 세 시험으로 교체하고 바이너리 시험을 두 모드 반복으로 확장한다.

```rust
#[test]
fn copy_modes_format_json_without_rewriting_tokens_or_string_spaces() {
    let pretty = clipboard_payload(
        &Artifact::new(br#"{"\u0061":1.00,"s":"x y"}"#.to_vec()),
        CopyMode::Pretty,
    )
    .unwrap();
    assert_eq!(
        pretty.text,
        "{\n  \"\\u0061\": 1.00,\n  \"s\": \"x y\"\n}"
    );
    assert_eq!(pretty.kind, CopyKind::Pretty);

    let raw = clipboard_payload(
        &Artifact::new(b" { \"a\" : 1.00, \"s\" : \"x y\" } \n".to_vec()),
        CopyMode::Raw,
    )
    .unwrap();
    assert_eq!(raw.text, r#"{"a":1.00,"s":"x y"}"#);
    assert_eq!(raw.kind, CopyKind::Raw);
}

#[test]
fn copy_modes_preserve_non_json_utf8_exactly() {
    let original = "plain text\n\twith spaces";
    for mode in [CopyMode::Pretty, CopyMode::Raw] {
        let payload =
            clipboard_payload(&Artifact::new(original.as_bytes().to_vec()), mode).unwrap();
        assert_eq!(payload.text, original);
    }
}

#[test]
fn pretty_copy_rejects_json_output_beyond_the_copy_limit() {
    assert!(format_text_for_copy("[1]", CopyMode::Pretty, 4).is_err());
}

#[test]
fn both_copy_modes_encode_binary_as_lowercase_hex() {
    for mode in [CopyMode::Pretty, CopyMode::Raw] {
        let payload =
            clipboard_payload(&Artifact::new(vec![0x00, 0xab, 0xff]), mode).unwrap();
        assert_eq!(payload.text, "00abff");
        assert_eq!(payload.kind, CopyKind::Hex);
    }
}
```

- [ ] **Step 2: 포맷 시험 실패 확인**

Run:

```bash
cargo test --lib copy_modes
cargo test --lib pretty_copy_rejects_json_output_beyond_the_copy_limit
cargo test --lib both_copy_modes_encode_binary_as_lowercase_hex
```

Expected: `CopyMode`, 새 `CopyKind` variant와 두 인자를 받는 `clipboard_payload`가 없어 컴파일 실패.

- [ ] **Step 3: 중앙 포맷 경로 최소 구현**

`src/tui/state.rs`의 crate import에 `TUI_OUTPUT_LIMIT`과 `TransformError`를 추가하고 기존 `CopyKind`, `clipboard_payload`를 다음 구조로 교체한다.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyMode {
    Pretty,
    Raw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyKind {
    Pretty,
    Raw,
    Hex,
}

fn copy_exact_text(raw: &str) -> Result<String, ()> {
    let mut text = String::new();
    text.try_reserve_exact(raw.len()).map_err(|_| ())?;
    text.push_str(raw);
    Ok(text)
}

fn format_text_for_copy(
    raw: &str,
    mode: CopyMode,
    output_limit: usize,
) -> Result<String, ()> {
    let transform_id = match mode {
        CopyMode::Pretty => "format-json",
        CopyMode::Raw => "minify-json",
    };
    let transform = transform_by_id(transform_id).expect("registered JSON copy transform");
    match (transform.apply)(raw.as_bytes(), output_limit) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| ()),
        Err(TransformError::InvalidJson { .. }) => copy_exact_text(raw),
        Err(_) => Err(()),
    }
}

fn clipboard_payload(
    artifact: &Artifact,
    mode: CopyMode,
) -> Result<ClipboardPayload, ()> {
    match std::str::from_utf8(artifact.bytes()) {
        Ok(raw) => Ok(ClipboardPayload {
            text: format_text_for_copy(raw, mode, TUI_OUTPUT_LIMIT)?,
            kind: match mode {
                CopyMode::Pretty => CopyKind::Pretty,
                CopyMode::Raw => CopyKind::Raw,
            },
        }),
        Err(_) => Ok(ClipboardPayload {
            text: binary_hex(artifact.bytes())?,
            kind: CopyKind::Hex,
        }),
    }
}
```

`InvalidJson`만 원문 fallback으로 취급한다. `OutputTooLarge`를 포함한 나머지 오류는 `Err(())`로 전달한다.

- [ ] **Step 4: 포맷 시험 통과 확인**

Run:

```bash
cargo test --lib copy_modes
cargo test --lib pretty_copy_rejects_json_output_beyond_the_copy_limit
cargo test --lib both_copy_modes_encode_binary_as_lowercase_hex
```

Expected: 모두 PASS.

- [ ] **Step 5: 전역 F3/F4와 Output Enter 시험 작성**

기존 `preview_enter_requests_copy_without_editing_input`을 유지하되 Pretty kind를 검사하도록 바꾸고 다음 시험을 추가한다.

```rust
#[test]
fn global_copy_keys_use_the_active_output_from_every_pane() {
    let start = now();
    for pane in [Pane::Input, Pane::Pipeline, Pane::Output] {
        for (code, expected_kind, expected_text) in [
            (
                KeyCode::F(3),
                CopyKind::Pretty,
                "{\n  \"a\": 1\n}",
            ),
            (KeyCode::F(4), CopyKind::Raw, "{\"a\":1}"),
        ] {
            let mut app = App::new(start, true);
            app.focus = pane;
            app.output.status = OutputStatus::Ready;
            app.output.active_artifact =
                Some(Artifact::new(b"{ \"a\" : 1 }".to_vec()));

            assert!(matches!(
                key(&mut app, code, KeyModifiers::NONE, start).as_slice(),
                [Effect::Copy(ClipboardPayload { text, kind })]
                    if text == expected_text && *kind == expected_kind
            ));
        }
    }
}

#[test]
fn modal_keys_take_priority_over_global_copy_keys() {
    let start = now();
    let mut app = App::new(start, true);
    app.output.status = OutputStatus::Ready;
    app.output.active_artifact = Some(Artifact::new(b"{\"a\":1}".to_vec()));
    app.open_picker();

    assert!(key(&mut app, KeyCode::F(3), KeyModifiers::NONE, start).is_empty());
    assert!(key(&mut app, KeyCode::F(4), KeyModifiers::NONE, start).is_empty());
    assert!(matches!(app.modal, Some(Modal::TransformPicker { .. })));
}

#[test]
fn global_copy_uses_the_current_step_artifact_not_the_cached_final() {
    let start = now();
    let mut app = App::new(start, true);
    app.focus = Pane::Pipeline;
    app.output.source = OutputSource::Step(0);
    app.output.status = OutputStatus::Ready;
    app.output.final_artifact = Some(Artifact::new(b"{\"final\":true}".to_vec()));
    app.output.active_artifact = Some(Artifact::new(b"{ \"step\" : 1 }".to_vec()));

    assert!(matches!(
        key(&mut app, KeyCode::F(4), KeyModifiers::NONE, start).as_slice(),
        [Effect::Copy(ClipboardPayload { text, kind })]
            if text == "{\"step\":1}" && *kind == CopyKind::Raw
    ));
}

#[test]
fn trace_view_blocks_global_and_output_copy_keys() {
    let start = now();
    let mut app = App::new(start, true);
    app.focus = Pane::Output;
    app.output.status = OutputStatus::Ready;
    app.output.view = ViewMode::Trace;
    app.output.active_artifact = Some(Artifact::new(b"hidden".to_vec()));

    for code in [KeyCode::F(3), KeyCode::F(4), KeyCode::Enter] {
        assert!(key(&mut app, code, KeyModifiers::NONE, start).is_empty());
    }
}
```

- [ ] **Step 6: 단축키 시험 실패 확인**

Run:

```bash
cargo test --lib global_copy_keys_use_the_active_output_from_every_pane
cargo test --lib modal_keys_take_priority_over_global_copy_keys
cargo test --lib global_copy_uses_the_current_step_artifact_not_the_cached_final
cargo test --lib trace_view_blocks_global_and_output_copy_keys
```

Expected: F3/F4 효과가 없고 기존 Enter payload kind가 맞지 않아 FAIL.

- [ ] **Step 7: 단축키·완료 상태·기존 fixture 구현**

`request_copy`에 mode를 전달하고 UTF-8 위험 문자 판정은 Hex만 제외한다.

```rust
fn request_copy(&mut self, mode: CopyMode) -> Vec<Effect> {
    if !self.can_copy() {
        return Vec::new();
    }
    let Some(artifact) = self.output.active_artifact.as_ref() else {
        return Vec::new();
    };
    let Ok(payload) = clipboard_payload(artifact, mode) else {
        self.set_status(Some("Copy unavailable".to_string()));
        return Vec::new();
    };
    if payload.kind != CopyKind::Hex
        && crate::error::contains_dangerous_control(&payload.text)
    {
        self.modal = Some(Modal::UnsafeCopyConfirm { payload });
        self.mark_dirty();
        Vec::new()
    } else {
        vec![Effect::Copy(payload)]
    }
}
```

`handle_key`의 Modal 처리 다음, Esc 처리 전에 F3/F4를 추가한다.

```rust
match (key.code, key.modifiers) {
    (KeyCode::F(3), KeyModifiers::NONE) => {
        return self.request_copy(CopyMode::Pretty);
    }
    (KeyCode::F(4), KeyModifiers::NONE) => {
        return self.request_copy(CopyMode::Raw);
    }
    _ => {}
}
```

Output의 기존 `Enter`와 `y`는 둘 다 Pretty로 유지한다.

```rust
(KeyCode::Enter | KeyCode::Char('y'), KeyModifiers::NONE) => {
    self.request_copy(CopyMode::Pretty)
}
```

클립보드 성공 상태를 정확히 구분한다.

```rust
Ok(()) if kind == CopyKind::Hex => "Copied as Hex".to_string(),
Ok(()) if kind == CopyKind::Raw => "Copied Raw".to_string(),
Ok(()) => "Copied Pretty".to_string(),
```

`src/tui/state.rs`와 `src/tui/render.rs` 시험 fixture의 기존
`CopyKind::Text`는 문맥에 따라 `CopyKind::Pretty`로 교체한다.
Raw 성공 메시지 case를 `clipboard_success_message_preserves_the_copy_kind`
시험에 추가한다. 기존 `global_shortcuts_reject_extra_modifiers`의 입력
목록에는 다음 F3/F4 변형을 추가해 정확히 modifier가 없는 경우만
복사하는지 유지한다.

```rust
(KeyCode::F(3), KeyModifiers::SHIFT),
(KeyCode::F(4), KeyModifiers::CONTROL),
```

해당 시험 loop의 `key` 호출 전에는 복사 가능 상태를 만들어 modifier
검사가 실제 효과 생성을 구분하게 한다.

```rust
app.output.status = OutputStatus::Ready;
app.output.active_artifact = Some(Artifact::new(b"{\"a\":1}".to_vec()));
```

- [ ] **Step 8: 상태·복사 시험 전체 통과 확인**

Run:

```bash
cargo test --lib tui::state::tests
cargo test --lib tui::render::tests
```

Expected: 모두 PASS.

- [ ] **Step 9: 복사 문서 동기화**

`README.md`의 TUI 복사 문단과 키 목록을 다음 의미로 교체한다.

```markdown
유효한 JSON 결과는 Pretty Copy에서 두 칸 들여쓰고 Raw Copy에서 구조
공백을 제거합니다. 그 밖의 UTF-8은 원문, 비 UTF-8은 공백 없는 소문자
Hex로 복사합니다. 복사는 표시 View가 아니라 현재 Output의 FINAL 또는
STEP 원본을 사용하며 Trace에서는 비활성입니다.

- 전역: `Tab`/`Shift+Tab` 패널 이동, `F3` Pretty Copy, `F4` Raw Copy,
  `Ctrl+P` 변환 추가, `F1` 도움말
- Output: `v`/`V` 보기, `p` 단계, `f` 최종, `Enter`/`y` Pretty Copy,
  `z` 확대
```

기존 TUI 작업판 설계의 전역 키 표에 F3/F4를 추가하고 Output Enter/y를
Pretty로 명시한다. `# 6.5 결과 원본과 복사`는 JSON Pretty/Raw,
비 JSON 원문, 바이너리 Hex와 `Copied Pretty`·`Copied Raw`·
`Copied as Hex` 상태를 반영한다.

- [ ] **Step 10: Task 1 검증과 커밋**

Run:

```bash
cargo fmt --check
cargo test --lib tui::state::tests
cargo test --lib tui::render::tests
git diff --check
```

Expected: 모두 PASS, whitespace 오류 없음.

```bash
git add src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "feat(tui): Pretty·Raw 복사 추가"
```

---

### Task 2: Neon Console Chrome과 반응형 세로 배치

**Files:**
- Modify: `src/tui/render.rs:1-99`
- Modify: `src/tui/render.rs:101-190`
- Modify: `src/tui/render.rs:276-384`
- Modify: `src/tui/render.rs:671-737`
- Test: `src/tui/render.rs:781-1072`
- Test: `src/tui/render.rs:1935-2055`
- Modify: `README.md:52-64`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:200-265`

**Interfaces:**
- Consumes: `App.focus`, `App.status`, `OutputStatus`, 기존 `render_input`, `render_output`, `render_pipeline`, `source_label`
- Produces: `stacked_pane_heights(u16) -> [u16; 3]`, `render_footer(&mut Frame, &App, Rect, Rect)`, 박스 없는 App Bar와 고정 두 줄 Footer

- [ ] **Step 1: 세로 배치·상단·하단 실패 시험 작성**

기존 좁은 너비와 높이 경계 시험을 다음 시험으로 교체한다.

```rust
#[test]
fn stacked_pane_heights_keep_minimums_and_give_remainder_to_output() {
    assert_eq!(stacked_pane_heights(9), [3, 3, 3]);
    assert_eq!(stacked_pane_heights(13), [4, 4, 5]);
    assert_eq!(stacked_pane_heights(25), [7, 7, 11]);
}

#[test]
fn narrow_layout_stacks_pipeline_input_and_output_in_order() {
    for width in [40, 89] {
        let lines = rendered_lines(width, 16, Pane::Output);
        let pipeline = lines.iter().position(|line| line.contains("$ PIPELINE")).unwrap();
        let input = lines.iter().position(|line| line.contains("> INPUT")).unwrap();
        let output = lines.iter().position(|line| line.contains("» OUTPUT")).unwrap();

        assert!(pipeline < input && input < output);
        assert!(lines[0].contains(">_ DOOP"));
        assert!(lines[0].contains("FOCUS: OUTPUT"));
        assert!(lines[14].contains("[OUTPUT]"));
        assert!(lines[15].contains("[COMMON]"));
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
```

- [ ] **Step 2: 레이아웃 시험 실패 확인**

Run:

```bash
cargo test --lib stacked_pane_heights_keep_minimums_and_give_remainder_to_output
cargo test --lib narrow_layout_stacks_pipeline_input_and_output_in_order
cargo test --lib ten_and_eleven_rows_show_only_the_focused_pane
```

Expected: helper와 새 타이틀이 없고 좁은 화면이 한 패널만 보여 FAIL.

- [ ] **Step 3: 최소 세로 배치 구현**

`ChromeVisibility`, `render_navigation`, `render_step_summary`,
`render_context`를 삭제하고 콘텐츠 높이 9행 이상의 좁은 영역을 다음
helper로 나눈다.

```rust
fn stacked_pane_heights(height: u16) -> [u16; 3] {
    let remaining = height.saturating_sub(9);
    let pipeline = 3 + remaining.saturating_mul(3) / 10;
    let input = 3 + remaining.saturating_mul(3) / 10;
    let output = height.saturating_sub(pipeline).saturating_sub(input);
    [pipeline, input, output]
}
```

`render`는 Tiny가 아닐 때 상단 1행, 콘텐츠, 하단 2행을 먼저 고정한다.

```rust
let [app_bar, content, focused_help, common_help] = Layout::vertical([
    Constraint::Length(1),
    Constraint::Min(0),
    Constraint::Length(1),
    Constraint::Length(1),
])
.areas(area);
```

포커스 단일 패널 경로는 다음 helper로 기존 세 renderer를 호출한다.

```rust
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
```

렌더 순서는 다음으로 고정한다.

```rust
let focused = app.zoom.unwrap_or(app.focus);
if area.height < 12 || app.zoom.is_some() {
    render_focused_pane(frame, app, content, focused, mode);
} else if mode == WidthMode::Narrow {
    let [pipeline_rows, input_rows, output_rows] =
        stacked_pane_heights(content.height);
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
    let [pipeline, right] = Layout::horizontal([
        Constraint::Length(pipeline_columns),
        Constraint::Min(0),
    ])
    .areas(content);
    let input_rows = (u32::from(right.height) * 42 / 100) as u16;
    let input_rows = input_rows.clamp(3, right.height.saturating_sub(3));
    let [input, output] = Layout::vertical([
        Constraint::Length(input_rows),
        Constraint::Min(3),
    ])
    .areas(right);
    render_pipeline(frame, app, pipeline, mode == WidthMode::Wide);
    render_input(frame, app, input);
    render_output(frame, app, output);
}
```

좌우 분할의 Input·Output 최소 테두리 높이는 5에서 3으로 낮춰 12행
화면에서도 두 패널이 안전하게 보이도록 한다.

- [ ] **Step 4: 세로 배치 시험 통과 확인**

Run:

```bash
cargo test --lib stacked_pane_heights_keep_minimums_and_give_remainder_to_output
cargo test --lib narrow_layout_stacks_pipeline_input_and_output_in_order
cargo test --lib ten_and_eleven_rows_show_only_the_focused_pane
```

Expected: 모두 PASS.

- [ ] **Step 5: Neon Console·Footer 상태 시험 작성**

```rust
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
```

- [ ] **Step 6: 상단·Footer 시험 실패 확인**

Run:

```bash
cargo test --lib app_bar_is_unboxed_and_footer_has_exactly_two_roles
cargo test --lib status_replaces_only_the_focused_help_line
```

Expected: 기존 App Bar가 tab 문자열을 사용하고 Footer가 한 줄이라 FAIL.

- [ ] **Step 7: Neon Console 스타일과 두 줄 Footer 구현**

Ratatui import에 `BorderType`을 추가한다. 패널은 기본 색상만 사용한다.

```rust
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

fn pane_block<'a>(app: &App, title: &'a str, focused: bool) -> Block<'a> {
    let style = pane_style(app, focused);
    Block::bordered()
        .border_type(BorderType::Thick)
        .title(title)
        .border_style(style)
        .title_style(style)
}
```

패널 제목을 다음 문자열로 바꾼다.

```rust
let input_block = pane_block(app, "> INPUT", app.focus == Pane::Input);
let pipeline_block = pane_block(app, "$ PIPELINE", app.focus == Pane::Pipeline);
let output_title = format!("» OUTPUT / {source} / {}", view.to_ascii_uppercase());
```

App Bar는 Span별 색상을 사용하되 `NO_COLOR`이면 모두 기본 Style이다.

```rust
let title_style = (!app.no_color)
    .then(|| Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    .unwrap_or_default();
let focus_style = (!app.no_color)
    .then(|| Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    .unwrap_or_default();
let line = Line::from(vec![
    Span::styled(">_ DOOP", title_style),
    Span::raw("  │  FOCUS: "),
    Span::styled(pane_label(app.focus).to_ascii_uppercase(), focus_style),
]);
```

하단 문자열을 한 곳에서 만든다.

```rust
fn focused_help(app: &App) -> &'static str {
    match app.focus {
        Pane::Input => "[INPUT] Text editing · Esc Cancel",
        Pane::Pipeline => {
            "[PIPELINE] ↑/↓ Select · Shift+↑/↓ Move · Space Toggle · Enter Inspect"
        }
        Pane::Output if app.can_copy() => {
            "[OUTPUT] Enter Pretty · v/V View · p Step · f Final · z Zoom"
        }
        Pane::Output => "[OUTPUT] v/V View · p Step · f Final · z Zoom",
    }
}

fn common_help() -> &'static str {
    "[COMMON] Tab Focus · F3 Pretty · F4 Raw · Ctrl+P Add · F1 Help · Ctrl+Q Quit"
}
```

첫째 줄의 우선순위는 Pipeline 실패, 취소, `App.status`, 포커스
도움말이다. 둘째 줄은 항상 `common_help()`다.

```rust
fn footer_first_line(app: &App, width: usize) -> String {
    match &app.output.status {
        OutputStatus::Failed(error) => crate::error::escape_external(
            &render_pipeline_error_summary(error),
            width,
        ),
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
```

Ratatui의 영역 폭 clipping을 이용해 오른쪽 낮은 우선순위 항목부터
숨긴다.

`render_help`의 세 패널 Context Help에도 F3/F4를 추가하고 기존
`Ctrl+C Force quit`를 유지한다.

- [ ] **Step 8: 렌더 회귀 시험 정합화**

기존 시험을 새 규칙에 맞게 바꾼다.

```rust
// NO_COLOR에서도 상태 문자를 유지한다.
assert!(screen.contains("[ON]  ›"));
assert!(screen.contains(">_ DOOP"));
assert!(screen.contains("[COMMON]"));
assert!(buffer.content().iter().all(|cell| {
    cell.fg == Color::Reset && cell.bg == Color::Reset
}));
```

기존 `Navigation`, `Step Summary`, `[Output]`, `FINAL |` 한 줄 Context를
기대하는 assertion은 새 App Bar와 두 줄 Footer assertion으로 교체한다.
색상 시험은 Indexed/RGB 금지를 그대로 유지한다.

- [ ] **Step 9: 레이아웃·스타일 문서 동기화**

`README.md`의 40~89열 설명을 Pipeline 30%, Input 30%, Output 40%의
세로 배치로 바꾸고, 높이 10~11행은 포커스 패널 하나, 그 미만은 크기
안내라고 명시한다. 도움말이 최하단 두 줄이며 상태가 첫째 줄만
대체한다는 문장을 추가한다.

기존 TUI 작업판 설계 `# 5. 화면과 조작`에서 App Bar,
Navigation Bar, Step Summary, Context Bar와 좁은 단일 패널 설명을
승인된 상단 한 줄, Neon Console 패널과 Footer 두 줄 규칙으로 교체한다.
`NO_COLOR`는 색상만 제거하고 기호를 유지하도록 현행화한다.

- [ ] **Step 10: Task 2 검증과 커밋**

Run:

```bash
cargo fmt --check
cargo test --lib tui::render::tests
git diff --check
```

Expected: 모두 PASS, whitespace 오류 없음.

```bash
git add src/tui/render.rs README.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "feat(tui): 반응형 네온 화면 적용"
```

---

### Task 3: Pipeline 행과 Add Transform 정보 구조

**Files:**
- Modify: `src/tui/render.rs:190-274`
- Modify: `src/tui/render.rs:405-488`
- Modify: `src/tui/render.rs:1124-1149`
- Modify: `src/tui/render.rs:1202-1329`
- Modify: `src/tui/render.rs:1632-1952`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:220-224,267-271`

**Interfaces:**
- Consumes: `StepStatus`, `TransformDefinition::{id, display_name, description, behavior, accepts_binary}`, `selection_style`, `input_condition`
- Produces: 한 번의 `[ON]`/`[OFF]`와 한 상태 기호를 가진 Pipeline 행, 두 줄 `ListItem`, 상세·키 도움말 구분선

- [ ] **Step 1: Pipeline 중복 제거 실패 시험 작성**

기존 `pipeline_rows_keep_selection_enablement_and_every_trace_state_textual`을
다음 기대 형식으로 교체한다. 모든 행이 보이도록 Pipeline Zoom을 사용한다.

```rust
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
    for duplicate in [" OK ", " ERROR ", " RUNNING ", " NOT RUN ", " CANCELLED ", "○"] {
        assert!(!screen.contains(duplicate), "unexpected {duplicate}: {screen}");
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
```

- [ ] **Step 2: Pipeline 행 시험 실패 확인**

Run:

```bash
cargo test --lib pipeline_rows_show_enablement_once_and_one_runtime_mark
cargo test --lib running_pipeline_row_uses_the_same_compact_shape_without_color
```

Expected: 기존 `OK`, `OFF`, `ERROR`, `RUNNING` 상태 단어와 비활성 원형이
남아 FAIL.

- [ ] **Step 3: Pipeline 행 최소 구현**

기존 `label`을 제거하고 status별 mark와 색상만 남긴다.

```rust
let (mark, color) = match status {
    StepStatus::Succeeded => ("✓ ", Color::Green),
    StepStatus::Disabled => (" ", Color::DarkGray),
    StepStatus::Failed => ("× ", Color::Red),
    StepStatus::NotExecuted => ("· ", Color::DarkGray),
    StepStatus::Cancelled => ("− ", Color::Yellow),
};
let text = format!(
    "{prefix} [{enabled}]  {mark}{}{sizes}",
    step.definition.display_name
);
```

실행 중의 기존 조기 반환도 같은 spacing을 사용한다.

```rust
let text = format!(
    "{prefix} [{enabled}]  › {}",
    step.definition.display_name
);
```

`NO_COLOR`에서도 mark를 지우지 않고 Style만 기본값으로 바꾼다.

- [ ] **Step 4: Pipeline 행 시험 통과 확인**

Run:

```bash
cargo test --lib pipeline_rows_show_enablement_once_and_one_runtime_mark
cargo test --lib running_pipeline_row_uses_the_same_compact_shape_without_color
```

Expected: 모두 PASS.

- [ ] **Step 5: Add Transform 두 줄·구분선 실패 시험 작성**

기존 Palette 시험을 다음 정보 구조로 바꾼다.

```rust
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
```

- [ ] **Step 6: Add Transform 시험 실패 확인**

Run:

```bash
cargo test --lib add_transform_separates_item_description_detail_and_key_help
cargo test --lib compact_add_transform_keeps_selected_description_and_close_keys
```

Expected: 기존 한 줄 목록과 `CLI description`·`CLI behavior` 상세 때문에
FAIL.

- [ ] **Step 7: Add Transform 두 줄 목록과 적응형 상세 구현**

`ratatui::widgets` import에 `Wrap`을 추가하고 Modal block에
`BorderType::Thick`을 적용한다. 내부 높이에 따라 상세 영역만 0 또는
4행으로 만든다.

```rust
let compact = inner.height < 14;
let detail_rows = if compact { 0 } else { 4 };
let detail_separator_rows = if compact { 0 } else { 1 };
let [query_area, list_area, detail_separator, detail_area, hint_separator, hint_area] =
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(2),
        Constraint::Length(detail_separator_rows),
        Constraint::Length(detail_rows),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
```

두 줄이 한 항목이므로 보이는 항목 수와 scroll 시작점을 행 수의 절반으로
계산한다.

```rust
let available = (list_area.height as usize / 2).max(1);
let start = if selected >= available {
    selected + 1 - available
} else {
    0
};
```

각 `ListItem`은 두 줄 전체에 같은 선택 Style을 적용한다.

```rust
let prefix = if is_selected { "> " } else { "  " };
let text = format!(
    "{prefix}{}  [{}]\n  {}",
    transform.display_name,
    transform.id,
    transform.description,
);
ListItem::new(text).style(if is_selected {
    selection_style(app)
} else {
    Style::default()
})
```

정상 크기의 상세 문자열은 중복 description을 제외한다.

```rust
let detail = format!(
    "INPUT     {}\nBEHAVIOR  {}\nTUI       Result remains bytes; Smart selects Text or Hex",
    input_condition(transform.accepts_binary),
    transform.behavior,
);
frame.render_widget(
    Paragraph::new(detail).wrap(Wrap { trim: false }),
    detail_area,
);
```

두 구분선은 얇은 선 문자로 렌더링한다.

```rust
fn separator(width: u16) -> String {
    "─".repeat(width as usize)
}
```

정상 키 도움말은
`↑/↓ Select · Enter Add · Backspace Search · Esc Cancel`, compact 도움말은
`Enter Add · Esc Cancel`로 고정한다.

- [ ] **Step 8: Pipeline·Picker 렌더 시험 전체 통과 확인**

Run:

```bash
cargo test --lib tui::render::tests
```

Expected: 모든 렌더 시험 PASS.

- [ ] **Step 9: Pipeline·Picker 문서 동기화**

기존 TUI 작업판 설계의 스타일 설명을 `[ON]`·`[OFF]`와
`✓`·`×`·`›`·`·`·`−` 단일 기호 규칙으로 바꾼다. Operation Palette
설명은 이름·ID 첫째 줄, description 둘째 줄, 선택 상세와 키 도움말의
두 구분선, compact 상세 생략을 명시한다.

- [ ] **Step 10: Task 3 검증과 커밋**

Run:

```bash
cargo fmt --check
cargo test --lib tui::render::tests
git diff --check
```

Expected: 모두 PASS, whitespace 오류 없음.

```bash
git add src/tui/render.rs docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "feat(tui): Pipeline·변환 선택 정돈"
```

---

### Task 4: 전체 검증과 구현 상태 현행화

**Files:**
- Modify: `README.md:70-116`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-ux-refresh-design.md:1-8,330-370`
- Verify only: `Cargo.toml`, `Cargo.lock`, `tests/cli.rs`, `tests/shell-smoke.sh`

**Interfaces:**
- Consumes: Task 1~3의 커밋과 전체 Cargo·Shell 시험
- Produces: 검증된 구현 상태, 현재 시험 수·측정값·플랫폼 근거와 최신 ccc index

- [ ] **Step 1: 형식·정적 분석·전체 시험**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
```

Expected: 모든 명령 exit 0, 시험 실패 0, Clippy와 rustdoc 경고 0.

- [ ] **Step 2: 렌더 성능 회귀 측정**

Run:

```bash
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
```

Expected: 측정 시험 PASS, 변경 시 렌더 중앙값 16 ms 이하, 표본마다 실제
redraw 1회. 시간은 정확성 시험의 pass/fail 기준이 아니라 기존 유지 판단
근거로 기록한다.

- [ ] **Step 3: 패키지·오프라인 설치 검증**

Run:

```bash
cargo package --locked
```

Expected: package 생성 성공, `.env`, `target/`, `.superpowers/` 미포함.

Run:

```bash
install_root=$(mktemp -d "${TMPDIR:-/tmp}/doop-install.XXXXXX")
trap 'rm -rf -- "$install_root"' EXIT
cargo install --locked --offline --path . --root "$install_root"
"$install_root/bin/doop" --version
```

Expected: 설치 성공, 출력은 `doop 0.2.0`.

- [ ] **Step 4: Shell·실제 macOS 클립보드 Smoke**

Run:

```bash
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
test "$(pbpaste)" = ff
DOOP_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
test "$(pbpaste)" = ff
```

Expected: 모든 명령 exit 0, 마지막 클립보드 값은 소문자 `ff`.

- [ ] **Step 5: README 검증 근거와 설계 상태 갱신**

`cargo test --all-targets --all-features --locked`가 출력한 실제 통과·무시
개수로 README `최신 로컬 검증 요약`의 시험 수를 교체한다. Task 4의
렌더 측정 출력에서 warmup 수, 표본 수, 반복 수, 최솟값·중앙값·최댓값과
redraw 횟수를 그대로 옮긴다. 실행하지 않은 Linux 결과는 기존처럼
미검증으로 유지한다.

새 설계 문서의 상태를 다음으로 바꾼다.

```markdown
* **상태:** 사용자 승인·구현 완료
```

완료 기준 아래에 Task 1~4에서 실행한 명령과 결과가 모두 충족되었음을
한국어 문장으로 기록한다. 민감한 입력, 클립보드 내용과 로컬 절대 경로는
문서에 기록하지 않는다.

- [ ] **Step 6: ccc index와 최종 diff 검증**

Run:

```bash
ccc index
git diff --check
git status --short
```

Expected: ccc error 0, diff whitespace 오류 없음, README와 새 설계 문서만
수정 상태.

- [ ] **Step 7: 검증 문서 커밋**

```bash
git add README.md docs/superpowers/specs/2026-07-31-doop-tui-ux-refresh-design.md
git commit -m "docs(tui): 개선 구현 검증 현행화"
```

- [ ] **Step 8: 커밋 후 최종 확인**

Run:

```bash
git status --short --branch
git --no-pager log -4 --oneline
```

Expected: 작업 트리 clean, Task 1~4의 네 커밋이 순서대로 표시됨.
