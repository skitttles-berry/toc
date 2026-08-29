# toc Output Module Deepening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 사용자에게 보이는 동작을 그대로 유지하면서 TUI Output의 수명주기, View, viewport, navigation과 표시 데이터 생성을 하나의 deep module에 집중한다.

**Architecture:** `src/tui/output.rs`가 Output 상태와 계산을 private implementation으로 소유하고 lifecycle, semantic navigation, read model만 노출한다. `App`은 request ID, worker effect, Input과 Pipeline을 계속 소유하며 `render.rs`는 Ratatui layout과 drawing만 담당한다.

**Tech Stack:** Rust 2021, Ratatui 0.29, Crossterm 0.28, Unicode Segmentation 1.12, Cargo의 기존 unit·integration·Shell smoke 테스트

**Spec:** [`docs/superpowers/specs/2026-08-29-toc-output-module-deepening-design.md`](../specs/2026-08-29-toc-output-module-deepening-design.md)

## Global Constraints

- 키, View 순서, scroll, copy, 화면 문구·제목·색상·layout, debounce·notice timing과 output limit을 바꾸지 않는다.
- `App`은 request ID, stale result 거부, jobs, effects, Input과 Pipeline을 계속 소유한다.
- Output seam에는 Ratatui 타입을 노출하지 않는다. `Viewport`는 `rows`와 `columns`만 가진다.
- 기존 4 KiB 표시 budget, `MAX_STEPS`, 기본 78×10 viewport와 UTF-8·Hex·Trace 규칙을 보존한다.
- 새 dependency, trait, adapter, factory와 speculative extension point를 추가하지 않는다.
- 직접 필드 조작 테스트는 Output interface 테스트로 교체하고 semantic render 테스트와 shell smoke는 유지한다.
- 동작과 구조가 바뀐 파일의 관련 문서를 같은 논리 변경에서 최소 diff로 현행화한다.
- credential, debug 출력과 미완료 표식을 남기지 않는다.

## File Map

| File | Change |
| --- | --- |
| `src/tui/views.rs` → `src/tui/output.rs` | 기존 계산을 보존하고 Output deep module로 확장 |
| `src/tui.rs` | module 이름과 공유 타입 import 변경 |
| `src/tui/state.rs` | Output 상태·offset 계산 제거, lifecycle·navigation 호출만 유지 |
| `src/tui/render.rs` | Output presentation과 summary만 소비 |
| `src/tui/clipboard.rs` | `output::Artifact` import로 변경 |
| `AGENTS.md` | tree의 `views.rs`를 `output.rs`로 현행화 |
| `CONTEXT.md` | 승인된 Input·Transform·Pipeline·Output·View 언어 유지 |
| `docs/superpowers/specs/2026-08-29-toc-output-module-deepening-design.md` | 승인 설계 기록 유지 |
| `docs/superpowers/plans/2026-08-29-toc-output-module-deepening.md` | 각 단계 완료 시 checkbox 갱신 |

---

## Task 1: `views`를 `output`으로 기계적으로 교체

**Files:**

- Rename: `src/tui/views.rs` → `src/tui/output.rs`
- Modify: `src/tui.rs`
- Modify: `src/tui/state.rs`
- Modify: `src/tui/render.rs`
- Modify: `src/tui/clipboard.rs`

- [x] **Step 1: rename 전 기준 테스트 실행**

Run:

    cargo test --locked tui::views::tests -- --show-output
    cargo test --locked tui::state::tests::hex_resize_preserves_the_visible_byte_and_clamps_to_the_new_full_page -- --exact
    cargo test --locked tui::render::tests::hex_table_adapts_columns_and_keeps_no_color_structure -- --exact

Expected: 기존 관련 테스트가 모두 통과한다.

- [x] **Step 2: 파일과 module 이름만 변경**

`apply_patch`의 move를 사용해 파일을 옮기고 다음 import만 바꾼다.

    // src/tui.rs
    mod output;

    // src/tui/clipboard.rs
    use super::output::Artifact;

`src/tui/state.rs`와 `src/tui/render.rs`의 기존 `super::views` import list는 항목을 바꾸지 않고 경로만 `super::output`으로 교체한다. `HexRow` 등 경로 기반 참조도 같은 방식으로 바꾼다. 이 단계에서 타입이나 동작을 재설계하지 않는다.

