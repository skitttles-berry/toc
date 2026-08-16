# toc INPUT 줄 시작 삭제 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** INPUT에서 macOS의 `Cmd+Backspace`와 Windows·Linux의 `Ctrl+Backspace`로 선택 영역 또는 커서부터 현재 논리 줄 시작까지를 삭제한다.

**Architecture:** INPUT 경계의 기존 Crossterm 키 정규화 함수가 두 정확한 조합을 `tui-textarea-2`의 기존 `Ctrl+J` 동작으로 변환한다. 문자열·커서·삭제 기록을 직접 구현하지 않고 편집기의 `delete_line_by_head()` 계약과 현재 `changed()` 경로를 재사용한다.

**Tech Stack:** Rust 2024 edition, Crossterm, `tui-textarea-2 0.12.1`, Ratatui, Cargo 내장 시험, GitHub Flavored Markdown

## Global Constraints

- 기준 명세는 `docs/superpowers/specs/2026-08-17-toc-input-line-head-delete-design.md`다.
- 신규 별칭은 INPUT에 포커스가 있고 Modal이 열리지 않은 기존 INPUT 처리 경로에만 적용한다.
- `SUPER+Backspace`와 `CONTROL+Backspace`를 운영체제 분기 없이 어느 운영체제에서든 같은 별칭으로 받는다.
- 선택 영역이 있으면 선택만 삭제하고, 없으면 커서부터 현재 논리 줄 시작까지 삭제한다.
- 논리 줄 시작에서는 직전 줄바꿈을 삭제하여 이전 줄과 합치고, 문서 첫 위치에서는 변경하지 않는다.
- `Shift`, `Alt`, `Meta` 등 추가 modifier가 섞인 조합은 매핑하지 않는다.
- 일반 `Backspace`의 한 문자 삭제와 `Alt+Backspace`의 이전 단어 삭제를 유지한다.
- OUTPUT, Pipeline, Picker, Help와 다른 Modal의 키 동작을 바꾸지 않는다.
- 신규 의존성, 운영체제 조건부 컴파일, 사용자 키 설정과 자체 삭제 알고리즘을 추가하지 않는다.
- 실제 modifier 전달은 터미널 기능에 의존하며, 전달되지 않거나 일반 `Backspace`와 구분되지 않는 입력을 추측하지 않는다.
- 하단 INPUT dock의 `Text editing` 표시는 변경하지 않는다.
- 관련 Help, README와 설계 명세를 제품 코드와 같은 기능 커밋에서 현행화한다.
- 기존 미추적 `docs/superpowers/` 문서는 사용자 소유이므로 이 기능의 명시된 파일만 stage한다.

---

## 파일 구조

- `src/tui/state.rs` — INPUT 키 정규화, 편집기 전달, 상태·회귀 시험
- `src/tui/render.rs` — 전체 Input Help 문구와 렌더링 시험
- `README.md` — 운영체제별 INPUT 키와 터미널 전달 제한
- `docs/superpowers/specs/2026-08-17-toc-input-line-head-delete-design.md` — 구현 완료 상태와 검증 기록
- `Cargo.toml`, `Cargo.lock` — 변경하지 않음

### Task 1: INPUT 줄 시작 삭제와 사용자 문서

**Files:**
- Modify: `src/tui/state.rs:39-62,1045-1098,3732-3929`
- Modify: `src/tui/render.rs:1169-1180,2257-2284`
- Modify: `README.md:55-84`
- Modify: `docs/superpowers/specs/2026-08-17-toc-input-line-head-delete-design.md:1-149`
- Test: `src/tui/state.rs`
- Test: `src/tui/render.rs`

**Interfaces:**
- Consumes: `KeyEvent`, `KeyCode`, `KeyModifiers`, `TextArea::input(&mut self, input: impl Into<Input>) -> bool`, `tui-textarea-2`의 `Ctrl+J -> delete_line_by_head()` 기본 매핑
- Produces: 비공개 `fn normalize_input_key(KeyEvent) -> KeyEvent`, INPUT 전용 `SUPER+Backspace`·`CONTROL+Backspace` 줄 시작 삭제 별칭

- [ ] **Step 1: 실행 작업 트리와 범위를 확인한다**

