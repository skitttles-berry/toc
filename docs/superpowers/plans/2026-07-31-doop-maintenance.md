# doop 안정화·단순화 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 승인된 안정화·단순화 설계에 따라 TUI 표시와 작업자 종료 결함을 고치고, CLI·Base64 자원 낭비와 클립보드 복원 시험을 제거하며, 측정 근거와 현재 문서만 남긴다.

**Architecture:** 기존 단일 Cargo 패키지와 CLI·TUI 모듈 경계를 유지한다. 각 결함은 발생 지점에서 최소 변경으로 수정하고 독립 시험과 문서를 같은 커밋에 포함한다. 입력 편집과 UTF-8 판정은 릴리스 측정 중앙값이 16 ms를 초과할 때만 승인된 조건부 최적화를 적용한다.

**Tech Stack:** Rust 1.97.1 stable, Rust 2024 Edition, clap 4.6.4, Ratatui 0.30.2, Crossterm 0.29.0, base64 0.23.0, tui-textarea-2 0.12.1, Bash, Zsh, Expect

## Global Constraints

* `main` 브랜치와 기존 단일 Cargo 패키지 구조를 유지한다.
* 공개 CLI 명령, 옵션, 8개 변환 ID, 성공 결과, 오류 형식과 종료 코드를 바꾸지 않는다.
* CLI 최대 입력 64 MiB, TUI 입력 1 MiB·65,536줄, TUI 출력 64 MiB와 최대 32단계를 유지한다.
* Text View의 표시용 자동 줄바꿈은 원본 Artifact와 클립보드 payload를 바꾸지 않는다.
* 작업자 채널 종료는 자동 재시작 없이 TUI 오류 코드 1과 기존 터미널 복구 경로로 처리한다.
* macOS 클립보드 Smoke는 복사값만 검증하고 백업, `changeCount`, 소유권 추적과 복원을 하지 않는다.
* 성능 측정은 5회 준비 실행 뒤 30회 표본의 중앙값을 사용하고 시간 자체를 시험 실패 조건으로 만들지 않는다.
* 측정 중앙값이 16 ms 이하인 경로에는 캐시나 작업자 전달 필드를 추가하지 않는다.
* 새 외부 의존성, 비동기 런타임, 설정, GitHub Actions와 원격 CI를 추가하지 않는다.
* 프로덕션 미완성 주석, 디버깅 출력과 입력·결과 전문 로그를 남기지 않는다.
* 관련 PRD, 승인 설계와 README는 해당 코드·시험과 같은 논리적 커밋에서 현행화한다.
* 커밋은 기존 `git config` 사용자로 작성하고 Conventional Commits 한국어 명사형 제목을 사용한다.

## File Map

```text
src/cli.rs
    CLI 단계 수를 입력 읽기 전에 검증하고 회귀 시험을 보관한다.

src/transforms/base64.rs
    허용 공백이 있을 때만 압축 버퍼를 만들고 원본 오류 위치를 유지한다.

src/tui/views.rs
    Text View의 그래핌·폭·행·4 KiB 예산과 UTF-8 측정을 담당한다.

src/tui/worker.rs
    결과 채널의 정상 결과, Empty와 Disconnected를 그대로 전달한다.

src/tui.rs
    Disconnected를 안전한 AppError로 바꾸고 기존 터미널 복구 경로를 사용한다.

src/tui/state.rs
    최대 입력 편집 측정을 보관하고, 기준 초과 때만 입력 통계를 캐시한다.

tests/shell-smoke.sh
    Bash·Zsh PTY와 운영체제별 실제 복사를 검증하되 macOS 원문은 복원하지 않는다.

README.md
    현재 사용법·검증 명령·최신 결과 한 벌만 보관한다.

docs/prd/init-prd.md
    단계 수 조기 검증, Text 표시와 작업자 종료의 현재 계약을 기록한다.

docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
    Text 자동 줄바꿈, 작업자 종료와 복원 없는 macOS Smoke를 현행화한다.

docs/superpowers/specs/2026-07-31-doop-maintenance-design.md
    측정 결과와 최종 구현 상태를 기록한다.

docs/superpowers/plans/*.md
    완료된 과거 계획 다섯 개를 삭제하고 현재 계획만 유지한다.
```

---

### Task 1: CLI 단계 수를 입력 읽기 전에 거부

**Files:**
- Modify: `src/cli.rs:95-120`
- Modify: `src/cli.rs:233-523`
- Modify: `docs/prd/init-prd.md:195-205`

**Interfaces:**
- Consumes: `crate::MAX_STEPS`, `AppError::Pipeline`, `PipelineError::TooManySteps`
- Produces: 변경 없는 `run_transform(...) -> Result<(), AppError>`와 입력 읽기 전 32단계 경계

- [x] **Step 1: 입력을 읽으면 실패하는 회귀 시험 작성**

`src/cli.rs` 시험 모듈에 다음 Reader와 시험을 추가한다.

```rust
struct PanicReader;

impl std::io::Read for PanicReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("step validation must happen before input is read");
    }
}

#[test]
fn too_many_steps_does_not_read_input() {
    let first = crate::transforms::transform_by_id("base64-encode").unwrap();
    let then = vec![first; crate::MAX_STEPS];
    let mut stdin = PanicReader;
    let mut stdout = Vec::new();

    let error = run_transform(
        first,
        &then,
        None,
        &mut stdin,
        false,
        &mut stdout,
        false,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        AppError::Pipeline(crate::error::PipelineError::TooManySteps {
            max: crate::MAX_STEPS
        })
    ));
    assert!(stdout.is_empty());
}
```

- [x] **Step 2: 회귀 시험이 현재 입력 읽기 패닉으로 실패하는지 확인**

Run:

