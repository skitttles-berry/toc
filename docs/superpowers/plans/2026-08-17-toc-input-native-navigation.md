# toc INPUT 네이티브 커서 이동 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** INPUT 편집기에서 Command·Option 수평 이동과 Shift 선택을 지원하면서 기존 Linux·Windows 키와 변환 결과 상태를 보존한다.

**Architecture:** Crossterm `KeyEvent`를 INPUT 처리 경계에서 기존 `tui-textarea-2` 키로 정규화한다. 커서·선택 계산은 현재 편집기에 위임하고, 기존 `TextArea::input` 반환값에 따른 내용 변경과 화면 이동 구분을 유지한다. 새 의존성, 운영체제 분기, 별도 키맵 계층은 만들지 않는다.

**Tech Stack:** Rust 2024, Crossterm 0.29.0, Ratatui 0.30.2, tui-textarea-2 0.12.1, Rust 단위 시험

## Global Constraints

- 적용 범위는 `toc tui`의 INPUT pane뿐이다. OUTPUT, Pipeline, Picker, Help와 다른 Modal의 키 처리를 바꾸지 않는다.
- `SUPER+Left`·`SUPER+Right`는 현재 논리 줄의 `Home`·`End`로 정규화한다.
- `ALT+Left`·`ALT+Right`와 `META+Left`·`META+Right`는 기존 `Ctrl+Left`·`Ctrl+Right` 단어 이동으로 정규화한다.
- 위 신규 조합의 선택용 `SHIFT`는 보존한다. 다른 modifier가 더 섞인 조합은 원본 그대로 둔다.
- 기존 Home·End, `Ctrl+A`·`Ctrl+E`, `Ctrl+Left`·`Ctrl+Right` 동작을 유지한다.
- 논리 줄과 단어 경계는 설치된 `tui-textarea-2 0.12.1` 동작을 그대로 사용한다.
- 커서·선택 이동은 `changed()`를 호출하지 않으며 Pipeline 실행, debounce, request ID, OUTPUT과 결과 cache를 바꾸지 않는다.
- 키 이벤트의 `KeyEventKind`와 `KeyEventState`를 보존한다.
- 운영체제별 `cfg`, Escape sequence 직접 해석, 터미널 설정 변경을 추가하지 않는다.
- 새 crate와 설정을 추가하지 않는다. `Cargo.toml`과 `Cargo.lock`은 변경하지 않는다.
- 하단 INPUT dock의 `Text editing` 문구와 compact Help는 유지한다.
- 전체 Input Help와 README에 신규 별칭, 기존 호환 키, Shift 선택과 터미널 가로채기 제약을 기록한다.
- 검증 완료 뒤 승인 설계 문서의 상태를 `사용자 승인·구현 완료`로 현행화한다.
- `Cmd+Up`·`Cmd+Down`, 문서 처음·끝, 단어 삭제와 시각 행 기준 Home·End는 추가하지 않는다.
- 관련 설계는 `docs/superpowers/specs/2026-08-17-toc-input-native-navigation-design.md`를 기준으로 한다.
- 기존 미추적 `docs/superpowers/` 문서는 수정하거나 함께 스테이징하지 않는다.

---

## File Structure

- `src/tui/state.rs`: INPUT 전용 키 정규화 함수, `handle_input_key` 연결, 상태·회귀 시험
- `src/tui/render.rs`: 전체 크기 Input Help 문구와 렌더링 회귀 시험
- `README.md`: 공개 INPUT 키 표와 터미널 호환성 주석
- `docs/superpowers/specs/2026-08-17-toc-input-native-navigation-design.md`: 구현 완료 상태와 검증 결과

새 파일이나 공용 모듈을 만들지 않는다. 정규화는 `state.rs` 안에서만 사용하는 private 함수 하나로 제한한다.

---

### Task 1: INPUT 수평 커서 이동과 공개 안내