실행 시 `superpowers:using-git-worktrees`로 계획 커밋에서 격리 작업 트리를 만든 뒤 아래를 실행한다.

```bash
git status --short --branch
git log -3 --oneline --decorate
git diff --check
```

Expected: 격리 작업 트리는 깨끗하고 HEAD가 이 계획 커밋이다. 기존 checkout에서 실행하는 경우에는 사용자 소유 미추적 `docs/superpowers/` 파일만 보이며, 추적 파일 차이는 없다.

- [ ] **Step 2: 키 정규화와 삭제 계약의 실패 시험을 작성한다**

`input_navigation_normalizer_maps_only_exact_aliases`를 `input_key_normalizer_maps_only_exact_aliases`로 이름만 바꾸고 기존 여섯 이동 case 뒤에 다음 두 case를 추가한다. 이 단계에서는 호출 대상 이름 `normalize_input_navigation_key`를 유지하여 현재 구현에 대한 동작 실패를 관찰한다.

```rust
(
    KeyCode::Backspace,
    KeyModifiers::SUPER,
    KeyCode::Char('j'),
    KeyModifiers::CONTROL,
),
(
    KeyCode::Backspace,
    KeyModifiers::CONTROL,
    KeyCode::Char('j'),
    KeyModifiers::CONTROL,
),
```

같은 시험의 정확 조합 거부 구간을 아래처럼 교체한다.

```rust
for (code, modifiers) in [
    (
        KeyCode::Left,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ),
    (
        KeyCode::Left,
        KeyModifiers::SUPER | KeyModifiers::ALT,
    ),
    (
        KeyCode::Backspace,
        KeyModifiers::SUPER | KeyModifiers::SHIFT,
    ),
    (
        KeyCode::Backspace,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ),
    (
        KeyCode::Backspace,
        KeyModifiers::SUPER | KeyModifiers::ALT,
    ),
    (
        KeyCode::Backspace,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ),
] {
    let input = KeyEvent::new(code, modifiers);
    assert_eq!(normalize_input_navigation_key(input), input);
}
```

기존 INPUT navigation 시험 바로 뒤에 아래 상태 시험을 추가한다.

```rust
#[test]
fn input_line_head_delete_aliases_handle_unicode_and_invalidate_preview() {
    let start = now();

    for modifiers in [KeyModifiers::SUPER, KeyModifiers::CONTROL] {
        let mut app = App::new(start, true);
        assert!(app.insert_paste("first\n한😀tail\nlast", start));
        key(&mut app, KeyCode::Up, KeyModifiers::NONE, start);
        assert_eq!(app.textarea.cursor(), (1, 4));

        app.output.status = OutputStatus::Ready;
        app.output.final_artifact = Some(Artifact::new(b"final".to_vec()));
        app.output.active_artifact = Some(Artifact::new(b"active".to_vec()));
        let expected_request_id = app.request_id + 1;

        let effects = key(&mut app, KeyCode::Backspace, modifiers, start);

        assert_eq!(app.input_text(), "first\nil\nlast");
        assert_eq!(app.textarea.cursor(), (1, 0));
        assert_eq!(app.request_id, expected_request_id);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Cancel(request_id)] if *request_id == expected_request_id
        ));
        assert!(app.output.final_artifact.is_none());
        assert_eq!(
            app.output.active_artifact.as_ref().unwrap().bytes(),
            b"active"
        );

        key(
            &mut app,
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
            start,
        );
        assert_eq!(app.input_text(), "first\n한😀tail\nlast");
        assert_eq!(app.textarea.cursor(), (1, 4));
    }
}

#[test]
fn input_line_head_delete_prefers_selection_and_undoes_once() {
    let start = now();
    let mut app = App::new(start, true);
    assert!(app.insert_paste("alpha beta", start));
    for _ in 0..2 {
        key(
            &mut app,
            KeyCode::Left,
            KeyModifiers::SHIFT,
            start,
        );
    }
    assert_eq!(app.textarea.selection_range(), Some(((0, 8), (0, 10))));

    key(
        &mut app,
        KeyCode::Backspace,
        KeyModifiers::SUPER,
        start,
    );

    assert_eq!(app.input_text(), "alpha be");
    assert!(app.textarea.selection_range().is_none());

    key(
        &mut app,
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
        start,
    );
    assert_eq!(app.input_text(), "alpha beta");
}

#[test]
fn input_line_head_delete_joins_lines_and_is_noop_at_document_start() {
    let start = now();
    let mut lines = App::new(start, true);
    assert!(lines.insert_paste("first\nsecond", start));
    key(&mut lines, KeyCode::Home, KeyModifiers::NONE, start);
    assert_eq!(lines.textarea.cursor(), (1, 0));

    key(
        &mut lines,
        KeyCode::Backspace,
        KeyModifiers::CONTROL,
        start,
    );
    assert_eq!(lines.input_text(), "firstsecond");
    assert_eq!(lines.textarea.cursor(), (0, 5));

    let mut document_start = App::new(start, true);
    assert!(document_start.insert_paste("first", start));
    key(
        &mut document_start,
        KeyCode::Home,
        KeyModifiers::NONE,
        start,
    );
    document_start.take_dirty();
    let request_id = document_start.request_id;

    key(
        &mut document_start,
        KeyCode::Backspace,
        KeyModifiers::SUPER,
        start,
    );

    assert_eq!(document_start.input_text(), "first");
    assert_eq!(document_start.request_id, request_id);
    assert!(!document_start.take_dirty());
}

#[test]
fn input_backspace_defaults_remain_character_and_word_based() {
    let start = now();
    let mut app = App::new(start, true);
    assert!(app.insert_paste("one two", start));

    key(
        &mut app,
        KeyCode::Backspace,
        KeyModifiers::NONE,
        start,
    );
    assert_eq!(app.input_text(), "one tw");

    key(
        &mut app,
        KeyCode::Backspace,
        KeyModifiers::ALT,
        start,
    );
    assert_eq!(app.input_text(), "one ");
}
```

