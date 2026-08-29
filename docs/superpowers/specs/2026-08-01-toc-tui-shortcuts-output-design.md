# toc TUI 단축키·출력 보기 개선 설계

**작성일:** 2026-08-01  
**상태:** 사용자 승인·구현 완료  
**대상 버전:** 0.2.x

> 2026-08-07 승인된 `2026-08-07-toc-terminal-native-tui-design.md`가 이 문서의
> 고정 RGB와 Output 위치 카운터 계약을 대체한다.

> 2026-08-07 단축키 후속 변경은 아래의 과거 `Delete`/`d`/`ㅇ`, `a`/`ㅁ`, Output `p`/`ㅔ`
> 계약을 대체한다. Picker의 `Backspace`는 계속 Search를 편집하고 전역 `Ctrl+p`/`Ctrl+ㅔ`
> Add는 유지한다.

## 1. 목적

현재 TUI의 중복 단축키와 상단 정보를 줄이고, 한글 입력 상태에서도 주요
단축키를 사용할 수 있게 한다. Output의 Hex와 Trace는 Ratatui 표를 사용해
열 정렬, 상태 색상과 작은 화면 대응을 개선한다.

이번 변경은 기존 상태·실행 흐름을 재사용하고 대용량 복사 준비만 별도 단일
작업자로 분리한다. 새 의존성, 사용자 키맵, 실행 엔진 변경은 추가하지 않는다.

## 2. 범위

### 2.1 포함

- Pipeline의 `j`, `k`, `J`, `K` 단축키 제거
- Output의 역방향 보기 전환 `V` 제거
- 전역 Pretty·Raw Copy 단축키 `F3`, `F4` 제거
- 영문 소문자 단축키와 같은 두벌식 자판 위치의 한글 단축키 지원
- `Ctrl+p`와 `Ctrl+ㅔ`, `Ctrl+q`와 `Ctrl+ㅂ` 동시 지원
- Output의 `Enter` Pretty Copy와 `Shift+Enter` Raw Copy
- Pipeline 선택 단계의 `Delete`, `d`, `ㅇ` 삭제
- 상단 App Bar의 `FOCUS` 제거
- 최종 Output 제목의 `FINAL` 제거
- Hex·Trace의 Ratatui 표 렌더링과 반응형 열 구성
- Trace 첫 실패 상세 자동 표시
- Output 제목의 현재 바이트·행 위치
- 대용량 복사 준비와 시스템 쓰기의 UI 스레드 분리
- 일반 Footer 상태의 2초 또는 사용자 조작 시 만료
- Output Viewport 크기에 맞춘 페이지 이동과 Resize 보정
- Pipeline 삭제 완료 상태
- 관련 도움말, README와 기존 TUI 설계 문서 현행화

### 2.2 제외

- CLI Pipeline 실행 엔진 최적화
- 사용자 설정형 키맵과 설정 파일
- 삭제 실행 취소와 삭제 확인 Modal
- Trace 행 선택과 상세 열기 단축키
- Output 검색, 선택 영역과 부분 복사
- 새로운 테마·아이콘·렌더링 의존성

CLI 지연은 `cargo run`의 기본 Debug 빌드가 원인이었다. Release 실행으로
해결되었으므로 실행 엔진과 변환 구현은 변경하지 않는다.

## 3. 설계 원칙

1. 기존 상태 변경 함수와 실행 예약 경로를 재사용한다.
2. Input 포커스에서는 일반 문자를 전역 명령으로 가로채지 않는다.
3. 도움말은 영문 소문자 기준으로만 표기하고 한글 별칭은 표시하지 않는다.
4. 색상 없이도 열, 상태 문자열과 기호만으로 의미를 구분할 수 있어야 한다.
5. Artifact와 Trace 전문을 렌더링용 단일 문자열로 만들지 않는다.
6. 화면에 보이는 행만 생성하고 기존 4 KiB 렌더링 예산을 유지한다.
7. Quiet Prism 색상 상수와 Trace 상태 문자열은 각각 기존 단일 경로에서 공유한다.

## 4. 키 바인딩

### 4.1 전역