**Files:**
- Modify: `src/tui/state.rs:1-55,1020-1073,1515-1533,3670-3745`
- Modify: `src/tui/render.rs:1170-1208,2266-2310`
- Modify: `README.md:54-78`
- Modify: `docs/superpowers/specs/2026-08-17-toc-input-native-navigation-design.md:5,132-136`
- Test: `src/tui/state.rs`
- Test: `src/tui/render.rs`

**Interfaces:**
- Consumes: `crossterm::event::KeyEvent`, `KeyCode`, `KeyModifiers`; `TextArea::input(&mut self, input: impl Into<tui_textarea::Input>) -> bool`; 기존 `App::handle_input_key(KeyEvent, Instant)`
- Produces: `fn normalize_input_navigation_key(key: KeyEvent) -> KeyEvent`
- Produces: INPUT에서만 동작하는 Command·Option·Meta 별칭과 Shift 선택
- Preserves: `KeyEvent.kind`, `KeyEvent.state`, 기존 global·Modal 우선순위, `handle_input_key`의 변경·이동 상태 분리

- [ ] **Step 1: 키 정규화와 실제 이동의 실패 시험을 작성한다**

`src/tui/state.rs` 시험 모듈의 Crossterm import에 metadata type을 추가한다.

```rust
use crossterm::event::{
    KeyCode, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
```

같은 시험 모듈에 정규화 계약과 실제 줄·단어 이동 시험을 추가한다.

