# toc TUI 작업판 기반 고도화 설계

* **작성일:** 2026-07-31
* **상태:** 사용자 승인·구현 완료
* **기준 문서:** `docs/prd/init-prd.md`, `docs/prd/v0.2-prd.md`
* **대상:** `toc tui`
* **제품 UI 언어:** 영어
* **프로젝트 문서 언어:** 한국어

---

> 2026-08-07 승인된 `2026-08-07-toc-terminal-native-tui-design.md`가 이 문서의
> 고정 RGB와 Output 위치 카운터 계약을 대체한다.

> 2026-08-02 `2026-08-01-toc-tui-shortcuts-output-design.md`의 후속 구현으로
> 대용량 복사는 별도 단일 작업자에서 준비·기록한다. 일반 Footer 상태는 2초 또는
> 다음 사용자 조작에 해제되고, Output 페이지 이동·위치 제목은 실제 Viewport를
> 사용한다. 공개 CLI·변환·키 바인딩과 의존성은 바뀌지 않는다.

# 1. 목적과 범위

이 설계는 현재 v0.2의 비파괴 TUI를 바이트 Pipeline 작업판으로 고도화한다. 사용자가 입력한 원문은 계속 Input에 남기고, Pipeline 결과와 단계별 상태는 Output에서 확인한다.

사용자에게 노출되는 변경 범위는 `toc tui`뿐이다. 기존 CLI 명령, 옵션, 출력, 오류 문구, 종료 코드와 8개 공개 변환 ID는 변경하지 않는다. 공용 파이프라인 내부는 TUI의 바이트 결과를 지원하도록 확장하되 CLI는 기존의 단계별 UTF-8 성공 조건을 유지한다.

이 문서는 `docs/prd/init-prd.md`와 `docs/prd/v0.2-prd.md`의 후속 승인 설계다. 기능 구현과 macOS·Linux 셸, 실제 TUI 및 필수 클립보드 통합 검증을 완료했다. 따라서 TUI에 한해 기존의 반응형 레이아웃, 고정 200밀리초 지연, 중간 결과 제외, 비 UTF-8 디코딩 결과 실패와 원시 이진 성공 결과 제외 규약을 대체한다. CLI 관련 규약은 대체하지 않는다.

## 1.1 포함 기능

* 바이트 결과를 손실 없이 보존하는 TUI 실행 정책
* 단계별 상태, 입출력 크기와 실행 시간
* 최종 결과와 선택 단계 결과의 명시적 전환
* Smart, Text, Hex, Trace Output 보기
* Pipeline 왼쪽, Input·Output 오른쪽의 반응형 작업판
* 읽기 전용 단계 Inspector
* 기존 8개 변환을 대상으로 한 Operation Palette
* 크기에 따른 50밀리초 또는 200밀리초 지연 실행
* 최신 요청만 반영하는 단일 Preview 작업 스레드
* 복사 준비와 시스템 쓰기를 담당하는 단일 클립보드 작업 스레드
* 바이너리 결과의 소문자 Hex 복사
* 컨텍스트 도움말, 패널 Zoom과 적응형 Unicode 표시

## 1.2 이번 범위에서 제외

* 신규 변환과 변환별 옵션
* 파일 열기·저장과 원시 바이트 내보내기
* Recipe, 세션 복원과 최근 사용 기록
* 설정 파일, 사용자 키맵과 사용자 테마
* JSON/JWT Tree와 별도 Bytes View
* 검색, 선택 영역과 부분 복사
* 중간 결과 캐시
* Tokio와 별도 비동기 런타임
* 실행 중인 동기 변환 내부의 강제 중단
* 플러그인, 외부 명령과 스트리밍 처리

제외 항목은 폐기하지 않고 이 문서의 `# 10. 후속 작업 대장`에 별도로 기록한다.

# 2. 승인된 사용자 경험