```bash
cargo test --lib cli::tests::too_many_steps_does_not_read_input -- --exact
```

Expected: FAIL with `step validation must happen before input is read`.

- [x] **Step 3: `run_transform` 시작에서 단계 수를 검증**

`then.len() + 1` 산술을 만들지 말고 다음 경계 검사를 `read_input`보다 앞에 둔다.

```rust
if then.len() >= crate::MAX_STEPS {
    return Err(AppError::Pipeline(
        crate::error::PipelineError::TooManySteps {
            max: crate::MAX_STEPS,
        },
    ));
}
```

Pipeline의 `execute_report` 안에 있는 기존 `steps.len() > MAX_STEPS` 검사는 제거하지 않는다.

- [x] **Step 4: PRD에 오류 우선순위를 기록**

`docs/prd/init-prd.md`의 임시 체인 규칙에서 최대 32단계 바로 뒤에 다음 계약을 추가한다.

```markdown
* CLI는 첫 변환과 `--then` 합계를 입력 파일이나 표준 입력보다 먼저 검증한다.
* 단계 초과와 입력 오류가 동시에 있으면 단계 초과 오류가 우선한다.
```

- [x] **Step 5: 집중 시험과 전체 CLI 시험 실행**

Run:

```bash
cargo test --lib cli::tests::too_many_steps_does_not_read_input -- --exact
cargo test --lib cli::tests
cargo test --test cli
```

Expected: 모두 PASS.

- [x] **Step 6: 변경 검토와 커밋**

Run:

```bash
git diff --check
git diff -- src/cli.rs docs/prd/init-prd.md
git add src/cli.rs docs/prd/init-prd.md
git commit -m "fix(cli): 단계 수 입력 전 검증"
```

---

### Task 2: 공백 없는 Base64 입력의 중간 복사 제거

**Files:**
- Modify: `src/transforms/base64.rs:1-80`
- Test: `src/transforms/base64.rs:82-149`

**Interfaces:**
- Consumes: `base64::Engine::decode_slice`, 기존 네 종류 ASCII 공백 규칙
- Produces: `compact_input(input: &[u8]) -> Cow<'_, [u8]>`; 공개 `decode` 서명과 오류는 불변

- [x] **Step 1: Borrowed와 Owned 경로를 구분하는 실패 시험 작성**

`base64.rs`에 `std::borrow::Cow`를 가져오고 시험 모듈에 다음 시험을 먼저 추가한다. 이 시점에는 `compact_input`이 없어 컴파일이 실패해야 한다.

```rust
#[test]
fn borrows_plain_input_and_owns_only_whitespace_compaction() {
    let plain = b"Zm9v";
    assert!(matches!(
        compact_input(plain),
        Cow::Borrowed(value) if value == plain
    ));
    assert!(matches!(
        compact_input(b" Zm9v\t\r\n"),
        Cow::Owned(value) if value == b"Zm9v"
    ));
}
```

- [x] **Step 2: 새 시험의 컴파일 실패 확인**

Run:

```bash
cargo test --lib transforms::base64::tests::borrows_plain_input_and_owns_only_whitespace_compaction -- --exact
```

Expected: FAIL because `compact_input` is not defined.

- [x] **Step 3: 조건부 압축 함수를 최소 구현**

파일 상단과 `original_offset` 앞에 다음 코드를 둔다.

```rust
use std::borrow::Cow;

fn is_ignored(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn compact_input(input: &[u8]) -> Cow<'_, [u8]> {
    if input.iter().copied().any(is_ignored) {
        Cow::Owned(
            input
                .iter()
                .copied()
                .filter(|byte| !is_ignored(*byte))
                .collect(),
        )
    } else {
        Cow::Borrowed(input)
    }
}
```

`original_offset`도 같은 `is_ignored`를 사용한다.

```rust
.filter(|(_, byte)| !is_ignored(**byte))
```

`decode`에서는 기존 무조건 `Vec` 수집을 다음으로 바꾸고 나머지 길이·패딩·오류 처리는 유지한다.

```rust
let compact = compact_input(input);
```

`decode_slice`에는 `compact.as_ref()`를 전달한다.

- [x] **Step 4: Borrowed 경로와 기존 Base64 계약 검증**

Run:

```bash
cargo test --lib transforms::base64::tests
cargo test --lib pipeline::tests
cargo test --test cli decoders_report_the_same_bounded_invalid_utf8_details
```

Expected: 모두 PASS. 공백 포함 오류 위치도 원본 입력 기준으로 유지된다.

- [x] **Step 5: 형식과 정적 검사 후 커밋**

Run:

```bash
cargo fmt --check
cargo clippy --lib -- -D warnings
git diff --check
git add src/transforms/base64.rs
git commit -m "refactor(base64): 무공백 입력 복사 제거"
```

---

### Task 3: Text View의 긴 단일 행 자동 줄바꿈

**Files:**
- Modify: `src/tui/views.rs:132-233`
- Test: `src/tui/views.rs:452-598`
- Modify: `docs/prd/init-prd.md:583-590`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:283-288`

**Interfaces:**
- Consumes: `Artifact`, `TextWindow`, Unicode 그래핌·표시 폭과 `VISIBLE_TEXT_BYTE_BUDGET`
- Produces: 원본을 바꾸지 않는 표시용 soft wrap과 마지막 완전 표시 그래핌 다음 `next_offset`

- [x] **Step 1: ASCII·넓은 문자·escape 줄바꿈 실패 시험 작성**

`views.rs` 시험 모듈에 다음 세 시험을 추가한다.

```rust
#[test]
fn text_window_soft_wraps_across_all_visible_rows() {
    let artifact = Artifact::new(b"abcdefgh".to_vec());

    let full = render_text_window(&artifact, 0, 3, 3);
    assert_eq!(full.text, "abc\ndef\ngh");
    assert_eq!(full.next_offset, 8);

    let first = render_text_window(&artifact, 0, 1, 3);
    assert_eq!(first.text, "abc");
    assert_eq!(first.next_offset, 3);

    let second = render_text_window(&artifact, first.next_offset, 1, 3);
    assert_eq!(second.text, "def");
    assert_eq!(second.next_offset, 6);
}

