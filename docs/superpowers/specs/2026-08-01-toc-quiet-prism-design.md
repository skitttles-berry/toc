# toc TUI Quiet Prism 고도화 설계

* **작성일:** 2026-08-01
* **상태:** 사용자 승인·구현 완료
* **기준 문서:** `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md`, `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md`
* **대상:** `toc tui`
* **제품 UI 언어:** 영어
* **프로젝트 문서 언어:** 한국어

---

> 2026-08-07 승인된 `2026-08-07-toc-terminal-native-tui-design.md`가 이 문서의
> 고정 RGB와 Output 위치 카운터 계약을 대체한다.

> 2026-08-02 승인된 `2026-08-01-toc-tui-shortcuts-output-design.md`가 이 문서의
> App Bar·Output 제목·Dock·Help 계약을 대체한다. 아래의 `FOCUS`, `FINAL`과 이전
> 키 표기는 Quiet Prism 구현 당시의 기록으로 남긴다. 현재 App Bar는 `>_ TOC`만,
> FINAL을 생략한 Ready Output 제목은 `BYTE 현재/전체` 또는 `ROW 현재/전체`를
> 공간에 맞춰 표시한다. Dock과 Help는 영문 소문자, `↑`/`↓`,
> `Shift+↑`/`Shift+↓`, `Enter` Pretty·`Shift+Enter` Raw Copy, `Delete`/`d`를
> 표시한다. 복사는 전용 작업자에서 준비·기록하며 일반 상태는 2초 또는 다음
> 사용자 조작에 해제된다. Output 페이지 이동은 실제 Viewport 크기를 사용한다.

# 1. 목적과 범위

이 설계는 현재 `Pipeline + Input + Output` 작업판의 구조와 조작을 유지하면서
시각적 위계를 Quiet Prism 방향으로 정돈한다. 변환 대기 중에는 Output을 상태
문구로 덮지 않고 이전 결과를 유지하며, 처리가 1초를 넘을 때만 정확한 안내를
표시한다. 하단 도움말은 선택한 Grouped Command Dock으로 교체하고 기존 Modal에
Dim과 Shadow를 추가한다.

기존 CLI 명령, 공개 변환 ID, Pipeline 실행 결과, Output View, Zoom, Inspector,
Picker, 마우스 동작, 클립보드 정책과 모든 키 바인딩은 변경하지 않는다. 새 키가
필요한 기능은 이 설계에 포함하지 않는다.

다음 기능은 명시적으로 제외한다.

* Activity Ribbon
* Pipeline 노드 그래프
* Output Segment Tab
* Metric Card, Mini Chart와 Sparkline
* JSON·JWT Tree와 Hex Byte Inspector
* Toast Stack과 Micro Animation
* 시작 Dashboard
* 사용자 테마·효과 설정과 새 UI 의존성

# 2. 승인된 사용자 경험

| 항목 | 결정 |
|---|---|
| 시각 방향 | Quiet Prism |
| 화면 구조 | 현재 App Bar와 `Pipeline + Input + Output` 유지 |
| 패널 배치·크기·제목 | 현재 반응형 규칙 유지 |
| Activity Ribbon | 표시하지 않음 |
| Pipeline 행 | 현재 `[ON]`·상태·이름·크기 형식 유지 |
| 포커스 | Cyan 테두리와 제목 |
| 비포커스 | Muted 테두리와 제목 |
| 선택 행 | Surface High 배경, Cyan 전경, Bold |
| 상태 색상 | 성공 Green, 오류 Red, 처리 Yellow, 비활성 Muted |
| Footer | 두 줄 Grouped Command Dock |
| 키 바인딩 | 기존 바인딩 전부 유지 |
| 짧은 지연 | 이전 Output 유지, 별도 문구 없음 |
| 장시간 처리 | 실행 시작 1초 후 안내 |
| Modal | 현재 내용·크기·조작 유지, Dim과 한 셀 Shadow 추가 |
| 애니메이션 | 추가하지 않음 |
| `NO_COLOR` | 색상만 제거하고 문자 정보를 유지 |

# 3. Quiet Prism 시각 체계

현재 App Bar의 `>_ TOC │ FOCUS: <PANE>`과 세 패널의 제목, 굵은 테두리,
레이아웃은 유지한다. 색상만 다음 팔레트의 필요한 항목으로 교체한다.

```text
Background       #11111B
Surface          #181825
Surface High     #242438
Border           #363A4F
Text             #CDD6F4
Muted            #6C7086
Cyan             #89DCEB
Green            #A6E3A1
Yellow           #F9E2AF
Red              #F38BA8
```