```rust
#[test]
fn input_navigation_normalizer_maps_only_exact_aliases() {
    let cases = [
        (
            KeyCode::Left,
            KeyModifiers::SUPER,
            KeyCode::Home,
            KeyModifiers::NONE,
        ),
        (
            KeyCode::Right,
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
            KeyCode::End,
            KeyModifiers::SHIFT,
        ),
        (
            KeyCode::Left,
            KeyModifiers::ALT,
            KeyCode::Left,
            KeyModifiers::CONTROL,
        ),
        (
            KeyCode::Right,
            KeyModifiers::ALT | KeyModifiers::SHIFT,
            KeyCode::Right,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ),
        (
            KeyCode::Left,
            KeyModifiers::META,
            KeyCode::Left,
            KeyModifiers::CONTROL,
        ),
        (
            KeyCode::Right,
            KeyModifiers::META | KeyModifiers::SHIFT,
            KeyCode::Right,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ),
    ];

    for (code, modifiers, expected_code, expected_modifiers) in cases {
        let input = KeyEvent::new_with_kind_and_state(
            code,
            modifiers,
            KeyEventKind::Repeat,
            KeyEventState::KEYPAD,
        );

        assert_eq!(
            normalize_input_navigation_key(input),
            KeyEvent::new_with_kind_and_state(
                expected_code,
                expected_modifiers,
                KeyEventKind::Repeat,
                KeyEventState::KEYPAD,
            )
        );
    }

    for modifiers in [
        KeyModifiers::CONTROL | KeyModifiers::ALT,
        KeyModifiers::SUPER | KeyModifiers::ALT,
    ] {
        let input = KeyEvent::new(KeyCode::Left, modifiers);
        assert_eq!(normalize_input_navigation_key(input), input);
    }

    for input in [
        KeyEvent::new(KeyCode::Up, KeyModifiers::SUPER),
        KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
    ] {
        assert_eq!(normalize_input_navigation_key(input), input);
    }
}

#[test]
fn input_navigation_aliases_follow_logical_line_and_word_boundaries() {
    let start = now();
    let mut lines = App::new(start, true);
    assert!(lines.insert_paste("first\n둘째 줄\nlast", start));

    key(&mut lines, KeyCode::Up, KeyModifiers::NONE, start);
    assert_eq!(lines.textarea.cursor(), (1, 4));
    key(&mut lines, KeyCode::Left, KeyModifiers::SUPER, start);
    assert_eq!(lines.textarea.cursor(), (1, 0));
    key(
        &mut lines,
        KeyCode::Right,
        KeyModifiers::SUPER | KeyModifiers::SHIFT,
        start,
    );
    assert_eq!(lines.textarea.cursor(), (1, 4));
    assert_eq!(lines.textarea.selection_range(), Some(((1, 0), (1, 4))));
    key(&mut lines, KeyCode::Home, KeyModifiers::NONE, start);
    assert_eq!(lines.textarea.cursor(), (1, 0));
    assert!(lines.textarea.selection_range().is_none());
    key(&mut lines, KeyCode::End, KeyModifiers::NONE, start);
    assert_eq!(lines.textarea.cursor(), (1, 4));
    key(
        &mut lines,
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
        start,
    );
    assert_eq!(lines.textarea.cursor(), (1, 0));
    key(
        &mut lines,
        KeyCode::Char('e'),
        KeyModifiers::CONTROL,
        start,
    );
    assert_eq!(lines.textarea.cursor(), (1, 4));

    let mut words = App::new(start, true);
    assert!(words.insert_paste("alpha, 한글 beta", start));
    key(&mut words, KeyCode::Left, KeyModifiers::ALT, start);
    assert_eq!(words.textarea.cursor(), (0, 10));
    key(&mut words, KeyCode::Left, KeyModifiers::META, start);
    assert_eq!(words.textarea.cursor(), (0, 7));
    key(&mut words, KeyCode::Left, KeyModifiers::CONTROL, start);
    assert_eq!(words.textarea.cursor(), (0, 5));
    key(
        &mut words,
        KeyCode::Right,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
        start,
    );
    assert_eq!(words.textarea.cursor(), (0, 7));
    assert_eq!(words.textarea.selection_range(), Some(((0, 5), (0, 7))));
    key(&mut words, KeyCode::Right, KeyModifiers::META, start);
    assert_eq!(words.textarea.cursor(), (0, 10));
    assert!(words.textarea.selection_range().is_none());
    key(&mut words, KeyCode::Right, KeyModifiers::CONTROL, start);
    assert_eq!(words.textarea.cursor(), (0, 14));
}

#[test]
fn input_navigation_aliases_do_not_apply_outside_input() {
    let start = now();
    let mut app = App::new(start, true);
    assert!(app.insert_paste("alpha beta", start));
    let cursor = app.textarea.cursor();

    for pane in [Pane::Output, Pane::Pipeline] {
        app.focus = pane;
        key(&mut app, KeyCode::Left, KeyModifiers::SUPER, start);
        key(&mut app, KeyCode::Right, KeyModifiers::ALT, start);
        assert_eq!(app.textarea.cursor(), cursor);
    }

    app.focus = Pane::Input;
    app.open_help();
    key(&mut app, KeyCode::Left, KeyModifiers::ALT, start);
    assert_eq!(app.textarea.cursor(), cursor);
    assert!(matches!(app.modal, Some(Modal::Help)));
}
```

기존 `cursor_and_selection_only_edits_keep_preview_ownership_and_cache`의 두 이동을 신규 별칭으로 바꾼다. 나머지 상태 단정은 그대로 둔다.

```rust
key(&mut app, KeyCode::Left, KeyModifiers::SUPER, start);
key(
    &mut app,
    KeyCode::Right,
    KeyModifiers::ALT | KeyModifiers::SHIFT,
    start,
);
```

