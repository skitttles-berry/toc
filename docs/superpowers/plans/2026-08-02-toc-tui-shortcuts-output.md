# toc TUI 단축키·출력 보기 개선 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 영문·한글 단축키, Output 전용 Enter 복사, 간결한 화면 정보와 반응형 Hex·Trace 표를 기존 TUI에 추가한다.

**Architecture:** 키 입력은 기존 `App` 상태 전이와 `Effect` 경로에 직접 연결하고, Shift+Enter 구분을 위해 Crossterm의 점진적 키보드 향상 플래그를 TUI 세션 수명에 맞춰 Push·Pop한다. `views.rs`는 화면에 필요한 안전한 Hex 행과 오류 요약만 만들고, `render.rs`가 Ratatui `Table`, `Row`, `Cell`로 색상과 반응형 열을 구성한다.

**Tech Stack:** Rust 1.97.1, Crossterm 0.29.0, Ratatui 0.30.2, tui-textarea-2 0.12.1, 기존 Rust 단위 시험·Ratatui TestBackend·Expect 셸 Smoke

## Global Constraints

- 새 Crate와 설정 파일을 추가하지 않는다.
- CLI Pipeline 실행 엔진과 여덟 개 공개 변환 ID는 변경하지 않는다.
- Input 1 MiB·65,536줄, TUI 단계 출력 64 MiB, Pipeline 32단계 제한을 유지한다.
- 화면 행 생성과 렌더링 문자열은 렌더당 4 KiB 예산을 유지한다.
- Input 포커스의 일반 문자는 `tui-textarea-2`에 전달하고 전역 단축키로 가로채지 않는다.
- 한글 별칭은 두벌식 자판의 같은 위치만 지원하며 도움말에는 영문 소문자만 표시한다.
- `Ctrl+c`는 운영체제 수준 강제 인터럽트로 유지하고 한글 별칭을 추가하지 않는다.
- Trace·실패·취소·실행·지연·Artifact 부재 상태의 복사 차단과 기존 위험 문자 확인을 유지한다.
- `NO_COLOR`에서는 색상만 제거하고 열, 상태 문자열과 기호를 유지한다.
- Shift+Enter는 점진적 키보드 향상을 이해하는 터미널에서 Crossterm `SHIFT` 수정자로 구분한다. 지원하지 않는 터미널의 한계는 README에 명시하되 TUI 시작을 차단하거나 2초 지원 조회를 추가하지 않는다.
- 기존 미추적 `.codex/`와 `.superpowers/brainstorm/` 산출물은 커밋하지 않는다.
- 구현 변경과 관련 문서는 같은 논리적 변경 커밋에서 동기화하고, 커밋은 한국어 Conventional Commits 명사형으로 작성한다.

## File Structure

- Modify: `src/tui.rs` — 터미널 키보드 향상 Push·Pop과 오류·패닉 복구
- Modify: `src/tui/state.rs` — 영문·한글 키 매칭, 복사 모드, 삭제 상태, 반응형 Hex 스크롤 범위
- Modify: `src/tui/views.rs` — 가시 Hex 행, 행당 바이트 계산, 안전한 Trace 상태·오류 요약
- Modify: `src/tui/render.rs` — App Bar·제목·Dock·Help와 Hex·Trace Ratatui 표
- Modify: `tests/shell-smoke.sh` — 새 터미널 제어 시퀀스, 제거된 FOCUS, Enter 복사 계약
- Modify: `README.md` — 현재 단축키, Output 보기, Shift+Enter 터미널 조건과 검증 결과
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md` — 이전 키·제목 계약 현행화
- Modify: `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md` — App Bar·Command Dock 계약 현행화
- Reference: `docs/superpowers/specs/2026-08-01-toc-tui-shortcuts-output-design.md` — 승인된 요구사항 원본

---

### Task 1: Shift+Enter 터미널 이벤트 활성화

**Files:**
- Modify: `src/tui.rs:8-16,43-114,199-225,251-340`
- Modify: `README.md:51-91` — Shift+Enter 터미널 조건

**Interfaces:**
- Consumes: 기존 `execute_tracked<W, C>(writer, active, command) -> io::Result<()>`와 `TerminalSession` 복구 순서
- Produces: `push_keyboard_enhancement<W: io::Write>(writer, active) -> io::Result<()>`, `pop_keyboard_enhancement<W: io::Write>(writer, active)`, 세션 수명 동안 활성화된 `DISAMBIGUATE_ESCAPE_CODES`

- [ ] **Step 1: Push·Pop 경계를 검증하는 실패 시험 작성**

```rust
#[test]
fn keyboard_enhancement_is_pushed_once_and_popped_only_when_active() {
    let mut writer = Vec::new();
    let mut active = false;

    push_keyboard_enhancement(&mut writer, &mut active).unwrap();
    assert!(active);
    assert_eq!(writer, b"\x1b[>1u");

    pop_keyboard_enhancement(&mut writer, &mut active);
    assert!(!active);
    assert_eq!(writer, b"\x1b[>1u\x1b[<1u");

    pop_keyboard_enhancement(&mut writer, &mut active);
    assert_eq!(writer, b"\x1b[>1u\x1b[<1u");
}
```

- [ ] **Step 2: 새 시험이 RED인지 확인**

Run: `rtk cargo test --locked tui::tests::keyboard_enhancement_is_pushed_once_and_popped_only_when_active -- --exact`  
Expected: 컴파일 실패로 `push_keyboard_enhancement`와 `pop_keyboard_enhancement`를 찾지 못하며, 시험 필터는 구현 뒤 `running 1 test`가 되어야 한다.

- [ ] **Step 3: 최소 Push·Pop 함수 구현**

```rust
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};

fn push_keyboard_enhancement<W: io::Write>(
    writer: &mut W,
    active: &mut bool,
) -> io::Result<()> {
    execute_tracked(
        writer,
        active,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
    )
}