화면 전체에 밝은 색을 사용하지 않는다. 포커스 테두리·제목과 선택 Pipeline
행만 Cyan으로 강조한다. 비포커스 테두리는 Border 또는 Muted를 사용하고 일반
본문은 Text를 사용한다. 성공·오류·처리 색상은 상태를 보조할 뿐 `✓`, `×`, `›`
등의 기존 문자 정보를 대체하지 않는다.

Pipeline의 행 내용과 높이는 변경하지 않는다. 선택 행은 Surface High 배경,
Cyan 전경과 Bold를 사용한다. 비선택 행은 상태별 전경색을 사용하되 배경을
추가하지 않는다. Gradient 로고, Glow 문자와 외부 아이콘 글꼴은 사용하지 않는다.

# 4. Grouped Command Dock

Footer는 현재 두 줄 높이와 역할을 유지한다.

1. 첫째 줄: 포커스 패널의 명령 또는 상태
2. 둘째 줄: 전역 명령

넓은 Output 화면의 기준 형식은 다음과 같다.

```text
OUTPUT │  Enter  Pretty   v/V  View │  p  Step   f  Final │  z  Zoom
GLOBAL │  Tab  Focus │  F3  Pretty   F4  Raw │  Ctrl+P  Add   F1  Help   Ctrl+Q  Quit
```

실제 렌더링은 `Line`과 `Span`을 사용한다. 색상 환경에서는 키 이름 양옆의 공백에
Surface High 배경, Cyan 전경과 Bold를 적용한다. `NO_COLOR`에서는 같은 영역을
`[ Enter ]`처럼 대괄호가 포함된 일반 텍스트로 표시한다. 그룹 이름과 `│` 구분자는
두 환경에서 모두 남긴다.

현재 `focused_help`의 키와 의미를 그대로 사용한다. Pipeline은 선택·이동·전환·
검사, Input은 편집·취소, Output은 복사·View·단계·최종·Zoom 명령을 표시한다.
전역 줄은 현재 Tab, F3, F4, Ctrl+P, F1과 Ctrl+Q를 유지한다. Modal의 키 우선순위와
`Ctrl+C` 강제 종료는 변경하지 않는다.

반응형 표시는 기존 WidthMode를 재사용한다.

| 너비 | Footer 정책 |
|---|---|
| 120열 이상 | 포커스 명령과 전역 명령 전체 표시 |
| 90~119열 | 같은 그룹 순서를 유지하고 낮은 우선순위 설명부터 축약 |
| 40~89열 | 핵심 키를 우선 표시하고 낮은 우선순위 명령을 생략 |
| 40열 미만 또는 높이 10행 미만 | 기존 터미널 크기 안내 유지 |

각 줄은 그룹 이름 뒤에 다음 원자적 명령 그룹을 순서대로 추가하고, 다음 그룹 전체가
남은 너비에 들어가지 않으면 그 뒤의 그룹을 표시하지 않는다.

```text
Input     Text editing → Esc Cancel
Pipeline  j/k Select → J/K Move → Space Toggle → Enter Inspect
Output    Enter Pretty → v/V View → p Step → f Final → z Zoom
Global    Tab Focus → F3 Pretty → F4 Raw → Ctrl+P Add → F1 Help → Ctrl+Q Quit
```

Output의 `Enter Pretty`는 현재처럼 복사 가능한 경우에만 표시한다. 각 너비에서
표시되지 않은 키도 계속 동작하며 F1 Help에서 확인할 수 있다. 문자열을 셀 경계에서
우연히 잘라내지 않고 완성된 명령 그룹만 렌더링한다. `NO_COLOR`의 대괄호까지 실제
표시 폭에 포함한다.

# 5. 변환 대기와 이전 결과

## 5.1 상태 전환

입력 또는 Pipeline 변경은 요청 ID를 증가시키고 최종 결과 캐시를 무효화한다.
`OutputStatus::Debouncing`으로 전환하되 현재 표시 중인 `active_artifact`, Trace,
OutputSource와 스크롤 위치는 유지한다. 따라서 50밀리초 또는 200밀리초 지연 중
Output 본문을 `Waiting for changes…`로 교체하지 않는다.

지연 시간이 끝나면 실행 상태에 시작 시각, 안내 표시 여부와 요청 대상을 보관한다.
선택 Step 실행도 같은 실행 상태를 사용한다. 대기 중 보이는 OutputSource는 이전
Artifact의 실제 출처를 계속 나타내며, 실행 대상은 별도 상태로 추적한다. 선택 Step
실행 표시도 OutputSource를 새 대상으로 미리 바꾸지 않고 실행 대상에서 계산한다.

```text
Input 또는 Pipeline 변경
  → Debouncing: 이전 표시 유지, 최종 캐시 무효화
  → Running < 1초: 이전 표시 유지
  → Running ≥ 1초: 이전 표시 유지, Footer 안내
  → Success: Artifact, Trace와 OutputSource를 함께 교체
  → Failed 또는 Cancelled: 이전 표시 제거, 실패·취소 상태 표시
```

