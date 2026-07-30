# doop TUI Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 승인된 바이트 Pipeline 작업판 설계에 따라 `doop tui`를 고도화하되, 기존 CLI의 명령·출력·오류·종료 코드 계약은 그대로 유지한다.

**Architecture:** 하나의 변환 레지스트리와 공용 Pipeline을 유지하면서 실행 정책만 `StrictText`와 `AllowBinary`로 나눈다. TUI는 원본 바이트 Artifact와 작은 Trace만 보관하고, 단일 작업 스레드에서 최신 요청만 실행한다. 현재 단일 `src/tui.rs`는 터미널 수명, 상태, 작업자, 렌더링과 View의 다섯 책임으로만 분리하며 별도 프레임워크나 추상화 계층은 만들지 않는다.

**Tech Stack:** Rust 1.97.1 stable, Cargo, Ratatui 0.30.2, Crossterm 0.29.0, tui-textarea-2 0.12.1, arboard 3.6.1, 기존 Unicode 라이브러리, Expect, Bash, Zsh.

## Global Constraints

- 기준 설계는 `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md`다.
- 사용자에게 노출되는 변경은 `doop tui`에 한정한다. 직접 실행 CLI 구조, 8개 변환 ID, 도움말, 출력 바이트, 오류 문구와 종료 코드는 바꾸지 않는다.
- 현재 `pipeline::execute(Vec<u8>, &[TransformStep], usize)` 서명과 단계별 UTF-8 성공 조건을 유지한다.
- TUI 입력 1 MiB·65,536줄, 단계 출력 64 MiB, 최대 32단계와 CLI의 기존 한도를 유지한다.
- 새 Cargo 의존성, Tokio, Snapshot 시험 의존성, GitHub Actions와 배포 작업을 추가하지 않는다.
- 중간 단계 바이트를 누적해서 보관하지 않는다. 최종 Artifact와 현재 요청한 단계 Artifact만 보관한다.
- 전체 결과에 비례하는 Text·Hex 표시 문자열이나 줄 인덱스를 만들지 않는다. 한 번의 View 렌더가 읽고 만드는 자료는 최대 4 KiB다.
- 입력·출력·클립보드 본문을 Trace, 사용자 표시 오류와 로그에 노출하지 않는다.
- 기존 터미널 진입·복구, bracketed paste, 대체 화면, raw mode와 패닉 복구 순서를 유지한다.
- 구현은 `main` 브랜치에서 수행하고, 각 논리 변경은 한국어 Conventional Commit으로 따로 커밋한다.
- 코드 변경과 관련 문서는 같은 구현 흐름에서 현행화한다. 현재 저장소에 없는 `AGENTS.md`, `ARCHITECTURE.md`, `DESIGN.md`는 새로 만들지 않는다.

---

### Task 1: 바이트 실행 정책과 단계 보고서

**Files:**

- Modify: `src/pipeline.rs:1-138`
- Modify: `src/error.rs:127-142`
- Modify: `src/transforms/base64.rs:33-82`
- Modify: `src/transforms/url.rs:38-81`
- Modify: `src/transforms/hex.rs:48-88`
- Test: `src/pipeline.rs:54-138`
- Test: `src/transforms/base64.rs:84-173`
- Test: `src/transforms/url.rs:83-153`
- Test: `src/transforms/hex.rs:90-174`
- Test: `tests/cli.rs`

**Interfaces:**

- Keep: `pub fn execute(input: Vec<u8>, steps: &[TransformStep], output_limit: usize) -> Result<Vec<u8>, PipelineError>`
- Add crate-private: `ExecutionPolicy`, `ExecutionTarget`, `ExecutionRequest`, `ExecutionReport`, `ExecutionOutcome`, `StepTrace`, `StepStatus`
- Add crate-private: `execute_report(request, is_cancelled) -> ExecutionReport`
- Consume: owned input bytes, immutable transform steps, output limit, request ID, policy, target and a cancellation predicate
- Produce: final or selected-stage bytes, safe Pipeline error or cancellation, and one metadata-only trace per configured step

- [x] Add failing decoder tests that prove syntax-valid non-UTF-8 values become raw bytes before Pipeline policy is applied:

```rust
#[test]
fn decode_returns_non_utf8_bytes_for_pipeline_policy() {
    assert_eq!(decode(b"/w==", 1024).unwrap(), vec![0xff]);
}
```

Add the equivalent `%FF -> [0xff]` URL test and `ff -> [0xff]` Hex test. Keep all malformed-input offset and pre-allocation output-limit tests unchanged.

- [x] Add failing Pipeline tests for both policies and the current public wrapper:

```rust
#[test]
fn allow_binary_preserves_decoder_bytes_but_public_execute_stays_strict() {
    let steps = [step("base64-decode", true)];
    let report = execute_report(
        ExecutionRequest {
            request_id: 7,
            input: b"/w==".to_vec(),
            steps: &steps,
            output_limit: 1024,
            policy: ExecutionPolicy::AllowBinary,
            target: ExecutionTarget::Final,
        },
        || false,
    );

    assert_eq!(report.request_id, 7);
    assert_eq!(
        report.outcome,
        ExecutionOutcome::Success(vec![0xff])
    );
    assert!(matches!(
        execute(b"/w==".to_vec(), &steps, 1024),
        Err(PipelineError::Step {
            step: 1,
            transform_id: "base64-decode",
            source: TransformError::InvalidUtf8Output { .. },
        })
    ));
}
```

