# doop TUI 시각·복사 경험 개선 설계

* **작성일:** 2026-07-31
* **상태:** 사용자 승인·구현 전
* **기준 문서:** `docs/superpowers/specs/2026-07-31-doop-tui-workbench-design.md`
* **대상:** `doop tui`
* **제품 UI 언어:** 영어
* **프로젝트 문서 언어:** 한국어

---

# 1. 목적과 범위

이 설계는 현재 TUI 작업판의 기능 구조를 유지하면서 Pipeline 상태 표시,
반응형 배치, Add Transform 정보 구조, 복사 단축키와 시각 체계를 개선한다.
입력, 변환 실행, 단계 결과, View와 작업자 구조는 바꾸지 않는다.

사용자에게 노출되는 변경은 다음과 같다.

* Pipeline의 활성화 상태 중복 제거
* 좁은 너비에서 Pipeline, Input, Output의 세로 배치
* Add Transform 항목 설명과 키 도움말 분리
* Pretty Copy와 Raw Copy
* 앱 타이틀, 포커스 표시, 패널 제목과 테두리 개선
* 최하단 두 줄 도움말

CLI 명령, 공개 변환 ID, 변환 결과, Pipeline 실행 정책, View 종류,
Inspector, Zoom, 입력 한도와 작업자 정책은 변경하지 않는다. 새 변환,
사용자 테마, 사용자 키맵, 외부 아이콘 글꼴과 새 의존성은 이번 범위에
포함하지 않는다.

# 2. 승인된 사용자 경험

| 항목 | 결정 |
|---|---|
| 기본 시각 방향 | Neon Console |
| 상단 | 박스 없는 `>_ DOOP │ FOCUS: <PANE>` 한 줄 |
| 패널 제목 | `$ PIPELINE`, `> INPUT`, `» OUTPUT / <SOURCE> / <VIEW>` |
| 패널 테두리 | 굵은 테두리와 포커스 색상 |
| Pipeline 활성화 | `[ON]`, `[OFF]`를 정확히 한 번만 표시 |
| 좁은 너비 | Pipeline 30%, Input 30%, Output 40% 세로 배치 |
| 하단 | 포커스 도움말 한 줄과 공통 도움말 한 줄 |
| Pretty Copy | 유효한 JSON은 두 칸 들여쓰기, 나머지는 원본 |
| Raw Copy | 유효한 JSON은 구조 공백 제거, 나머지는 원본 |
| 바이너리 복사 | 두 모드 모두 공백 없는 소문자 Hex |
| 복사 대상 | 현재 Output의 FINAL 또는 STEP Artifact |
| 복사와 View | 원본 Artifact를 사용하고 Trace에서는 비활성 |
| 전역 복사 키 | `F3` Pretty, `F4` Raw |
| Output 복사 키 | `Enter` Pretty |
| 강제 종료 | 기존 `Ctrl+C` 유지 |

Pretty와 Raw는 표시 View를 복사하는 기능이 아니다. Hex View의 오프셋,
열 제목과 공백, Trace 표는 클립보드에 포함하지 않는다.

# 3. Pipeline 표시

## 3.1 활성화와 실행 상태

Pipeline 행은 활성화 여부와 실행 결과를 서로 다른 위치에서 한 번씩만
표시한다.

```text
[ON]  ✓ JSON Prettify
[OFF]   URL Decode
[ON]  × Base64 Decode
```

`[ON]`과 `[OFF]`는 현재 단계 활성화 여부다. `✓`와 `×`는 마지막 실행의
성공과 실패다. 비활성 단계에는 실행 상태 `OFF`나 비활성 기호를 다시
붙이지 않는다.

실행 중, 미실행과 취소 상태도 활성화 표시 뒤의 실행 상태 위치만 사용한다.
너비가 부족하면 단계 크기와 낮은 우선순위 상태 문구부터 숨기되 `[ON]`,
`[OFF]`, 선택 표시와 변환 이름은 유지한다. 색상은 상태를 보조하며
문자 정보의 대체 수단이 아니다.

## 3.2 선택과 크기

기존 선택, 활성화 전환, 이동, 삭제, Inspector와 Zoom 동작은 유지한다.
선택 단계가 보이는 범위를 벗어나면 기존처럼 목록을 스크롤한다. 넓은
화면의 입출력 바이트 크기는 공간이 있을 때만 표시한다.