첫 실행처럼 이전 Artifact와 Trace가 없으면 빈 Output을 그대로 유지한다.

## 5.2 장시간 처리 안내

실행 시작 뒤 1초가 되는 최초 Tick에서 안내 상태를 한 번만 활성화하고 화면을 다시
그린다. 이후 Tick마다 불필요한 redraw를 만들지 않는다.

이전 결과가 있으면 Footer 첫째 줄을 다음 문구로 교체한다.

```text
Still processing · Previous result shown · Esc Cancel
```

이전 결과가 없으면 사실과 다른 설명을 피하기 위해 다음 문구를 사용한다.

```text
Still processing · Esc Cancel
```

작업이 1초 안에 끝나면 장시간 처리 문구를 한 번도 표시하지 않는다. 별도 Spinner와
애니메이션은 추가하지 않는다.

## 5.3 결과 사용 제한

`can_copy`는 계속 `Ready` 상태에서만 참이므로 Debouncing과 Running 중 보이는 이전
결과를 복사할 수 없다. `v`·`V`, 스크롤과 Zoom은 보존된 결과에 그대로 동작한다.
`p`는 현재처럼 새 선택 Step 요청을 시작하며 pending target만 교체한다. `f`는 유효한
최종 캐시가 있을 때만 요청 ID를 증가시키고 그 결과로 돌아간다.

입력이나 Pipeline이 바뀌면 `final_artifact`와 `final_traces`를 즉시 무효화하여 `f`로
오래된 최종 결과를 복원할 수 없게 한다. 선택 Step 실행은 입력과 Pipeline을 바꾸지
않으므로 기존의 유효한 최종 결과 캐시를 유지한다.

성공한 최신 요청만 새 Artifact와 Trace를 게시한다. 요청 ID가 다른 작업자 결과는
계속 폐기한다. 실패나 취소는 보존하던 이전 표시를 제거한 뒤 현재 오류 또는 취소
상태를 표시한다.

# 6. Modal Dim과 Shadow

Modal의 내용, 크기, 입력 처리, 마우스 영역과 접근 키는 변경하지 않는다. Picker,
Inspector, Help, Quit Confirm과 Unsafe Copy Confirm에 같은 깊이 규칙을 적용한다.

렌더 순서는 다음과 같다.

```text
1. Background와 App Bar
2. Pipeline, Input과 Output
3. Grouped Command Dock
4. Modal이 있으면 기존 화면에 DIM Modifier 적용
5. Modal 오른쪽·아래 한 셀 Shadow
6. Popup 영역 Clear
7. 기존 Modal 본문과 테두리
8. Modal 입력 위치 또는 Input cursor
```

Ratatui 0.30.2가 제공하는 Buffer 스타일, Clear와 Shadow 기능을 재사용한다. 색상
환경의 Shadow는 기존 셀 문자를 유지하는 overlay에 Surface High 배경을 적용하고,
Popup보다 오른쪽과 아래에 한 셀만 표시한다. 새 렌더링 crate와 애니메이션 타이머는
추가하지 않는다.

`NO_COLOR`에서는 배경 RGB를 사용하지 않는다. DIM과 Shadow가 터미널에서 구분되지
않아도 Modal 테두리와 제목이 구조를 전달하도록 유지한다.

# 7. 코드 경계

현재 프로덕션 코드 경계는 다음과 같다.

```text
src/tui.rs
  터미널 수명, Preview·클립보드 작업자 이벤트 루프, 공유 Quiet Prism 색상 상수

src/tui/state.rs
  실행·복사 상태 전이, 일반 상태 수명, Output Viewport 보정

src/tui/clipboard.rs
  복사 payload 준비, 위험 문자 검사와 시스템 클립보드 단일 작업자

src/tui/render.rs
  Quiet Prism 스타일, Grouped Command Dock, 이전 결과 렌더링,
  복사 진행 문구, 위치 제목, Modal Dim과 Shadow

src/tui/views.rs
  Text·Hex·Trace의 공유 페이지·행 계산과 Trace 상태 문자열
```

`README.md`와 기존 관련 설계 문서의 현재 동작 설명은 구현 커밋에서 최소 diff로
현행화한다. Pipeline 실행 엔진, Preview 작업자, 변환 구현과 CLI는 변경하지
않는다. 새 trait, 테마 registry, 설정 parser와 의존성은 추가하지 않는다.

# 8. 오류와 보안