- [x] Add failing report tests covering all approved execution boundaries:

  - `/w== -> base64-decode -> hex-encode` succeeds as `ff` under `AllowBinary`.
  - `/w== -> base64-decode -> url-encode` fails at step 2 with `InvalidUtf8Input`.
  - a disabled step reports identical input/output size and `Disabled`.
  - `Step(index)` executes through the zero-based target, including a disabled target, and marks every later step `NotExecuted`.
  - the first failed step is `Failed` and every later step is `NotExecuted`.
  - cancellation observed before or after a step returns `Cancelled` and never runs a later transform.
  - empty Pipeline succeeds with the original bytes and an empty Trace.
  - 32 steps succeed and 33 steps fail identically under both policies.
  - output limits are identical under both policies.

- [x] Run the focused tests and confirm they fail because the new report types do not exist and the decoders still reject non-UTF-8 output:

```bash
cargo test pipeline::tests -- --nocapture
cargo test transforms::base64::tests::decode_returns_non_utf8_bytes_for_pipeline_policy
cargo test transforms::url::tests::decode_returns_non_utf8_bytes_for_pipeline_policy
cargo test transforms::hex::tests::decode_returns_non_utf8_bytes_for_pipeline_policy
```

Expected result: compilation or assertions fail before implementation.

- [x] Add the following crate-private model to `src/pipeline.rs`. `ExecutionTarget::Step` is zero-based internally; `StepTrace::step` remains one-based for display and errors:

```rust
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionPolicy {
    StrictText,
    AllowBinary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionTarget {
    Final,
    Step(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionOutcome {
    Success(Vec<u8>),
    Failed(PipelineError),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepStatus {
    Succeeded,
    Disabled,
    Failed,
    NotExecuted,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StepTrace {
    pub step: usize,
    pub transform_id: &'static str,
    pub input_bytes: Option<usize>,
    pub output_bytes: Option<usize>,
    pub elapsed: Option<Duration>,
    pub status: StepStatus,
    pub error: Option<TransformError>,
}

pub(crate) struct ExecutionRequest<'a> {
    pub request_id: u64,
    pub input: Vec<u8>,
    pub steps: &'a [TransformStep],
    pub output_limit: usize,
    pub policy: ExecutionPolicy,
    pub target: ExecutionTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutionReport {
    pub request_id: u64,
    pub target: ExecutionTarget,
    pub outcome: ExecutionOutcome,
    pub traces: Vec<StepTrace>,
}
```

- [x] Implement `execute_report` with one loop and no policy-specific transform registry:

  1. Reject more than `MAX_STEPS` before executing and return an empty Trace.
  2. Calculate the inclusive target boundary with checked `index + 1`, clamped to `steps.len()`.
  3. Before each step, check `is_cancelled`. Record the current step as `Cancelled` with the known input size, append metadata-only `NotExecuted` rows with unknown sizes, and return.
  4. Record disabled steps without calling `apply`; known input and output sizes are equal and elapsed time is absent.
  5. Before an active transform, reject non-UTF-8 input when `accepts_binary` is false.
  6. Call the existing `apply`, enforce the existing output limit, and under `StrictText` validate every active step output with `invalid_utf8_output`.
  7. Check `is_cancelled` again before committing the produced bytes. If stale, mark that step `Cancelled`, discard the bytes, append `NotExecuted` rows and return.
  8. On success, record sizes and `Instant::elapsed`; on failure, record only the safe `TransformError`.
  9. For a selected-stage target, append `NotExecuted` rows for all later configured steps.

- [x] Change Base64, URL and Hex decoder success paths to return validated raw bytes. Remove only their final `std::str::from_utf8` checks; keep input UTF-8 validation, malformed-input positions, canonicality checks, checked allocation and output-limit order.

- [x] Make the public `execute` a strict compatibility wrapper:

```rust
pub fn execute(
    input: Vec<u8>,
    steps: &[TransformStep],
    output_limit: usize,
) -> Result<Vec<u8>, PipelineError> {
    match execute_report(
        ExecutionRequest {
            request_id: 0,
            input,
            steps,
            output_limit,
            policy: ExecutionPolicy::StrictText,
            target: ExecutionTarget::Final,
        },
        || false,
    )
    .outcome
    {
        ExecutionOutcome::Success(output) => Ok(output),
        ExecutionOutcome::Failed(error) => Err(error),
        ExecutionOutcome::Cancelled => unreachable!("strict synchronous execution cannot cancel"),
    }
}
```

- [x] Move the old decoder-unit `InvalidUtf8Output` assertions to Pipeline and CLI boundaries. Assert Base64 `/w==`, URL `%FF` and Hex `ff` still produce exit code `4`, empty stdout and the existing bounded preview. Do not change registry descriptions or CLI help text.

- [x] Run focused and full regression checks:

```bash
cargo test pipeline::tests -- --nocapture
cargo test transforms::
cargo test --test cli
cargo test --all-targets --all-features
```

Expected result: all tests pass and public CLI output remains byte-for-byte unchanged.

- [x] Commit:

```bash
git add src/pipeline.rs src/error.rs src/transforms/base64.rs src/transforms/url.rs src/transforms/hex.rs tests/cli.rs
git commit -m "feat(pipeline): 바이트 실행 보고서"
```

---

### Task 2: TUI 모듈 책임 분리