| 항목 | 결정 |
|---|---|
| 기본 레이아웃 | Pipeline 왼쪽, Input·Output 오른쪽의 분할 작업판 |
| 중간 너비 | 같은 구조를 압축하고 부가 정보만 숨김 |
| 좁은 너비 | 한 번에 한 패널을 표시하는 탭 방식 |
| 기본 스타일 | 터미널 배경을 따르는 적응형 Unicode |
| Input | 포커스되면 별도 모드 없이 바로 편집 |
| 기본 Output | Smart |
| Smart 규칙 | 유효한 UTF-8은 Text, 바이너리는 Hex, 실행 오류는 Trace |
| 수동 View | 다시 Smart를 선택할 때까지 유지 |
| 기본 결과 원본 | 최종 Pipeline 결과 |
| 선택 단계 결과 | `p`로 요청하고 `f`로 최종 결과 복귀 |
| 바이너리 결과 | Pipeline 성공으로 취급하고 Hex로 확인 |
| 바이너리 복사 | 공백 없는 소문자 Hex 문자열 |
| 중간 결과 보관 | 보관하지 않고 요청 시 해당 단계까지 재계산 |
| 실시간 지연 | 256 KiB 이하는 50 ms, 초과 입력은 200 ms |

CLI는 TUI와 달리 기존 UTF-8 성공 결과 규약을 유지한다. TUI에서 성공하는 비 UTF-8 디코딩 결과도 동일한 CLI 명령에서는 기존처럼 `InvalidUtf8Output` 오류가 된다.

# 3. 공용 실행 엔진

## 3.1 실행 정책

공용 엔진은 다음 두 정책을 구분한다.

```text
StrictText
    각 활성 단계 결과를 UTF-8로 검증
    기존 CLI와 기존 execute(...)가 사용

AllowBinary
    바이트 결과를 그대로 다음 단계로 전달
    TUI가 사용
```

변환의 입력 조건은 정책과 무관하게 유지한다. `accepts_binary`가 거짓인 단계가 비 UTF-8 입력을 받으면 그 단계는 `InvalidUtf8Input`으로 실패한다. 따라서 TUI의 바이너리 결과는 `hex-encode`나 `base64-encode`처럼 바이트 입력을 허용하는 다음 단계로만 계속 전달할 수 있다.

Base64, URL과 Hex 디코더의 내부 성공 값은 바이트로 만든다. `StrictText` 정책은 각 디코더 직후 기존과 같은 UTF-8 검증과 제한된 16진수 미리보기를 적용한다. 이 검증 위치 변경은 내부 구현 세부 사항이며 CLI의 관찰 가능한 결과를 바꾸지 않는다.

## 3.2 실행 요청과 결과

실행 요청은 다음 정보를 가진다.

```text
ExecutionRequest
├── request_id
├── input bytes
├── immutable steps
├── output limit
├── policy
└── target: Final | Step(index)
```

`request_id`는 Input·Pipeline 변경뿐 아니라 `p`, `f` 같은 결과 원본 전환에서도 증가한다. 작업 결과는 현재 `request_id`와 정확히 일치할 때만 화면에 반영한다. 이 규칙은 같은 Input에서 여러 단계 결과를 빠르게 요청하거나, 단계 재계산 중 최종 결과로 돌아가는 경우에도 오래된 결과가 화면을 덮어쓰지 못하게 한다.

실행 결과는 예상된 Pipeline 실패까지 포함하는 보고서다.

```text
ExecutionReport
├── request_id
├── target
├── outcome: Success(bytes) | Failed(error) | Cancelled
└── traces: StepTrace[]
```

`StepTrace`는 단계 ID, 활성 여부, 입력 크기, 출력 크기, 실행 시간과 다음 상태 중 하나를 기록한다.

```text
Succeeded | Disabled | Failed | NotExecuted | Cancelled
```

입력이나 결과 전문은 Trace와 로그에 넣지 않는다. 실행 시간은 화면 정보일 뿐 기능 결과나 정밀 성능 계약으로 사용하지 않는다.

## 3.3 기존 API 호환성

현재 `pipeline::execute`의 서명과 엄격한 결과는 유지한다. 내부 보고서 실행 함수에 `StrictText`를 전달하고 성공 바이트 또는 기존 `PipelineError`로 변환하는 얇은 진입점으로 둔다. CLI와 기존 시험은 계속 이 진입점을 사용한다.

TUI만 `AllowBinary` 보고서 실행 경로를 사용한다. CLI와 TUI에 변환 레지스트리나 변환 구현을 따로 만들지 않는다.

# 4. TUI 상태와 작업 실행

## 4.1 상태 분리

`tui.rs`의 터미널 수명 관리는 유지하고, 현재 한 파일에 모인 상태·작업·렌더링만 다음 경계로 분리한다.