#[test]
fn text_window_soft_wraps_wide_graphemes_by_display_width() {
    let artifact = Artifact::new("界界界".as_bytes().to_vec());

    let window = render_text_window(&artifact, 0, 2, 4);

    assert_eq!(window.text, "界界\n界");
    assert_eq!(window.next_offset, artifact.bytes().len());
}

#[test]
fn text_window_wraps_escaped_controls_without_changing_source() {
    let source = b"a\x1bb".to_vec();
    let artifact = Artifact::new(source.clone());

    let window = render_text_window(&artifact, 0, 2, 4);

    assert_eq!(window.text, "a\n\\x1b");
    assert_eq!(window.next_offset, 2);
    assert_eq!(artifact.bytes(), source.as_slice());
}
```

- [x] **Step 2: 현재 첫 행에서 중단되어 시험이 실패하는지 확인**

Run:

```bash
cargo test --lib tui::views::tests::text_window_soft_wraps -- --nocapture
```

Expected: FAIL because the current output contains only the first visual row.

- [x] **Step 3: 그래핌을 소비하기 전에 표시용 줄바꿈 적용**

`render_text_window`의 일반 그래핌 분기에서 기존 `used_width + rendered.width() > columns` 즉시 중단을 다음 흐름으로 바꾼다.

```rust
let rendered_width = rendered.width();
if output.len() + rendered.len() > VISIBLE_TEXT_BYTE_BUDGET {
    fallback = Some((start + relative + grapheme.len(), false));
    break;
}
if rendered_width > columns {
    fallback = Some((start + relative + grapheme.len(), false));
    break;
}
if used_width + rendered_width > columns {
    if row + 1 >= rows
        || output.len() + 1 + rendered.len() > VISIBLE_TEXT_BYTE_BUDGET
    {
        fallback = Some((start + relative, false));
        break;
    }
    output.push('\n');
    row += 1;
    used_width = 0;
}
output.push_str(rendered);
cursor = start + relative + grapheme.len();
used_width += rendered_width;
```

기존 실제 `LF`, `CRLF`, 잘린 UTF-8 조각, 첫 그래핌 전진 보장과 4 KiB 상한 분기는 유지한다. 행이나 예산 부족으로 표시하지 못한 정상 그래핌은 `cursor`를 전진시키지 않는다.

- [x] **Step 4: PRD와 TUI 설계 현행화**

`docs/prd/init-prd.md`의 Text View 규칙과 TUI 설계의 `6.2 Text`에 다음 의미를 기록한다.

```markdown
줄바꿈 없는 긴 행은 그래핌과 표시 폭을 기준으로 Viewport 안에서 자동 줄바꿈한다. 이 줄바꿈은 화면 표시 전용이며 원본 Artifact와 클립보드 내용에는 추가하지 않는다.
```

- [x] **Step 5: Text 경계와 전체 TUI 시험 실행**

Run:

```bash
cargo test --lib tui::views::tests
cargo test --lib tui::state::tests::text_pages_stay_bounded_and_long_graphemes_render_without_scalar_crawl -- --exact
cargo test --lib tui::render::tests
```

Expected: 모두 PASS. 기존 제어문자 비활성화와 4 KiB 예산 시험도 유지된다.

- [x] **Step 6: 형식·정적 검사와 커밋**

Run:

```bash
cargo fmt --check
cargo clippy --lib -- -D warnings
git diff --check
git add src/tui/views.rs docs/prd/init-prd.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "fix(tui): 긴 Text 결과 자동 줄바꿈"
```

---

### Task 4: 작업자 채널 종료 감지와 안전한 TUI 종료

**Files:**
- Modify: `src/tui/worker.rs:40-130`
- Test: `src/tui/worker.rs:132-455`
- Modify: `src/tui.rs:130-176`
- Modify: `docs/prd/init-prd.md:567-575,1146-1153`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:175-181,420-427`

**Interfaces:**
- Consumes: `mpsc::Receiver<PreviewResult>::try_recv`
- Produces: `PreviewWorker::try_recv(&self) -> Result<PreviewResult, mpsc::TryRecvError>`

- [x] **Step 1: Empty와 Disconnected 보존 실패 시험 작성**

`worker.rs` 시험 모듈에 다음 시험을 추가한다.

```rust
#[test]
fn try_recv_distinguishes_empty_from_disconnected() {
    let live = PreviewWorker::new();
    assert!(matches!(live.try_recv(), Err(mpsc::TryRecvError::Empty)));

    let shared = Arc::new(WorkerShared {
        state: Mutex::new(WorkerState {
            pending: None,
            shutdown: false,
        }),
        pending_changed: Condvar::new(),
        latest_request_id: AtomicU64::new(0),
    });
    let (sender, results) = mpsc::channel();
    drop(sender);
    let disconnected = PreviewWorker { shared, results };

    assert!(matches!(
        disconnected.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
}
```

- [x] **Step 2: 현재 `Option` 반환 때문에 시험이 실패하는지 확인**

Run:

```bash
cargo test --lib tui::worker::tests::try_recv_distinguishes_empty_from_disconnected -- --exact
```

Expected: FAIL with a type mismatch between `Option` and `Result`.

- [x] **Step 3: 작업자 수신 결과를 손실 없이 반환**

`PreviewWorker::try_recv`를 다음으로 바꾼다.

