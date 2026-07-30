# doop 전체 기능 감사 및 최소 리팩터링 구현 계획

> **에이전트 작업자 필수 지침:** `superpowers:subagent-driven-development`를 사용해 작업별로 구현·검토하고, 모든 코드 변경은 `superpowers:test-driven-development`의 실패-성공 순서를 따른다.

**목표:** doop v0.2의 모든 공개 CLI·TUI 기능을 실제 경계까지 검증하고, 리뷰로 확인된 중복 상태와 불필요한 공개 범위만 최소 리팩터링한다.

**구조:** 기존 단일 Cargo 패키지, 정적 변환 레지스트리, 공용 파이프라인과 단일 TUI 모듈을 유지한다. 회귀 시험을 먼저 추가한 뒤 오류 생성과 TUI 상태 표현을 축소하고, macOS와 격리된 Linux 환경의 실제 결과를 문서화한다.

**기술:** Rust 1.97.1, Cargo, Clap, Ratatui, Crossterm, tui-textarea-2, arboard, Expect, Bash, Zsh, Docker.

## 전역 제약

- 공개 제품 계약은 여덟 변환 식별자, 직접 실행 CLI, `--then`, `--input`, `--list`, `doop tui`, 출력 바이트, 오류 문구와 종료 코드다.
- Rust 라이브러리의 문서화되지 않은 TUI 내부 `pub` 항목은 호환성 계약이 아니다.
- 새 기능, 새 의존성, TUI 파일 분할, 일괄 의존성 업그레이드, GitHub Actions를 추가하지 않는다.
- macOS와 Linux의 Bash·Zsh를 검증하고, X11과 지원되는 Wayland 자료 제어 환경의 클립보드를 검증한다.
- macOS 클립보드는 기존 텍스트를 안전하게 백업할 수 있을 때만 변경하고 시험 뒤 반드시 복원한다.
- 코드와 관련 문서는 같은 논리적 커밋에서 갱신한다.
- 커밋은 한국어 Conventional Commits 형식, 50자 이내, 명사형 종결문을 사용한다.

---

### Task 1: CLI 경계와 공통 디코딩 오류

**파일:**
- 수정: `tests/cli.rs`
- 수정: `src/error.rs`
- 수정: `src/transforms/base64.rs`
- 수정: `src/transforms/url.rs`
- 수정: `src/transforms/hex.rs`

**인터페이스:**
- 추가: `pub(crate) fn invalid_utf8_output(bytes: &[u8]) -> TransformError`
- 변경: `AppError::Usage(String)`을 `AppError::Usage`로 축소
- 변경: `AppError::Output(std::io::ErrorKind)`을 `AppError::Output`으로 축소
- 제거: 생성되지 않는 `AppError::Internal`
- 유지: 모든 공개 오류 문구와 종료 코드

- [ ] 존재하지 않는 `--input` 경로가 코드 `3`, 빈 표준 출력, 안전한 표준 오류를 만드는 통합 시험을 먼저 추가하고 실패 원인을 확인한다.
- [ ] Base64·URL·Hex 세 디코더의 65바이트 비 UTF-8 결과가 같은 64바이트 미리보기와 전체 길이를 보고하는 시험을 먼저 추가하고, Hex가 빠져 실패함을 확인한다.
- [ ] `invalid_utf8_output`을 최소 구현하고 세 디코더의 중복 오류 생성을 교체한다.
- [ ] 사용되지 않는 `AppError` 변형과 자료를 축소하고 기존 오류 출력·종료 코드 시험을 통과시킨다.
- [ ] 관련 단위·통합 시험, 형식 검사와 경고 금지 린트를 실행한다.
- [ ] `refactor(error): 디코딩 오류 처리 통합`으로 커밋한다.

### Task 2: TUI 상태와 공개 범위 축소

**파일:**
- 수정: `src/tui.rs`

**인터페이스:**
- 유지: `pub fn check_terminal_entry(...) -> Result<(), AppError>`
- 유지: `pub fn run() -> Result<i32, AppError>`
- 변경: `PreviewState::Running`은 단위 변형, `Ready { document }`, `Error { message }`
- 유지: `App::generation`, `PreviewJob::generation`, `PreviewResult::generation`을 오래된 결과 판정 기준으로 사용

