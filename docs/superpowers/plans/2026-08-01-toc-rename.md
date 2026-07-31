# toc Application Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 제품, Cargo 패키지·크레이트·실행 파일, CLI·TUI, 시험과 프로젝트 문서를 `toc` 및 TUI Object Converter로 빠짐없이 통일한다.

**Architecture:** 이름을 별도 추상화로 감싸지 않고 각 소유 경계의 기존 상수를 직접 교체한다. 실제 실행 파일·도움말·TUI를 검증하는 시험을 먼저 실패시킨 뒤 Cargo와 Rust 구현을 변경하고, 마지막에 문서와 프로젝트 메타데이터를 원자적으로 이동한다.

**Tech Stack:** Rust 2024, Cargo, Clap 4, Crossterm 0.29, Ratatui 0.30, Bash, Zsh, Expect

## Global Constraints

- 기준 설계는 `docs/superpowers/specs/2026-07-31-toc-rename-design.md`다.
- 제품명과 실행 명령어는 `toc`, 한글 발음은 톡 (`toc`), 풀네임은 TUI Object Converter다.
- Cargo 패키지·Rust 크레이트·실행 파일은 모두 `toc`, TUI 상단 제목은 `>_ TOC`다.
- 시험 환경변수 접두사는 `TOC_SMOKE_`, 임시 자원 접두사는 `toc-`다.
- 이전 실행 이름의 호환 별칭은 제공하지 않는다.
- 버전 `0.2.0`, 8개 변환 ID, CLI 문법, TUI 동작, 오류와 종료 코드는 변경하지 않는다.
- 저장소 경로 `/Users/ruffin/mydev/beep`, Git 기록, 무시된 `target/`, 저장소 밖 설치물은 변경하거나 삭제하지 않는다.
- 새 의존성, 이름 추상화, 배포 자동화와 이름 변경 밖의 리팩터링을 추가하지 않는다.
- 코드, 시험, README·PRD·설계·계획·Serena 현행화는 하나의 구현 커밋에 포함한다.

## File Map

| 파일 | 책임 |
| --- | --- |
| `Cargo.toml`, `Cargo.lock` | 패키지·크레이트·실행 파일 이름과 패키지 설명 |
| `src/main.rs` | 라이브러리 크레이트 경로 |
| `src/cli.rs` | Clap 루트 이름, 풀네임, 도움말과 시험 입력 |
| `src/tui.rs` | TUI 진입 오류와 시험 기대값 |
| `src/tui/render.rs` | TUI 상단 제목과 렌더 시험 기대값 |
| `tests/cli.rs` | 실제 Cargo 실행 파일, 도움말·버전·오류 통합 계약 |
| `tests/shell-smoke.sh` | 실제 `toc` 실행, `TOC_SMOKE_*`, 설치·PTY 회귀 |
| `README.md`, `docs/prd/*.md` | 사용자 이름, 발음, 풀네임, 명령과 설치 예시 |
| `docs/superpowers/specs/*.md`, `docs/superpowers/plans/*.md` | 보존 설계·계획의 제품 이름과 상호 참조 |
| `.serena/project.yml`, `.serena/memories/*.md` | 프로젝트 도구 이름과 현행 명령 |

---

