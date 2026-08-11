# toc README TUI Guide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** README의 TUI 사용법을 Terminal Blueprint와 항목별 표로 고도화해 화면 구조와 실제 조작을 빠르게 파악할 수 있게 한다.

**Architecture:** 제품 코드는 바꾸지 않고 `README.md`의 `TUI 사용법` 섹션만 교체한다. 기존 미커밋 README 수정은 작업 전후 diff로 보존하고, 커밋할 때 TUI hunk만 부분 스테이징한다.

**Tech Stack:** GitHub Flavored Markdown, `text` 코드 블록, `<kbd>`, Rust/Cargo

## Global Constraints

- `README.md`의 `TUI 사용법` 섹션만 수정한다.
- 현재 미커밋 변경에서 제거된 안전 경계 설명, 문서 링크, Apache-2.0 링크를 복원하거나 커밋하지 않는다.
- 외부 이미지, GIF, 배지, 스타일시트를 추가하지 않는다.
- Terminal Blueprint는 개념 예시임을 밝히고 실제 변환 결과만 사용한다.
- 단계, Output View, 단축키는 항목당 한 행으로 표시한다.
- 키나 View 이름을 쉼표로 길게 이어 쓰지 않는다.
- 제품 코드, TUI 화면, 키 바인딩, View 동작, 의존성을 변경하지 않는다.
- 커밋에는 README의 TUI 변경 hunk만 포함한다.

---

### Task 1: TUI 사용법 시각화와 항목별 안내

**Files:**
- Modify: `README.md:46-67`
- Reference: `docs/superpowers/specs/2026-08-11-toc-readme-tui-guide-design.md`
- Reference: `src/tui/render.rs`
- Reference: `src/tui/state.rs`

**Interfaces:**
- Consumes: 현재 `TUI 사용법` Markdown, `OutputView`, 실제 TUI 키 처리, `toc base64-encode --then sha256`
- Produces: Terminal Blueprint, 4단계 시작 표, Output View 표, 키캡 표가 포함된 `TUI 사용법`

- [ ] **Step 1: 기존 사용자 변경과 실제 예시 결과를 확인한다**

Run:

```bash
git diff -- README.md
printf '%s' 'hello' | target/debug/toc base64-encode --then sha256
```

Expected: 기존 README diff에는 TUI 밖의 안전 경계·문서·Apache-2.0 제거가 있고, 변환 결과는 다음 값이다.

```text
333d6b3a3c1f5db6c9bdda5939b136986d170f4649172a68368d54ecb44c2ff2
```

- [ ] **Step 2: TUI 사용법 섹션만 교체한다**

`## TUI 사용법`부터 `## 지원 변환` 직전까지 다음 구조로 바꾼다.

````markdown
## TUI 사용법

TUI는 원본을 덮어쓰지 않습니다. Input에서 시작해 Pipeline으로 변환을 연결하고 Output에서 결과를 확인합니다.

### 화면 구성

다음은 세 패널의 관계를 보여주는 개념 예시입니다.

```text
>_ TOC

┌ PIPELINE ────────────────────┐  ┌ INPUT ──────────────────────┐
│ > 01 Base64 Encode    5→8 B │  │ hello                       │
│   02 SHA-256         8→64 B │  └─────────────────────────────┘
└─────────────────────────────┘  ┌ OUTPUT [SMART] [64 B] ──────┐
                                  │ 333d6b3a3c1f…               │
                                  └─────────────────────────────┘
```

터미널 크기에 따라 실제 배치는 달라질 수 있습니다.

### 4단계로 시작

| 단계 | 작업 | 방법 |
|---:|---|---|
| 1 | 입력 | Input에 원문 작성 |
| 2 | 추가 | <kbd>Ctrl</kbd> + <kbd>p</kbd>로 변환 선택 |
| 3 | 실행 | <kbd>s</kbd>로 선택 단계 실행 |
| 4 | 확인 | Output에서 결과 확인 |