```rust
pub(super) fn try_recv(&self) -> Result<PreviewResult, mpsc::TryRecvError> {
    self.results.try_recv()
}
```

- [x] **Step 4: 이벤트 루프에서 세 채널 상태를 명시적으로 처리**

`src/tui.rs`의 `while let Some`을 다음 루프로 교체한다.

```rust
loop {
    match worker.try_recv() {
        Ok(result) => effects.extend(app.handle_event(AppEvent::PreviewFinished(result))),
        Err(std::sync::mpsc::TryRecvError::Empty) => break,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            return Err(AppError::Tui(
                "Preview worker stopped unexpectedly".to_string(),
            ));
        }
    }
}
```

오류 반환 뒤에는 `Tick`, Submit, Copy를 처리하지 않는다. `tui::run`의 기존 정리 경로와 `AppError::Tui` 종료 코드 1을 그대로 사용한다.

- [x] **Step 5: PRD와 TUI 후속 대장에서 구현 상태 반영**

`docs/prd/init-prd.md` 작업자 규칙과 TUI 설계 작업자 절에 다음 계약을 추가한다.

```markdown
결과 채널이 종료되면 작업자를 자동 재시작하지 않고 안전한 TUI 오류로 종료하며 기존 터미널 복구 경로를 실행한다.
```

두 문서의 후속 아이디어에서 `Empty`·`Disconnected` 구분 항목은 제거한다.

- [x] **Step 6: 작업자·TUI 회귀 시험 실행**

Run:

```bash
cargo test --lib tui::worker::tests
cargo test --lib tui::tests
cargo test --test cli tui_has_explicit_temporary_code_one_path
```

Expected: 모두 PASS. Drop은 대기하지 않고 기존 취소·최신 요청 동작도 유지된다.

- [x] **Step 7: 형식·정적 검사와 커밋**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
git add src/tui.rs src/tui/worker.rs docs/prd/init-prd.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "fix(tui): 작업자 종료 감지"
```

---

### Task 5: macOS 클립보드 Smoke에서 복원 제거

**Files:**
- Modify: `tests/shell-smoke.sh:1-130,250-400,440-455,592-665`
- Modify: `README.md:71-278`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:385-395`

**Interfaces:**
- Consumes: TUI `Copy as Hex`, `pbpaste`, 기존 `read_clipboard macos`
- Produces: `DOOP_SMOKE_CLIPBOARD_MODE=macos`가 `ff`를 검증하고 그대로 남기는 로컬 시험

- [x] **Step 1: 현재 복원 동작이 새 완료 조건과 다른지 실제 macOS에서 확인**

현재 클립보드를 시험용 일반 문자열로 바꾸고 기존 Smoke를 실행한다.

```bash
printf 'doop-before-smoke' | pbcopy
DOOP_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
test "$(pbpaste)" = ff
```

Expected: 마지막 `test`가 FAIL. 기존 스크립트는 `doop-before-smoke`를 복원한다.

- [x] **Step 2: 복원 전용 셸 상태와 cleanup 분기 제거**

`tests/shell-smoke.sh`에서 다음 변수를 삭제한다.

```text
smoke_clipboard_backup
smoke_clipboard_verify
smoke_clipboard_initial_count
smoke_clipboard_owned_count
smoke_clipboard_backed_up
smoke_preserve_tmp
```

셸 함수 `macos_change_count`와 `cleanup`의 백업·소유권·복원 전체 분기를 삭제한다. `cleanup`은 일반 임시 입력·출력·오류·기대값 파일만 삭제하고 원래 종료 상태를 반환한다.

- [x] **Step 3: Expect의 changeCount와 소유권 검사를 제거**

Tcl 코드에서 다음 프로시저와 호출을 삭제한다.

```text
macos_change_count
macos_owned_transition
verify_macos_owned_transition
```

macOS 분기는 복사 성공 화면을 확인한 뒤 기존 `read_clipboard macos 124`로 값을 읽고 `DOOP_SMOKE_CLIPBOARD_EXPECTED`와 비교한다.

```tcl
} elseif {$mode eq "macos"} {
    expect {
        -exact "ied as Hex" {}
        -exact "Clipboard" {
            expect_exact "unavailable" 123 123
            exit 123
        }
        eof { exit 127 }
        timeout { exit 128 }
    }
    set copied [read_clipboard macos 124]
    if {$copied ne $env(DOOP_SMOKE_CLIPBOARD_EXPECTED)} {
        exit 125
    }
}
```

`DOOP_SMOKE_CLIPBOARD_INITIAL_COUNT`와 `DOOP_SMOKE_CLIPBOARD_OWNED_COUNT` 환경변수도 제거한다.

- [x] **Step 4: macOS 실행 분기를 복사값 검증만 남기도록 축소**

macOS case에서 `osascript`, `pbcopy`, pasteboard 형식 사전 검사, 백업 파일과 종료 코드 148~154 처리를 제거한다. `pbpaste` 존재만 확인하고 다음 순서를 유지한다.

```bash
command -v pbpaste >/dev/null 2>&1 || fail "pbpaste command is required"
set +e
tui_run macos
exit_status=$?
set -e
case "$exit_status" in
    123) fail "macOS clipboard product copy reported unavailable" ;;
    124) fail "macOS clipboard verification failed: pbpaste could not read copied text" ;;
    125) fail "macOS clipboard product copy did not match expected text" ;;
esac
assert_eq '130' "$exit_status" "macOS clipboard path"
```

- [x] **Step 5: 현재 문서에서 복원 필수 규약 제거**

README의 최신 macOS 검증 설명과 TUI 설계의 플랫폼 시험 규칙을 다음 의미로 바꾼다.