fn pop_keyboard_enhancement<W: io::Write>(writer: &mut W, active: &mut bool) {
    if *active {
        let _ = execute!(writer, PopKeyboardEnhancementFlags);
        *active = false;
    }
}
```

- [ ] **Step 4: TerminalSession과 패닉 복구에 연결**

`TerminalSession`에 `keyboard_enhanced: bool`을 추가한다. 진입 순서는 Raw → Alternate → Keyboard Enhancement → Bracketed Paste → Mouse → Cursor Hide로 하고, `restore`는 Cursor Show → Mouse Disable → Paste Disable → Keyboard Pop → Alternate Leave → Raw Disable의 역순을 사용한다.

```rust
execute_tracked(&mut stdout, &mut session.alternate, EnterAlternateScreen)?;
push_keyboard_enhancement(&mut stdout, &mut session.keyboard_enhanced)?;
execute_tracked(&mut stdout, &mut session.paste, EnableBracketedPaste)?;
```

`best_effort_restore_terminal`은 `keyboard_enhanced: bool`을 받아 활성 세션에서만 `PopKeyboardEnhancementFlags`를 실행한다. 패닉 Hook은 `TerminalSession::enter`가 성공한 뒤 설치되므로 `session.keyboard_enhanced` 값을 복사해 전달한다.

- [ ] **Step 5: 시험과 정적 검증 실행**

Run: `rtk cargo test --locked tui::tests::keyboard_enhancement_is_pushed_once_and_popped_only_when_active -- --exact`  
Expected: `running 1 test`, `1 passed`

Run: `rtk cargo test --locked tui::tests::tracked_command_marks_state_when_flush_fails_after_write -- --exact`  
Expected: `running 1 test`, `1 passed`

Run: `rtk cargo clippy --locked --all-targets --all-features -- -D warnings`  
Expected: 경고와 오류 없이 성공

- [ ] **Step 6: 커밋**

```bash
rtk git add -- src/tui.rs README.md
rtk git commit -m 'feat(tui): 수정자 Enter 입력 활성화'
```

---

### Task 2: 영문·한글 단축키와 삭제·복사 상태 전이

**Files:**
- Modify: `src/tui/state.rs:36-43,427-470,892-981,1038-1047,1134-1277,1690-2440`
- Modify: `README.md:51-91` — 영문·한글 단축키와 Pipeline 삭제
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md` — 이전 키 계약 현행화
- Modify: `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md` — 이전 키 계약 현행화

**Interfaces:**
- Consumes: `request_copy(CopyMode) -> Vec<Effect>`, `delete_selected(now)`, `request_selected_step(now)`, `restore_final()`, `changed(now)`
- Produces: 영문·한글 키가 같은 기존 상태 함수로 수렴하는 키 계약, `Enter` Pretty·`Shift+Enter` Raw, 예를 들어 `Removed JSON Prettify`와 같은 Footer 상태

- [ ] **Step 1: 새 키 계약의 실패 시험 작성**

기존 `global_copy_keys_use_the_active_output_from_every_pane`와 `pipeline_supports_all_selection_edit_inspect_palette_and_zoom_keys`는 제거된 계약을 주장하므로 아래 시험으로 교체한다.

```rust
#[test]
fn latin_and_hangul_global_shortcuts_open_palette_or_quit() {
    let start = now();
    for character in ['p', 'ㅔ'] {
        let mut app = App::new(start, true);
        key(
            &mut app,
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
            start,
        );
        assert!(matches!(app.modal, Some(Modal::TransformPicker { .. })));
    }
    for character in ['q', 'ㅂ'] {
        let mut app = App::new(start, true);
        assert!(matches!(
            key(
                &mut app,
                KeyCode::Char(character),
                KeyModifiers::CONTROL,
                start,
            )
            .as_slice(),
            [Effect::Quit(0)]
        ));
    }
}

#[test]
fn output_enter_selects_pretty_or_raw_and_removed_copy_keys_do_nothing() {
    let start = now();
    let mut app = App::new(start, true);
    app.focus = Pane::Output;
    app.output.status = OutputStatus::Ready;
    app.output.active_artifact = Some(Artifact::new(b"{ \"a\" : 1 }".to_vec()));

    assert!(matches!(
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start).as_slice(),
        [Effect::Copy(ClipboardPayload { text, kind: CopyKind::Pretty })]
            if text == "{\n  \"a\": 1\n}"
    ));
    assert!(matches!(
        key(&mut app, KeyCode::Enter, KeyModifiers::SHIFT, start).as_slice(),
        [Effect::Copy(ClipboardPayload { text, kind: CopyKind::Raw })]
            if text == "{\"a\":1}"
    ));
    for code in [KeyCode::Char('y'), KeyCode::F(3), KeyCode::F(4)] {
        assert!(key(&mut app, code, KeyModifiers::NONE, start).is_empty());
    }
}

#[test]
fn pipeline_delete_aliases_share_one_path_and_report_the_removed_transform() {
    let start = now();
    for code in [KeyCode::Delete, KeyCode::Char('d'), KeyCode::Char('ㅇ')] {
        let mut app = App::new(start, true);
        app.add_transform("base64-encode", start);
        app.add_transform("format-json", start);
        app.focus = Pane::Pipeline;
        app.selected_step = 1;

        key(&mut app, code, KeyModifiers::NONE, start);

        assert_eq!(app.steps.len(), 1);
        assert_eq!(app.selected_step, 0);
        assert_eq!(app.status.as_deref(), Some("Removed JSON Prettify"));
        assert!(matches!(app.output.status, OutputStatus::Debouncing { .. }));
    }
}

#[test]
fn removed_navigation_and_uppercase_view_keys_are_no_ops() {
    let start = now();
    let mut app = App::new(start, true);
    app.steps = ["base64-encode", "url-encode"]
        .into_iter()
        .map(|id| TransformStep {
            definition: transform_by_id(id).unwrap(),
            enabled: true,
        })
        .collect();
    app.focus = Pane::Pipeline;

    for (code, modifiers) in [
        (KeyCode::Char('j'), KeyModifiers::NONE),
        (KeyCode::Char('k'), KeyModifiers::NONE),
        (KeyCode::Char('J'), KeyModifiers::SHIFT),
        (KeyCode::Char('K'), KeyModifiers::SHIFT),
    ] {
        key(&mut app, code, modifiers, start);
    }
    assert_eq!(app.selected_step, 0);
    assert_eq!(app.steps[0].definition.id, "base64-encode");

    app.focus = Pane::Output;
    app.output.view = ViewMode::Smart;
    key(&mut app, KeyCode::Char('V'), KeyModifiers::SHIFT, start);
    assert_eq!(app.output.view, ViewMode::Smart);
}
```