| 도움말 표기 | 실제 입력 | 동작 |
|---|---|---|
| `Tab`, `Shift+Tab` | 동일 | 다음·이전 패널 |
| `Ctrl+p` | `Ctrl+p`, `Ctrl+ㅔ` | Operation Palette |
| `F1` | 동일 | 컨텍스트 도움말 |
| `?` | Input 이외에서 동일 | 컨텍스트 도움말 |
| `Ctrl+q` | `Ctrl+q`, `Ctrl+ㅂ` | 종료 또는 폐기 확인 |
| `Ctrl+c` | 동일 | 강제 종료 |
| `Esc` | 동일 | Modal·Zoom 닫기 또는 실행 취소 |

`F3`과 `F4`는 모든 패널에서 제거한다. 전역 Copy 단축키는 두지 않는다.
`Ctrl+c`는 운영체제 수준 강제 인터럽트이므로 기존 입력 계약을 유지하고 한글
별칭을 추가하지 않는다.

### 4.2 Pipeline

| 도움말 표기 | 실제 입력 | 동작 |
|---|---|---|
| `↑`, `↓` | 동일 | 선택 이동 |
| `Shift+↑`, `Shift+↓` | 동일 | 선택 단계 재정렬 |
| `Space` | 동일 | 선택 단계 활성화 전환 |
| `Backspace` | 동일 | 선택 단계 삭제 |
| `Enter` | 동일 | 읽기 전용 Inspector |
| `s` | `s`, `ㄴ` | 선택 Pipeline 단계 결과 요청 |
| `z` | `z`, `ㅋ` | Pipeline Zoom |

`j`, `k`, `J`, `K`는 아무 동작도 하지 않는다.

### 4.3 Output

| 도움말 표기 | 실제 입력 | 동작 |
|---|---|---|
| `Enter` | `Enter` | Pretty Copy |
| `Shift+Enter` | 동일 | Raw Copy |
| `v` | `v`, `ㅍ` | 다음 View |
| `f` | `f`, `ㄹ` | 보관된 최종 결과 복귀 |
| `z` | `z`, `ㅋ` | Output Zoom |
| 방향키, `PageUp`, `PageDown`, `Home`, `End` | 동일 | 현재 View 스크롤 |

View 순서는 Smart → Text → Hex → Trace → Smart다. `V` 역방향 전환과
`y` Pretty Copy는 제거한다. Trace와 복사 불가능 상태에서는 `Enter`와
`Shift+Enter`가 아무 효과도 만들지 않는다.

`PageUp`과 `PageDown`은 마지막으로 렌더링한 Output 내부 크기를 사용한다.
Text는 그래핌·제어 문자·줄바꿈 규칙으로 계산한 이전·다음 페이지 시작점으로,
Hex와 Trace는 Header와 실패 상세를 제외한 실제 데이터 행 수만큼 이동한다.
`End`는 마지막 데이터가 포함된 전체 페이지 시작점으로 이동한다.

### 4.4 Modal

확인 Modal은 `Enter`, `y`, `ㅛ`를 승인으로, `Esc`, `n`, `ㅜ`를
취소로 처리한다. 대문자 `Y`, `N`은 별칭으로 사용하지 않는다.

Operation Palette가 열려 있을 때 일반 한글 문자는 검색어 입력으로 처리한다.
Modal이 일반 패널 단축키보다 우선하는 기존 규칙은 유지한다.

### 4.5 문자 별칭 처리

영문·한글 별칭은 키 처리 분기에서 명시적으로 함께 매칭한다. 입력 처리와
도움말을 생성하는 범용 키맵 계층은 만들지 않는다. Input 편집에는
`tui-textarea-2`의 기존 키 처리를 그대로 전달한다.

## 5. 화면 기본 구조

### 5.1 App Bar

App Bar는 `>_ TOC`만 표시한다. `FOCUS: INPUT`, `FOCUS: PIPELINE`,
`FOCUS: OUTPUT`은 제거한다.

현재 포커스는 다음 두 요소로 구분한다.

- 포커스된 패널의 굵은 Accent 테두리
- Footer 첫째 줄의 `INPUT`, `PIPELINE`, `OUTPUT` 범위명

### 5.2 Output 제목

준비된 최종 결과는 다음과 같이 표시한다.