```markdown
macOS 실제 클립보드 Smoke는 제품 복사 뒤 `pbpaste`로 소문자 `ff`를 확인한다. 시험 전 내용을 백업하거나 시험 뒤 복원하지 않는다.
```

과거 검증 전문 자체는 Task 7에서 삭제한다.

- [x] **Step 6: 셸 문법·기본 경로와 실제 macOS 경로 검증**

Run:

```bash
bash -n tests/shell-smoke.sh
zsh -n tests/shell-smoke.sh
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
test "$(pbpaste)" = ff
DOOP_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
test "$(pbpaste)" = ff
```

Expected: 모두 PASS. 마지막 클립보드 값은 `ff`다.

- [x] **Step 7: 복원 코드 부재 확인과 커밋**

Run:

```bash
rg --color=never -n "changeCount|clipboard_backup|clipboard_owned|restore macOS clipboard|원문 복원" tests/shell-smoke.sh README.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
```

Expected: 복원 요구나 구현 match 없음. 제품의 일반 터미널 복구 문구는 검색 대상이 아니다.

Run:

```bash
git diff --check
git add tests/shell-smoke.sh README.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "test(clipboard): 복원 없는 macOS 검증"
```

---

### Task 6: TUI 경계 성능 측정과 16 ms 조건부 최적화

**Files:**
- Modify: `src/tui/state.rs:1090-2965`
- Modify: `src/tui/views.rs:410-695`
- Conditional Modify: `src/tui/state.rs:150-345,740-795`
- Conditional Modify: `src/tui/worker.rs:25-90`
- Conditional Modify: `src/tui/views.rs:8-31`
- Conditional Modify: `src/tui/render.rs:900-920,1760-1785`
- Modify: `README.md:71-90`
- Modify: `docs/superpowers/specs/2026-07-31-doop-maintenance-design.md:115-134`

**Interfaces:**
- Produces: ignored release measurements `max_input_edit_release_measurement`, `utf8_validation_release_measurement`
- Conditional Produces: `InputMetrics` cache 또는 `PreviewResult::new(report)`의 작업자 UTF-8 판정

- [x] **Step 1: 최대 입력 편집 릴리스 측정 추가**

`state.rs` 시험 모듈에 다음 측정을 추가한다.

```rust
#[test]
#[ignore = "release-only maximum input edit measurement"]
fn max_input_edit_release_measurement() {
    const WARMUPS: usize = 5;
    const SAMPLES: usize = 30;

    let mut input = "\n".repeat(TUI_INPUT_LINE_LIMIT - 1);
    input.push_str(&"a".repeat(TUI_INPUT_LIMIT - 1 - input.len()));

    let measure = || {
        let event_time = now();
        let mut app = App::new(event_time, true);
        app.handle_event(AppEvent::Paste(input.clone(), event_time));
        assert_eq!(app.input_len(), TUI_INPUT_LIMIT - 1);

        let started = Instant::now();
        std::hint::black_box(key(
            &mut app,
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            event_time,
        ));
        let elapsed = started.elapsed();
        assert_eq!(app.input_len(), TUI_INPUT_LIMIT);
        elapsed
    };

    for _ in 0..WARMUPS {
        std::hint::black_box(measure());
    }
    let mut samples = (0..SAMPLES).map(|_| measure()).collect::<Vec<_>>();
    samples.sort_unstable();
    eprintln!(
        "max input edit release measurement: warmups={WARMUPS}, samples={SAMPLES}, min={:?}, median={:?}, max={:?}",
        samples[0],
        samples[SAMPLES / 2],
        samples[SAMPLES - 1]
    );
}
```

- [x] **Step 2: 64 MiB UTF-8 판정 릴리스 측정 추가**

`views.rs` 시험 모듈에 다음 측정을 추가한다.

```rust
#[test]
#[ignore = "release-only UTF-8 validation measurement"]
fn utf8_validation_release_measurement() {
    const WARMUPS: usize = 5;
    const SAMPLES: usize = 30;

    let bytes = vec![b'a'; crate::TUI_OUTPUT_LIMIT];
    let measure = || {
        let started = std::time::Instant::now();
        assert!(std::str::from_utf8(std::hint::black_box(bytes.as_slice())).is_ok());
        started.elapsed()
    };

    for _ in 0..WARMUPS {
        std::hint::black_box(measure());
    }
    let mut samples = (0..SAMPLES).map(|_| measure()).collect::<Vec<_>>();
    samples.sort_unstable();
    eprintln!(
        "UTF-8 validation release measurement: warmups={WARMUPS}, samples={SAMPLES}, min={:?}, median={:?}, max={:?}",
        samples[0],
        samples[SAMPLES / 2],
        samples[SAMPLES - 1]
    );
}
```

- [x] **Step 3: 두 측정을 릴리스로 실행하고 중앙값 판정**

Run:

```bash
cargo test --release max_input_edit_release_measurement -- --ignored --nocapture
cargo test --release utf8_validation_release_measurement -- --ignored --nocapture
```

Expected: 두 시험 PASS, 각 출력에 `warmups=5`, `samples=30`, `median=`이 존재한다.

판정:

* 중앙값이 각각 16 ms 이하이면 Steps 4~5의 해당 조건부 코드를 추가하지 않는다.
* 최대 입력 편집 중앙값만 16 ms를 초과하면 Step 4만 수행한다.
* UTF-8 판정 중앙값만 16 ms를 초과하면 Step 5만 수행한다.
* 둘 다 초과하면 Steps 4~5를 모두 수행한다.

- [ ] **Step 4: 입력 편집이 16 ms를 초과한 경우에만 일반 삽입 통계 캐시**

먼저 `state.rs`에 다음 상태와 계산 함수를 추가한다.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InputMetrics {
    bytes: usize,
    lines: usize,
}