```text
src/tui.rs          터미널 시작·복구, 이벤트 루프와 외부 효과
src/tui/state.rs    App 상태, 키 입력과 상태 전이
src/tui/worker.rs   지연 실행, 최신 요청과 Pipeline 작업
src/tui/clipboard.rs 복사 준비, 위험 문자 검사와 시스템 클립보드 쓰기
src/tui/render.rs   레이아웃, 패널, Overlay와 상태 표시줄
src/tui/views.rs    Smart 판정, Text·Hex·Trace 표시
```

별도 trait, UI 프레임워크, Cargo Workspace나 공용 엔진 크레이트는 만들지 않는다.

주요 상태는 다음과 같다.

```text
App
├── input editor
├── focus and zoom
├── pipeline steps and selection
├── output source: Final | Step(index)
├── view mode: Smart | Text | Hex | Trace
├── final result and active result
├── step traces
├── request_id and debounce deadline
├── copy phase and copy request_id
├── modal, status and status deadline
└── dirty
```

`tui.rs`는 Crossterm 마우스 이벤트를 `AppEvent::Mouse`로 전달한다. 렌더러는 매
프레임 실제로 표시한 패널과 Pipeline 행의 `Rect`만 `MouseRegions`에 저장하며,
resize·zoom 뒤 이전 좌표를 유지하지 않는다.

## 4.2 지연 실행

Input 또는 Pipeline이 변경되면 최종 결과 요청을 예약한다.

* Input이 256 KiB 이하이면 마지막 변경 후 50 ms에 실행한다.
* Input이 256 KiB를 초과하면 마지막 변경 후 200 ms에 실행한다.
* bracketed paste는 완성된 Paste 이벤트 하나로 반영하고 한 번만 예약한다.
* `p`로 요청한 선택 단계 결과는 문서 변경이 아니므로 별도 지연 없이 즉시 요청한다.
* `f`는 실행하지 않고 보관 중인 현재 최종 결과를 즉시 다시 표시한다.
* 새 최종 결과가 준비되기 전에는 현재 Output과 Trace를 이전 결과로 유지하되
  복사를 비활성화한다. Input 또는 Pipeline이 바뀌면 최종 결과 캐시는 즉시
  무효화하며, 최신 실행의 실패·취소 뒤에는 이전 표시를 제거한다. 실행 시작 뒤
  1초가 지나면 Footer 첫째 줄에 이전 결과 표시와 취소 키를 안내한다.

입력 제한 1 MiB와 65,536줄, 단계 출력 제한 64 MiB, 최대 32단계는 유지한다.

## 4.3 최신 요청 우선 Preview 작업자

Preview에는 운영체제 작업 스레드 하나를 사용한다. 실행 중 새 요청이 오면 대기 슬롯에는 최신 요청 하나만 남긴다. 작업자는 각 단계 실행 전후에 자신이 최신 요청인지 확인한다.

이미 시작한 동기 변환 하나를 강제로 중단하지 않는다. 오래 걸리는 단계가 끝나면 다음 단계로 넘어가지 않고 취소한다. 완료된 오래된 보고서도 상태, 화면, 복사 가능 여부나 클립보드 상태를 변경하지 못한다.

Modal과 Zoom이 없는 상태에서 `Esc`를 누르면 현재 요청 ID를 무효화하고 화면 상태를 `Cancelled`로 바꾼다. 실행 중인 동기 변환은 작업 스레드에서 끝까지 실행될 수 있지만 결과는 폐기한다. 이후 Input이나 Pipeline이 바뀌면 새 최종 결과 요청을 정상적으로 예약한다.

이 구조는 현재 8개 내장 변환에 충분하며 Tokio를 추가하지 않는다.

Preview 결과 채널이 종료되면 작업자를 자동 재시작하지 않고 안전한 TUI 오류로 종료하며 기존 터미널 복구 경로를 실행한다.

클립보드는 별도 단일 작업자가 Artifact 스냅샷과 복사 요청 번호를 받아 JSON
정리, Binary Hex 변환, 위험 문자 검사와 시스템 쓰기를 수행한다. 한 요청이
준비·확인·쓰기 중이면 추가 요청을 쌓지 않는다. 늦은 요청 번호는 폐기하고,
작업자 채널 종료와 준비·쓰기 실패는 TUI를 종료하지 않고 `Copy unavailable`로
복구한다.

## 4.4 단계 결과 재계산

정상 실행 후 메모리에 계속 보관하는 값은 다음뿐이다.