```text
» OUTPUT / HEX · BYTE 32/100
```

선택 단계 결과만 출처를 추가한다.

```text
» OUTPUT / STEP 02 / TRACE · ROW 4/10
```

최종 결과에는 `FINAL`을 표시하지 않는다. `OutputStatus::Ready`일 때
Text·Smart·Hex는 0부터 시작하는 `BYTE 현재/전체`, Trace는 1부터 시작하는
`ROW 현재/전체`를 표시한다. 실행·지연·실패·취소 상태에서는 보이는 이전
Artifact에 카운터를 붙이지 않는다. 너비가 부족하면 기존 `· N B`, 기본 제목
순서로 축약한다.

### 5.3 Footer와 Help

Output Footer 첫째 줄은 복사 가능할 때 다음 키를 우선 표시한다.

```text
OUTPUT │ Enter Pretty  Shift+Enter Raw  v View  f Final │ z Zoom
```

전역 Footer와 F1 Help에서 `F3/F4`, `j/k`, `J/K`, `v/V`와
`Enter/y` 표기를 제거한다. 한글 별칭은 표시하지 않으며 `Ctrl+p`,
`Ctrl+q`도 영문 소문자로 표기한다.

Pipeline·Output 전체 Help는 기존 높이를 유지하고 `Ctrl+c`와 같은 행에
`Esc Close zoom or cancel request`를 표시한다. Footer 첫째 줄의 우선순위는
실패·취소, 장시간 변환, 복사 진행, 일반 상태, 패널 명령 순서다. 복사 준비와
쓰기 중에는 각각 `Preparing copy…`, `Writing clipboard…`를 표시한다.
일반 상태는 2초 또는 다음 키 입력·Input 붙여넣기·좌클릭·휠 중 먼저 발생한
시점에 사라진다.

## 6. Pipeline 삭제

기존 `delete_selected`를 모든 삭제 키의 공통 경로로 사용한다.

1. 선택 단계가 없으면 아무 상태도 변경하지 않는다.
2. 선택 단계 하나를 제거한다.
3. 선택 인덱스를 남은 단계 범위로 보정한다.
4. 기존 `changed` 경로로 최종 결과 캐시를 무효화하고 실행을 예약한다.
5. Footer 상태에 안전하게 정리한 표시 이름을 사용해
   `Removed JSON Prettify`처럼 표시한다.

삭제 확인과 별도 삭제 상태는 만들지 않는다. 최대 32단계의 현재 Pipeline은
`Vec::remove`로 충분하다.

## 7. Hex View

### 7.1 구조

Hex는 Ratatui `Table`, `Row`, `Cell`로 렌더링한다. 한 행의 열은
Offset, 앞쪽 바이트, 뒤쪽 바이트, ASCII다. 화면 너비에 따라 다음 구성을
사용한다.

| Output 내부 너비 | 행당 바이트 | 표시 열 |
|---|---:|---|
| 78열 이상 | 16 | Offset, 0–7, 8–15, ASCII |
| 60–77열 | 16 | Offset, 0–7, 8–15 |
| 60열 미만 | 8 | Offset, 0–7 |

최소 지원 터미널인 40열에서는 패널 테두리 안에 Offset과 8바이트가 들어간다.
Offset은 바이트 위치를 나타내는 8자리 대문자 16진수다.

### 7.2 스타일

- Header와 빈 셀: Muted
- Offset: Cyan
- ASCII 열의 출력 가능한 문자: Green
- 제어 문자·비 ASCII 바이트의 Hex 값: Yellow
- 출력 가능한 ASCII 바이트의 Hex 값: 기본 Text

`NO_COLOR`에서는 모든 색상을 제거하고 값, 간격과 열 구성은 유지한다.

### 7.3 Viewport

행당 바이트를 결정하는 순수 함수를 렌더링과 스크롤 범위 계산이 함께
사용한다. Page 이동은 실제 데이터 행 수를 사용하며 최대 Offset은 전체 행에서
보이는 행을 뺀 값이다. Resize 뒤 현재 최상단 바이트를 새 행 크기에 맞춰
정렬하고 유효 범위로 보정한다. 따라서 넓은 화면과 좁은 화면을 전환해도 결과
끝을 벗어난 빈 Viewport가 나타나지 않는다.