fn measured_input(lines: &[String]) -> InputMetrics {
    InputMetrics {
        bytes: lines.iter().map(String::len).sum::<usize>()
            + lines.len().saturating_sub(1),
        lines: lines.len(),
    }
}
```

`App`에 `input_metrics: InputMetrics`를 추가하고 기본값을 `bytes: 0, lines: 1`로 둔다. `input_len`은 `self.input_metrics.bytes`를 반환한다.

기존 `can_insert` 계산을 다음 반환형으로 바꾼다.

```rust
fn projected_input_metrics(&self, bytes: usize, lines: usize) -> Option<InputMetrics> {
    let retained_bytes = self
        .input_metrics
        .bytes
        .saturating_sub(self.selected_input_len());
    let retained_lines = self
        .input_metrics
        .lines
        .saturating_sub(self.selected_line_count());
    let projected = InputMetrics {
        bytes: retained_bytes.checked_add(bytes)?,
        lines: retained_lines.checked_add(lines)?,
    };
    (projected.bytes <= self.input_limit && projected.lines <= self.input_line_limit)
        .then_some(projected)
}
```

Paste, 일반 문자·Enter와 yank는 편집 전에 `projected_input_metrics`를 구한다. 편집이 성공하면 계산한 값을 `self.input_metrics`에 대입한 뒤 `changed`를 호출한다. 삭제, undo와 redo처럼 사전 증가량이 없는 성공 편집은 다음 한 줄로 실제 editor 상태와 동기화한 뒤 `changed`를 호출한다.

```rust
self.input_metrics = measured_input(self.textarea.lines());
```

거부된 입력과 커서·선택 이동은 캐시를 바꾸지 않는다. 기존 입력 한도 시험에 다음 검증 도우미를 추가해 paste, 선택 교체, Backspace, undo와 redo 뒤 캐시가 실제 문서와 같은지 확인한다.

```rust
fn assert_input_metrics(app: &App) {
    assert_eq!(app.input_metrics, measured_input(app.textarea.lines()));
}

#[test]
fn cached_input_metrics_follow_replacement_deletion_and_undo() {
    let start = now();
    let mut app = App::new(start, true);

    app.handle_event(AppEvent::Paste("a\n界".to_string(), start));
    assert_input_metrics(&app);

    app.textarea.select_all();
    key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE, start);
    assert_eq!(app.input_text(), "x");
    assert_input_metrics(&app);

    key(&mut app, KeyCode::Backspace, KeyModifiers::NONE, start);
    assert_eq!(app.input_text(), "");
    assert_input_metrics(&app);

    key(
        &mut app,
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
        start,
    );
    assert_eq!(app.input_text(), "x");
    assert_input_metrics(&app);
}
```

Run:

```bash
cargo test --lib tui::state::tests
cargo test --release max_input_edit_release_measurement -- --ignored --nocapture
```

Expected: 상태 시험 PASS. 새 중앙값을 문서에 기록하며 16 ms 초과 여부와 관계없이 추가 캐시는 다시 제거하지 않는다.

- [ ] **Step 5: UTF-8 판정이 16 ms를 초과한 경우에만 작업자에서 판정**

`worker.rs`의 결과에 작업자 계산 값을 추가한다.

```rust
pub(super) struct PreviewResult {
    pub(super) report: ExecutionReport,
    pub(super) output_is_utf8: Option<bool>,
}

impl PreviewResult {
    pub(super) fn new(report: ExecutionReport) -> Self {
        let output_is_utf8 = match &report.outcome {
            ExecutionOutcome::Success(bytes) => Some(std::str::from_utf8(bytes).is_ok()),
            ExecutionOutcome::Failed(_) | ExecutionOutcome::Cancelled => None,
        };
        Self {
            report,
            output_is_utf8,
        }
    }
}
```

`worker.rs`의 Pipeline import에 `ExecutionOutcome`을 추가하고 작업자 전송은 `PreviewResult::new(report)`를 사용한다. 다음 시험으로 텍스트, 바이너리와 실패 보고서의 전달 값을 고정한다.

```rust
#[test]
fn preview_result_carries_success_utf8_validity_only() {
    let text = PreviewResult::new(ExecutionReport {
        request_id: 1,
        target: ExecutionTarget::Final,
        outcome: ExecutionOutcome::Success(b"text".to_vec()),
        traces: Vec::new(),
    });
    assert_eq!(text.output_is_utf8, Some(true));

    let binary = PreviewResult::new(ExecutionReport {
        request_id: 2,
        target: ExecutionTarget::Final,
        outcome: ExecutionOutcome::Success(vec![0xff]),
        traces: Vec::new(),
    });
    assert_eq!(binary.output_is_utf8, Some(false));

    let failed = PreviewResult::new(ExecutionReport {
        request_id: 3,
        target: ExecutionTarget::Final,
        outcome: ExecutionOutcome::Failed(crate::error::PipelineError::TooManySteps {
            max: crate::MAX_STEPS,
        }),
        traces: Vec::new(),
    });
    assert_eq!(failed.output_is_utf8, None);
}
```

`views.rs`의 `Artifact`에는 검증된 생성자를 추가한다.

```rust
pub(super) fn new_validated(bytes: Vec<u8>, is_utf8: bool) -> Self {
    Self {
        bytes: Arc::from(bytes),
        is_utf8,
    }
}
```

기존 `Artifact::new`는 시험과 직접 생성 경로를 위해 내부 UTF-8 판정을 유지한다. `state.rs::finish_preview`는 `PreviewResult { report, output_is_utf8 }`를 분해하고 성공 결과에서 다음 불변식을 사용한다.

```rust
let artifact = Artifact::new_validated(
    bytes,
    output_is_utf8.expect("successful preview must include UTF-8 validity"),
);
```

`state.rs`와 `render.rs`의 시험용 `PreviewResult` 리터럴은 모두 `PreviewResult::new(ExecutionReport { ... })`로 바꾼다.

Run:

```bash
cargo test --lib tui::worker::tests
cargo test --lib tui::state::tests
cargo test --lib tui::render::tests
cargo test --release utf8_validation_release_measurement -- --ignored --nocapture
```

Expected: 모두 PASS. UI `finish_preview`에서 64 MiB 전수 UTF-8 판정을 다시 하지 않는다.

- [x] **Step 6: 측정값과 조건부 적용 결과 문서화**

README 로컬 검증 절과 안정화 설계 `# 8`에 두 측정 명령, 출력된 중앙값, 각 경로의 `최적화 적용` 또는 `현재 구현 유지` 결정을 정확히 기록한다. 시간 숫자를 성공 기준으로 표현하지 않는다.