* 현재 Input
* 최종 결과 바이트
* 단계별 작은 Trace 메타데이터
* 사용자가 현재 보고 있는 선택 단계 결과

`p`는 선택 단계까지 Pipeline을 다시 실행한다. `f`는 보관된 최종 결과로 돌아가므로 재실행하지 않는다. Input 또는 Pipeline이 바뀌면 선택 단계 결과를 폐기하고 최종 결과 요청으로 돌아간다.

# 5. 화면과 조작

## 5.1 반응형 레이아웃

### 120열 이상

Pipeline은 왼쪽 30%를 사용하되 28~42열로 제한한다. 오른쪽은 Input 42%, Output 58%의 세로 구조다. 상단에는 박스 없는 한 줄 App Bar를 표시하고, 각 패널은 굵은 Neon Console 테두리와 제목을 사용한다. 최하단 두 줄 Footer는 포커스별 도움말과 공통 도움말을 고정 표시한다.

### 90~119열

Pipeline은 28~32열로 줄이고 같은 분할 구조를 유지한다. Inspector는 Overlay로 표시한다.

### 40~89열

Pipeline, Input, Output을 차례대로 30%, 30%, 40%로 쌓아 표시한다. `Tab`과 `Shift+Tab`으로 패널을 전환하며 현재 포커스는 App Bar에 텍스트로 표시한다.

### 10~11행

포커스된 패널 하나만 표시한다. 상단 App Bar와 두 줄 Footer는 유지한다.

### 40열 미만 또는 10행 미만

편집기와 결과를 렌더링하지 않고 터미널 크기를 늘리라는 안내만 표시한다.

높이 12행 이상에서는 각 패널에 최소 3개의 테두리 행을 남긴다. 실패·취소,
장시간 변환, 복사 진행, 일반 상태 순으로 Footer 첫째 줄만 대체하며, 둘째 줄
공통 도움말은 항상 표시한다. 일반 상태는 2초 또는 다음 키 입력·Input 붙여넣기·
좌클릭·휠에 해제된다.

Modal이 열려 있으면 Modal 영역만 입력 판정에 사용한다. Pipeline·Input·Output의
테두리와 내용 클릭은 해당 패널에 포커스를 주고, Pipeline·Add Transform의 실제
표시 행 클릭은 선택도 바꾼다. Output 휠은 기존 스크롤 3단위, Pipeline·Add
Transform 휠은 선택 1개를 이동하며 포커스는 유지한다. Modal은 표시된
Add·Confirm·Cancel·Close 동작만 클릭으로 실행한다. Input caret, 드래그 선택,
Output 마우스 복사, Pipeline 직접 편집, Hover와 Footer 클릭은 지원하지 않는다.

일반 크기 F1 도움말은 Input의 focus-only 클릭, Pipeline의 클릭 선택·휠 이동,
Output의 focus-only 클릭·휠 스크롤을 안내한다. compact Help와 두 줄 Footer는
기존 키 정보를 유지한다.

## 5.2 스타일과 접근성

기본 배경은 터미널 배경을 그대로 사용한다. Accent, Success, Warning, Error와 Selection 역할만 제한된 16색 계열로 표현한다. Pipeline 행은 활성 상태를 한 번만 `[ON]` 또는 `[OFF]`로 표시한 뒤 실행 상태를 `✓`, `×`, `›`, `·`, `−` 중 하나의 기호로 표시한다. 비활성 단계에는 실행 기호를 두지 않는다.

`NO_COLOR`에서는 색상만 제거하고 패널·상태 기호를 유지한다. 이모지 전용 제목, 고정 RGB 배경과 사용자 테마 설정은 이번 범위에 포함하지 않는다.

## 5.3 키 바인딩

Input은 포커스되면 즉시 편집된다. 따라서 일반 문자 `1`~`4`, `?`, `z`를 Input의 전역 명령으로 가로채지 않는다.

### 전역

| 키 | 동작 |
|---|---|
| `Tab`, `Shift+Tab` | 다음·이전 패널 |
| `F3` | Pretty Copy |
| `F4` | Raw Copy |
| `Ctrl+P` | Operation Palette |
| `F1` | 컨텍스트 도움말 |
| `?` | Input 이외의 패널에서 도움말 |
| `Ctrl+Q` | 종료 또는 기존 폐기 확인 |
| `Ctrl+C` | 강제 종료 |
| `Esc` | Modal 또는 Zoom 닫기, 실행 요청 취소 |

