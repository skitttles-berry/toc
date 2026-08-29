# toc 애플리케이션 이름 변경 설계

## 1. 결정

애플리케이션의 단일 정식 이름을 다음과 같이 고정한다.

| 구분 | 값 |
| --- | --- |
| 제품명 | `toc` |
| 한글 발음 | 톡 (`toc`) |
| 풀네임 | TUI Object Converter |
| 실행 명령어 | `toc` |
| Cargo 패키지·Rust 크레이트·실행 파일 | `toc` |
| TUI 상단 제목 | `>_ TOC` |
| 시험 환경변수 접두사 | `TOC_SMOKE_` |
| 임시 파일·디렉터리 접두사 | `toc-` |

기존 실행 이름의 호환 별칭은 제공하지 않는다. 버전은 `0.2.0`을 유지하며,
변환 ID, CLI 문법, TUI 동작, 입력·출력과 오류 처리 규약은 변경하지 않는다.

## 2. 목표와 제외 범위

### 목표

* 사용자에게 보이는 제품명, 도움말, 오류, 설치 결과와 TUI 제목을 하나의 이름으로 통일한다.
* Cargo 패키지, Rust 크레이트와 실행 파일의 내부 식별자도 공개 이름과 일치시킨다.
* 시험 코드, 환경변수, 임시 경로, 프로젝트 문서와 문서 파일명에서 이전 식별자를 제거한다.
* README 첫 소개와 제품 요구사항에 한글 발음과 풀네임을 명시한다.

### 제외 범위

* 저장소 작업 경로 `/Users/ruffin/mydev/beep`는 애플리케이션 공개 인터페이스가 아니므로 변경하지 않는다.
* Git 커밋 기록, 태그와 과거 외부 기록은 다시 작성하지 않는다.
* `target/` 아래의 무시된 빌드 산출물은 직접 수정하거나 삭제하지 않는다. 새 빌드가 `toc` 산출물을 생성한다.
* 저장소 밖에 이미 설치된 이전 실행 파일은 자동 삭제하지 않는다.
* 이름 변경과 무관한 기능, 의존성, 추상화와 배포 자동화는 추가하지 않는다.

## 3. 변경 범위

### Cargo와 Rust

`Cargo.toml`의 패키지 이름을 변경해 기본 라이브러리 크레이트와 실행 파일이
`toc`가 되도록 한다. `Cargo.lock`의 루트 패키지 이름도 같은 값으로 갱신한다.
`src/main.rs`의 크레이트 경로, Clap 루트 명령과 도움말, TUI 진입 오류와 TUI
상단 제목을 새 식별자로 교체한다. `toc --help`에는 풀네임을 표시하고,
`toc --version`은 정확히 `toc 0.2.0`을 출력한다.

### 시험과 임시 자원

CLI 통합 시험은 `CARGO_BIN_EXE_toc`로 빌드된 실행 파일을 호출한다. Shell Smoke는
`target/debug/toc`를 사용하고 모든 전용 환경변수를 `TOC_SMOKE_*`로 통일한다.
시험의 지역 변수와 임시 파일·설치 루트 접두사도 같은 이름 규칙을 따른다.
이 변경은 시험 격리와 민감한 입력 보호 규칙을 바꾸지 않는다.

### 사용자 문서와 프로젝트 메타데이터

README, 두 PRD, 현재 보존된 설계·계획 문서와 Serena 메모리의 제품 식별자,
명령 예시, 설치 경로와 환경변수를 모두 갱신한다. 제품을 처음 소개하는 문단에는
`toc`, 톡 (`toc`), TUI Object Converter를 함께 기록한다. 영어 CLI·TUI 문자열에는
한글을 섞지 않고 기존 언어 정책을 유지한다.

이전 식별자가 포함된 보존 문서 파일은 다음 이름으로 이동하고 모든 내부 링크를
새 경로에 맞춘다.

* `docs/superpowers/specs/2026-07-29-toc-v0.1-design.md`
* `docs/superpowers/specs/2026-07-29-toc-v0.2-hex-design.md`
* `docs/superpowers/specs/2026-07-31-toc-maintenance-design.md`
* `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md`
* `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md`
* `docs/superpowers/specs/2026-07-31-toc-tui-mouse-design.md`
* `docs/superpowers/plans/2026-07-31-toc-maintenance.md`
* `docs/superpowers/plans/2026-07-31-toc-tui-ux-refresh.md`
* `docs/superpowers/plans/2026-07-31-toc-tui-mouse.md`

`.serena/project.yml`의 프로젝트 이름과 `.serena/memories/`의 현재 프로젝트 설명도
`toc`로 맞춘다. 실제 저장소 디렉터리 이름은 그대로 둔다.

## 4. 동작과 오류 처리

명령 자료 흐름은 기존과 같다. 운영체제가 `toc`를 실행하면 Clap이 동일한 옵션과
변환 하위 명령을 해석하고, 공용 레지스트리와 Pipeline을 거쳐 기존 바이트 결과와
종료 코드를 반환한다. TUI도 같은 상태 전이, 작업자, 렌더링과 터미널 복구 경로를
사용한다. 이름 변경은 사용자 입력이나 변환 결과를 재작성하지 않는다.

기존 실행 이름은 명령을 찾을 수 없는 운영체제 오류가 되며 애플리케이션 내부에서
별도 안내나 우회 처리를 만들지 않는다. 저장소 밖의 기존 설치물을 자동 제거하지
않아 의도하지 않은 파일 삭제를 방지한다.

## 5. 시험 우선 구현

1. CLI 도움말, 버전, TUI 오류와 TUI 제목의 기대값을 먼저 새 이름으로 바꾸고,
   현재 구현에서 이름 불일치로 실패하는지 확인한다.
2. Shell Smoke의 실행 파일과 환경변수 계약을 먼저 새 이름으로 바꾸고, 현재
   Cargo 패키지가 해당 실행 파일을 만들지 않아 실패하는지 확인한다.
3. Cargo 패키지와 Rust 구현을 최소 변경해 집중 시험을 통과시킨다.
4. 문서 내용과 파일명을 이동하고 상호 참조를 갱신한다.
5. 추적 파일 내용과 파일명에서 이전 식별자가 남지 않았는지 대소문자 구분 없이
   전수 검색한다. Git 기록과 무시된 `target/`은 검색 대상에서 제외한다.

최종 검증은 다음을 모두 포함한다.

* `cargo fmt --check`
* `cargo clippy --all-targets --all-features -- -D warnings`
* `cargo test --all-targets --all-features --locked`
* 경고를 오류로 처리한 rustdoc 생성
* `cargo package --locked`
* 새 임시 설치 루트에 잠금·오프라인 설치 후 `bin/toc --version` 확인
* Bash와 Zsh Shell Smoke
* `ccc index`, 공백 오류 검사와 깨끗한 작업 트리 확인

## 6. 완료 조건

* Cargo가 단일 `toc` 패키지와 단일 `toc` 실행 파일을 생성한다.
* `toc --help`, `toc --version`, 변환 명령과 `toc tui`가 기존 기능 규약대로 동작한다.
* TUI 제목은 `>_ TOC`이며 관련 오류와 도움말에 이전 식별자가 없다.
* README와 PRD가 한글 발음 톡 (`toc`)과 풀네임 TUI Object Converter를 명시한다.
* 추적 파일의 내용과 파일명에 이전 식별자가 남지 않는다.
* 전체 자동 시험과 두 셸의 PTY 검증이 실패 없이 끝난다.