- [x] **Step 7: 형식·정적 검사와 조건에 맞는 커밋**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
git diff --check
```

조건부 프로덕션 최적화가 하나라도 적용되었으면:

```bash
git add src/tui/state.rs src/tui/views.rs src/tui/worker.rs src/tui/render.rs README.md docs/superpowers/specs/2026-07-31-doop-maintenance-design.md
git commit -m "perf(tui): 측정 기반 경로 최적화"
```

두 중앙값이 모두 16 ms 이하이면:

```bash
git add src/tui/state.rs src/tui/views.rs README.md docs/superpowers/specs/2026-07-31-doop-maintenance-design.md
git commit -m "test(perf): TUI 경계 측정 추가"
```

---

### Task 7: 현재 문서만 남기고 완료 계획·검증 전문 정리

**Files:**
- Modify: `README.md:71-278`
- Delete: `docs/superpowers/plans/2026-07-29-doop-v0.1.md`
- Delete: `docs/superpowers/plans/2026-07-29-doop-v0.2-hex.md`
- Delete: `docs/superpowers/plans/2026-07-30-doop-full-audit-refactor.md`
- Delete: `docs/superpowers/plans/2026-07-30-doop-v0.1-hardening.md`
- Delete: `docs/superpowers/plans/2026-07-31-doop-tui-workbench.md`
- Preserve: `docs/superpowers/plans/2026-07-31-doop-maintenance.md`

**Interfaces:**
- Consumes: Task 5 클립보드 결과와 Task 6 실제 측정 출력
- Produces: README 사용법·현재 검증 명령·최신 결과 한 벌과 현재 구현 계획만 남는 문서 트리

- [x] **Step 1: 삭제 전 현재 문서의 중복 범위 확인**

Run:

```bash
rg --color=never -n "^### 2026-|검증 기준 코드|docker run|changeCount|원문 복원" README.md docs/superpowers/plans
```

Expected: README의 세 과거 검증 절과 완료 계획의 실행 전문이 검색된다.

- [x] **Step 2: README의 과거 세대별 검증 전문을 최신 요약 하나로 교체**

README의 `## 로컬 검증` 명령은 유지하고, `2026-07-30 v0.1`, `2026-07-30 v0.2`, `2026-07-31 TUI 작업판` 절 전체를 삭제한다.

그 자리에 `### 최신 로컬 검증 요약` 하나를 두고 다음 사실만 기록한다.

```markdown
### 최신 로컬 검증 요약

현재 `main`에서 형식, 경고 금지 Clippy, 전체 단위·CLI 시험, rustdoc,
패키징, 오프라인 잠금 설치, Bash·Zsh PTY를 검증한다. macOS 실제 복사는
`pbpaste`로 소문자 `ff`를 확인하며 이전 클립보드 내용을 복원하지 않는다.
Linux의 미지원·X11과 Wayland 경로는 사용할 수 있는 로컬 환경에서 별도로
실행하고, 실행하지 못한 환경은 미검증으로 명시한다.

릴리스 측정은 렌더링, 최대 입력 편집과 64 MiB UTF-8 판정의 실제 값만
기록하며 시간 자체를 시험 성공 기준으로 사용하지 않는다.
```

Task 6에서 얻은 정확한 중앙값과 적용 결정을 이 절 마지막에 한 문단으로 추가한다.

- [x] **Step 3: 완료된 과거 구현 계획 다섯 개 삭제**

`apply_patch`로 다음 파일만 삭제한다.

```text
docs/superpowers/plans/2026-07-29-doop-v0.1.md
docs/superpowers/plans/2026-07-29-doop-v0.2-hex.md
docs/superpowers/plans/2026-07-30-doop-full-audit-refactor.md
docs/superpowers/plans/2026-07-30-doop-v0.1-hardening.md
docs/superpowers/plans/2026-07-31-doop-tui-workbench.md
```

현재 파일 `docs/superpowers/plans/2026-07-31-doop-maintenance.md`와 `docs/superpowers/specs`는 삭제하지 않는다.

- [x] **Step 4: 현행 문서와 후속 아이디어가 남았는지 확인**

Run:

```bash
lsd --color=never --icon=never docs/superpowers/plans docs/superpowers/specs docs/prd
rg --color=never -n "후속 작업 대장|파일 열기|Output 검색|플러그인" docs/prd/init-prd.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
rg --color=never -n "^### 2026-|검증 기준 코드|docker run|changeCount|원문 복원" README.md
```

Expected:

* plans에는 현재 maintenance 계획만 존재한다.
* specs와 PRD 및 후속 아이디어는 존재한다.
* README에는 과거 검증 전문과 복원 문구가 없다.

- [x] **Step 5: 문서 차이와 링크 확인 후 커밋**

Run:

```bash
git diff --check
rg --color=never -n "docs/superpowers/plans/2026-07-(29|30).*\\.md|2026-07-31-doop-tui-workbench\\.md" README.md docs/prd docs/superpowers/specs
```