- [x] **Step 3: rename 회귀 검증**

Run:

    cargo fmt --check
    cargo test --locked tui::output::tests -- --show-output
    cargo test --locked tui::state::tests::hex_resize_preserves_the_visible_byte_and_clamps_to_the_new_full_page -- --exact
    cargo test --locked tui::render::tests::hex_table_adapts_columns_and_keeps_no_color_structure -- --exact
    git diff --check

Expected: test 이름의 module 경로만 바뀌고 동작은 동일하다.

- [x] **Step 4: 논리 커밋**

    git add src/tui.rs src/tui/output.rs src/tui/views.rs src/tui/state.rs src/tui/render.rs src/tui/clipboard.rs docs/superpowers/plans/2026-08-29-toc-output-module-deepening.md
    git diff --cached --check
    git commit -m "refactor(tui): Output 모듈 명칭 정리"

---

## Task 2: Output lifecycle과 snapshot 규칙 집중

**Files:**

- Modify: `src/tui/output.rs`
- Modify: `src/tui/state.rs`

- [ ] **Step 1: 새 lifecycle interface의 실패 테스트 작성**

`src/tui/output.rs` 테스트에 interface 호출만 사용하는 helper와 다음 네 회귀 테스트를 먼저 추가한다.

    fn finish(output: &mut Output, target: ExecutionTarget, bytes: &[u8]) {
        assert_eq!(
            output.update(Lifecycle::Start {
                started_at: Instant::now(),
                target,
            }),
            LifecycleChange::Changed
        );
        assert_eq!(
            output.update(Lifecycle::Finish {
                target,
                outcome: ExecutionOutcome::Success(bytes.to_vec()),
                traces: Vec::new(),
            }),
            LifecycleChange::Changed
        );
    }

추가할 test 이름과 assertion은 다음과 같다.

- `selected_output_preserves_the_final_snapshot_for_restore`: Final과 Step을 순서대로 `finish`한 뒤 `RestoreFinal`이 `Changed`이고 summary의 source·artifact·traces가 Final snapshot인지 확인한다.
- `invalidation_keeps_current_output_but_blocks_copy_and_final_restore`: `Invalidate` 후 summary에는 직전 artifact가 남지만 `copy_artifact()`는 `None`, `RestoreFinal`은 `FinalUnavailable`인지 확인한다.
- `tick_requests_final_once_at_deadline`: deadline 직전은 `Unchanged`, deadline은 `StartFinal`, 같은 deadline의 재호출은 `Unchanged`인지 확인한다.
- `tick_shows_the_long_running_notice_once_at_threshold`: running threshold 직전은 `Unchanged`, threshold는 최초 한 번 `Changed`, 이후는 `Unchanged`인지 확인한다.

Run:

    cargo test --locked tui::output::tests::selected_output_preserves_the_final_snapshot_for_restore -- --exact

Expected: `Output`, `Lifecycle`과 `LifecycleChange`가 아직 없어 compile failure.

- [ ] **Step 2: 최소 lifecycle 타입과 read-only summary 구현**