### Pipeline

| 키 | 동작 |
|---|---|
| `↑`, `↓`, `j`, `k` | 선택 이동 |
| `Space` | 활성화 전환 |
| `Shift+↑`, `Shift+↓`, `J`, `K` | 단계 재정렬 |
| `Delete`, `d` | 단계 삭제 |
| `Enter` | 읽기 전용 Inspector |
| `a` | Operation Palette |
| `z` | Pipeline Zoom |

### Output

| 키 | 동작 |
|---|---|
| `v`, `V` | 다음·이전 View |
| `p` | 선택 단계 결과 요청 |
| `f` | 최종 결과 복귀 |
| `Enter`, `y` | 현재 결과 Pretty Copy |
| 방향키, `PageUp`, `PageDown`, `Home`, `End` | 현재 View 스크롤 |
| `z` | Output Zoom |

Input의 편집 키는 `tui-textarea-2` 기본 동작을 유지한다. 검색, 선택 영역, Vim식 Normal/Edit 모드와 사용자 키맵은 추가하지 않는다.

## 5.4 Operation Palette와 Inspector

Palette는 기존 8개 변환만 표시한다. 이름과 공개 ID의 대소문자 무시 부분 문자열 검색을 유지하며, 목록 첫째 줄에는 이름과 ID, 둘째 줄에는 설명을 표시한다. 일반 크기에서는 선택 항목의 입력 조건·동작·바이트 결과 힌트를 표시하고, 목록·상세와 키 도움말 사이에 각각 구분선을 둔다. 작은 Modal에서는 상세를 생략하되 선택 항목의 설명과 `Enter Add`, `Esc Cancel`은 유지한다. 8개 항목에 퍼지 검색 의존성, 최근 사용 순위와 카테고리 체계를 추가하지 않는다.

Inspector는 선택 단계의 이름, ID, 입력 조건, 상태, 입출력 크기, 실행 시간과 안전한 오류만 표시한다. 현재 변환에는 사용자 옵션이 없으므로 옵션 스키마와 Form Renderer는 만들지 않는다.

# 6. Output 보기

## 6.1 Smart

Smart는 Artifact를 변경하지 않고 표시 방식만 선택한다.

1. Pipeline 실행이 실패하면 Trace
2. 결과가 유효한 UTF-8이면 Text
3. 나머지는 Hex

Smart가 아닌 View를 사용자가 직접 선택하면 다시 Smart를 선택할 때까지 그 View를 유지한다. 선택한 View로 현재 결과를 표현할 수 없으면 자동 전환하지 않고 안전한 안내와 사용 가능한 View를 표시한다.

## 6.2 Text

Text는 유효한 UTF-8 결과만 표시한다. UTF-8 검증이 실패하면 `Switch to Hex view` 안내만 표시하고 손실 대체 문자를 만들지 않는다.

일반 문자열, 탭과 줄바꿈은 기존 표시 규칙을 유지한다. ESC, NUL과 터미널을 제어할 수 있는 C0/C1 문자는 `\xNN` 같은 표시 가능한 문자열로 바꿔 렌더링한다. 이 변경은 표시 전용이며 원본 Artifact를 수정하지 않는다.

줄바꿈 없는 긴 행은 그래핌과 표시 폭을 기준으로 Viewport 안에서 자동 줄바꿈한다. 이 줄바꿈은 화면 표시 전용이며 원본 Artifact와 클립보드 내용에는 추가하지 않는다.

## 6.3 Hex

Hex는 한 행에 16바이트를 표시한다.

```text
OFFSET    HEX BYTES                                      ASCII
00000000  FF                                             |.|
```

Offset은 0부터 시작하는 8자리 대문자 16진수다. 바이트는 대문자 두 자리로 표시하고 8바이트 경계에 간격을 하나 더 둔다. ASCII 영역은 `0x20`~`0x7e`만 원문 문자로 표시하고 나머지는 `.`으로 표시한다.

화면에 보이는 행만 만들며 전체 결과에 비례하는 표시 문자열을 생성하지 않는다. 기존 렌더당 4 KiB 처리·출력 예산을 지킨다.

## 6.4 Trace

Trace는 다음 열을 표시한다.