추가로 다음 계약을 같은 Task의 상태 시험에 포함한다.

- `v/ㅍ`, `p/ㅔ`, `f/ㄹ`, `z/ㅋ`, `a/ㅁ`, `y/ㅛ`, `n/ㅜ` 각 쌍이 같은 상태 함수와 Effect를 만듦
- Input·Pipeline·Output 각 포커스에서 `F3/F4`가 어떤 Effect도 만들지 않음
- Trace, 실패, 취소, 실행, 지연, Artifact 부재 상태에서 두 Enter 조합이 모두 차단됨
- 빈 Pipeline의 `Delete/d/ㅇ`가 선택·요청 ID·Output 상태를 변경하지 않음
- 대문자 `Y/N`이 확인 Modal에서 무동작임

- [ ] **Step 2: 상태 시험이 RED인지 확인**

Run: `rtk cargo test --locked tui::state::tests::output_enter_selects_pretty_or_raw_and_removed_copy_keys_do_nothing -- --exact`  
Expected: `running 1 test` 뒤 Raw Copy 단언 실패

Run: `rtk cargo test --locked tui::state::tests::pipeline_delete_aliases_share_one_path_and_report_the_removed_transform -- --exact`  
Expected: `running 1 test` 뒤 한글 별칭 또는 삭제 상태 단언 실패

- [ ] **Step 3: 확인 Modal과 삭제 상태를 최소 변경**

```rust
fn confirmation_choice(key: &KeyEvent) -> Option<bool> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, KeyModifiers::NONE)
        | (KeyCode::Char('y'), KeyModifiers::NONE)
        | (KeyCode::Char('ㅛ'), KeyModifiers::NONE) => Some(true),
        (KeyCode::Esc, KeyModifiers::NONE)
        | (KeyCode::Char('n'), KeyModifiers::NONE)
        | (KeyCode::Char('ㅜ'), KeyModifiers::NONE) => Some(false),
        _ => None,
    }
}

fn delete_selected(&mut self, now: Instant) {
    let Some(step) = self.steps.get(self.selected_step) else {
        return;
    };
    let removed = step.definition.display_name;
    self.steps.remove(self.selected_step);
    self.selected_step = self.selected_step.min(self.steps.len().saturating_sub(1));
    self.set_status(Some(format!("Removed {removed}")));
    self.changed(now);
}
```

- [ ] **Step 4: 패널별 키 Match를 승인된 계약으로 교체**

`handle_output_key`에서 다음 Match만 유지한다.

```rust
(KeyCode::Enter, KeyModifiers::NONE) => self.request_copy(CopyMode::Pretty),
(KeyCode::Enter, KeyModifiers::SHIFT) => self.request_copy(CopyMode::Raw),
(KeyCode::Char('p' | 'ㅔ'), KeyModifiers::NONE) => self.request_selected_step(now),
(KeyCode::Char('f' | 'ㄹ'), KeyModifiers::NONE) => self.restore_final(),
(KeyCode::Char('v' | 'ㅍ'), KeyModifiers::NONE) => {
    self.cycle_view();
    Vec::new()
}
(KeyCode::Char('z' | 'ㅋ'), KeyModifiers::NONE) => {
    self.toggle_zoom(Pane::Output);
    Vec::new()
}
```

`cycle_view(&mut self)`는 Smart → Text → Hex → Trace → Smart의 단방향 Match로 단순화한다. `handle_pipeline_key`는 방향키와 Shift+방향키를 유지하고 `j/k/J/K` Match를 제거한 뒤 `d/ㅇ`, `a/ㅁ`, `z/ㅋ`를 추가한다.

전역 `handle_key`의 `F3/F4` 분기를 삭제하고 다음 두 Control Match를 사용한다.

```rust
(KeyCode::Char('p' | 'ㅔ'), KeyModifiers::CONTROL) => { /* open picker */ }
(KeyCode::Char('q' | 'ㅂ'), KeyModifiers::CONTROL) => { /* request quit */ }
```

- [ ] **Step 5: 상태 시험 전체 실행**

Run: `rtk cargo test --locked tui::state::tests -- --show-output`  
Expected: 모든 `tui::state::tests` 통과, 삭제된 키를 기대하던 이전 시험 없음

Run: `rtk cargo clippy --locked --all-targets --all-features -- -D warnings`  
Expected: 경고와 오류 없이 성공

- [ ] **Step 6: 커밋**

```bash
rtk git add -- src/tui/state.rs README.md \
  docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md \
  docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md
rtk git commit -m 'feat(tui): 한글 단축키와 출력 복사 적용'
```

---

### Task 3: App Bar·Output 제목·Command Dock 정리