### Task 1: 애플리케이션 전체 이름 원자적 교체

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock`
- Modify: `src/main.rs`, `src/cli.rs`, `src/tui.rs`, `src/tui/render.rs`
- Modify: `tests/cli.rs`, `tests/shell-smoke.sh`
- Modify: `README.md`, `docs/prd/init-prd.md`, `docs/prd/v0.2-prd.md`
- Modify: `.serena/project.yml`, `.serena/memories/*.md`
- Rename: `docs/superpowers/specs/2026-07-29-d[o]op-v0.1-design.md` → `docs/superpowers/specs/2026-07-29-toc-v0.1-design.md`
- Rename: `docs/superpowers/specs/2026-07-29-d[o]op-v0.2-hex-design.md` → `docs/superpowers/specs/2026-07-29-toc-v0.2-hex-design.md`
- Rename: `docs/superpowers/specs/2026-07-31-d[o]op-maintenance-design.md` → `docs/superpowers/specs/2026-07-31-toc-maintenance-design.md`
- Rename: `docs/superpowers/specs/2026-07-31-d[o]op-tui-workbench-design.md` → `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md`
- Rename: `docs/superpowers/specs/2026-07-31-d[o]op-tui-ux-refresh-design.md` → `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md`
- Rename: `docs/superpowers/specs/2026-07-31-d[o]op-tui-mouse-design.md` → `docs/superpowers/specs/2026-07-31-toc-tui-mouse-design.md`
- Rename: `docs/superpowers/plans/2026-07-31-d[o]op-maintenance.md` → `docs/superpowers/plans/2026-07-31-toc-maintenance.md`
- Rename: `docs/superpowers/plans/2026-07-31-d[o]op-tui-ux-refresh.md` → `docs/superpowers/plans/2026-07-31-toc-tui-ux-refresh.md`
- Rename: `docs/superpowers/plans/2026-07-31-d[o]op-tui-mouse.md` → `docs/superpowers/plans/2026-07-31-toc-tui-mouse.md`

**Interfaces:**
- Consumes: `cli::command() -> clap::Command`, `tui::check_terminal_entry(bool, bool) -> Result<(), AppError>`, `render_app_bar(&mut Frame, &App, Rect)`와 기존 Shell Smoke helper
- Produces: 단일 Cargo 패키지·크레이트·실행 파일 `toc`, `toc --help`, `toc --version`, `toc tui`, `>_ TOC`, `TOC_SMOKE_*`와 새 문서 경로

- [ ] **Step 1: 깨끗한 기준선과 기존 회귀 확인**

```bash
git status --short --branch
cargo test --all-targets --all-features --locked
```

Expected: 작업 트리가 깨끗하고 일반 시험 255개 통과·3개 무시·실패 0이다.

- [ ] **Step 2: 새 실행 파일 계약을 검증하는 실패 시험 추가**

`tests/cli.rs`에 실제 Cargo 바이너리를 실행하는 다음 시험을 추가한다.

```rust
#[test]
fn cargo_builds_a_toc_binary_that_identifies_itself() {
    let binary = option_env!("CARGO_BIN_EXE_toc").expect("Cargo must build the toc binary");
    let output = Command::new(binary).arg("--version").output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"toc 0.2.0\n");
    assert!(output.stderr.is_empty());
}
```

Mutation check: Cargo 실행 파일 이름이나 Clap 루트 이름이 다른 값이면 각각
`expect` panic 또는 stdout 불일치로 실패해야 한다.

- [ ] **Step 3: 실행 파일 시험이 올바른 이유로 실패하는지 확인**

```bash
cargo test --locked --test cli cargo_builds_a_toc_binary_that_identifies_itself -- --exact
```

Expected: `Cargo must build the toc binary` panic으로 FAIL한다. 철자나 시험 설정이
아니라 현재 Cargo가 `toc` 실행 파일을 만들지 않는 것이 실패 원인이다.

- [ ] **Step 4: Cargo 패키지·크레이트·실행 파일만 최소 변경**

`Cargo.toml`의 패키지 부분을 다음 값으로 바꾼다.

```toml
[package]
name = "toc"
version = "0.2.0"
edition = "2024"
rust-version = "1.97.1"
license = "MIT OR Apache-2.0"
description = "TUI Object Converter for local text and byte transformations"
```

`Cargo.lock`의 루트 패키지 이름을 `toc`로 바꾸고, `src/main.rs`의 라이브러리
경로를 모두 다음 형태로 교체한다.

```rust
use toc::{
    cli::{Invocation, ParseOutcome},
    error::{AppError, escape_external},
    transforms::transforms,
};
```

같은 파일의 완전 수식 경로는 각각 `toc::cli::parse_from`,
`toc::cli::write_result`, `toc::cli::run_transform`,
`toc::tui::check_terminal_entry`, `toc::tui::run`,
`toc::error::render_app_error`로 바꾼다.

`src/cli.rs`의 `command()`에서는 우선 루트 이름만 바꾼다.

```rust
let mut command = Command::new("toc")
    .version(env!("CARGO_PKG_VERSION"))
    .about("Local text transformations")
    .after_help("Transform help: toc <transform-id> --help")
    .disable_help_subcommand(true)
    .args_conflicts_with_subcommands(true);
```

`tests/cli.rs`의 공용 실행 helper도 새 Cargo 실행 파일을 사용한다.

```rust
let mut child = Command::new(env!("CARGO_BIN_EXE_toc"))
    .args(args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
```

- [ ] **Step 5: 새 Cargo 실행 파일 계약을 통과하는지 확인**

```bash
cargo test --locked --test cli cargo_builds_a_toc_binary_that_identifies_itself -- --exact
```

Expected: 1개 시험이 실행되고 PASS한다.

- [ ] **Step 6: 공개 CLI·TUI 이름 기대값을 먼저 변경**

`tests/cli.rs`의 도움말·버전·TUI 오류 계약을 다음 리터럴로 갱신한다.

```rust
for token in [
    "TUI Object Converter",
    "Usage: toc [OPTIONS]",
    "Commands:",
    "tui",
    "--list",
    "Transform help: toc <transform-id> --help",
] {
    assert!(help.contains(token), "{args:?}: {token}");
}

assert_eq!(output.stdout, b"toc 0.2.0\n");
assert_eq!(
    output.stderr,
    b"TUI error: toc tui requires terminal stdin and stdout\n"
);
```

버전 시험 이름은 `version_reports_toc_v0_2_0`으로 바꾼다. `src/tui.rs`의 시험은
`TUI error: toc tui requires terminal stdin and stdout`을 기대한다.
`src/tui/render.rs`의 상단 제목 시험은 `>_ TOC`를 기대하고 Tiny 화면에서는
`TOC`가 렌더되지 않음을 확인한다.

`tests/shell-smoke.sh`는 다음 이름 규칙으로 먼저 바꾼다.

```bash
perl -pi -e 's/D[O]OP/TOC/g; s/d[o]op/toc/g' tests/shell-smoke.sh
```

```bash
smoke_bin="$smoke_target_dir/debug/toc"
smoke_tmp=$(mktemp -d "${TMPDIR:-/tmp}/toc-smoke.XXXXXX")
export TOC_SMOKE_SHELL="$smoke_shell"
export TOC_SMOKE_SHELL_KIND="$smoke_shell_kind"
export TOC_SMOKE_BIN="$smoke_bin"
export TOC_SMOKE_INPUT="$smoke_input"
export TOC_SMOKE_OUTPUT="$smoke_output"
export TOC_SMOKE_ERROR="$smoke_error"
export TOC_SMOKE_TEXT_EXPECTED="$smoke_text_expected"
export TOC_SMOKE_CLIPBOARD_EXPECTED="$smoke_clipboard_expected"
```

나머지 전용 환경변수와 Expect `$env(...)`, 지역 상태 변수도 각각
`TOC_SMOKE_*`, `toc_status`로 통일한다. Cargo 빌드 직후 도움말을 검증한다.

```bash
actual=$("$smoke_bin" --help)
case "$actual" in
    *"TUI Object Converter"*) ;;
    *) fail "root help omitted the full product name" ;;
esac
```

- [ ] **Step 7: 공개 문자열 시험이 구현 전 실패하는지 확인**

```bash
cargo test --locked --test cli help_version_and_list_are_successful_english_output -- --exact
cargo test --locked tui::tests::tui_requires_both_standard_streams_to_be_terminals -- --exact
cargo test --locked tui::render::tests::app_bar_is_unboxed_and_footer_has_exactly_two_roles -- --exact
bash tests/shell-smoke.sh
```

Expected: CLI는 풀네임 부재, TUI 진입은 이전 오류 문자열, 렌더는 이전 제목,
Shell Smoke는 `root help omitted the full product name`으로 각각 FAIL한다.

- [ ] **Step 8: CLI·TUI 공개 문자열과 내부 시험 입력을 최소 변경**

남은 Rust 이름 리터럴은 명시된 네 파일에서만 기계적으로 교체한다.

```bash
perl -pi -e 's/d[o]op/toc/g; s/D[O]OP/TOC/g' \
    src/cli.rs src/tui.rs src/tui/render.rs tests/cli.rs
```

`src/cli.rs`의 최종 루트 명령은 다음과 같다.

```rust
let mut command = Command::new("toc")
    .version(env!("CARGO_PKG_VERSION"))
    .about("TUI Object Converter")
    .after_help("Transform help: toc <transform-id> --help")
    .disable_help_subcommand(true)
    .args_conflicts_with_subcommands(true);
```

같은 파일의 시험 입력 첫 원소는 모두 `toc`, 임시 입력 접두사는 `toc-input-`으로
바꾼다. `src/tui.rs`의 오류 본문은 `toc tui requires terminal stdin and stdout`,
`src/tui/render.rs`의 제목은 다음 리터럴로 교체한다.

```rust
Span::styled(">_ TOC", title_style)
```

`tests/cli.rs`의 임시 입력 접두사도 `toc-cli-input-`으로 바꾼다. 상태 전이,
레이아웃, 변환과 오류 분류 코드는 수정하지 않는다.

- [ ] **Step 9: 공개 이름 집중 시험과 두 셸의 실제 동작 확인**

```bash
cargo test --locked --test cli
cargo test --locked tui::tests::tui_requires_both_standard_streams_to_be_terminals -- --exact
cargo test --locked tui::render::tests::app_bar_is_unboxed_and_footer_has_exactly_two_roles -- --exact
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
```

Expected: CLI 통합 시험 16개와 두 TUI 집중 시험이 PASS하고 Bash·Zsh가 각각
`shell smoke passed`로 끝난다.

- [ ] **Step 10: 문서·메타데이터의 이전 식별자와 파일명 잔존 확인**

```bash
rg --color=never --hidden -n -i -g '!target/**' -g '!.git/**' -g '!.cocoindex_code/**' 'd[o]op' .
fd --color=never --hidden --ignore-case --exclude target --exclude .git --exclude .cocoindex_code 'd[o]op' .
```

Expected: 첫 명령은 README, PRD, 보존 문서와 Serena 메모리의 남은 내용 참조를
보여 주고, 둘째 명령은 이동 전 보존 문서 9개를 보여 준다. `src/`와 `tests/`에
결과가 있으면 Step 8의 누락으로 간주해 먼저 고친다.

- [ ] **Step 11: 사용자 문서·보존 문서·Serena 이름 일괄 현행화**

사람용 문서와 Serena 메모리의 이전 식별자를 정확히 치환한다. 괄호 패턴은 계획
파일 자체에 이전 리터럴을 다시 남기지 않으면서 현재 파일만 선택한다.

```bash
perl -pi -e 's/d[o]op/toc/g; s/D[O]OP/TOC/g' \
    README.md \
    docs/prd/init-prd.md \
    docs/prd/v0.2-prd.md \
    .serena/memories/*.md \
    docs/superpowers/specs/*d[o]op*.md \
    docs/superpowers/plans/*d[o]op*.md
```

README 첫 소개는 다음 문장으로 고정한다.

```markdown
# toc — TUI Object Converter

`toc`은 TUI Object Converter의 약자이며 한글 발음은 톡 (`toc`)입니다.
로컬에서 동작하는 텍스트 변환 CLI이자 비파괴 TUI 작업판이며, 입력과 변환
결과를 네트워크로 전송하지 않습니다.
```

`docs/prd/init-prd.md`의 제품 식별 절에는 다음 값을 기록하고,
`docs/prd/v0.2-prd.md`의 제목과 후속 링크도 `toc` 경로를 사용한다.

```markdown
* **제품명 및 실행 파일명:** `toc`
* **한글 발음:** 톡 (`toc`)
* **풀네임:** TUI Object Converter
```

`.serena/project.yml`은 다음 값으로 바꾼다.

```yaml
project_name: "toc"
```

내용 치환 뒤 보존 문서를 다음 명령으로 이동한다.

```bash
mv docs/superpowers/specs/2026-07-29-d[o]op-v0.1-design.md docs/superpowers/specs/2026-07-29-toc-v0.1-design.md
mv docs/superpowers/specs/2026-07-29-d[o]op-v0.2-hex-design.md docs/superpowers/specs/2026-07-29-toc-v0.2-hex-design.md
mv docs/superpowers/specs/2026-07-31-d[o]op-maintenance-design.md docs/superpowers/specs/2026-07-31-toc-maintenance-design.md
mv docs/superpowers/specs/2026-07-31-d[o]op-tui-workbench-design.md docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md
mv docs/superpowers/specs/2026-07-31-d[o]op-tui-ux-refresh-design.md docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md
mv docs/superpowers/specs/2026-07-31-d[o]op-tui-mouse-design.md docs/superpowers/specs/2026-07-31-toc-tui-mouse-design.md
mv docs/superpowers/plans/2026-07-31-d[o]op-maintenance.md docs/superpowers/plans/2026-07-31-toc-maintenance.md
mv docs/superpowers/plans/2026-07-31-d[o]op-tui-ux-refresh.md docs/superpowers/plans/2026-07-31-toc-tui-ux-refresh.md
mv docs/superpowers/plans/2026-07-31-d[o]op-tui-mouse.md docs/superpowers/plans/2026-07-31-toc-tui-mouse.md
```

- [ ] **Step 12: 이름 내용·파일명·링크의 완전 교체 확인**

```bash
rg --color=never --hidden -n -i -g '!target/**' -g '!.git/**' -g '!.cocoindex_code/**' 'd[o]op' .
fd --color=never --hidden --ignore-case --exclude target --exclude .git --exclude .cocoindex_code 'd[o]op' .
rg --color=never -n 'TUI Object Converter|톡 \(`toc`\)' README.md docs/prd
```

Expected: 첫 명령은 일치 없음으로 stdout 없이 종료 코드 1, 둘째 명령은 stdout
없이 종료 코드 0이다. 셋째 명령은 README와 PRD에서 풀네임과 발음을 찾는다.

- [ ] **Step 13: 전체 품질·문서·PTY·패키지 검증**

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
env RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
cargo package --locked --allow-dirty
```

Expected: 형식 차이·Clippy·rustdoc 경고와 시험 실패가 없다. 일반 시험은 새 CLI
식별 시험을 포함해 256개 통과·3개 무시이고, 두 셸은 `shell smoke passed`,
패키지는 `toc v0.2.0`을 생성·검증한다.

- [ ] **Step 14: 새 임시 설치 루트에서 단일 실행 파일 확인**

```bash
toc_install_root=$(mktemp -d /private/tmp/toc-install.XXXXXX)
cargo install --locked --offline --path . --root "$toc_install_root"
"$toc_install_root/bin/toc" --version
fd --color=never --type f --max-depth 1 . "$toc_install_root/bin"
```

Expected: 버전 출력이 정확히 `toc 0.2.0`이고 `bin/`의 실행 파일은 `toc` 하나다.
정리 전에 임시 경로가 예상 접두사인지 검증한다.

```bash
test -n "$toc_install_root"
test "${toc_install_root#/private/tmp/toc-install.}" != "$toc_install_root"
rm -r -- "$toc_install_root"
```

- [ ] **Step 15: 코드 검색 색인과 diff 위생 확인**

```bash
ccc index
git diff --check
git status --short
git diff --stat
```

Expected: `ccc index` 오류 0, 공백 오류 없음, 의도한 이름 변경·문서 이동만 표시된다.

- [ ] **Step 16: 코드와 관련 문서를 하나의 논리적 변경으로 커밋**

변경 목록에 사용자 소유의 범위 밖 파일이 없는지 확인한 뒤 전체 이름 변경을 같은
커밋에 넣는다.

```bash
git add -A
git commit -m "feat(toc): 애플리케이션 이름 통일"
```

Expected: Conventional Commits 형식의 한국어 커밋 하나가 생성되고 트레일러가 없다.

- [ ] **Step 17: 커밋 후 정확한 패키징과 깨끗한 상태 재검증**

```bash
cargo package --locked
git status --short --branch
git log -2 --oneline
```

Expected: dirty 허용 없이 `toc v0.2.0` 패키징이 통과하고 작업 트리가 깨끗하다.
