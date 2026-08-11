# Add Transform 순환 이동·상세 구분 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Transform 목록을 키보드와 마우스로 순환 탐색하고 선택 항목명과 상세 설명을 명확히 구분한다.

**Architecture:** 기존 `Modal::TransformPicker`와 `handle_modal_key` 경로를 유지한 채 `Up`·`Down`의 경계 계산만 순환 방식으로 바꾼다. 렌더링은 기존 상세 구분선 한 줄을 선택 항목 제목으로 재사용하고, 작은 화면에서는 이름과 설명을 한 문자열 안에서 구분한다.

**Tech Stack:** Rust, Crossterm, Ratatui, 기존 단위 시험

## Global Constraints

- 키보드 `↑`·`↓`와 목록 위 마우스 휠 모두 같은 순환 규칙을 사용한다.
- 검색 결과가 없거나 하나면 선택과 dirty 상태가 바뀌지 않는다.
- 검색어가 바뀌면 기존처럼 첫 결과를 선택한다.
- 일반 화면은 기존 구분선에 선택한 표시 이름을 넣고 행 수를 늘리지 않는다.
- 작은 화면은 `표시 이름 — 설명` 형식을 사용한다.
- 검색, 클릭, Enter, Esc, Backspace와 기존 Modal 크기를 유지한다.
- 공개 CLI, 변환 레지스트리, 키 바인딩, 의존성은 변경하지 않는다.
- 범용 순환 목록 자료형이나 새 렌더링 계층을 추가하지 않는다.
- 사용자 소유 변경인 `.gitignore`, `README.md`, `docs/test-reports/`를 수정·스테이징하지 않는다.

---

### Task 1: Picker 선택 순환

**Files:**
- Modify: `src/tui/state.rs:922-971`
- Test: `src/tui/state.rs:1784-1838`
- Test: `src/tui/state.rs:2020-2038`

**Interfaces:**
- Consumes: `App::filtered_transforms() -> Vec<&'static TransformDefinition>`, `Modal::TransformPicker { query: String, selected: usize }`, `App::handle_modal_key(KeyEvent, Instant) -> Vec<Effect>`
- Produces: `KeyCode::Up`·`KeyCode::Down`의 양방향 순환 선택; `handle_modal_mouse`는 기존 키 변환 경로를 통해 같은 동작을 얻는다.

- [ ] **Step 1: 키보드와 마우스 순환 실패 시험을 작성한다**

기존 `picker_key_selection_clamps_and_backspace_edits_query`를 다음 경계 계약을 검사하도록 바꾼다.

```rust
#[test]
fn picker_key_selection_wraps_and_backspace_edits_query() {
    let start = now();
    let mut app = App::new(start, true);
    app.open_picker();
    let last = transforms().len() - 1;

    key(&mut app, KeyCode::Up, KeyModifiers::NONE, start);
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected, .. }) if selected == last
    ));
    key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 0, .. })
    ));

    key(&mut app, KeyCode::Char('!'), KeyModifiers::NONE, start);
    app.take_dirty();
    key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);
    assert!(app.filtered_transforms().is_empty());
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 0, .. })
    ));
    assert!(!app.take_dirty());

    key(&mut app, KeyCode::Backspace, KeyModifiers::NONE, start);
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE, start);
    assert_eq!(app.steps.len(), 1);
    assert_eq!(app.steps[0].definition.id, "base64-encode");
}
```

기존 `picker_wheel_moves_one_item_and_clamps`는 첫 항목 위와 마지막 항목 아래를 직접 검사하도록 바꾼다.

```rust
#[test]
fn picker_wheel_wraps_at_both_ends() {
    let mut app = App::new(now(), true);
    app.open_picker();
    app.mouse_regions.picker_content = Some(Rect::new(10, 10, 20, 8));
    let last = transforms().len() - 1;

    app.handle_event(mouse(MouseEventKind::ScrollUp, 11, 11, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected, .. }) if selected == last
    ));

    app.handle_event(mouse(MouseEventKind::ScrollDown, 11, 11, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 0, .. })
    ));
}
```

단일 검색 결과에서는 이동과 redraw가 없는 시험을 추가한다.

```rust
#[test]
fn picker_single_match_does_not_redraw_when_moving() {
    let start = now();
    let mut app = App::new(start, true);
    app.open_picker();
    for character in "sha512".chars() {
        app.picker_insert(character);
    }
    assert_eq!(app.filtered_transforms().len(), 1);
    app.take_dirty();

    key(&mut app, KeyCode::Up, KeyModifiers::NONE, start);
    key(&mut app, KeyCode::Down, KeyModifiers::NONE, start);

    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker { selected: 0, .. })
    ));
    assert!(!app.take_dirty());
}
```

- [ ] **Step 2: 새 경계 시험이 현재 고정 동작에서 실패하는지 확인한다**

Run:

```bash
cargo test --locked picker_key_selection_wraps_and_backspace_edits_query
cargo test --locked picker_wheel_wraps_at_both_ends
cargo test --locked picker_single_match_does_not_redraw_when_moving
```

Expected: 첫 두 시험은 현재 `saturating_sub`·`min` 고정 동작 때문에 실패한다. 단일 결과 시험은 기존 동작을 고정하는 회귀 시험으로 통과할 수 있다.

- [ ] **Step 3: `Up`·`Down` 경계 계산만 순환 방식으로 바꾼다**

`handle_modal_key`의 두 분기를 다음 최소 계산으로 교체한다.