# 4. 반응형 레이아웃

## 4.1 상단과 패널

상단에는 박스를 만들지 않고 앱 타이틀과 현재 포커스를 구분자로 나눈다.

```text
>_ DOOP  │  FOCUS: OUTPUT
```

기존 Navigation과 Step Summary 상단 줄은 제거한다. 패널은 승인된
Neon Console 제목과 굵은 테두리를 사용한다.

```text
$ PIPELINE
> INPUT
» OUTPUT / FINAL / SMART
```

포커스 패널은 황색, 나머지는 녹색 계열로 구분한다. 장식은 Ratatui가
이미 제공하는 테두리, 기본 색상과 터미널 문자만 사용한다. 이미지,
외부 아이콘 글꼴, 고정 RGB 배경과 새 UI 의존성은 추가하지 않는다.

## 4.2 너비와 높이

레이아웃 경계는 다음과 같다.

| 조건 | 배치 |
|---|---|
| 너비 120열 이상 | Pipeline 왼쪽, Input·Output 오른쪽 |
| 너비 90~119열 | 같은 분할 구조, 부가 정보 축소 |
| 너비 40~89열, 높이 12행 이상 | Pipeline 30%, Input 30%, Output 40% 세로 배치 |
| 높이 10~11행 | 현재 포커스 패널 하나만 표시 |
| 너비 40열 미만 또는 높이 10행 미만 | 크기 확대 안내 |

세로 비율은 포커스에 따라 바뀌지 않는다. 정수 행 배분에서 생기는
나머지는 Output에 우선 배정한다. 각 패널은 테두리를 포함해 최소 3행을
확보한다.

Zoom은 너비 모드와 관계없이 기존처럼 대상 패널이 전체 콘텐츠 영역을
사용한다. Modal은 현재 레이아웃 위에 표시한다.

## 4.3 색상 비활성 환경

`NO_COLOR`에서는 색상만 제거한다. 타이틀, 구분자, 굵은 테두리,
`[ON]`·`[OFF]`, `✓`·`×`와 포커스 문구는 유지한다. 포커스와 상태를
색상만으로 전달하지 않는다.

# 5. 최하단 도움말과 상태

도움말은 화면 최하단 두 줄에만 표시한다.

1. 포커스된 패널의 도움말
2. 공통 도움말

예:

```text
[OUTPUT] Enter Pretty · v/V View · p Step · f Final · z Zoom
[COMMON] Tab Focus · F3 Pretty · F4 Raw · F1 Help · Ctrl+Q Quit
```

너비가 부족하면 각 줄의 낮은 우선순위 항목부터 생략한다. 두 줄의 역할과
순서는 바꾸지 않는다.

Pipeline 오류, 복사 완료와 클립보드 오류 같은 상태가 있으면 첫 번째 줄을
상태 메시지로 교체한다. 두 번째 공통 도움말은 유지한다. 다음 상태 변경이
기존 `App.status`를 비우면 포커스 도움말을 다시 표시한다. 별도 타이머,
세 번째 상태 줄과 상단 도움말은 추가하지 않는다.

F1 Context Help Modal은 유지한다. 여기서 "최하단에만"은 상시 노출되는
화면 도움말의 위치를 뜻하며, 사용자가 명시적으로 연 Help Modal을
제거한다는 뜻이 아니다.

# 6. Add Transform

Add Transform은 검색, 목록, 선택 항목 상세와 키 도움말의 네 영역을
유지하되 정보 구조를 바꾼다.

각 변환 항목은 두 줄이다.

```text
> JSON Prettify  [format-json]
  Indent strict JSON while preserving keys and value tokens
```

첫째 줄에는 표시 이름과 공개 식별자를 표시한다. 둘째 줄에는 등록 목록의
한 줄 설명을 표시한다. 선택 강조는 두 줄 전체에 적용한다. 목록의 설명과
선택 상세에서 같은 설명을 반복하지 않는다.

목록과 선택 상세 사이에는 구분선을 둔다. 선택 상세는 입력 조건,
`behavior`와 TUI 결과 규칙을 별도 줄에 표시한다. 상세와 최하단 키
도움말 사이에도 구분선을 둔다.