```text
STEP  OPERATION  INPUT  OUTPUT  TIME  STATUS
#1 base64-decode OK in:4B out:1B 6ms
#2 hex-encode OFF in:1B out:1B
```

첫 실패 단계는 안전한 오류 분류와 가능한 경우 바이트 오프셋을 함께 표시한다. 뒤 단계는 `NOT RUN`으로 표시한다. 입력과 결과 전문은 Trace에 포함하지 않는다.

## 6.5 결과 원본과 복사

기본 원본은 최종 결과다. Pipeline 선택 이동만으로 Output은 바뀌지 않는다. `p`를 누르면 선택 단계 결과를 표시하고 `f`를 누르면 최종 결과로 돌아간다. 현재 Context Bar는 없으며 Output 제목은 최종 결과의 `FINAL`을 생략하고 단계 결과에만 `STEP NN`을 표시한다. Ready 상태의 Text·Smart·Hex에는 0부터 시작하는 `BYTE 현재/전체`, Trace에는 1부터 시작하는 `ROW 현재/전체`를 덧붙이고 공간이 부족하면 기존 크기와 기본 제목 순으로 축약한다. 두 줄 Footer의 첫 줄은 포커스 도움말 또는 완료·오류 상태를, 둘째 줄은 공통 키 도움말을 표시한다.

`PageUp`·`PageDown`은 마지막 Output 내부 크기를 사용한다. Text는 렌더링과 같은
그래핌·제어 문자·줄바꿈 규칙을, Hex·Trace는 실제 데이터 행 수를 사용한다.
`End`는 마지막 전체 페이지 시작점으로 이동하며 Resize 뒤 Hex의 최상단 바이트를
새 행 폭과 Viewport에 맞춰 보정한다.

유효한 JSON 결과는 Pretty Copy에서 두 칸 들여쓰고 Raw Copy에서 구조 공백을 제거한다. JSON 변환은 숫자 토큰과 문자열 안 공백을 다시 쓰지 않으며, Pretty Copy도 리터럴 `\u0061`을 `a`로 바꾸지 않는다. JSON으로 해석할 수 없는 유효한 UTF-8 결과는 두 복사 모드에서 원문 전체를 유지하고 기존 위험한 제어 문자 확인 절차를 적용한다. 비 UTF-8 결과는 공백 없는 소문자 Hex 문자열로 복사하며 Footer 첫 줄에 `Copied Pretty`, `Copied Raw`, `Copied as Hex` 완료 상태를 각각 표시한다. Trace와 실패·취소·실행·지연 중인 상태, 오래되거나 없는 Artifact에는 복사를 허용하지 않는다.

복사 형식은 현재 View가 아니라 Enter 시점의 원본 Artifact 스냅샷과 선택한 Pretty 또는 Raw 모드로 결정한다. 따라서 Text를 수동 고정한 상태에서 바이너리 결과가 나오더라도 복사 payload는 소문자 compact Hex이고 완료 상태는 `Copied as Hex`다. Hex 문자열의 두 배 길이는 검사된 산술로 계산한다. 작업자가 만든 위험한 UTF-8 payload는 확인 Modal이 소유하고 승인 시 같은 작업자의 시스템 쓰기로 이동하며, 취소나 F1 전환 시 폐기한다. 그 사이 활성 Artifact가 바뀌어도 승인 대상은 처음 확인한 payload다. 준비, 할당, JSON 출력 한도, 채널 종료 또는 운영체제 클립보드 작업이 실패하면 Input, Pipeline, Artifact, 결과 원본과 Trace를 유지하고 `Copy unavailable`로 복구한다.

# 7. 오류와 보안

## 7.1 Pipeline 오류

잘못된 Base64·URL·Hex·JSON 입력, 텍스트 전용 단계의 비 UTF-8 입력과 출력 한도 초과는 해당 단계에서 실패한다. 첫 실패 뒤 단계는 실행하지 않는다.

Smart는 실패 보고서에 Trace를 사용한다. Text나 Hex를 수동으로 고정한 경우에는 해당 View 안에 안전한 오류 요약과 Trace 전환 안내를 표시한다. 이전 성공 결과와 부분 결과를 현재 성공 결과처럼 표시하거나 복사하지 않는다.

## 7.2 터미널 안전성