- [ ] **Step 2: 상태 시험이 구현 부재로 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never input_navigation_
```

Expected: `normalize_input_navigation_key`를 찾을 수 없다는 컴파일 오류로 실패한다. 함수를 임시 선언해 RED를 우회하지 않는다.

- [ ] **Step 3: INPUT 키 정규화를 최소 구현한다**

`src/tui/state.rs`의 `confirmation_choice` 앞에 private 함수를 추가한다.

```rust
fn normalize_input_navigation_key(mut key: KeyEvent) -> KeyEvent {
    let selection = key.modifiers & KeyModifiers::SHIFT;
    let mut navigation = key.modifiers;
    navigation.remove(KeyModifiers::SHIFT);

    match (key.code, navigation) {
        (KeyCode::Left, KeyModifiers::SUPER) => {
            key.code = KeyCode::Home;
            key.modifiers = selection;
        }
        (KeyCode::Right, KeyModifiers::SUPER) => {
            key.code = KeyCode::End;
            key.modifiers = selection;
        }
        (KeyCode::Left, modifier)
            if modifier == KeyModifiers::ALT || modifier == KeyModifiers::META =>
        {
            key.modifiers = KeyModifiers::CONTROL | selection;
        }
        (KeyCode::Right, modifier)
            if modifier == KeyModifiers::ALT || modifier == KeyModifiers::META =>
        {
            key.modifiers = KeyModifiers::CONTROL | selection;
        }
        _ => {}
    }

    key
}
```

`App::handle_input_key`의 첫 줄에서 이벤트를 한 번 정규화한다. 이후 입력 한도 검사와 `TextArea::input` 경로는 그대로 둔다.

```rust
fn handle_input_key(&mut self, key: KeyEvent, now: Instant) {
    let key = normalize_input_navigation_key(key);
```

- [ ] **Step 4: 상태·불변 시험을 실행해 GREEN을 확인한다**

Run:

```bash
cargo test --locked --lib --color never input_navigation_
cargo test --locked --lib --color never cursor_and_selection_only_edits_keep_preview_ownership_and_cache
```

Expected: 신규 3개 시험과 기존 cache 불변 시험이 모두 통과한다. 이동 뒤 INPUT 내용, request ID, OUTPUT artifact·trace와 source가 유지된다.

- [ ] **Step 5: 전체 Input Help의 실패 시험을 작성한다**

`one_context_help_modal_lists_only_real_keys_for_each_pane`의 `Pane::Input` 기대 배열을 아래처럼 바꾼다. compact Help 시험은 수정하지 않는다.

```rust
&[
    "Input Help",
    "Cmd+←/→",
    "Home/End",
    "Ctrl+A/E",
    "Option+←/→",
    "Ctrl+←/→",
    "Shift + movement",
    "Tab",
    "Ctrl+p",
    "F1",
    "Ctrl+q",
    "Ctrl+c",
    "Esc",
][..],
```

- [ ] **Step 6: 도움말 시험이 새 문구 부재로 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never one_context_help_modal_lists_only_real_keys_for_each_pane
```

Expected: 렌더링 결과에 `Cmd+←/→`가 없다는 단정 실패가 발생한다.

- [ ] **Step 7: 전체 Input Help와 README를 현행화한다**

`render_help`의 전체 크기 `Pane::Input` 본문만 아래 문자열로 교체한다. compact 분기는 유지한다.

```rust
Pane::Input => (
    "Input Help",
    "Text editing: tui-textarea defaults\nCmd+←/→ · Home/End · Ctrl+A/E  Line start/end\nOption+←/→ · Ctrl+←/→  Previous/next word\nShift + movement  Select while moving\nTab / Shift+Tab  Next / previous pane\nCtrl+p  Add transform\nF1  Context help\nCtrl+q  Quit\nCtrl+c  Force quit\nEsc  Close zoom or cancel request\nMouse Click  Focus only".to_string(),
),
```

README의 전역 키 다음, Pipeline 키 앞에 INPUT 행을 추가한다.

```markdown
| Input | <kbd>Cmd</kbd> + <kbd>←</kbd> / <kbd>→</kbd><br><kbd>Home</kbd> / <kbd>End</kbd><br><kbd>Ctrl</kbd> + <kbd>A</kbd> / <kbd>E</kbd> | 현재 논리 줄의 처음·끝으로 이동 |
|  | <kbd>Option</kbd> + <kbd>←</kbd> / <kbd>→</kbd><br><kbd>Ctrl</kbd> + <kbd>←</kbd> / <kbd>→</kbd> | 이전·다음 단어 경계로 이동 |
|  | 위 이동 키 + <kbd>Shift</kbd> | 이동 구간 선택 |
```

키 표 바로 아래, 기존 `Shift+Enter` 주석 앞에 터미널 호환성 문단을 추가한다.

```markdown
일부 터미널은 Command·Option 조합을 탭 전환 같은 자체 단축키로 먼저 처리합니다. 이 경우
<kbd>Home</kbd>·<kbd>End</kbd>, <kbd>Ctrl</kbd> + <kbd>A</kbd>·<kbd>E</kbd>,
<kbd>Ctrl</kbd> + <kbd>←</kbd>·<kbd>→</kbd>를 사용하세요.
```

- [ ] **Step 8: 도움말과 전체 TUI 단위 시험을 실행한다**

Run:

```bash
cargo test --locked --lib --color never one_context_help_modal_lists_only_real_keys_for_each_pane
cargo test --locked --lib --color never tui::state::tests
cargo test --locked --lib --color never tui::render::tests
```

Expected: 전체 Input Help에 신규·호환 키가 보이고 compact Help와 기존 pane 도움말을 포함한 상태·렌더링 시험이 모두 통과한다.

- [ ] **Step 9: 전체 자동 검증을 실행한다**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --color never --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked --color never
RUSTDOCFLAGS='-D warnings' cargo doc --color never --locked --no-deps --all-features
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
git diff --check
```

Expected: Format, 경고 금지 Clippy, 전체 시험, rustdoc, 기존 Bash·Zsh shell smoke와 diff 검사가 모두 성공한다. `tests/shell-smoke.sh`는 수정하지 않는다.

- [ ] **Step 10: 전달 가능한 터미널에서 수동 동작을 확인한다**

Run:

```bash
cargo build --release --locked --color never
./target/release/toc tui
```

Expected: 여러 줄 INPUT에서 Command+Left·Right는 현재 논리 줄의 처음·끝, Option+Left·Right는 이전·다음 단어 경계로 이동한다. Shift를 함께 누르면 구간을 선택하며 OUTPUT은 다시 계산되지 않는다. 현재 터미널이 해당 조합을 가로채면 Home·End와 Control 조합을 확인하고, 터미널 설정이나 애플리케이션 parsing을 변경하지 않는다.

- [ ] **Step 11: 승인 설계 문서를 구현 완료 상태로 현행화한다**

`docs/superpowers/specs/2026-08-17-toc-input-native-navigation-design.md`의 상태를 아래처럼 바꾼다.

```markdown
**상태:** 사용자 승인·구현 완료
```

문서 끝에 실제 통과한 자동 검증을 기록한다.

```markdown
## 9. 구현 검증

2026-08-17에 Format, 경고 금지 Clippy, 전체 잠금 시험, rustdoc와 기존 Bash·Zsh shell
smoke를 통과했다.
```

- [ ] **Step 12: 정확한 네 파일만 검토·커밋한다**

```bash
git diff -- src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-08-17-toc-input-native-navigation-design.md
git status --short
git add -- src/tui/state.rs src/tui/render.rs README.md docs/superpowers/specs/2026-08-17-toc-input-native-navigation-design.md
git diff --cached --check
git diff --cached --name-only
git commit -m "feat(tui): INPUT 네이티브 커서 이동"
git status --short --branch
```

Expected: 구현 커밋에는 `src/tui/state.rs`, `src/tui/render.rs`, `README.md`, 승인 설계 문서만 포함된다. `Cargo.toml`, `Cargo.lock`, 하단 dock, compact Help, shell smoke와 다른 기존 미추적 `docs/superpowers/` 문서는 변경되지 않는다.