### Output View

| View | 용도 |
|---|---|
| `SMART` | 결과 형식에 알맞은 View 자동 선택 |
| `TEXT` | UTF-8 텍스트 확인 |
| `HEX` | 바이트를 Offset·Hex·ASCII 열로 확인 |
| `TRACE` | Pipeline 단계별 상태와 안전한 실패 요약 확인 |

### 키 한눈에 보기

| 구역 | 키 | 동작 |
|---|---|---|
| 전역 | <kbd>Tab</kbd><br><kbd>Shift</kbd> + <kbd>Tab</kbd> | 패널 이동 |
|  | <kbd>Ctrl</kbd> + <kbd>p</kbd> | 변환 추가 |
|  | <kbd>F1</kbd> | 도움말 |
|  | <kbd>Ctrl</kbd> + <kbd>q</kbd> | 정상 종료 |
|  | <kbd>Esc</kbd> | 창·확대 닫기 또는 실행 취소 |
| Pipeline | <kbd>↑</kbd><br><kbd>↓</kbd> | 단계 선택 |
|  | <kbd>Shift</kbd> + <kbd>↑</kbd><br><kbd>Shift</kbd> + <kbd>↓</kbd> | 단계 이동 |
|  | <kbd>Space</kbd> | 단계 활성화 전환 |
|  | <kbd>Backspace</kbd> | 단계 삭제 |
|  | <kbd>Enter</kbd> | 단계 검사 |
|  | <kbd>s</kbd> | 선택 단계 실행 |
|  | <kbd>f</kbd> | 최종 결과 복원 |
|  | <kbd>z</kbd> | Pipeline 확대 |
| Output | <kbd>Enter</kbd> | Pretty Copy |
|  | <kbd>Shift</kbd> + <kbd>Enter</kbd> | Raw Copy |
|  | <kbd>v</kbd> | View 전환 |
|  | <kbd>z</kbd> | Output 확대 |

`Shift+Enter`를 구분하지 못하는 터미널에서는 Raw Copy가 제한될 수 있습니다.
````

- [ ] **Step 3: 내용과 변경 경계를 검사한다**

Run:

```bash
git diff -- README.md
rg --color=never --fixed-strings '333d6b3a3c1f…' README.md
rg --color=never --fixed-strings '<kbd>Ctrl</kbd> + <kbd>p</kbd>' README.md
rg --color=never --fixed-strings '| `SMART` | 결과 형식에 알맞은 View 자동 선택 |' README.md
rg --color=never --fixed-strings '| `TRACE` | Pipeline 단계별 상태와 안전한 실패 요약 확인 |' README.md
! rg --color=never --quiet 'docs/test-reports|shields\.io|<img|!\[' README.md
```

Expected: 새 변경은 TUI 섹션에만 있고, 기존 안전 경계·문서·Apache-2.0 제거 diff는 그대로 남는다. 각 단계, View, 단축키는 표의 개별 행에 표시되고 외부 시각 자산은 없다.

- [ ] **Step 4: Markdown과 Rust 회귀 검증을 실행한다**

Run:

```bash
git diff --check -- README.md
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features
```

Expected: Markdown 공백 오류가 없고 Format, Clippy, 전체 시험, rustdoc가 모두 성공한다.

- [ ] **Step 5: TUI 변경 hunk만 부분 스테이징하고 커밋한다**

Run interactively:

```bash
git add -p README.md
```

`## TUI 사용법` hunk에는 `y`, 기존 사용자 소유의 안전 경계·문서·라이선스 hunk에는 `n`을 입력한다.

Run:

```bash
git diff --cached --check
git diff --cached -- README.md
git diff -- README.md
git commit -m "docs(readme): TUI 안내 고도화"
```

Expected: cached diff와 커밋에는 TUI 섹션만 포함된다. 커밋 뒤 working-tree diff에는 기존 안전 경계·문서·Apache-2.0 제거만 남는다.