**Files:**

- Modify: `src/tui.rs:1-2439`
- Create: `src/tui/state.rs`
- Create: `src/tui/worker.rs`
- Create: `src/tui/render.rs`
- Create: `src/tui/views.rs`

**Interfaces:**

- Keep public: `tui::check_terminal_entry`, `tui::run`
- Keep in `src/tui.rs`: `TerminalSession`, terminal command tracking, `run_loop`, panic-hook restoration and external clipboard effect handling
- Move to `state.rs`: `Pane`, preview/modal/app events, `Effect`, `App`, editing and state transitions
- Move to `worker.rs`: `PreviewJob`, `PreviewResult`, worker shared state and `PreviewWorker`
- Move to `render.rs`: style, layout, panel, status, modal and `draw_if_dirty`
- Move to `views.rs`: `PreviewDocument` and current safe visible-text calculation

- [x] Record the behavior-preserving baseline:

```bash
cargo test tui::tests
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
```

Expected result: all current TUI tests pass and the release test reports one dirty redraw.

- [x] Add only these module declarations and narrow re-exports in `src/tui.rs`:

```rust
mod render;
mod state;
mod views;
mod worker;

use render::draw_if_dirty;
use state::{App, AppEvent, Effect};
use worker::PreviewWorker;
```

- [x] Move existing items without changing field names, branches, key behavior, rendering strings or test expectations:

  - `state.rs`: current lines 117-800 중 `PreviewDocument`를 제외한 상태·입력 처리와 `normalize_paste`.
  - `views.rs`: current `PreviewDocument`, line starts and `visible_safe_text`.
  - `render.rs`: current lines 854-1174.
  - `worker.rs`: current lines 1199-1270.
  - `tui.rs`: current lines 1-115, clipboard function, event loop, restore and `run`.

- [x] Use `pub(super)` only where sibling TUI modules require access. Do not add traits, builders, controllers, generic UI components or a separate crate.

- [x] Move tests next to the responsibility they exercise:

  - terminal entry, command tracking, panic and full event-loop tests stay in `tui.rs`;
  - App editing, limits, modal and state-transition tests move to `state.rs`;
  - worker pending-slot and shutdown tests move to `worker.rs`;
  - viewport safety tests move to `views.rs`;
  - TestBackend layout, colors and dirty-render tests move to `render.rs`.

- [x] Run format and the same baseline tests:

```bash
cargo fmt --all
cargo test tui::
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
cargo clippy --all-targets --all-features -- -D warnings
```

Expected result: no observable behavior or test count is lost.

- [x] Commit:

```bash
git add src/tui.rs src/tui
git commit -m "refactor(tui): 책임별 모듈 분리"
```

---

### Task 3: 바이트 Artifact와 Text·Hex·Trace View

**Files:**

- Modify: `src/tui/views.rs`
- Test: `src/tui/views.rs`

**Interfaces:**

- Add: `Artifact`, `ViewMode`, `EffectiveView`, `TextWindow`
- Add: `effective_view`, `render_text_window`, `render_hex_window`, `render_trace_window`
- Keep temporarily: the current `PreviewDocument` and `visible_safe_text` compatibility path until Task 4 connects `Artifact` to App and render
- Consume: immutable Artifact bytes, current byte or row offset, viewport rows and columns
- Produce: at most 4 KiB of terminal-safe visible text and the next/previous bounded scroll offsets

- [x] Add failing tests for Smart selection:

```rust
#[test]
fn smart_uses_trace_for_failure_text_for_utf8_and_hex_for_binary() {
    assert_eq!(effective_view(ViewMode::Smart, None, true), EffectiveView::Trace);
    assert_eq!(
        effective_view(ViewMode::Smart, Some(&Artifact::new(b"hello".to_vec())), false),
        EffectiveView::Text
    );
    assert_eq!(
        effective_view(ViewMode::Smart, Some(&Artifact::new(vec![0xff])), false),
        EffectiveView::Hex
    );
}
```

- [x] Add failing Text tests covering valid Unicode boundaries, tab/newline preservation, ESC, OSC 52, NUL, every C0/C1 control class, a single line longer than 4 KiB and newline-dense 64 MiB metadata behavior. The tests must assert both bytes inspected and output string length never exceed 4 KiB; do not allocate a line-offset vector.

- [x] Add failing Hex tests for exactly 16 bytes per row, the extra gap after byte 7, uppercase byte/offset display, printable ASCII range `0x20..=0x7e`, dot replacement, offset overflow handling and viewport-only generation:

```text
00000000  00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F  |................|
00000010  20 41 FF                                         | A.|
```

- [x] Add failing Trace tests for `OK`, `OFF`, `ERROR`, `NOT RUN`, `CANCELLED`, one-based step number, input/output sizes, optional elapsed time and sanitized bounded error. Assert no input or output body appears.

- [x] Add a byte-owning Artifact that does not pre-index all lines. Do not remove `PreviewDocument` in this task because Task 2’s App and renderer still depend on it:

```rust
#[derive(Clone, Debug)]
pub(super) struct Artifact {
    bytes: Arc<[u8]>,
    is_utf8: bool,
}

impl Artifact {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        let is_utf8 = std::str::from_utf8(&bytes).is_ok();
        Self {
            bytes: Arc::from(bytes),
            is_utf8,
        }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn is_utf8(&self) -> bool {
        self.is_utf8
    }
}
```