기존 `input_navigation_aliases_do_not_apply_outside_input`을 `input_key_aliases_do_not_apply_outside_input`으로 바꾸고 본문을 아래처럼 교체한다.

```rust
#[test]
fn input_key_aliases_do_not_apply_outside_input() {
    let start = now();
    let mut app = App::new(start, true);
    assert!(app.insert_paste("alpha beta", start));
    let input = app.input_text();
    let cursor = app.textarea.cursor();

    for pane in [Pane::Output, Pane::Pipeline] {
        app.focus = pane;
        for (code, modifiers) in [
            (KeyCode::Left, KeyModifiers::SUPER),
            (KeyCode::Right, KeyModifiers::ALT),
            (KeyCode::Backspace, KeyModifiers::SUPER),
            (KeyCode::Backspace, KeyModifiers::CONTROL),
        ] {
            key(&mut app, code, modifiers, start);
        }
        assert_eq!(app.input_text(), input);
        assert_eq!(app.textarea.cursor(), cursor);
    }

    app.focus = Pane::Input;
    app.open_help();
    key(&mut app, KeyCode::Left, KeyModifiers::ALT, start);
    key(
        &mut app,
        KeyCode::Backspace,
        KeyModifiers::SUPER,
        start,
    );
    key(
        &mut app,
        KeyCode::Backspace,
        KeyModifiers::CONTROL,
        start,
    );
    assert_eq!(app.input_text(), input);
    assert_eq!(app.textarea.cursor(), cursor);
    assert!(matches!(app.modal, Some(Modal::Help)));

    key(&mut app, KeyCode::Esc, KeyModifiers::NONE, start);
    app.open_picker();
    key(
        &mut app,
        KeyCode::Backspace,
        KeyModifiers::SUPER,
        start,
    );
    key(
        &mut app,
        KeyCode::Backspace,
        KeyModifiers::CONTROL,
        start,
    );
    assert_eq!(app.input_text(), input);
    assert_eq!(app.textarea.cursor(), cursor);
    assert!(matches!(
        app.modal,
        Some(Modal::TransformPicker {
            ref query,
            selected: 0,
        }) if query.is_empty()
    ));
}
```