보이는 행만 생성하고 Header를 제외한 실제 높이만큼 제한한다. 생성된 텍스트와
셀 내용의 합은 기존 4 KiB 예산을 넘지 않는다.

## 8. Trace View

### 8.1 표

Trace는 Ratatui 표로 다음 열을 표시한다.

| 열 | 값 |
|---|---|
| Step | `#1` 형식의 1부터 시작하는 단계 |
| Operation | 등록된 변환 표시 이름 |
| Input | 입력 바이트 수 또는 `—` |
| Output | 출력 바이트 수 또는 `—` |
| Time | 실행 시간 또는 `—` |
| Status | `OK`, `OFF`, `ERROR`, `NOT RUN`, `CANCELLED` |

70열 미만에서는 Input과 Output을 `24→17 B` 형식의 Size 열로 합치고 Time을
생략한다. 변환 표시 이름은 등록 정보에서 가져오되 찾을 수 없는 ID는 기존처럼
안전하게 Escape한 공개 ID를 사용한다.

상태 색상은 다음과 같다.

- `OK`: Green
- `ERROR`: Red
- `CANCELLED`: Yellow
- `OFF`, `NOT RUN`: Muted

`NO_COLOR`에서는 상태 문자열을 그대로 유지한다.

### 8.2 첫 실패 상세

실패 Trace가 있으면 첫 실패의 안전한 상세를 표 아래에 자동 표시한다.
행 선택 상태나 상세 열기 키는 추가하지 않는다.

```text
STEP 3 · JSON Prettify
invalid JSON syntax at line 1, column 8
```

Output 내부 높이가 5행 이상일 때 최대 3행을 상세에 예약한다. 높이가 부족하면
실패 행과 `ERROR` 상태를 우선하고, 상세는 남은 공간에 맞춰 그래핌 경계에서
축약한다.

Header와 실패 상세를 뺀 실제 데이터 행 수를 렌더링과 페이지 이동이 함께
사용한다. 마지막 페이지의 최대 행은 전체 Trace 행에서 이 수를 뺀 값이다.

입력·출력 전문, 비 UTF-8 미리보기와 외부 제어 문자는 상세에 포함하지 않는다.
기존 오류 요약 함수와 외부 문자열 Escape 경로를 재사용한다.

## 9. 복사와 오류

- Pretty·Raw payload 생성과 JSON 처리 규칙은 변경하지 않는다.
- `Artifact`의 저비용 스냅샷, 복사 모드와 요청 번호를 전용 단일 작업자에 보낸다.
- 작업자는 JSON 정리, Binary Hex 변환, 위험 문자 검사와 시스템 쓰기를 수행한다.
- `arboard::Clipboard`는 작업자 안에서 지연 생성하고 TUI 종료까지 유지한다.
- 복사 상태는 `Idle → Preparing → AwaitingConfirmation → Writing → Idle`이다.
- 복사 중 추가 Enter는 무시하고, 다른 요청 번호의 늦은 결과는 폐기한다.
- 위험한 제어 문자는 준비된 payload를 소유하는 기존 확인 Modal을 거친다.
- 확인 취소나 F1 전환은 준비된 payload를 폐기한다.
- 준비·쓰기 실패와 채널 종료는 `Copy unavailable`로 복구한다.
- 클립보드 실패는 기존 화면 상태와 Artifact를 유지한다.
- Trace, 실패, 취소, 실행, 지연과 Artifact 부재 상태에서는 복사를 차단한다.
- Hex·Trace의 화면 스타일은 복사 payload에 영향을 주지 않는다.

## 10. 시험 전략

새 시험 프레임워크 없이 기존 단위 시험과 Ratatui `TestBackend` 시험을
확장한다.

### 10.1 상태와 키