**Files:**
- Modify: `src/tui/render.rs:102-130,303-540,819-870,1530-1905,2620-2672`
- Modify: `README.md:51-91` — App Bar·Output 제목·도움말
- Modify: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md` — 이전 제목·Dock 계약 현행화
- Modify: `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md` — App Bar·Dock 계약 현행화

**Interfaces:**
- Consumes: `App.focus`, `OutputSource`, `OutputStatus::Ready`, `Artifact::bytes()`, `App::can_copy()`
- Produces: `output_title(app: &App, available_width: u16) -> String`, FOCUS·FINAL 없는 제목, 영문 소문자 전용 Dock·Help

- [ ] **Step 1: 화면 기본 구조의 실패 시험 작성**

```rust
#[test]
fn app_bar_omits_focus_and_output_title_shows_only_useful_source_and_size() {
    let mut app = App::new(now(), true);
    app.focus = Pane::Output;
    app.output.status = OutputStatus::Ready;
    app.output.active_artifact = Some(Artifact::new(b"valid text".to_vec()));

    let final_screen = rendered_app(120, 20, &mut app);
    assert!(final_screen.lines().next().unwrap().starts_with(">_ TOC"));
    assert!(!final_screen.contains("FOCUS:"));
    assert!(final_screen.contains("» OUTPUT / SMART · 10 B"));
    assert!(!final_screen.contains("/ FINAL"));

    app.output.source = OutputSource::Step(1);
    let step_screen = rendered_app(120, 20, &mut app);
    assert!(step_screen.contains("» OUTPUT / STEP 02 / SMART · 10 B"));

    app.output.status = OutputStatus::Debouncing { deadline: now() };
    let pending = rendered_app(120, 20, &mut app);
    assert!(pending.contains("» OUTPUT / STEP 02 / SMART"));
    assert!(!pending.contains("SMART · 10 B"));
}

#[test]
fn dock_and_help_show_lowercase_current_keys_without_hangul_aliases() {
    let mut app = App::new(now(), true);
    app.focus = Pane::Output;
    app.output.status = OutputStatus::Ready;
    app.output.active_artifact = Some(Artifact::new(b"copyable".to_vec()));

    let screen = rendered_app(120, 20, &mut app);
    let lines = screen.lines().collect::<Vec<_>>();
    assert!(lines[18].contains("[ Enter ] Pretty"));
    assert!(lines[18].contains("[ Shift+Enter ] Raw"));
    assert!(lines[18].contains("[ v ] View"));
    assert!(lines[19].contains("[ Ctrl+p ] Add"));
    assert!(lines[19].contains("[ Ctrl+q ] Quit"));
    for removed in ["F3", "F4", "v/V", "Enter/y", "ㅔ", "ㅂ"] {
        assert!(!screen.contains(removed), "unexpected {removed}: {screen}");
    }
}
```

- [ ] **Step 2: 렌더 시험이 RED인지 확인**

Run: `rtk cargo test --locked tui::render::tests::app_bar_omits_focus_and_output_title_shows_only_useful_source_and_size -- --exact`  
Expected: `running 1 test` 뒤 기존 `FOCUS` 또는 `FINAL` 단언 실패

- [ ] **Step 3: App Bar와 Output 제목 구현**

```rust
fn output_title(app: &App, available_width: u16) -> String {
    let view = match app.output.view {
        ViewMode::Smart => "SMART",
        ViewMode::Text => "TEXT",
        ViewMode::Hex => "HEX",
        ViewMode::Trace => "TRACE",
    };
    let base = match app.output.source {
        OutputSource::Final => format!("» OUTPUT / {view}"),
        OutputSource::Step(index) => format!("» OUTPUT / STEP {:02} / {view}", index + 1),
    };
    let Some(artifact) = app.output.active_artifact.as_ref() else {
        return base;
    };
    if !matches!(app.output.status, OutputStatus::Ready) {
        return base;
    }
    let with_size = format!("{base} · {} B", artifact.bytes().len());
    if with_size.width().saturating_add(2) <= available_width as usize {
        with_size
    } else {
        base
    }
}
```

`render_app_bar`의 Line은 `>_ TOC` Span 하나만 사용한다. 더는 사용되지 않는 `pane_label`과 `source_label`은 제거하고 관련 Zoom 시험은 실제 패널 제목을 직접 비교한다.

- [ ] **Step 4: Dock·Help 문자열을 새 키 계약으로 교체**

- Pipeline Dock: `↑/↓ Select`, `Shift+↑/↓ Move`, `Space Toggle`, `Delete/d Delete`, `Enter Inspect`, `a Add`, `z Zoom` 순서
- Output Dock: `Enter Pretty`, `Shift+Enter Raw`, `v View`, `p Step`, `f Final`, `z Zoom` 순서
- Global Dock: `Tab Focus`, `Ctrl+p Add`, `F1 Help`, `Ctrl+q Quit` 순서
- Help: `F3/F4`, `j/k`, `J/K`, `v/V`, `Enter/y` 제거
- Help에서 `Ctrl+p`, `Ctrl+q`, `Ctrl+c`는 모두 영문 소문자로 표기하고 `?` 열기 안내를 유지
- Compact Help도 같은 계약을 사용하고 한글 별칭은 출력하지 않음

폭 제한은 기존 `dock_line`의 완전한 명령 단위 생략을 재사용한다. 복사 불가능 Output은 `Enter`와 `Shift+Enter` 두 명령을 함께 제외한다.

- [ ] **Step 5: 렌더 시험 실행**

Run: `rtk cargo test --locked tui::render::tests -- --show-output`  
Expected: 모든 `tui::render::tests` 통과

Run: `rtk cargo fmt --check`  
Expected: Diff 없이 성공

- [ ] **Step 6: 커밋**

```bash
rtk git add -- src/tui/render.rs README.md \
  docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md \
  docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md
rtk git commit -m 'feat(tui): 단축키 안내와 화면 제목 정리'
```

---

### Task 4: 반응형 Hex 행과 Ratatui 표

**Files:**
- Modify: `src/tui/views.rs:1-7,268-316,423-658`
- Modify: `src/tui/state.rs:17-21,176-186,1038-1132,2369-2422`
- Modify: `src/tui/render.rs:1-22,121-202,1908-2047,2552-2618`
- Modify: `README.md:51-91` — 반응형 Hex 표

**Interfaces:**
- Produces: `hex_bytes_per_row(columns: usize) -> usize`
- Produces: `visible_hex_rows<'a>(artifact: &'a Artifact, row_offset: usize, rows: usize, columns: usize) -> Vec<HexRow<'a>>`
- Produces: `HexRow<'a> { pub offset: usize, pub bytes: &'a [u8] }`
- Produces: `App::reflow_hex_offset(new_columns: usize)`
- Consumes: `Artifact::bytes()`, `OutputState.row_offset`, 이전 Draw의 `MouseRegions.output_content`