```text
───────────────────────────────────────────────────────────
INPUT     Valid UTF-8 text
BEHAVIOR  Two-space indentation; token spelling and order
          are preserved
TUI       Result remains bytes; Smart selects Text or Hex
───────────────────────────────────────────────────────────
↑/↓ Select · Enter Add · Backspace Search · Esc Cancel
```

두 줄 항목 때문에 한 화면에 보이는 변환 수가 줄어들 수 있다. 목록은 기존
선택 추적 스크롤을 사용한다. 작은 Modal은 검색, 선택 항목, 핵심 설명과
닫기 키를 우선하며 바깥 프레임을 넘지 않는다.

# 7. Pretty Copy와 Raw Copy

## 7.1 키 우선순위

일반 작업 화면에서 다음 키를 처리한다.

| 키 | 범위 | 동작 |
|---|---|---|
| `F3` | Pipeline, Input, Output | Pretty Copy |
| `F4` | Pipeline, Input, Output | Raw Copy |
| `Enter` | Output | Pretty Copy |
| `Ctrl+C` | 전역 실행 루프 | 기존 강제 종료 |

Modal이 열려 있을 때는 기존 Modal 키 처리가 우선한다. F3와 F4는 Modal
뒤의 Artifact를 복사하지 않는다. Input의 일반 문자 입력, Pipeline의
Inspector `Enter`, Add Transform의 추가 `Enter`는 변경하지 않는다.

## 7.2 복사 대상

복사 대상은 `OutputState.active_artifact`다. `OutputSource::Final`이면
최종 결과, `OutputSource::Step(index)`면 현재 선택 단계 요청의 결과를
복사한다.

복사 가능 조건은 기존과 같다.

* Output 상태가 `Ready`
* 활성 Artifact가 존재
* View가 Trace가 아님

복사 키가 Output 포커스 밖에서 눌려도 이 조건을 만족하지 않으면 효과를
만들지 않는다. 오래되었거나 실행·지연·실패·취소 중인 결과는 복사하지
않는다.

## 7.3 중앙 포맷 경로

현재 `clipboard_payload`를 Pretty와 Raw 모드를 받는 단일 경로로
확장한다. 모든 현재·향후 변환의 Artifact가 이 함수를 통과한다.

```text
Artifact
  ├─ non-UTF-8 → lowercase compact Hex
  └─ UTF-8
       ├─ Pretty → try format-json
       └─ Raw    → try minify-json
             ├─ valid strict JSON → transformed text
             ├─ invalid JSON      → exact original text
             └─ resource failure  → Copy unavailable
```

JSON 처리는 새 parser를 만들지 않고 등록된 `format-json`과
`minify-json` 실행 함수를 재사용한다. 두 함수는 키 순서와 키·값 토큰
표기를 보존한다. JSON 문자열 내부의 공백은 제거하지 않는다.

`InvalidJson`은 포맷 불일치로 취급해 원문을 복사한다.
`OutputTooLarge`와 메모리 확보 실패는 원문으로 조용히 대체하지 않고
`Copy unavailable` 상태를 표시한다. 포맷 결과는 기존 TUI 출력 한도를
넘지 않는다.

앞으로 추가되는 변환도 기본적으로 UTF-8 원문 또는 바이너리 Hex 동작을
자동으로 적용받는다. 두 번째 구조화 포맷이 실제로 추가되고 공백 변경의
안전성이 정의되었을 때만 중앙 함수에 명시적인 포맷 분기를 추가한다.
현재는 포맷 trait, formatter registry와 변환별 copy metadata를 만들지
않는다.

## 7.4 안전성과 완료 상태

위험한 제어 문자 검사는 Pretty 또는 Raw payload를 완성한 뒤 적용한다.
확인 Modal은 그 payload를 소유하므로 확인 중 Output 결과가 바뀌어도
승인 대상이 바뀌지 않는다. 취소와 다른 Modal 전환은 payload를 폐기한다.

클립보드 쓰기 실패는 Input, Pipeline, Artifact, 결과 원본과 Trace를
보존한다. 성공 상태는 Pretty, Raw와 Hex를 구분한다. 사용자 입력이나
복사 내용을 로그와 오류 메시지에 포함하지 않는다.

# 8. 코드 경계

변경은 기존 경계를 유지한다.