- `j`, `k`, `J`, `K`, `V`, `F3`, `F4`가 아무 효과도 만들지 않음
- 각 영문 소문자와 한글 별칭이 같은 상태·Effect를 만듦
- `Ctrl+p/ㅔ`, `Ctrl+q/ㅂ`가 같은 동작을 함
- `Enter`는 Pretty, `Shift+Enter`는 Raw 복사를 요청함
- Trace와 복사 불가능 상태에서 두 Enter 조합이 차단됨
- 준비 중 중복 요청 차단, Artifact 스냅샷 보존과 늦은 결과 폐기
- 위험 문자 확인 전 시스템 쓰기 금지
- 준비·쓰기 실패와 채널 종료의 비치명적 복구
- 일반 상태의 2초 만료와 사용자 조작 시 즉시 해제
- `Backspace`가 선택 단계를 삭제하고 선택 위치를 보정함
- 빈 Pipeline 삭제가 진정한 무동작임
- 삭제가 기존 최종 결과 재계산을 예약함
- 대문자 별칭이 다시 활성화되지 않음

### 10.2 렌더링

- App Bar에 `FOCUS`가 없음
- 최종 Output 제목에 `FINAL`이 없고 단계 결과에만 `STEP NN`이 있음
- 준비된 결과에만 바이트·행 위치가 있고 좁은 제목은 크기·기본 제목으로 축약됨
- Footer 우선순위와 복사 진행 문구가 정확함
- Footer와 Help가 영문 소문자 기준이며 제거된 키를 포함하지 않음
- Pipeline·Output Help에 실제 `Esc` 계약이 있음
- 78열 이상, 60–77열, 60열 미만 Hex 열과 행당 바이트가 정확함
- Hex Offset, 바이트 분류 색상과 `NO_COLOR` 구조가 정확함
- 넓은 Trace의 여섯 열과 좁은 Trace의 병합 열이 정확함
- 모든 Trace 상태의 문자열·색상과 `NO_COLOR` 구조가 정확함
- 첫 실패 상세와 작은 높이 축약이 안전함
- Text의 ASCII·한글·결합문자·개행·제어 문자 페이지 경계가 정확함
- Hex 8·16바이트 행, Trace 상세 유무와 Resize 보정이 정확함
- `PageUp`·`PageDown`·`Home`·`End` 경계와 4 KiB 예산을 넘지 않음

### 10.3 회귀

- 기존 Input 편집과 한글 입력
- Operation Palette 검색
- Pipeline 재정렬·활성화·Inspector
- Text·Smart View와 스크롤
- 최종 결과·선택 단계 결과 전환
- Pretty·Raw·Hex Copy와 위험 문자 확인
- 마우스 포커스·선택·휠
- 전체 단위·통합·셸 Smoke 시험
- Format과 Clippy

## 11. 문서 동기화

구현 변경과 같은 커밋에서 최소 Diff로 다음 문서를 현행화한다.

- `README.md`의 TUI 키 바인딩과 최신 검증 요약
- `docs/superpowers/specs/2026-07-31-toc-tui-ux-refresh-design.md`
- `docs/superpowers/specs/2026-08-01-toc-quiet-prism-design.md`
- `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md`

기존 문서의 `F3/F4`, `j/k`, `J/K`, `v/V`, `Enter/y`, `FINAL`,
`FOCUS` 설명을 새 계약과 동기화한다.

## 12. 완료 기준

- 한글 입력 상태에서도 승인된 문자 단축키가 동작한다.
- 도움말은 영문 소문자 기준으로만 표시된다.
- 제거 대상 키는 어느 패널에서도 기존 동작을 실행하지 않는다.
- Output의 Enter 조합이 Pretty·Raw Copy를 정확히 구분한다.
- Pipeline 삭제가 기존 재계산 흐름과 안전하게 결합된다.
- App Bar와 Output 제목에서 승인된 중복 정보가 제거된다.
- Hex와 Trace가 승인된 반응형 표·색상·첫 실패 상세를 제공한다.
- 대용량 복사 준비와 시스템 쓰기가 UI 스레드를 막지 않는다.
- 일반 상태가 자동 또는 사용자 조작으로 해제되고 실제 `Esc` 도움말이 표시된다.
- Output 페이지 이동과 위치 카운터가 실제 Viewport에 맞는다.
- 색상 비활성, 작은 화면과 오류 상태에서도 정보가 손실되지 않는다.
- 관련 문서와 기존 회귀 시험이 함께 현행화된다.