- [ ] **Step 1: Hex 행 크기·예산·Resize 실패 시험 작성**

```rust
#[test]
fn hex_rows_switch_between_sixteen_and_eight_bytes_at_exact_widths() {
    let artifact = Artifact::new((0..40).collect());
    assert_eq!(hex_bytes_per_row(78), 16);
    assert_eq!(hex_bytes_per_row(60), 16);
    assert_eq!(hex_bytes_per_row(59), 8);

    let wide = visible_hex_rows(&artifact, 1, 2, 78);
    assert_eq!(wide[0].offset, 16);
    assert_eq!(
        wide[0].bytes,
        &[16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31],
    );

    let narrow = visible_hex_rows(&artifact, 1, 2, 59);
    assert_eq!(narrow[0].offset, 8);
    assert_eq!(narrow[0].bytes, &[8, 9, 10, 11, 12, 13, 14, 15]);
    assert_eq!(narrow[1].offset, 16);
}

#[test]
fn hex_rows_are_bounded_by_the_existing_view_budget() {
    let artifact = Artifact::new(vec![0xff; 64 * 1024]);
    for columns in [38, 60, 78] {
        let rows = visible_hex_rows(&artifact, 0, 10_000, columns);
        let row_cost = match columns {
            78.. => 77,
            60..=77 => 59,
            _ => 34,
        };
        let rendered_cost = rows.len().saturating_add(1).saturating_mul(row_cost);
        assert!(rendered_cost <= VISIBLE_TEXT_BYTE_BUDGET);
    }
}

#[test]
fn widening_hex_preserves_the_visible_byte_and_clamps_the_last_row() {
    let start = now();
    let mut app = App::new(start, true);
    app.focus = Pane::Output;
    app.output.status = OutputStatus::Ready;
    app.output.view = ViewMode::Hex;
    app.output.active_artifact = Some(Artifact::new(vec![0; 40]));
    app.mouse_regions.output_content = Some(Rect::new(0, 0, 59, 10));
    app.output.row_offset = 4;

    app.reflow_hex_offset(78);

    assert_eq!(app.output.row_offset, 2);
}
```

- [ ] **Step 2: Hex 시험이 RED인지 확인**

Run: `rtk cargo test --locked tui::views::tests::hex_rows_switch_between_sixteen_and_eight_bytes_at_exact_widths -- --exact`  
Expected: 새 Type과 함수를 찾지 못해 컴파일 실패

- [ ] **Step 3: 가시 Hex 행 생성 구현**

```rust
#[derive(Debug, PartialEq, Eq)]
pub(super) struct HexRow<'a> {
    pub(super) offset: usize,
    pub(super) bytes: &'a [u8],
}

pub(super) fn hex_bytes_per_row(columns: usize) -> usize {
    if columns < 60 { 8 } else { 16 }
}

pub(super) fn visible_hex_rows<'a>(
    artifact: &'a Artifact,
    row_offset: usize,
    rows: usize,
    columns: usize,
) -> Vec<HexRow<'a>> {
    let bytes_per_row = hex_bytes_per_row(columns);
    let row_cost = match columns {
        78.. => 77,
        60..=77 => 59,
        _ => 34,
    };
    let header_cost = row_cost;
    let budget_rows = VISIBLE_TEXT_BYTE_BUDGET
        .saturating_sub(header_cost)
        / row_cost.max(1);
    let mut visible = Vec::with_capacity(rows.min(budget_rows));
    for row in row_offset..row_offset.saturating_add(rows.min(budget_rows)) {
        let Some(offset) = row.checked_mul(bytes_per_row) else { break };
        if offset >= artifact.bytes().len() { break }
        let end = offset.saturating_add(bytes_per_row).min(artifact.bytes().len());
        visible.push(HexRow {
            offset,
            bytes: &artifact.bytes()[offset..end],
        });
    }
    visible
}
```

기존 `render_hex_window`와 문자열 조립 시험은 제거하고 위 구조 시험으로 대체한다.

- [ ] **Step 4: 상태 스크롤과 Resize 재배치 연결**

`App::output_columns()`는 마지막 Draw의 `mouse_regions.output_content.width`를 반환하고 없으면 78을 사용한다. `output_max_offset`의 Hex 분기는 같은 `hex_bytes_per_row`로 마지막 행을 계산한다.

`reflow_hex_offset`은 이전 행당 바이트로 현재 시작 바이트를 계산한 뒤 새 행 크기로 나누고 마지막 행 이하로 Clamp한다. `render_output`이 새 내부 너비를 얻은 직후 호출하며, Event 처리 뒤 저장되는 새 Mouse 영역이 다음 스크롤 계산의 기준이 된다.

```rust
fn output_columns(&self) -> usize {
    self.mouse_regions
        .output_content
        .map_or(78, |area| area.width as usize)
}

fn reflow_hex_offset(&mut self, new_columns: usize) {
    let old_bytes_per_row = hex_bytes_per_row(self.output_columns());
    let new_bytes_per_row = hex_bytes_per_row(new_columns);
    if old_bytes_per_row == new_bytes_per_row {
        return;
    }
    let visible_byte = self.output.row_offset.saturating_mul(old_bytes_per_row);
    let maximum = self.output.active_artifact.as_ref().map_or(0, |artifact| {
        artifact.bytes().len().saturating_sub(1) / new_bytes_per_row
    });
    self.output.row_offset = (visible_byte / new_bytes_per_row).min(maximum);
}
```

- [ ] **Step 5: Hex Table 렌더 구현**