```text
src/tui/state.rs
  복사 모드, 중앙 payload 생성, F3/F4와 Enter 처리, 완료 상태

src/tui/render.rs
  상단, 패널 스타일, Pipeline 행, 반응형 배치, 하단 도움말,
  Add Transform 렌더링
```

Pipeline 실행 엔진, 작업자, View window renderer와 변환 구현은 변경하지
않는다. 새 모듈, trait, formatter registry와 의존성은 추가하지 않는다.

구현 시 `README.md`와 기존 TUI 작업판 설계의 화면·키·복사 설명을 같은
논리적 변경에서 현행화한다.

# 9. 오류 처리

* JSON이 아니면 오류를 표시하지 않고 원문을 복사한다.
* JSON 포맷 출력 한도와 메모리 실패는 `Copy unavailable`로 표시한다.
* 클립보드 오류의 외부 문자열은 기존 제한 길이 escape를 적용한다.
* 복사 실패는 현재 작업판 상태를 변경하지 않는다.
* Trace, 없는 Artifact와 Ready가 아닌 결과는 복사 효과를 만들지 않는다.
* 좁은 화면의 정수 배분은 포화 산술과 최소 높이를 사용한다.
* 외부 상태와 오류 문자열은 렌더 버퍼에 제어 문자로 전달하지 않는다.

# 10. 시험 전략

## 10.1 상태와 복사

* F3와 F4가 세 패널에서 각각 Pretty와 Raw를 요청함
* Modal이 F3와 F4보다 우선함
* Enter가 Output에서만 Pretty를 요청함
* FINAL과 STEP의 현재 활성 Artifact를 복사함
* Trace와 Ready가 아닌 상태에서는 복사하지 않음
* View가 Text·Hex·Smart여도 원본 Artifact를 사용함
* 유효한 JSON Pretty와 Raw가 토큰과 문자열 내부 공백을 보존함
* 비 JSON UTF-8은 정확한 원문을 유지함
* 비 UTF-8은 소문자 compact Hex를 생성함
* 포맷 출력 한도와 클립보드 실패가 작업판 상태를 보존함
* 위험 문자 확인이 완성된 payload를 소유함

## 10.2 렌더링

Ratatui 시험 백엔드로 다음을 검증한다.

* 상단이 박스 없는 타이틀과 포커스 한 줄임
* Navigation과 Step Summary가 상단에 남지 않음
* 패널 제목, 굵은 테두리와 포커스 스타일
* Pipeline의 `[ON]`과 `[OFF]`가 단계마다 한 번만 나타남
* 승인된 성공·비활성·실패 행 형식
* 120열, 90~119열, 40~89열의 경계 배치
* 좁은 화면에서 Pipeline, Input, Output 순서와 30/30/40 배분
* 높이 10~11행의 포커스 단일 패널과 최소 크기 안내
* 하단이 정확히 두 줄이고 상태가 첫째 줄만 대체함
* Add Transform 항목의 두 줄 설명과 두 구분선
* `NO_COLOR`에서 정보가 유지되고 색상만 제거됨

## 10.3 회귀

기존 Pipeline 편집, Input 편집, View 전환, 단계 결과, Zoom, Modal,
취소, 종료와 클립보드 안전 시험을 유지한다. 다음 명령을 통과해야 한다.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
```

# 11. 완료 기준

다음을 모두 만족하면 구현이 완료된 것으로 본다.

* Pipeline 활성화 상태가 `[ON]` 또는 `[OFF]`로 한 번만 표시된다.
* 좁은 너비에서 세 패널이 승인된 순서와 비율로 동시에 보인다.
* Add Transform의 각 항목 설명과 최하단 키 도움말이 분리된다.
* F3, F4와 Output Enter가 승인된 복사 모드와 범위로 동작한다.
* 복사는 현재 Output Artifact를 View와 무관하게 처리한다.
* Pretty와 Raw가 JSON에만 안전한 공백 변환을 적용한다.
* 향후 변환 결과도 중앙 복사 경로의 안전한 기본값을 적용받는다.
* 타이틀, 포커스, 패널 제목과 테두리가 승인된 Neon Console 시각 체계를
  따른다.
* 도움말은 최하단 두 줄에만 상시 표시된다.
* 오류와 `NO_COLOR` 환경에서 기존 보안·접근성 규약이 유지된다.
* 관련 문서와 전체 필수 품질 명령이 현행 상태와 일치한다.