`src/tui/output.rs`에 기존 `OutputSource`, `OutputStatus`, `OutputState`를 이동해 아래 책임만 노출한다.

    pub(super) enum Source {
        Final,
        Step(usize),
    }

    pub(super) enum Status {
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

    pub(super) enum Lifecycle {
        Invalidate { deadline: Instant },
        Start { started_at: Instant, target: ExecutionTarget },
        Finish {
            target: ExecutionTarget,
            outcome: ExecutionOutcome,
            traces: Vec<StepTrace>,
        },
        RestoreFinal,
        Cancel,
        Tick { now: Instant },
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum LifecycleChange {
        Unchanged,
        Changed,
        StartFinal,
        FinalUnavailable,
    }

Output에는 기존 source, requested View, status, final/current artifact와 traces, byte/row offset, viewport를 이동하며 모든 필드를 private으로 둔다. 노출하는 method signature는 `Output::new() -> Self`, `update(&mut self, Lifecycle) -> LifecycleChange`, `summary(&self) -> Summary<'_>`, `copy_artifact(&self) -> Option<Artifact>`뿐이다.

`Summary<'_>`는 source, requested/effective View, status reference, ready bytes, current artifact와 traces를 읽기 전용으로 제공한다. `Ready`만 copy artifact를 반환하고 final snapshot은 `Artifact`와 traces를 함께 저장·삭제한다.

- [ ] **Step 3: App lifecycle 호출을 새 interface로 교체**

`src/tui/state.rs`에서 다음 기존 흐름을 `Output::update`로 위임한다.

- Input·Pipeline 변경 → `Lifecycle::Invalidate`
- Preview 시작 → `Lifecycle::Start`
- request ID 검증 후 결과 적용 → `Lifecycle::Finish`
- Final 복원 → `Lifecycle::RestoreFinal`
- 취소 → `Lifecycle::Cancel`
- timer 처리 → `Lifecycle::Tick`

`LifecycleChange::StartFinal`일 때만 App이 기존 `PreviewJob`과 effect를 생성한다. request ID 비교, job 생성과 transient 안내 문구는 App에 남긴다.

- [ ] **Step 4: lifecycle 테스트와 App 회귀 테스트 실행**

Run:

    cargo fmt --check
    cargo test --locked tui::output::tests -- --show-output
    cargo test --locked tui::state::tests -- --show-output
    git diff --check

Expected: final/step/restore/invalidate/tick 규칙과 기존 App event 테스트가 통과한다.

- [ ] **Step 5: 논리 커밋**

    git add src/tui/output.rs src/tui/state.rs docs/superpowers/plans/2026-08-29-toc-output-module-deepening.md
    git diff --cached --check
    git commit -m "refactor(tui): Output 수명주기 집중"

---

## Task 3: navigation과 presentation 집중

**Files:**

- Modify: `src/tui/output.rs`
- Modify: `src/tui/state.rs`
- Modify: `src/tui/render.rs`

- [ ] **Step 1: viewport·navigation·presentation 실패 테스트 작성**

`src/tui/output.rs`에 최소 두 테스트를 interface만 사용해 먼저 작성한다.

    fn ready_output(bytes: Vec<u8>) -> Output {
        let mut output = Output::new();
        finish(&mut output, ExecutionTarget::Final, &bytes);
        output
    }

    fn first_hex_offset(presentation: Presentation<'_>) -> usize {
        match presentation.body {
            Body::Hex(rows) => rows.first().map_or(0, |row| row.offset),
            _ => panic!("expected hex presentation"),
        }
    }

    #[test]
    fn hex_resize_preserves_the_first_visible_byte() {
        let mut output = ready_output((0_u8..=127).collect());
        output.navigate(Navigation::NextView); // Smart Text → Text
        output.navigate(Navigation::NextView); // Text → Hex
        output.present(Viewport { rows: 5, columns: 78 });
        output.navigate(Navigation::Page(1));

        let before = first_hex_offset(output.present(Viewport { rows: 5, columns: 78 }));
        let after = first_hex_offset(output.present(Viewport { rows: 5, columns: 40 }));

        assert_eq!(after, before);
    }

    #[test]
    fn text_end_presents_the_last_full_page_without_exposing_an_offset() {
        let mut output = ready_output(b"one\ntwo\nthree\nfour\nfive".to_vec());
        output.present(Viewport { rows: 2, columns: 8 });

        assert!(output.navigate(Navigation::End));
        let Presentation { body: Body::Text(text), .. } =
            output.present(Viewport { rows: 2, columns: 8 })
        else {
            panic!("expected text presentation");
        };

        assert_eq!(text, "four\nfive");
    }

Run:

    cargo test --locked tui::output::tests::hex_resize_preserves_the_first_visible_byte -- --exact

Expected: `Viewport`, `Navigation`, `Presentation`과 `present`가 없어 compile failure.

- [ ] **Step 2: semantic navigation과 presentation 타입 추가**

`src/tui/output.rs`에 다음 API를 추가하고 모든 offset 필드는 private으로 유지한다.

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct Viewport {
        pub(super) rows: usize,
        pub(super) columns: usize,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum Navigation {
        NextView,
        Line(i8),
        Page(i8),
        Home,
        End,
    }

    pub(super) struct Presentation<'a> {
        pub(super) summary: Summary<'a>,
        pub(super) body: Body<'a>,
    }

    pub(super) enum Body<'a> {
        Empty,
        Cancelled,
        Failure {
            error: &'a PipelineError,
            switch_to_trace: bool,
        },
        Text(String),
        Hex(Vec<HexRow<'a>>),
        Trace(TraceWindow<'a>),
        TextUnavailable,
    }

    pub(super) struct TraceWindow<'a> {
        pub(super) traces: &'a [StepTrace],
        pub(super) visible: Range<usize>,
        pub(super) failure: Option<usize>,
        pub(super) detail_height: usize,
        pub(super) byte_budget: usize,
    }

Output은 `pub(super) fn navigate(&mut self, Navigation) -> bool`와 `pub(super) fn present(&mut self, Viewport) -> Presentation<'_>`를 구현한다.

`present`는 다음 순서를 한 번에 수행한다.

1. 기존 viewport에서 보이던 Hex 첫 byte 또는 Trace start를 계산한다.
2. 새 `rows`·`columns`에 맞춰 offset을 clamp한다.
3. effective View와 visible Text·Hex·Trace 데이터를 만든다.
4. 같은 상태의 `Summary`와 `Body`를 반환한다.

0×0 viewport는 empty visible data를 반환하고 navigation은 no-op이다. 기존 `views.rs`의 Unicode, budget, Hex와 Trace helper는 private implementation으로 재사용한다.

- [ ] **Step 3: App의 key mapping만 남기기**

`src/tui/state.rs`에서 `cycle_view`, `output_page_rows`, `output_max_offset`, `page_output`, `scroll_output`, `output_home_or_end`와 viewport reflow 계산을 제거한다.

`handle_output_key`는 기존 키를 아래 semantic command로 매핑한다.

    KeyCode::Char('v' | 'ㅍ') => Navigation::NextView,
    KeyCode::Up | KeyCode::Left => Navigation::Line(-1),
    KeyCode::Down | KeyCode::Right => Navigation::Line(1),
    KeyCode::PageUp => Navigation::Page(-1),
    KeyCode::PageDown => Navigation::Page(1),
    KeyCode::Home => Navigation::Home,
    KeyCode::End => Navigation::End,

`navigate`가 `true`를 반환할 때만 `mark_dirty`를 호출한다. copy와 zoom key는 기존 App 흐름에 남긴다.

- [ ] **Step 4: render를 presentation 소비자로 축소**

`src/tui/render.rs`는 Output content `Rect`에서 다음 값만 만든다.

    let viewport = Viewport {
        rows: area.height as usize,
        columns: area.width as usize,
    };
    let presentation = app.output.present(viewport);

그 뒤 `Body`를 match해 기존 widget과 style을 그대로 사용한다. Text·Hex·Trace helper에는 App이나 Output을 넘기지 않고 presentation data와 기존 `no_color` 값만 전달한다. title, footer, pipeline과 inspector는 `Summary`를 사용한다.

- [ ] **Step 5: 내부 상태 누수와 동작 회귀 확인**

Run:

    cargo fmt --check
    cargo test --locked tui::output::tests -- --show-output
    cargo test --locked tui::state::tests -- --show-output
    cargo test --locked tui::render::tests -- --show-output
    rg --color=never -n 'byte_offset|row_offset|effective_view|hex_bytes_per_row|last_text_page_offset|trace_start_row' src/tui/state.rs src/tui/render.rs
    rg --color=never -n 'output\.[a-z_]+\s*=' src/tui/state.rs src/tui/render.rs
    git diff --check

Expected: 세 test module이 통과하고 두 `rg` 명령은 match가 없어 exit 1이다.

- [ ] **Step 6: 논리 커밋**

    git add src/tui/output.rs src/tui/state.rs src/tui/render.rs docs/superpowers/plans/2026-08-29-toc-output-module-deepening.md
    git diff --cached --check
    git commit -m "refactor(tui): Output 표시와 탐색 집중"

---

## Task 4: 직접 필드 테스트를 Output interface 회귀 테스트로 교체

**Files:**

- Modify: `src/tui/output.rs`
- Modify: `src/tui/state.rs`
- Modify: `src/tui/render.rs`

- [ ] **Step 1: 반복 setup을 lifecycle helper 하나로 통합**

Output 테스트의 성공 setup은 Task 3에서 만든 `ready_output` 하나로 통합한다. 별도 builder나 fixture struct는 만들지 않는다.

failure와 traces가 필요한 테스트는 `Lifecycle::Finish` 값을 test 안에 직접 적어 의미를 숨기지 않는다.

- [ ] **Step 2: 상태 기반 동작 테스트를 interface 기준으로 이동**

다음 기존 테스트 그룹을 `src/tui/output.rs`로 옮기고 직접 offset·viewport 필드 assertion을 presentation의 첫 visible item 또는 반환된 body assertion으로 바꾼다.

- Hex resize, hidden viewport, page와 end
- Trace byte budget, first failure, 작은 viewport, page와 end
- manual View 순환과 Smart 판정
- Text arrow·page·home·end, grapheme와 UTF-8 boundary
- final/step failure·cancel과 copy 가능 조건

기존 bounded Unicode와 allocation budget 테스트는 helper implementation 회귀 가치가 있으므로 유지한다.

- [ ] **Step 3: render fixture의 직접 필드 조작 제거**

`src/tui/render.rs` 테스트 setup은 `Lifecycle::Finish`, `Navigation`과 `present`로 상태를 만든다. `#[cfg(test)]` setter나 offset getter는 추가하지 않는다. 기존 제목, no-color, 열, 작은 화면과 failure detail의 semantic assertion은 그대로 유지한다.

- [ ] **Step 4: 중복 테스트 삭제 후 focused suite 실행**

Run:

    cargo fmt --check
    cargo test --locked tui::output::tests -- --show-output
    cargo test --locked tui::state::tests -- --show-output
    cargo test --locked tui::render::tests -- --show-output
    rg --color=never -n 'output\.(byte_offset|row_offset|viewport|view|status|active_artifact|traces)\s*=' src/tui/state.rs src/tui/render.rs
    git diff --check

Expected: 모든 focused test가 통과하고 마지막 `rg`는 match가 없다.

- [ ] **Step 5: 논리 커밋**

    git add src/tui/output.rs src/tui/state.rs src/tui/render.rs docs/superpowers/plans/2026-08-29-toc-output-module-deepening.md
    git diff --cached --check
    git commit -m "test(tui): Output 인터페이스 회귀 검증"

---

## Task 5: 문서 동기화와 전체 검증

**Files:**

- Modify: `AGENTS.md`
- Verify: `CONTEXT.md`
- Verify: `docs/superpowers/specs/2026-08-29-toc-output-module-deepening-design.md`
- Modify: `docs/superpowers/plans/2026-08-29-toc-output-module-deepening.md`

- [ ] **Step 1: 구조 문서 최소 변경**

`AGENTS.md` tree에서 다음 한 줄만 교체한다.

    ├── output.rs          # Output 상태·View·탐색·표시 데이터

`CONTEXT.md`의 Output 정의가 구현과 일치하는지 확인한다. 승인 설계와 구현이 다르지 않다면 spec과 CONTEXT에 설명을 더하지 않는다.

- [ ] **Step 2: 사용자-visible CLI 계약 확인**

Run:

    cargo run --locked -- --help
    cargo run --locked -- --list

Expected: help 문구가 기존과 같고 transform 목록은 36개다.

- [ ] **Step 3: 정적 검사와 전체 Rust 검증**

Run:

    cargo fmt --check
    cargo clippy --locked --all-targets --all-features -- -D warnings
    cargo test --locked --all-targets --all-features
    cargo build --locked --release

Expected: 경고와 실패 없이 모두 exit 0.

- [ ] **Step 4: Shell·PTY smoke 검증**

Run:

    bash tests/shell-smoke.sh
    zsh tests/shell-smoke.sh

Expected: 두 shell에서 모든 smoke assertion이 통과한다.

- [ ] **Step 5: 인덱스와 최종 diff 검증**

Run:

    ccc index
    git diff --check
    git status --short
    git diff --stat

Expected: ccc index error 0, whitespace 오류 없음, 의도한 파일만 변경됨.

- [ ] **Step 6: 문서 논리 커밋**

    git add AGENTS.md CONTEXT.md docs/superpowers/specs/2026-08-29-toc-output-module-deepening-design.md docs/superpowers/plans/2026-08-29-toc-output-module-deepening.md
    git diff --cached --check
    git commit -m "docs(tui): Output 구조 문서 현행화"

- [ ] **Step 7: 최종 저장소 상태 확인**

Run:

    git status --short
    git log -5 --oneline

Expected: worktree가 clean이고 이 계획의 다섯 refactor/test/docs 논리 변경이 최근 commit으로 보인다. Push는 별도 사용자 요청이 있을 때만 수행한다.