`render_output`을 `app: &mut App`으로 바꾸고 Hex 분기를 `render_hex_table(frame, app, inner)`로 분리한다. `Table::new(rows, widths).header(header).column_spacing(2)`를 사용한다.

- 78열 이상: Offset, 0–7, 8–15, ASCII
- 60–77열: Offset, 0–7, 8–15
- 60열 미만: Offset, 0–7

Offset은 Cyan, 제어·비 ASCII Hex 값은 Yellow, ASCII 열 출력 문자는 Green으로 만들고 `app.no_color`이면 모든 Cell Style을 `Style::default()`로 둔다. ASCII는 `0x20..=0x7e`만 문자로 표시하고 나머지는 `.`으로 표시한다.

- [ ] **Step 6: TestBackend 반응형·색상 시험 추가 및 실행**

Output Zoom을 사용해 내부 너비를 정확히 78, 60, 38로 만든다.

```rust
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
```

별도 Colored TestBackend 시험에서 Offset Cell은 `CYAN`, `00` Cell은 `YELLOW`, ASCII `A` Cell은 `GREEN`인지 확인한다.

Run: `rtk cargo test --locked tui::views::tests -- --show-output`  
Expected: 모든 View 시험 통과

Run: `rtk cargo test --locked tui::render::tests::hex_table_adapts_columns_and_keeps_no_color_structure -- --exact`  
Expected: `running 1 test`, `1 passed`

- [ ] **Step 7: 커밋**

```bash
rtk git add -- src/tui/views.rs src/tui/state.rs src/tui/render.rs README.md
rtk git commit -m 'feat(tui): 반응형 Hex 표 적용'
```

---

### Task 5: Trace 표와 첫 실패 상세

**Files:**
- Modify: `src/tui/views.rs:318-421,659-745`
- Modify: `src/tui/render.rs:1-22,121-202,1908-2325,2552-2618`
- Modify: `README.md:51-91` — Trace 표와 첫 실패 상세

**Interfaces:**
- Consumes: `StepTrace`, `StepStatus`, `render_transform_error_summary(error) -> String`, `transform_by_id(id)`
- Produces: `trace_status(status: StepStatus) -> &'static str`, `render_trace_table(frame, app, area)`, 자동 첫 실패 상세

- [ ] **Step 1: Trace 표·상세의 실패 시험 작성**

```rust
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
        "STEP", "OPERATION", "INPUT", "OUTPUT", "TIME", "STATUS",
        "Base64 Decode", "JSON Prettify", "OK", "ERROR",
        "STEP 2 · JSON Prettify", "output is not valid UTF-8 (6 bytes)",
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
```

- [ ] **Step 2: Trace 시험이 RED인지 확인**

Run: `rtk cargo test --locked tui::render::tests::trace_table_uses_display_names_columns_and_safe_first_failure_detail -- --exact`  
Expected: `running 1 test` 뒤 기존 한 줄 Trace 형식 때문에 실패

- [ ] **Step 3: 안전한 상태 Label을 렌더러에 공개**

`views.rs`의 `trace_status`를 `pub(super)`으로 바꾸고 `render_trace_window`와 문자열 표 시험을 제거한다. `render_transform_error_summary`와 `render_pipeline_error_summary`는 그대로 유지한다.

- [ ] **Step 4: Wide·Compact Trace Table 구현**

`render_trace_table`은 최대 32개 Trace 중 `row_offset`부터 현재 Table 높이만 가져온다. 알려진 ID는 `transform_by_id(id).display_name`을 사용하고, 알 수 없는 ID는 `escape_external(id, operation_width)`로 현재 열 너비까지만 표시한다.

- 내부 너비 70 이상: Step, Operation, Input, Output, Time, Status
- 내부 너비 70 미만: Step, Operation, `input→output B` Size, Status
- Step 값은 `format!("#{}", trace.step)`로 1부터 시작하는 기존 번호를 표시
- `OK` Green, `ERROR` Red, `CANCELLED` Yellow, `OFF`·`NOT RUN` Muted
- 실패 Row는 색상 모드에서 `SURFACE_HIGH` 배경과 Red 전경을 사용
- `NO_COLOR`은 문자열과 열만 유지

시간은 기존 정밀도를 잃지 않도록 `format!("{} µs", elapsed.as_micros())`를 사용하고 값이 없으면 `—`을 사용한다.

행 수는 헤더와 실패 상세의 실제 바이트 길이를 4 KiB에서 먼저 뺀 뒤, 남은 예산을 내부 너비로 나눈 값과 화면 높이 중 작은 값으로 제한한다. 각 Cell 문자열도 해당 열 너비에서 자르며, 헤더·행·상세의 합이 `VISIBLE_TEXT_BYTE_BUDGET`을 넘지 않는다.

- [ ] **Step 5: 첫 실패 상세 구현**

첫 `StepStatus::Failed`를 찾고 내부 높이가 5 이상이면 `Layout::vertical([Constraint::Min(2), Constraint::Length(3)])`로 Table과 상세를 나눈다. 상세는 Left Border만 가진 `Block`과 두 줄 `Paragraph`로 렌더링한다.

```rust
let detail = Paragraph::new(vec![
    Line::styled(
        format!("STEP {} · {}", trace.step, operation_name(trace.transform_id)),
        failure_style(app),
    ),
    Line::raw(render_transform_error_summary(error)),
]);
```

높이가 5 미만이면 실패 Row가 보이도록 시작 행을 실패 인덱스로 조정한다. 헤더와 실패 Row를 우선 할당하고 남은 행이 있을 때만 안전 오류 요약을 그래핀 경계에서 잘라 1–2행으로 표시한다. 오류 문자열은 기존 안전 요약만 사용하고 Preview Hex와 원문은 참조하지 않는다.

- [ ] **Step 6: 상태 색상·작은 높이·예산 시험 실행**

다음 시험을 추가한다.