```rust
(KeyCode::Up, KeyModifiers::NONE) => {
    let mut changed = false;
    if let Some(Modal::TransformPicker { selected, .. }) = &mut self.modal {
        let next = match filtered_len {
            0 => 0,
            _ if *selected == 0 => filtered_len - 1,
            _ => *selected - 1,
        };
        changed = *selected != next;
        *selected = next;
    }
    if changed {
        self.mark_dirty();
    }
}
(KeyCode::Down, KeyModifiers::NONE) => {
    let mut changed = false;
    if let Some(Modal::TransformPicker { selected, .. }) = &mut self.modal {
        let next = if filtered_len == 0 {
            0
        } else {
            (*selected).saturating_add(1) % filtered_len
        };
        changed = *selected != next;
        *selected = next;
    }
    if changed {
        self.mark_dirty();
    }
}
```

`handle_modal_mouse`는 휠을 `KeyCode::Up`·`KeyCode::Down`으로 전달하므로 수정하지 않는다.

- [ ] **Step 4: Picker 상태 시험을 실행한다**

Run:

```bash
cargo test --locked picker_
```

Expected: Picker 검색·클릭·키보드·휠 시험이 모두 통과한다.

- [ ] **Step 5: 상태 변경만 커밋한다**

```bash
git add src/tui/state.rs
git diff --cached --check
git commit -m "feat(tui): Picker 선택 순환"
```

### Task 2: 선택 항목 상세 제목

**Files:**
- Modify: `src/tui/render.rs:965-1087`
- Test: `src/tui/render.rs:2178-2200`
- Test: `src/tui/render.rs:2329-2346`

**Interfaces:**
- Consumes: `App::filtered_transforms()`, `TransformDefinition::display_name`, `TransformDefinition::description`, 기존 `separator(width: u16) -> String`
- Produces: 일반 화면 `──── <display_name> ────` 구분선과 작은 화면 `<display_name> — <description>` 상세 문자열

- [ ] **Step 1: 일반·작은 화면의 실패 렌더링 시험을 작성한다**

`add_transform_uses_one_line_names_and_exact_detail_metadata`에서 색상 사용 여부와 관계없이 제목 구분선을 검사한다.

```rust
for color_enabled in [true, false] {
    let mut app = App::new(now(), color_enabled);
    app.open_picker();
    let screen = rendered_app(80, 20, &mut app);

    assert!(screen.contains("──── Base64 Encode ─"), "missing detail title: {screen}");
    assert!(screen.contains("ID        base64-encode"));
    assert!(screen.contains("ABOUT     Encode bytes using padded RFC 4648 Base64"));
}
```

`compact_add_transform_keeps_a_separate_selected_description`에 작은 화면 구분 형식을 추가한다.

```rust
assert!(
    screen.contains("Base64 Encode — Encode bytes"),
    "missing compact name and description: {screen}"
);
```

- [ ] **Step 2: 렌더링 시험이 현재 이름 없는 상세 영역에서 실패하는지 확인한다**

Run:

```bash
cargo test --locked add_transform_uses_one_line_names_and_exact_detail_metadata
cargo test --locked compact_add_transform_keeps_a_separate_selected_description
```

Expected: 일반 화면 제목 구분선과 작은 화면 `이름 — 설명` assertion이 실패한다.

- [ ] **Step 3: 선택 정의를 한 번 얻어 상세 문자열과 제목 구분선에 재사용한다**

`render_picker`에서 `filtered`를 만든 직후 선택 정의와 상세 텍스트를 계산한다.

```rust
let filtered = app.filtered_transforms();
let selected_transform = filtered.get(selected).copied();
let detail = selected_transform.map_or_else(
    || "No matching transforms".to_string(),
    |transform| {
        if compact {
            format!("{} — {}", transform.display_name, transform.description)
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
let detail_separator_text = selected_transform.map_or_else(
    || separator(detail_separator.width),
    |transform| {
        let prefix = format!("──── {} ", transform.display_name);
        let remaining = detail_separator
            .width
            .saturating_sub(prefix.width() as u16);
        format!("{prefix}{}", separator(remaining))
    },
);
```

기존 `detail_separator` 렌더링은 계산한 문자열을 사용한다.

```rust
frame.render_widget(
    Paragraph::new(detail_separator_text).style(muted_style()),
    detail_separator,
);
```

목록 생성은 기존처럼 `filtered.into_iter()`를 사용한다. 빈 결과에서는 기존 이름 없는 구분선과 `No matching transforms`가 유지된다.

- [ ] **Step 4: 렌더링과 Picker 시험을 실행한다**

Run:

```bash
cargo test --locked add_transform_
cargo test --locked picker_
```

Expected: 일반·작은 화면과 Picker 상태 시험이 모두 통과한다.

- [ ] **Step 5: 전체 검증을 실행한다**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features
git diff --check
```

Expected: Format, Clippy, 전체 시험, rustdoc와 diff 검사가 모두 성공한다.

- [ ] **Step 6: 렌더링 변경만 커밋하고 사용자 변경 보존을 확인한다**

```bash
git add src/tui/render.rs
git diff --cached --check
git diff --cached --name-only
git commit -m "feat(tui): Picker 상세 구분"
git status --short
```

Expected: 두 번째 커밋에는 `src/tui/render.rs`만 포함된다. 작업 트리에는 기존 사용자 소유 `.gitignore`, `README.md`, `docs/test-reports/`만 남는다.