* ANSI ESC, CSI, OSC와 하이퍼링크 Sequence를 터미널에 전달하지 않는다.
* Text는 위험한 문자를 표시용 escape로 바꾼다.
* Hex의 ASCII 영역은 출력 가능한 ASCII만 사용한다.
* 외부 오류 문자열에 스타일 제어 코드를 허용하지 않는다.
* TUI 종료, 인터럽트, 패닉과 렌더 오류에서도 기존 역순 터미널 복구를 유지한다.

마우스 캡처는 raw mode, alternate screen, bracketed paste 다음에 활성화하고
cursor hide 전에 완료한다. 정상 종료·오류·패닉 복구는 cursor show, mouse
capture disable, bracketed paste disable, alternate screen leave, raw mode
disable 순서다.

## 7.3 데이터와 자원

* Input, Output, 단계 결과와 클립보드 내용을 로그에 기록하지 않는다.
* 종료 시 Input, Pipeline과 결과를 저장하지 않는다.
* 기존 입력, 줄, 단계와 단계 출력 제한을 유지한다.
* 중간 Artifact를 누적하지 않는다.
* 화면 렌더링은 현재 Viewport와 4 KiB 예산으로 제한한다.

# 8. 시험 전략

새 시험 프레임워크를 추가하지 않고 기존 Rust 단위 시험, Ratatui `TestBackend`, CLI 통합 시험과 셸 Smoke 시험을 확장한다.

## 8.1 실행 엔진

* 모든 활성 단계의 출력이 유효한 UTF-8일 때 `StrictText`와 `AllowBinary`의 최종 결과가 동일함. 중간 단계가 비 UTF-8이면 이후 바이트 허용 단계가 최종 텍스트를 만들더라도 `StrictText`는 해당 중간 단계에서 실패함
* 비 UTF-8 Base64·URL·Hex 디코딩이 CLI에서는 기존 오류이고 TUI에서는 성공함
* TUI 바이너리 결과가 바이트 입력 단계에는 전달되고 텍스트 입력 단계에서는 실패함
* 첫 실패 뒤 단계가 `NotExecuted`임
* 비활성 단계의 입출력 크기와 상태가 정확함
* 선택 단계 대상 실행이 해당 단계 뒤를 실행하지 않음
* 단계 출력 한도와 최대 32단계가 두 정책에서 동일함

## 8.2 상태와 작업자

* 256 KiB 경계에서 50 ms와 200 ms 지연이 정확함
* 붙여넣기 하나가 실행 요청 하나만 예약함
* 최신 대기 요청 하나만 실행됨
* 오래된 `request_id`의 성공, 오류와 취소 결과가 상태를 변경하지 않음
* 선택 단계 재계산 중 `f`로 돌아가면 늦은 단계 결과가 반영되지 않음
* Input 또는 Pipeline 변경 시 선택 단계 결과가 폐기됨
* Smart 자동 판정과 수동 View 고정이 유지됨
* `Esc` 취소 뒤 늦게 끝난 동기 변환 결과가 상태를 변경하지 않음

## 8.3 렌더링과 조작

* 120열 이상, 90~119열, 40~89열과 최소 크기 안내
* 낮은 높이에서 선택적 행을 우선순위대로 숨김
* Text의 ANSI·OSC 52·NUL과 C0/C1 escape
* Hex의 Offset, 8바이트 간격, ASCII 영역과 Viewport 제한
* Trace의 성공·비활성·실패·미실행 상태
* `NO_COLOR`에서 색상 없는 텍스트 상태
* Input 직접 편집과 패널별 키 충돌 부재
* UTF-8 원문 복사와 바이너리 소문자 Hex 복사
* 최대 크기 바이너리의 Hex 복사 길이 계산과 클립보드 실패

## 8.4 회귀와 플랫폼

현재 플랫폼 결과는 README의 `최신 로컬 검증 요약`을 따른다. 2026-07-31 macOS에서 Bash·Zsh 기본 PTY와 실제 복사 경로가 통과했고 `pbpaste`로 소문자 `ff`를 확인했으며 이전 클립보드 내용은 복원하지 않았다. 이 환경에는 Linux가 없어 Linux 미지원·X11·Wayland 경로는 미검증이다.