Expected: 삭제한 계획을 현재 문서가 필수 자료로 참조하지 않는다.

Run:

```bash
git add README.md docs/superpowers/plans
git commit -m "docs(project): 완료 기록 현행화"
```

---

### Task 8: 전체 로컬 검증, 문서 완료 상태와 CCC 인덱스

**Files:**
- Modify: `docs/superpowers/specs/2026-07-31-doop-maintenance-design.md:1-8,207-222`
- Modify if results changed: `README.md:71-110`
- Modify: `docs/superpowers/plans/2026-07-31-doop-maintenance.md` checkboxes

**Interfaces:**
- Consumes: Tasks 1~7의 커밋과 실제 로컬 검증 출력
- Produces: 구현 완료 설계, 재현 가능한 최신 검증 요약, 최신 CCC 인덱스와 깨끗한 `main`

- [x] **Step 1: 형식, 정적 검사와 전체 시험**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
```

Expected: 모두 exit 0. 일반 시험에서 릴리스 전용 ignored 측정만 제외된다.

- [x] **Step 2: 모든 릴리스 측정 실행**

Run:

```bash
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
cargo test --release max_input_edit_release_measurement -- --ignored --nocapture
cargo test --release utf8_validation_release_measurement -- --ignored --nocapture
```

Expected: 세 측정 모두 5회 준비 실행 뒤 30표본의 중앙값을 출력하고 PASS. README와 안정화 설계에 기록한 중앙값·적용 결정이 최종 실행 결과와 일치한다.

- [x] **Step 3: 패키징과 오프라인 잠금 설치**

아직 문서 상태 변경이 남아 있으므로 첫 패키징에는 `--allow-dirty`를 사용한다.

```bash
cargo package --locked --allow-dirty
```

새 임시 설치 루트를 만들고 설치 결과를 실행한다.

```bash
install_root=$(mktemp -d "${TMPDIR:-/tmp}/doop-install.XXXXXX")
cargo install --locked --offline --path . --root "$install_root"
"$install_root/bin/doop" --version
```

Expected: `doop 0.2.0`.

- [x] **Step 4: Bash·Zsh와 실제 macOS 클립보드 Smoke**

Run:

```bash
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
test "$(pbpaste)" = ff
DOOP_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
test "$(pbpaste)" = ff
```

Expected: 모두 PASS. 클립보드는 `ff`로 남는다.

Linux 로컬 환경을 사용할 수 있으면 다음 경로도 실행한다.

```bash
env -u DISPLAY -u WAYLAND_DISPLAY -u XDG_RUNTIME_DIR \
  DOOP_SMOKE_CLIPBOARD_MODE=unavailable bash tests/shell-smoke.sh
xvfb-run -a env DOOP_SMOKE_CLIPBOARD_MODE=x11 zsh tests/shell-smoke.sh
```

Wayland 세션을 사용할 수 있으면 기존 `DOOP_SMOKE_CLIPBOARD_MODE=wayland` 경로를 실행한다. 사용할 수 없는 Linux 또는 Wayland 환경은 README 최신 요약에 미검증으로 남긴다.

- [x] **Step 5: 완료 문서 현행화와 자체 검토**

안정화 설계 상태를 다음으로 바꾼다.

```markdown
* **상태:** 사용자 승인·구현 완료
```

완료 기준 12개가 실제 코드·시험·문서에 대응하는지 확인하고 측정값, 시험 수, 실행한 운영체제 경로를 README 최신 요약에 정확히 반영한다.

Run:

```bash
rg --color=never -n "T[B]D|TO[D]O|FIX[M]E|적절[히] 처리|나중[에] 구현" README.md docs/prd docs/superpowers/specs docs/superpowers/plans/2026-07-31-doop-maintenance.md
rg --color=never -n "changeCount|clipboard_backup|clipboard_owned|원문 복원" tests/shell-smoke.sh README.md docs/prd docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git diff --check
```

Expected: 미완성 표시, 클립보드 복원 구현·현행 규약과 공백 오류가 없다. 후속 작업 대장의 승인된 보류 아이디어는 유지된다.

- [x] **Step 6: 최종 문서 커밋**

Run:

```bash
git add README.md docs/superpowers/specs/2026-07-31-doop-maintenance-design.md docs/superpowers/plans/2026-07-31-doop-maintenance.md
git commit -m "docs(project): 정비 구현 완료 기록"
```

- [x] **Step 7: 깨끗한 커밋에서 패키징 재검증과 CCC 인덱싱**

Run:

```bash
cargo package --locked
ccc index
git status --short --branch
git log -10 --oneline --decorate
```

Expected:

* `cargo package --locked` exit 0
* CCC error 0
* `git status`는 `## main`만 출력
* 최근 로그에 Tasks 1~8의 논리적 커밋이 순서대로 존재

- [x] **Step 8: 최종 전체 차이 리뷰**

설계 커밋 `9ea47c8` 다음부터 현재 HEAD까지 검토한다.

```bash
git diff --stat 9ea47c8..HEAD
git diff --check 9ea47c8..HEAD
git diff 9ea47c8..HEAD -- src tests README.md docs/prd docs/superpowers/specs docs/superpowers/plans
```

확인 항목:

* 공개 CLI와 8개 변환 ID 불변
* Text 표시용 줄바꿈이 원본·복사값과 분리됨
* Disconnected 뒤 작업 제출이나 오래된 결과 반영 없음
* macOS 클립보드 복원 코드 없음, 터미널 복구 유지
* 16 ms 이하 경로에 조건부 캐시·필드가 추가되지 않음
* 완료 계획만 삭제되고 후속 아이디어는 유지됨
* 새 의존성, CI, 디버깅 출력과 프로덕션 미완료 표시 없음