- [ ] 오래된 작업 결과가 현재 상태를 바꾸지 않고 새 결과만 반영하는 기존 시험을 상태의 중복 세대 번호 없이 표현하도록 먼저 변경하고 컴파일 실패를 확인한다.
- [ ] `Running`, `Ready`, `Error`에서 읽히지 않는 세대 번호를 제거하고 모든 상태 전이·렌더 시험을 통과시킨다.
- [ ] 외부 사용이 없는 TUI 상태, 모달, 이벤트, 효과, 문서, 작업자, 렌더 함수와 메서드의 `pub`를 제거한다.
- [ ] `tui.rs`를 분할하거나 새 추상화를 만들지 않는다.
- [ ] TUI 단위 시험, 전체 형식 검사와 경고 금지 린트를 실행한다.
- [ ] `refactor(tui): 내부 상태 공개 범위 축소`로 커밋한다.

### Task 3: 실제 셸·의사 터미널 기능 검증

**파일:**
- 수정: `tests/shell-smoke.sh`

**인터페이스:**
- 기존 `DOOP_SMOKE_CLIPBOARD_MODE=skip|unavailable|x11`을 유지한다.
- 추가: macOS 전용 선택 실행 모드로 호스트 텍스트 클립보드 복사를 검증하되 스크립트 종료 경로에서 원문을 복원한다.
- 추가: Wayland 전용 선택 실행 모드에서 TUI가 살아 있는 동안 외부 붙여넣기 도구로 정확한 값을 읽는다.

- [ ] 의사 터미널로 위험한 CLI 결과를 직접 출력할 때 코드 `4`와 빈 표준 출력을 검증하는 셸 시험을 먼저 추가하고, 현재 스크립트에 경로가 없어 실패함을 확인한다.
- [ ] 같은 결과를 파일로 재지정하면 원본 제어 바이트가 보존되는지 검증한다.
- [ ] TUI에서 `hello` 입력, `hex-encode` 추가, `68656c6c6f` 미리보기, 복사를 실제 키 입력으로 검증한다.
- [ ] macOS 클립보드 시험은 텍스트 백업 성공 때만 실행하며 신호·실패·성공 모든 종료 경로에서 복원한다.
- [ ] Wayland 시험은 데이터 제어 프로토콜을 지원하는 compositor에서만 실행하고, 화면 환경 부재와 제품 오류를 다른 메시지로 보고한다.
- [ ] Bash와 Zsh에서 기본 스모크 시험을 실행하고 선택적 클립보드 모드는 해당 환경에서 별도로 실행한다.
- [ ] `test(platform): 실제 터미널 경계 확대`로 커밋한다.

### Task 4: 전체 플랫폼·보안 검증과 문서 동기화

**파일:**
- 수정: `README.md`

**인터페이스:** 공개 동작 변경 없음.

- [ ] macOS에서 형식 검사, 경고 금지 린트, 전체 시험, 릴리스 렌더링 측정, 문서 빌드, 패키징, 오프라인 잠금 설치와 버전 확인을 실행한다.
- [ ] macOS Bash·Zsh 기본 스모크와 백업·복원형 실제 클립보드 시험을 실행한다.
- [ ] 저장소를 읽기 전용으로 연결한 Linux 컨테이너에서 Bash·Zsh, `/dev/full`, Xvfb/X11 복사를 검증한다.
- [ ] wlroots headless compositor와 `wl-paste`를 사용해 지원되는 Wayland 자료 제어 복사를 검증한다. 환경 구성이 불가능하면 정확한 실패 원인과 미검증 상태를 기록한다.
- [ ] 최신 RustSec 데이터베이스로 `Cargo.lock`을 감사한다.
- [ ] README의 기존 2026-07-30 기록을 v0.1 역사 기록으로 명확히 구분하고, 현재 커밋·환경·명령·성공·미검증 결과를 v0.2 기록으로 추가한다.
- [ ] 문서 변경 후 전체 필수 검증을 다시 실행하고 `git diff --check`와 작업 트리를 점검한다.
- [ ] `docs(review): v0.2 전체 검증 기록`으로 커밋한다.

### Task 5: 최종 독립 리뷰와 완료 판정

- [ ] 시작 커밋부터 현재 HEAD까지 전체 diff를 독립 리뷰어에게 전달해 요구사항, 정확성, 보안, 회귀, 과설계를 검토한다.
- [ ] 중요 이상 발견은 하나의 수정 작업으로 처리하고 해당 시험을 다시 실행한 뒤 범위 재검토를 받는다.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`, 릴리스 렌더링 시험, Bash·Zsh 스모크, 오프라인 설치를 새로 실행한다.
- [ ] 공개 계약과 문서가 실제 결과와 일치하는지 확인하고 완료 상태를 보고한다.