- `trace_status_cells_keep_color_and_no_color_meaning`: 다섯 상태의 Color와 Label
- `short_trace_prioritizes_failure_and_uses_remaining_detail_space`: 내부 높이 4에서 ERROR Row와 남은 상세 행만 표시
- `trace_table_never_exposes_invalid_utf8_preview`: Preview Hex·해석 문자열 부재
- 기존 `composed_hex_and_trace_views_reserve_header_space_inside_the_budget`를 Table 행 수와 상세 문자열의 합이 4 KiB 이하인지 검증하도록 교체

Run: `rtk cargo test --locked tui::render::tests -- --show-output`  
Expected: 모든 Render 시험 통과

Run: `rtk cargo test --locked tui::views::tests -- --show-output`  
Expected: 모든 View 시험 통과

- [ ] **Step 7: 커밋**

```bash
rtk git add -- src/tui/views.rs src/tui/render.rs README.md
rtk git commit -m 'feat(tui): Trace 표와 실패 상세 적용'
```

---

### Task 6: PTY Smoke를 새 터미널 계약에 동기화

**Files:**
- Modify: `tests/shell-smoke.sh:96-145,181-249,299-305`

**Interfaces:**
- Consumes: TUI 시작 `ESC [ > 1 u`, 종료 `ESC [ < 1 u`, Output `Enter` Pretty Copy
- Produces: Bash·Zsh PTY에서 키보드 향상·마우스·붙여넣기·대체 화면의 진입·역순 복구 증거

- [ ] **Step 1: 새 터미널 시퀀스를 요구하도록 Smoke 변경**

TUI 시작 직후 다음 순서를 기다린다.

```tcl
expect_exact "\033\[?1049h" 146 147
expect_exact "\033\[>1u" 148 149
expect_exact "\033\[?2004h" 150 151
expect_exact "\033\[?1000h\033\[?1002h\033\[?1003h\033\[?1015h\033\[?1006h" 152 153
expect_exact ">_" 91 92
expect_exact "TOC" 91 92
```

`FOCUS:` 기대를 삭제한다. Output 복사는 `send -- "y"` 대신 `send -- "\r"`을 사용한다. 종료 시 Mouse Disable → Paste Disable → `\033[<1u` → Alternate Leave 순서를 검증한다.

```tcl
expect_exact "\033\[?1006l\033\[?1015l\033\[?1003l\033\[?1002l\033\[?1000l" 158 159
expect_exact "\033\[?2004l" 112 113
expect_exact "\033\[<1u" 114 115
expect_exact "\033\[?1049l" 116 117
```

- [ ] **Step 2: Bash Smoke 실행**

Run: `rtk bash tests/shell-smoke.sh`  
Expected: 종료 코드 0, PTY 원복 확인

- [ ] **Step 3: Zsh Smoke 실행**

Run: `rtk zsh tests/shell-smoke.sh`  
Expected: 종료 코드 0, PTY 원복 확인

- [ ] **Step 4: macOS 실제 클립보드 경로 실행**

Run: `rtk env TOC_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh`  
Expected: 종료 코드 0, `Copied as Hex`

Run: `rtk zsh -c '[ "$(pbpaste)" = ff ]'`  
Expected: 종료 코드 0

- [ ] **Step 5: 커밋**

```bash
rtk git add -- tests/shell-smoke.sh
rtk git commit -m 'test(tui): 새 키 입력 PTY 계약 검증'
```

---

### Task 7: 사용자 문서와 이전 설계 계약 일관성 감사

**Files:**
- Inspect: `README.md:51-91`
- Inspect: `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md:37-53,104-162,207-220`
- Inspect: `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md:21-58,85-123`

**Interfaces:**
- Consumes: Tasks 1–6에서 확정된 실제 키, 제목, Hex·Trace와 터미널 호환성
- Produces: Tasks 1–5의 각 기능 커밋에 이미 포함된 README·설계 문서가 실행 중인 제품과 일치한다는 감사 결과

- [ ] **Step 1: README의 TUI 계약 감사**

다음 내용이 Tasks 1–5의 커밋에 이미 반영되었는지 확인한다.

```markdown
- 전역: `Tab`/`Shift+Tab` 패널 이동, `Ctrl+p`/`Ctrl+ㅔ` 변환 추가, `F1` 도움말
- Pipeline: `↑`/`↓` 선택, `Shift+↑`/`Shift+↓` 이동, `Space` 전환,
  `Delete`/`d`/`ㅇ` 삭제, `Enter` 검사, `a`/`ㅁ` 추가, `z`/`ㅋ` 확대
- Output: `v`/`ㅍ` 보기, `p`/`ㅔ` 단계, `f`/`ㄹ` 최종,
  `Enter` Pretty Copy, `Shift+Enter` Raw Copy, `z`/`ㅋ` 확대
- `Esc`: 창·확대 닫기 또는 실행 취소, `Ctrl+q`/`Ctrl+ㅂ` 정상 종료,
  `Ctrl+c` 강제 종료
```

App Bar의 FOCUS와 최종 Output의 FINAL 제거, Hex의 16·8바이트 반응형 표, Trace 첫 실패 상세가 모두 설명되었는지 확인한다. 점진적 키보드 향상을 지원하지 않는 터미널은 Shift+Enter를 Enter와 구분하지 못할 수 있으며, 해당 환경에서는 Raw Copy 키가 제한됨이 명시되었는지 확인한다.

- [ ] **Step 2: 이전 두 설계 문서의 현재 계약 감사**

`2026-07-31-toc-tui-ux-refresh-design.md`와 `2026-08-01-toc-quiet-prism-design.md`에서 다음 이전 문자열이 현재 계약으로 교체되었는지 확인한다.

- `>_ TOC │ FOCUS: OUTPUT` → `>_ TOC`
- `OUTPUT / FINAL / SMART` → 예시 `OUTPUT / SMART · 17 B`
- `F3/F4` 전역 Copy → Output `Enter/Shift+Enter`
- `j/k`, `J/K` → 방향키, Shift+방향키
- `v/V` → `v`
- `Ctrl+P/Q/C` 도움말 표기 → `Ctrl+p/q/c`