- [ ] **Step 3: 상태 시험이 현재 구현에서 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never input_key_normalizer_maps_only_exact_aliases
cargo test --locked --lib --color never input_line_head_delete
cargo test --locked --lib --color never input_backspace_defaults_remain_character_and_word_based
cargo test --locked --lib --color never input_key_aliases_do_not_apply_outside_input
```

Expected:

- 정규화 시험은 `SUPER+Backspace`가 `CONTROL+Char('j')`로 바뀌지 않아 FAIL.
- 줄 시작 삭제 시험은 `SUPER+Backspace`가 한 문자 삭제로 축소되고 `CONTROL+Backspace`가 처리되지 않아 FAIL.
- 일반·단어 Backspace 회귀 시험과 INPUT 외부 격리 시험은 PASS.

- [ ] **Step 4: 전체 Input Help의 실패 시험을 작성한다**

`src/tui/render.rs`의 `one_context_help_modal_lists_only_real_keys_for_each_pane`에서 `Pane::Input` 기대 배열의 `"Input Help"` 다음에 두 항목을 추가한다.

```rust
"Cmd+Backspace",
"Ctrl+Backspace",
```

Run:

```bash
cargo test --locked --lib --color never one_context_help_modal_lists_only_real_keys_for_each_pane
```

Expected: 렌더링된 Input Help에 `Cmd+Backspace`가 없어 FAIL.

- [ ] **Step 5: 기존 키 정규화에 두 삭제 별칭을 최소 구현한다**

`normalize_input_navigation_key`를 아래 `normalize_input_key`로 교체한다.

```rust
fn normalize_input_key(mut key: KeyEvent) -> KeyEvent {
    let selection = key.modifiers & KeyModifiers::SHIFT;
    let mut base_modifiers = key.modifiers;
    base_modifiers.remove(KeyModifiers::SHIFT);

    match (key.code, base_modifiers) {
        (KeyCode::Backspace, modifier)
            if selection.is_empty()
                && (modifier == KeyModifiers::SUPER || modifier == KeyModifiers::CONTROL) =>
        {
            key.code = KeyCode::Char('j');
            key.modifiers = KeyModifiers::CONTROL;
        }
        (KeyCode::Left, KeyModifiers::SUPER) => {
            key.code = KeyCode::Home;
            key.modifiers = selection;
        }
        (KeyCode::Right, KeyModifiers::SUPER) => {
            key.code = KeyCode::End;
            key.modifiers = selection;
        }
        (KeyCode::Left | KeyCode::Right, modifier)
            if modifier == KeyModifiers::ALT || modifier == KeyModifiers::META =>
        {
            key.modifiers = KeyModifiers::CONTROL | selection;
        }
        _ => {}
    }

    key
}
```

`handle_input_key`의 첫 줄과 정규화 단위 시험의 호출 세 곳을 새 이름으로 바꾼다.

```rust
let key = normalize_input_key(key);
```

```rust
normalize_input_key(input)
```

`handle_input_key`의 입력 한도, yank, `TextArea::input`, `changed()`와 dirty 분기는 수정하지 않는다.

- [ ] **Step 6: 상태 시험과 기존 INPUT 이동 회귀 시험을 통과시킨다**

Run:

```bash
cargo test --locked --lib --color never input_key_normalizer_maps_only_exact_aliases
cargo test --locked --lib --color never input_line_head_delete
cargo test --locked --lib --color never input_backspace_defaults_remain_character_and_word_based
cargo test --locked --lib --color never input_key_aliases_do_not_apply_outside_input
cargo test --locked --lib --color never input_navigation_aliases_follow_logical_line_and_word_boundaries
cargo test --locked --lib --color never cursor_and_selection_only_edits_keep_preview_ownership_and_cache
```

Expected: 모두 PASS. 삭제는 request ID와 최종 cache를 무효화하고, 기존 커서·선택 이동만으로는 cache를 무효화하지 않는다.

- [ ] **Step 7: Help와 README를 구현에 맞게 갱신한다**

`render_help`의 전체 크기 `Pane::Input` 본문에서 `Text editing` 다음에 한 줄을 추가한다. compact Help 분기와 하단 dock은 유지한다.

```rust
Pane::Input => (
    "Input Help",
    "Text editing: tui-textarea defaults\nCmd+Backspace · Ctrl+Backspace  Delete to line start\nCmd+←/→ · Home/End · Ctrl+A/E  Line start/end\nOption+←/→ · Ctrl+←/→  Previous/next word\nShift + movement  Select while moving\nTab / Shift+Tab  Next / previous pane\nCtrl+p  Add transform\nF1  Context help\nCtrl+q  Quit\nCtrl+c  Force quit\nEsc  Close zoom or cancel request\nMouse Click  Focus only".to_string(),
),
```

README의 INPUT 이동 행 다음에 아래 행을 추가한다.

```markdown
|  | <kbd>Cmd</kbd> + <kbd>Backspace</kbd> (macOS)<br><kbd>Ctrl</kbd> + <kbd>Backspace</kbd> (Windows·Linux) | 커서부터 현재 논리 줄 처음까지 삭제<br>줄 처음에서는 이전 줄과 합침 |
```

키 표 아래의 터미널 호환성 문단을 아래 내용으로 교체한다.

```markdown
일부 터미널은 Command·Option 조합을 자체 단축키로 먼저 처리하거나,
<kbd>Ctrl</kbd> + <kbd>Backspace</kbd>를 일반 <kbd>Backspace</kbd>와 구분하지 못합니다.
이 경우 줄 시작 삭제 별칭을 사용할 수 없습니다. 커서 이동에는 <kbd>Home</kbd>·<kbd>End</kbd>,
<kbd>Ctrl</kbd> + <kbd>A</kbd>·<kbd>E</kbd>, <kbd>Ctrl</kbd> + <kbd>←</kbd>·<kbd>→</kbd>를
사용하세요.
```

- [ ] **Step 8: Help 시험과 문서 계약을 확인한다**

Run:

```bash
cargo test --locked --lib --color never one_context_help_modal_lists_only_real_keys_for_each_pane
rg --color=never -n "Cmd\+Backspace|Ctrl\+Backspace|Delete to line start" src/tui/render.rs
rg --color=never -n "Cmd</kbd> \+ <kbd>Backspace|Ctrl</kbd> \+ <kbd>Backspace|줄 시작 삭제 별칭" README.md
```

Expected: Help 시험 PASS. 첫 검색은 두 alias와 동작 문구를, 둘째 검색은 두 운영체제별 키와 터미널 제약을 찾는다.

- [ ] **Step 9: 전체 저장소 검증을 실행한다**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features --color never -- -D warnings
cargo test --all-targets --all-features --locked --color never
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features --color never
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
git diff --check
```