- [x] Define the explicit and effective View modes:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ViewMode {
    Smart,
    Text,
    Hex,
    Trace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EffectiveView {
    Text,
    Hex,
    Trace,
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct TextWindow {
    pub text: String,
    pub previous_offset: usize,
    pub next_offset: usize,
    pub inspected_bytes: usize,
}
```

`effective_view` selects Trace on failure, Text for valid UTF-8 and Hex for binary only when the configured mode is Smart. An explicitly pinned incompatible Text view returns `Unavailable` with `Switch to Hex view`; it never silently changes the configured mode.

- [x] Reuse the existing grapheme-width and control escaping logic inside `render_text_window`, but start from a UTF-8 boundary at the current byte offset and stop when either the viewport is full, 4 KiB of input has been inspected or 4 KiB of escaped output has been produced. Return bounded next/previous byte offsets; never scan the whole result to move one page.

- [x] Implement Hex with checked `row_offset * 16` and only the visible rows. Implement Trace from at most 32 `StepTrace` values. Crop only the already-bounded rendered lines to the current terminal width.

- [x] Run:

```bash
cargo test tui::views::tests -- --nocapture
```

Expected result: all View tests pass without a new dependency. Defer `-D warnings` Clippy until Task 4 connects these new helpers to the production path.

- [x] Keep Task 3 changes uncommitted and continue directly to Task 4.

---

### Task 4: 최신 요청 상태와 단일 작업자

**Files:**

- Modify: `src/tui/state.rs`
- Modify: `src/tui/worker.rs`
- Modify: `src/tui/render.rs`
- Modify: `src/tui/views.rs`
- Modify: `src/tui.rs`
- Test: `src/tui/state.rs`
- Test: `src/tui/worker.rs`

**Interfaces:**

- Replace generation-only jobs with monotonic `request_id`
- Add: `OutputSource::Final | Step(usize)`
- Add: `OutputStatus::Idle | Debouncing | Running | Ready | Failed | Cancelled`
- Change: `Effect::Submit(PreviewJob) | Cancel(u64) | Copy(String) | Quit(i32)`. Task 4 keeps the current UTF-8-only copy behavior only so the intermediate commit compiles; Task 7 completes binary formatting, checked allocation and ownership transfer.
- Consume in worker: owned input, owned immutable step vector, target, request ID
- Produce from worker: `ExecutionReport` using `AllowBinary`

- [x] Add failing state tests for the exact delay boundary:

```rust
#[test]
fn debounce_is_50_ms_through_256_kib_and_200_ms_above_it() {
    assert_eq!(debounce_for(256 * 1024), Duration::from_millis(50));
    assert_eq!(debounce_for(256 * 1024 + 1), Duration::from_millis(200));
}
```

Also assert one bracketed Paste event increments `request_id` once and schedules one final job.

- [x] Implement the boundary as one pure helper used by every Input and Pipeline change:

```rust
fn debounce_for(input_bytes: usize) -> Duration {
    if input_bytes <= 256 * 1024 {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(200)
    }
}
```

- [x] Add failing state tests for result ownership:

  - Input or Pipeline change increments `request_id`, returns `Cancel(new_id)`, clears final/active Artifact and Trace, disables copy and schedules Final.
  - `p` increments `request_id`, immediately submits `Step(selected_index)` and retains the cached final Artifact.
  - `p` with an empty Pipeline is a no-op with `No pipeline step selected`; it does not create `Step(0)` or any Effect.
  - `f` increments `request_id`, cancels a running selected-stage request, restores the cached final Artifact and submits no job.
  - `f` without a cached final reports `Final output unavailable` and still submits no job.
  - moving Pipeline selection alone changes neither source nor result.
  - manual View mode remains pinned across document changes; Smart is recomputed from each report.
  - stale success, failure and cancellation reports change no Artifact, status, copy eligibility or user status message.
  - `Esc` changes an active request to `Cancelled`; a later document change schedules a fresh Final request.

- [x] Replace the old preview state with the minimum explicit model:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OutputSource {
    Final,
    Step(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OutputStatus {
    Idle,
    Debouncing { deadline: Instant },
    Running,
    Ready,
    Failed(PipelineError),
    Cancelled,
}

pub(super) struct OutputState {
    pub source: OutputSource,
    pub view: ViewMode,
    pub status: OutputStatus,
    pub final_artifact: Option<Artifact>,
    pub active_artifact: Option<Artifact>,
    pub traces: Vec<StepTrace>,
    pub byte_offset: usize,
    pub row_offset: usize,
}
```

- [x] Keep the editor, focus, Pipeline vector, selected index, modal, status, limits and dirty flag in `App`. Add only `zoom`, `request_id` and `OutputState`; do not introduce a controller or reducer abstraction.

- [x] In the same task, migrate `render.rs` from `app.preview`/`PreviewState` to `app.output`/`OutputStatus` and the new bounded View functions while retaining the current pre-Task-5 layout. Only after App and render compile together, remove `PreviewDocument`, the old line-index path and `PreviewState`.

- [x] Make `App::handle_event` compare `request_id` before and after event handling. When an event invalidates work, prepend `Effect::Cancel(new_id)` immediately rather than waiting for debounce submission. A later Tick submits the same ID when its deadline is reached.

- [x] Change the job boundary to owned data:

```rust
pub(super) struct PreviewJob {
    pub request_id: u64,
    pub input: Vec<u8>,
    pub steps: Vec<TransformStep>,
    pub target: ExecutionTarget,
}

pub(super) struct PreviewResult {
    pub report: ExecutionReport,
}
```

The worker constructs `ExecutionRequest` with `ExecutionPolicy::AllowBinary` and `TUI_OUTPUT_LIMIT`.

- [x] Add `AtomicU64 latest_request_id` to the existing worker shared state. `submit` stores the ID and replaces the one pending job; `cancel` stores the ID and clears pending. Pass this predicate to `execute_report`:

```rust
let request_id = job.request_id;
let report = execute_report(request, || {
    latest_request_id.load(Ordering::Acquire) != request_id
});
```

- [x] Add failing worker tests proving:

  - a pending old job is replaced, not queued;
  - cancellation before execution returns no applicable old result;
  - a new ID observed between two steps prevents the second transform;
  - a running synchronous transform may finish, but its report is cancelled or rejected by App;
  - worker Drop updates the latest ID, clears pending work and returns without joining a running synchronous transform; the detached worker exits itself after the current step;
  - Trace, status and user-visible errors never carry input or output bodies.

- [x] Run:

```bash
cargo test tui::
cargo test pipeline::tests
cargo clippy --all-targets --all-features -- -D warnings
```

Expected result: state and worker tests pass; no TUI transform executes on the event-loop thread.

- [x] Keep the verified Task 3–4 changes uncommitted and continue directly to Task 5. Tasks 3–7 are one user-visible workbench change and will be committed with their synchronized documentation in Task 7.

---

### Task 5: 반응형 Pipeline 작업판 렌더링

**Files:**

- Modify: `src/tui/render.rs`
- Modify: `src/tui/state.rs`
- Modify: `src/tui/views.rs`
- Test: `src/tui/render.rs`

**Interfaces:**

- Rename visible panes: `Input`, `Output`, `Pipeline`
- Add: `WidthMode::Wide | Medium | Narrow | Tiny`
- Add pure helpers: `width_mode`, `pipeline_width`, `chrome_visibility`
- Consume: current App state and terminal `Rect`
- Produce: App Bar, optional Navigation/Step Summary, content panes and Context Bar

- [x] Add failing TestBackend boundary tests at widths 120, 119, 90, 89, 40 and 39, and heights 16, 15, 13, 11, 10 and 9.

Expected assertions:

  - width 120 uses a 28–42-column Pipeline and split Input/Output;
  - width 119 and 90 use a 28–32-column Pipeline and the same split;
  - width 89 and 40 show only the focused panel with a textual tab label;
  - width 39 or height 9 shows only `Increase terminal size`;
  - height 15 hides Step Summary, height 13 also hides Navigation details, height 11 shows one focused pane, and every visible bordered pane retains at least three content rows.

- [x] Define the four width modes without an additional layout trait:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WidthMode {
    Wide,
    Medium,
    Narrow,
    Tiny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChromeVisibility {
    navigation: bool,
    step_summary: bool,
    full_context: bool,
    all_panes: bool,
}
```

- [x] Implement the pure width calculation:

```rust
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

fn chrome_visibility(height: u16) -> ChromeVisibility {
    ChromeVisibility {
        navigation: height >= 14,
        step_summary: height >= 16,
        full_context: height >= 12,
        all_panes: height >= 12,
    }
}
```

- [x] Apply this deterministic height policy:

  - height `>= 16`: App Bar, Navigation, Step Summary, all eligible panes and full Context Bar;
  - height `14..=15`: hide Step Summary;
  - height `12..=13`: also hide Navigation;
  - height `10..=11`: render App Bar, one focused pane and minimal Context Bar;
  - height `< 10`: render only resize guidance.

- [x] Render Wide and Medium as Pipeline on the left and Input 42%/Output 58% vertically on the right. Respect `zoom` by giving the focused Pipeline or Output the whole content area. Narrow always renders the focused pane only.

- [x] When both right panes are visible, clamp each bordered pane to at least five rows before applying the 42%/58% preference. At height 12 this yields 5/5 rows and therefore three content rows per pane; at larger heights the ratio resumes naturally.

- [x] Render Output from `effective_view` and the bounded View functions. Its title always contains `FINAL` or `STEP NN` and the configured View. Failed Smart shows Trace; explicitly pinned incompatible views show safe guidance rather than old or partial bytes.

- [x] Render Pipeline rows with selected state, enabled state, display name and Trace status. Wide may show byte-size changes; Medium hides them first. Disabled, failed, cancelled and not-run states must remain distinguishable as text.

- [x] Use terminal-default background and the current limited 16-color styles. In color mode pair stable-width Unicode marks with `OK`, `ERROR`, `OFF`, `RUNNING` text. Under `NO_COLOR`, render only textual states and selection markers; do not use RGB colors or emoji.

- [x] Give current error, cancellation and clipboard status precedence over contextual key hints in every width. Reduced-height Context Bars may omit help but must not omit an active error.

- [x] Add focused render tests for:

  - valid text, binary Smart Hex and failure Smart Trace;
  - pinned Text over binary displays `Switch to Hex view`;
  - pinned Text or Hex on failure displays a safe error summary and `Switch to Trace view` without automatic mode change;
  - a binary Artifact shows `Copy as Hex` in the Context Bar regardless of Smart/Text/Hex mode;
  - Trace includes `STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS`, and Hex includes the `OFFSET` and `ASCII` header;
  - 4 KiB render budget with a small newline-dense adversarial fixture; the single 64 MiB metadata boundary remains in Task 3;
  - `NO_COLOR` status text;
  - escaped ANSI, OSC 52, NUL and C1 bytes never appear as terminal control sequences;
  - dirty rendering still redraws only after state changes.

- [x] Run:

```bash
cargo test tui::
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
```

Expected result: layout boundaries and one-redraw behavior pass.

- [x] Keep the verified Task 5 changes uncommitted and continue directly to Task 6. Tasks 5–7 form one user-visible workbench change, so their code and synchronized product documentation must land in the same commit.

---

### Task 6: 패널 조작, Palette, Inspector, Help와 Zoom

**Files:**

- Modify: `src/tui/state.rs`
- Modify: `src/tui/render.rs`
- Test: `src/tui/state.rs`
- Test: `src/tui/render.rs`

**Interfaces:**

- Focus cycle: `Input -> Output -> Pipeline -> Input`
- Add modal variants: `TransformPicker`, `StepInspector`, `Help`, `QuitConfirm`, `UnsafeCopyConfirm`
- Add: `zoom: Option<Pane>`
- Keep Palette search: case-insensitive substring over display name and public ID
- Keep Inspector read-only

- [x] Add failing key tests that prove Input receives ordinary `1`–`4`, `?`, `z`, `a`, `d`, `j`, `k`, `p`, `f`, `v` and `y` as editor input instead of global commands.

- [x] Add failing global tests for `Tab`, `Shift+Tab`, `Ctrl+P`, `F1`, `Ctrl+Q`, `Ctrl+C` and the exact `Esc` priority:

```text
open modal -> Esc closes modal
no modal + zoom -> Esc closes zoom
no modal + no zoom + running request -> Esc invalidates request and shows Cancelled
otherwise Esc does nothing destructive
```

- [x] Add failing Pipeline tests for arrows and `j`/`k`, `Space`, Shift+arrows and `J`/`K`, `Delete`/`d`, `Enter`, `a` and `z`. Pipeline edits must schedule Final; selection movement alone must not.

- [x] Add failing Output tests for:

  - `v` and `V` cycling `Smart -> Text -> Hex -> Trace`;
  - `p` requesting the selected Pipeline step immediately;
  - `f` restoring cached Final without a job;
  - `Enter` and `y` requesting whole-result copy;
  - arrows, PageUp/PageDown, Home and End changing only bounded byte/row offsets;
  - `z` toggling Output zoom.

- [x] Extend the existing 8-item Palette, not its search algorithm. Its detail region displays the selected transform’s public ID and `Text input` or `Bytes accepted`. Label both existing registry strings as `CLI description` and `CLI behavior`, then show `TUI result: bytes; Smart uses Text or Hex` as the primary TUI output hint so decoder wording cannot imply that TUI discards binary bytes. Do not change CLI registry/help strings or add fuzzy search, recent items, aliases or categories.

- [x] Implement a read-only Inspector that renders selected step name, ID, input condition, runtime status, input/output sizes, elapsed time and safe error. It must not contain an option form because no transform has options.

- [x] Implement context Help as one modal whose body is selected from the current pane. F1 always opens it; `?` opens it only outside Input. Include the actual keys from the approved design and keep the UI language English.

- [x] Render Zoom by reusing the existing panel render function with a larger `Rect`; do not create separate zoom widgets or state copies.

- [x] Run:

```bash
cargo test tui::
cargo clippy --all-targets --all-features -- -D warnings
```

Expected result: all context-sensitive keys and overlays pass without changing Input editing semantics.

- [x] Keep the verified Task 5–6 changes uncommitted and continue directly to Task 7.

---

### Task 7: 원본 기준 클립보드 복사

**Files:**

- Modify: `src/tui/state.rs`
- Modify: `src/tui.rs`
- Modify: `src/tui/render.rs`
- Modify: `src/tui/views.rs`
- Modify: `README.md:50-75`
- Modify: `docs/prd/init-prd.md:435-558`
- Modify: `docs/prd/init-prd.md:669-729`
- Modify: `docs/prd/init-prd.md:787-919`
- Modify: `docs/prd/init-prd.md:1094-1204`
- Modify: `docs/prd/v0.2-prd.md:18-26`
- Modify: `docs/prd/v0.2-prd.md:46-68`
- Modify: `docs/prd/v0.2-prd.md:96-130`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:1-30`
- Test: `src/tui/state.rs`
- Test: `src/tui.rs`

**Interfaces:**

- Add: `ClipboardPayload { text: String, kind: CopyKind }`
- Change from Task 4: `Effect::Copy(ClipboardPayload)` preserves copy kind until the operating-system write finishes
- Change: `AppEvent::ClipboardFinished { kind: CopyKind, result: Result<(), String> }`
- Keep: dangerous UTF-8 control confirmation
- Reject: Trace, failure, cancellation, stale and missing Artifact copy

- [x] Add failing tests proving copy format depends on Artifact validity, not the current View:

```rust
#[test]
fn binary_artifact_copies_as_lowercase_hex_in_every_view() {
    let artifact = Artifact::new(vec![0x00, 0xab, 0xff]);
    let payload = clipboard_payload(&artifact).unwrap();
    assert_eq!(payload.text, "00abff");
    assert_eq!(payload.kind, CopyKind::Hex);
}
```

Add valid Unicode exact-copy, a binary Artifact under pinned Text, Trace/error/no-result rejection, old-result rejection and control-confirmation tests.

- [x] Define the payload locally in `state.rs`; it is not a public clipboard abstraction:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyKind {
    Text,
    Hex,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ClipboardPayload {
    pub text: String,
    pub kind: CopyKind,
}
```

- [x] Implement checked allocation without `format!` in the byte loop:

```rust
fn checked_hex_len(byte_len: usize) -> Option<usize> {
    byte_len.checked_mul(2)
}

fn binary_hex(bytes: &[u8]) -> Result<String, ()> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let capacity = checked_hex_len(bytes.len()).ok_or(())?;
    let mut output = String::new();
    output.try_reserve_exact(capacity).map_err(|_| ())?;
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(output)
}
```

For UTF-8, reserve exactly the source length and `push_str` the validated original. Do not escape, normalize or use a lossy conversion for clipboard data.

- [x] Define `Modal::UnsafeCopyConfirm { payload: ClipboardPayload }` and store the already-built UTF-8 payload there so confirmation does not allocate a second full copy. On approval, move the payload into `Effect::Copy`.

- [x] Change the clipboard boundary to consume the String:

```rust
fn set_clipboard_text(
    clipboard: &mut Option<arboard::Clipboard>,
    text: String,
) -> Result<(), String> {
    if clipboard.is_none() {
        *clipboard =
            Some(arboard::Clipboard::new().map_err(|_| "Clipboard unavailable".to_string())?);
    }
    clipboard
        .as_mut()
        .ok_or_else(|| "Clipboard unavailable".to_string())?
        .set_text(text)
        .map_err(|_| "Clipboard unavailable".to_string())
}
```

- [x] In `run_loop`, save `payload.kind`, move `payload.text` into `set_clipboard_text`, then emit `ClipboardFinished { kind, result }`. Show `Copied` for UTF-8 and `Copied as Hex` for binary only after a successful operating-system write. Allocation or platform clipboard failure becomes a safe status message; it must not clear or mutate Input, Pipeline or Artifact.

- [x] Test arithmetic overflow with `checked_hex_len(usize::MAX) == None`, App handling of clipboard failure, and safe state retention. Use the existing real-platform smoke paths instead of introducing a mock clipboard trait.

- [x] Run:

```bash
cargo test tui::
cargo test --all-targets --all-features
```

Expected result: UTF-8 text is exact, binary text is lowercase Hex, and old/error results cannot be copied.

- [x] Request one independent component review covering the strict/allow-binary engine, cancellation timing, `p`/`f`, terminal escaping, clipboard ownership/allocation, stale-result handling and input preservation. Resolve important findings before integration smoke tests.

- [x] Before committing the completed user-visible workbench, synchronize its actual behavior in README, both PRDs and the approved design:

  - Pipeline/Input/Output layout, responsive modes and focus order;
  - Smart/Text/Hex/Trace, selected-stage `p`, final `f` and binary Hex copy;
  - 50/200 ms debounce, latest-request worker and split TUI module tree;
  - Palette, Inspector, Help, Zoom and pane-specific keys;
  - TUI binary success versus unchanged CLI strict-text behavior;
  - active limits, terminal safety, tests and architecture summaries.

Keep the historical verification records unchanged and label platform-wide verification as pending until Tasks 8–9 finish.

- [x] Commit:

```bash
git add src/tui.rs src/tui README.md docs/prd/init-prd.md docs/prd/v0.2-prd.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md
git commit -m "feat(tui): 바이트 작업판 고도화"
```

---

### Task 8: CLI 회귀와 실제 TUI 셸 Smoke

**Files:**

- Modify: `tests/shell-smoke.sh`
- Modify: `tests/cli.rs`

**Interfaces:**

- Keep: `DOOP_SMOKE_CLIPBOARD_MODE=skip|unavailable|x11|macos|wayland`
- Change selected clipboard fixture: valid UTF-8 Hex text to a raw binary Base64 decode result
- Produce expected display `FF` and clipboard text `ff`

- [x] Add CLI integration assertions before changing the shell fixture:

  - Base64 `/w==`, URL `%FF` and Hex `ff` each exit `4`, write no stdout and preserve the existing `InvalidUtf8Output` rendering.
  - 8 IDs, help/list/version, `doop tui` and rejection of additional TUI arguments remain unchanged.

Keep dangerous valid UTF-8 control-byte TTY refusal and redirected raw-byte preservation in the existing `src/cli.rs` unit tests and Expect shell path, where a real pseudoterminal exists. Do not try to emulate a TTY with the piped stdout used by `tests/cli.rs`.

- [x] Split the current Expect helper into text and binary preparation:

```tcl
proc prepare_text_preview {} {
    global env spawn_id
    send -- "\033\[200~hello\033\[201~"
    expect_exact "hello" 93 94
    send -- "\020"
    expect_exact "Search:" 119 120
    send -- "hex-encode\r"
    expect_exact $env(DOOP_SMOKE_TEXT_EXPECTED) 121 122
}

proc prepare_binary_preview {} {
    global env spawn_id
    send -- "\033\[200~/w==\033\[201~"
    expect_exact "/w==" 130 131
    send -- "\020"
    expect_exact "Search:" 132 133
    send -- "base64-decode\r"
    expect_exact "00000000" 134 135
    expect_exact "FF" 136 137
}
```

- [x] Keep normal resize and discard-confirmation checks on `prepare_text_preview`. Use `prepare_binary_preview`, `Tab` to Output and `y` for every clipboard mode. Change the externally verified clipboard value to lowercase `ff`.

- [x] Update exact screen expectations from Preview/Chain and old focus order to Output/Pipeline and `Input -> Output -> Pipeline`. Continue asserting bracketed-paste disable, alternate-screen exit and terminal mode equality on normal, interrupt and clipboard paths.

- [x] Run local Bash and Zsh smoke:

```bash
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
```

Expected result: both pass on the current host.

- [x] When the corresponding local environment is available, run the existing protected clipboard modes without weakening backup/restore:

```bash
DOOP_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=unavailable bash tests/shell-smoke.sh
xvfb-run -a env DOOP_SMOKE_CLIPBOARD_MODE=x11 bash tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=wayland bash tests/shell-smoke.sh
```

Run Wayland only when the current `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR` identify a usable session; do not replace the session display name with a hard-coded value. Record unavailable environments as unverified and never report them as passed.

- [x] Commit:

```bash
git add tests/cli.rs tests/shell-smoke.sh
git commit -m "test(tui): 바이너리 작업판 경계"
```

---

### Task 9: 문서 현행화, 전체 검증과 완료 리뷰

**Files:**

- Modify: `README.md` local verification record
- Modify: `docs/prd/init-prd.md:435-447`
- Modify: `docs/prd/v0.2-prd.md:18-26`
- Modify: `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md:1-30`
- Modify: `docs/superpowers/plans/2026-07-31-doop-tui-workbench.md`

**Interfaces:** 문서가 실제 구현, 시험 결과와 공개 계약만 설명한다.

- [x] Compare the Task 7 documentation commit against the current code and approved design. Keep all CLI-only UTF-8, error and exit-code requirements unchanged; any behavioral mismatch is an implementation or documentation defect, not a verification-note exception.

- [x] Run an independent whole-diff review from design commit `691951a` through HEAD for requirement coverage, CLI compatibility, concurrency, terminal/clipboard security, bounded memory and over-engineering. Fix every important finding with a focused regression test, synchronize any affected documentation in that fix commit, and repeat the complete suite below.

- [x] Run the complete candidate verification suite while the documentation record is still uncommitted. Use `--allow-dirty` only for this pre-commit package check and use a fresh temporary install root:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo package --locked --allow-dirty
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
install_root=$(mktemp -d "${TMPDIR:-/tmp}/doop-install.XXXXXX")
cargo install --locked --offline --path . --root "$install_root"
"$install_root/bin/doop" --version
git diff --check
```

Expected result: every available check passes, the release measurement reports one dirty redraw, package/install reports `doop 0.2.0`, and no GitHub Actions file exists.

- [x] Add a new Task 9 verification record to README for this workbench candidate, naming its execution date and exact candidate commit SHA and listing only commands and platform paths actually run. Do not alter the historical 2026-07-30 records or claim unavailable paths passed.

- [x] Only after the candidate suite passes, evaluate that new candidate-specific record. If it contains actual macOS and Linux results for Bash and Zsh, real TUI paths and each platform's required clipboard path, change the design status to `구현 완료` and replace conditional precedence wording in both PRDs. Otherwise keep the status `기능 구현 완료·플랫폼 통합 검증 대기` and preserve the conditional precedence.

- [x] Re-index the significantly changed module tree using the `ccc` skill:

```bash
ccc index
```

If the project index is missing, run `ccc init`, then `ccc index`. Confirm the index includes `src/tui/state.rs`, `worker.rs`, `render.rs` and `views.rs`.

- [x] Confirm `git status --short --branch` contains only the expected verification-record and plan-document changes, with no generated artifacts.

- [x] Commit documentation:

```bash
git add README.md docs/prd/init-prd.md docs/prd/v0.2-prd.md docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md docs/superpowers/plans/2026-07-31-doop-tui-workbench.md
git commit -m "docs(tui): 작업판 구현 현행화"
git status --short --branch
```

Expected result: the documentation commit succeeds and the final status is clean. The candidate suite immediately before this docs-only commit is the completion evidence; do not create a second full-suite/checklist cycle. If the commit or status check fails, fix the root cause and repeat this step.

---

## Final Self-Review Checklist

- [x] Every approved section of the design maps to at least one task and one runnable check.
- [x] No task adds a new dependency, future-only abstraction, options form, cache, plugin, file I/O or CI.
- [x] Every type referenced by a task is defined in this plan before use.
- [x] Public CLI behavior has explicit regression coverage at the Pipeline, integration and shell boundaries.
- [x] Text, Hex, Trace, clipboard and worker paths have bounded-memory and stale-result checks.
- [x] Search the plan for placeholders and remove them:

```bash
rg --color=never -n "TO[D]O|FIX[M]E|T[B]D|나중[에] 구현|적절[히] 처리" docs/superpowers/plans/2026-07-31-doop-tui-workbench.md
```

Expected result: no matches.

- [x] Inspect every literal ellipsis separately. The only allowed match is the intentional 16-byte ASCII fixture in Task 3:

```bash
rg --color=never -n "\\.{3}" docs/superpowers/plans/2026-07-31-doop-tui-workbench.md
```

- [x] Confirm the plan itself is format-clean and tracked:

```bash
git diff --check
git status --short
```