* 기존 CLI의 도움말, 목록, 성공 출력, 오류 문구와 종료 코드가 바뀌지 않음
* 기존 8개 공개 변환 ID와 직접 명령 구조가 유지됨
* macOS와 Linux의 Bash·Zsh에서 CLI와 TUI Smoke 시험
* 클립보드 사용 가능·불가능 상태와 X11 경로
* macOS 필수 클립보드 Smoke는 제품 복사 뒤 `pbpaste`로 소문자 `ff`를 확인하며, 시험 전 내용을 백업하거나 시험 뒤 복원하지 않음
* raw mode, 대체 화면, bracketed paste와 mouse capture의 정상·인터럽트·패닉 복구
* SGR 마우스로 패널 포커스, Pipeline·Add Transform 선택, Output·목록 휠과 Modal 동작

필수 로컬 검증 명령은 기존 README의 형식, Clippy, 전체 시험, 릴리스 렌더 측정과 셸 Smoke 명령을 유지한다.

# 9. 완료 기준

다음 조건을 모두 만족하면 이 고도화 범위가 완료된 것으로 본다.

* `toc tui`에서 비 UTF-8 디코딩 결과를 Hex로 손실 없이 확인하고 복사할 수 있다.
* 기존 CLI는 같은 결과를 기존 UTF-8 오류로 처리한다.
* 최종 결과와 선택 단계 결과를 명시적으로 전환할 수 있다.
* Text, Hex, Trace가 안전하고 Viewport에 비례해 렌더링된다.
* Wide, Medium, Narrow와 최소 크기 화면이 승인된 구조를 따른다.
* UI 입력은 Pipeline 실행으로 차단되지 않는다.
* 오래된 결과가 화면과 복사 가능 상태에 반영되는 경우가 없다.
* 최종 결과 외의 단계 바이트를 무제한 보관하지 않는다.
* 기존 CLI·TUI 회귀 시험과 macOS·Linux 셸 점검이 통과한다.

# 10. 후속 작업 대장

아래 항목은 이번 구현에서 제외하지만 폐기하지 않는다. 실제 사용 사례, 측정 결과 또는 별도 요구사항이 확인되면 독립된 설계와 구현 계획으로 승격한다.

| 후속 작업 | 검토 조건 |
|---|---|
| 파일 열기·원자적 저장·원시 바이트 내보내기 | 클립보드 Hex로 해결할 수 없는 바이너리 작업 흐름이 확인될 때 |
| JSON/JWT Tree와 노드 탐색 | 구조화 Preview 요구와 대형 문서 시험 자료가 있을 때 |
| 변환별 옵션과 스키마 Form | 고정 동작으로 해결되지 않는 승인된 옵션이 생길 때 |
| Recipe, 세션 복원과 최근 사용 기록 | 저장 형식, 권한, 손상 복구와 민감정보 정책을 함께 설계할 때 |
| 사용자 키맵·테마·아이콘 모드 | 기본 키나 적응형 테마로 해결되지 않는 접근성 요구가 확인될 때 |
| 퍼지 검색, 별칭·카테고리와 최근 사용 순위 | 변환 수 증가로 단순 부분 검색이 실제로 느려질 때 |
| Output 검색·선택 영역·부분 복사 | 긴 Text·Hex 결과의 재현 가능한 탐색 요구가 있을 때 |
| 예산 기반 단계 결과 캐시 | 요청 시 재계산이 측정상 병목일 때 |
| 변환 내부 협력 취소 | 한 단계가 사용자가 체감할 만큼 오래 실행되는 변환을 추가할 때 |
| 붙여넣기 정규화 단일 패스 | 최대 약 8 MiB인 임시 정규화 버퍼가 측정상 메모리 문제로 확인될 때 |
| Tokio·Task Scheduler | 네트워크나 다수 비동기 I/O 작업이 제품 범위에 들어올 때 |
| 대용량 Streaming Pipeline | 현재 입력·출력 제한을 넘는 승인된 파일 사용 사례가 있을 때 |
| 신규 인코딩·해시·압축·JWT Operation | 각 형식의 입력 규약, 보안 경계와 시험 벡터가 승인될 때 |
| 속성 시험·퍼징·전용 Snapshot 의존성 | 재현 결함, 공격 표면 또는 기존 시험으로 잡기 어려운 회귀가 확인될 때 |
| 플러그인과 사용자 코드 | 파일·네트워크·프로세스 격리와 자원 제한이 먼저 승인될 때 |

이 대장은 `docs/prd/init-prd.md`의 후속 아이디어를 대체하지 않는다. TUI 작업판과 직접 관련된 항목의 구현 우선순위와 승격 조건만 구체화한다.