Expected: Format 차이와 경고가 없고, 모든 비무시 시험·rustdoc·Bash/Zsh smoke·diff 검사가 PASS. 기존 명시적 `#[ignore]` 성능 측정 시험만 제외된다.

- [ ] **Step 10: 설계 상태와 검증 기록을 현행화한다**

`docs/superpowers/specs/2026-08-17-toc-input-line-head-delete-design.md`의 상태를 아래처럼 바꾼다.

```markdown
**상태:** 사용자 승인·구현 완료
```

문서 끝에 아래 절을 추가한다. Step 9가 모두 성공한 뒤에만 추가한다.

```markdown
## 11. 구현 검증

2026-08-17에 Format, 경고 금지 Clippy, 전체 잠금 시험, rustdoc와 기존 Bash·Zsh shell
smoke를 통과했다.
```

- [ ] **Step 11: 기능 범위만 검토하고 한 커밋으로 고정한다**

Run:

```bash
git status --short
git diff --stat
git diff -- src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-08-17-toc-input-line-head-delete-design.md
git diff --check
```

Expected: 변경은 위 네 파일뿐이고 Cargo 파일, 하단 dock, INPUT 외부 키 처리에는 차이가 없다. 기존 사용자 소유 미추적 문서는 stage 대상이 아니다.

Stage and commit only the feature files:

```bash
git add -- src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-08-17-toc-input-line-head-delete-design.md
git diff --cached --check
git diff --cached --name-status
git commit -m "feat(tui): INPUT 줄 시작 삭제"
git show --stat --oneline --decorate HEAD
```

Expected: 정확히 네 파일을 포함한 `feat(tui): INPUT 줄 시작 삭제` 커밋이 생성되고 작업 트리에 이 기능의 추적 변경이 남지 않는다.