* 대기 중 이전 결과는 표시만 하며 복사할 수 없다.
* 이전 결과의 OutputSource를 새 실행 대상으로 잘못 표시하지 않는다.
* 실패·취소 결과 뒤에 이전 성공 결과를 현재 결과처럼 남기지 않는다.
* 오래된 요청은 Artifact, Trace와 상태를 변경하지 못한다.
* Input, Output, Trace payload와 클립보드 내용을 로그나 상태 문구에 기록하지 않는다.
* 기존 ANSI·OSC·제어 문자 escape와 외부 오류 길이 제한을 유지한다.
* Modal Dim과 Shadow는 입력 우선순위와 마우스 hitbox를 변경하지 않는다.
* `NO_COLOR`에서도 포커스, 선택, 상태와 키를 문자 정보로 식별할 수 있다.

# 9. 시험 전략

새 시험 프레임워크를 추가하지 않고 기존 Rust 단위 시험과 Ratatui `TestBackend`를
확장한다.

## 9.1 상태와 데이터 흐름

* Debouncing과 Running에서 이전 Artifact, Trace, OutputSource와 스크롤 유지
* 입력·Pipeline 변경 시 최종 결과 캐시 무효화
* 선택 Step 실행 시 유효한 최종 결과 캐시 유지
* Debouncing과 Running 중 복사 차단
* 실행 시작 999밀리초에는 안내 없음, 1초에는 안내 활성화
* 1초 이후 Tick이 반복 redraw를 만들지 않음
* 성공 시 최신 Artifact, Trace와 OutputSource를 함께 게시
* 실패·취소 시 이전 표시 제거
* 오래된 작업자 결과 폐기
* 최종 실행과 선택 Step 실행의 같은 보존 정책

## 9.2 렌더링

* `Waiting for changes…`가 모든 화면에서 사라짐
* 짧은 지연과 1초 미만 실행에서 이전 Output 본문 유지
* 1초 이후 이전 결과 유무에 맞는 Footer 문구
* Quiet Prism 포커스·비포커스·선택·성공·오류 스타일
* Wide, Medium과 Narrow의 완성된 Footer 명령 그룹
* 색상 환경의 Keycap Span과 `NO_COLOR`의 대괄호 표기
* 모든 Modal의 Dim, 한 셀 Shadow와 Popup Clear 순서
* Modal이 기존 마우스 영역과 입력 우선순위를 유지함
* Tiny, Zoom과 Modal의 기존 반응형 회귀

## 9.3 전체 검증

구현 뒤 다음 명령을 통과해야 한다.

```bash
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
```

현재 Bash·Zsh PTY, macOS 클립보드와 마우스 Smoke 계약에 영향을 주는 변경이 있으면
`tests/shell-smoke.sh`도 두 셸에서 다시 실행한다. 시간 측정값은 시험 성공 조건이
아니며, 변경 시 redraw가 불필요하게 반복되지 않는지 확인하는 근거로만 사용한다.

# 10. 문서 동기화

구현과 같은 커밋에서 기존 파일의 구조와 문체를 유지하며 다음 문서를 최소 diff로
현행화한다.

* `README.md`
* `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md`
* `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md`

`AGENTS.md`, `ARCHITECTURE.md`, `DESIGN.md`와 `docs/design/`은 현재 프로젝트에
없으므로 새로 만들지 않는다.

# 11. 완료 기준

다음을 모두 만족하면 구현이 완료된 것으로 본다.

* 현재 App Bar와 `Pipeline + Input + Output` 구조가 유지된다.
* Activity Ribbon과 제외된 고밀도 기능이 추가되지 않는다.
* Quiet Prism 색상 위계가 포커스와 상태에만 집중된다.
* 선택 Pipeline 행과 비선택 행의 우선순위가 명확하다.
* Footer가 기존 두 줄 Grouped Command Dock으로 표시된다.
* 기존 키 바인딩이 모두 그대로 동작한다.
* 짧은 대기 중 이전 결과가 유지되고 `Waiting for changes…`가 표시되지 않는다.
* 실행 시작 1초 뒤에만 정확한 장시간 처리 안내가 표시된다.
* 대기 중 이전 결과는 복사·복원할 수 없다.
* 성공, 실패, 취소와 오래된 요청이 승인된 데이터 안전성 규칙을 따른다.
* 모든 기존 Modal에 Dim과 한 셀 Shadow가 적용된다.
* 새 의존성·설정·애니메이션 없이 전체 품질 명령과 회귀 시험이 통과한다.
* 관련 기존 문서가 구현과 같은 커밋에서 현행화된다.

2026-08-01 전체 형식, Clippy, 잠금 전체 시험, rustdoc, release redraw 측정,
잠금 패키징, 임시 경로 오프라인 설치, Bash·Zsh PTY와 macOS 클립보드 Smoke를
실행해 통과했다. redraw 측정은 1회의 변경 시 렌더만 확인하며 시간값을 성공
기준으로 사용하지 않는다.