역사적 구현 완료 기록은 삭제하지 않고 “2026-08-02 단축키·출력 보기 설계가 이 키·제목 계약을 대체한다”는 한 문장으로 범위를 명확히 한다.

- [ ] **Step 3: 문서 검색으로 오래된 계약 잔존 확인**

Run: `rtk rg --color=never -n 'F3|F4|j/k|J/K|v/V|Enter/y|FOCUS:|OUTPUT / FINAL' README.md docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md`  
Expected: 현재 계약으로 대체되었다고 설명하는 역사 문맥 외에는 결과 없음

- [ ] **Step 4: 문서 Diff·커밋 경계 점검**

Run: `rtk git -c color.ui=false diff --check`  
Expected: 공백 오류 없음

Run: `rtk git -c color.ui=false status --short`  
Expected: 문서 수정이 남아 있지 않음. 누락이 있으면 새 문서 커밋을 만들지 않고 해당 기능 Task의 검토 루프로 되돌린다.

---

### Task 8: 전체 회귀 검증과 검증 기록

**Files:**
- Modify: `README.md:116-150`

**Interfaces:**
- Consumes: Tasks 1–7의 커밋과 저장소 전체 시험
- Produces: 2026-08-02 실제 검증 결과와 깨끗한 구현 브랜치

- [ ] **Step 1: 형식·정적 분석·전체 시험 실행**

```bash
rtk cargo fmt --check
rtk cargo clippy --locked --all-targets --all-features -- -D warnings
rtk cargo test --all-targets --all-features --locked
rtk env RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
```

Expected: 모두 종료 코드 0. 전체 시험 출력에서 대상 필터가 아닌 실제 시험 수와 `3 ignored`를 확인한다. 실패하면 README를 수정하지 말고 공유 함수의 근본 원인을 고친 뒤 해당 Task 범위의 별도 `fix` 커밋으로 남기고 Step 1부터 다시 실행한다.

- [ ] **Step 2: Release 측정·패키징·격리 설치 실행**

```bash
rtk cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
rtk cargo test --release max_input_edit_release_measurement -- --ignored --nocapture
rtk cargo test --release utf8_validation_release_measurement -- --ignored --nocapture
rtk cargo package --locked
```

격리 설치는 아래처럼 `mktemp -d`로 만든 명시적 임시 경로를 사용하고 설치본의 `toc --version`이 `toc 0.2.0`인지 확인한다. 시간값은 회귀 관찰값으로만 기록하고 성공 임계값으로 만들지 않는다.

```bash
rtk zsh -c 'install_root=$(mktemp -d) || exit 1; trap '\''rm -rf -- "$install_root"'\'' EXIT; cargo install --locked --path . --root "$install_root"; [ "$("$install_root/bin/toc" --version)" = "toc 0.2.0" ]'
```

- [ ] **Step 3: 셸·클립보드 Smoke 재실행**

```bash
rtk bash tests/shell-smoke.sh
rtk zsh tests/shell-smoke.sh
rtk env TOC_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
rtk zsh -c '[ "$(pbpaste)" = ff ]'
rtk env TOC_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
rtk zsh -c '[ "$(pbpaste)" = ff ]'
```

Expected: 모든 명령 종료 코드 0, 두 `pbpaste` 결과가 소문자 `ff`

- [ ] **Step 4: 지원 터미널에서 Shift+Enter 실제 조작 확인**

Run: `rtk cargo run --release --locked -- tui`

1. Input에 `{ "a" : 1 }`을 입력한다.
2. Tab으로 Output에 포커스한다.
3. `Enter` 후 Footer가 `Copied Pretty`인지 확인한다.
4. `Shift+Enter` 후 Footer가 `Copied Raw`인지 확인한다.
5. `pbpaste`가 `{"a":1}`인지 확인한다.
6. `Ctrl+q`로 종료하고 터미널 입력·마우스·화면이 정상 복구됐는지 확인한다.

Expected: 점진적 키보드 향상을 지원하는 현재 검증 터미널에서 두 Enter 조합이 구분된다.

- [ ] **Step 5: README에 관찰한 검증 결과 기록**

`README.md`의 최신 로컬 검증 요약 맨 위에 2026-08-02 항목을 추가한다. Step 1의 Cargo 출력에서 `test result: ok.` 줄의 실제 통과·무시 수를 그대로 옮기고, Step 2의 세 Release 관찰값, 패키징·격리 설치, Step 3의 Bash·Zsh와 `pbpaste=ff`, Step 4의 Shift+Enter 구분 여부를 사실대로 기록한다. 예측 수치나 실행하지 않은 Linux 결과를 쓰지 않는다.

- [ ] **Step 6: 최종 Diff·금지 항목·작업 트리 점검**

```bash
rtk git -c color.ui=false diff --check
rtk rg --color=never -n 'console\.log|TODO|FIXME|F3|F4|j/k|J/K|v/V|Enter/y|FOCUS:' src tests README.md
rtk git -c color.ui=false status --short
```

Expected: 의도적인 과거 계약 검증 또는 제거 단언 외 금지 항목 없음. `.env`, `node_modules/`, `target/`가 Staged 상태가 아니며 기존 미추적 `.codex/`만 남는다.

- [ ] **Step 7: 검증 기록 커밋**

```bash
rtk git add -- README.md
rtk git commit -m 'docs(tui): 단축키와 출력 보기 검증 결과'
```

- [ ] **Step 8: 최종 커밋 범위 확인**

Run: `rtk git -c color.ui=false log --oneline --decorate -10`  
Expected: 각 Task가 하나의 논리적 한국어 Conventional Commit이며 Co-Authored-By Trailer가 없음

Run: `rtk git -c color.ui=false status --short`  
Expected: 구현 파일과 문서가 깨끗하고 기존 미추적 `.codex/`만 표시
