# toc Output module deepening 설계

**작성일:** 2026-08-29
**상태:** 사용자 승인

## 1. 목표

`Output` 결과 수명주기, `View`, viewport와 navigation 규칙을 하나의 deep module에
집중한다. 사용자에게 보이는 키, 화면, scroll, copy와 실행 동작은 바꾸지 않는다.

## 2. 현재 문제

- `OutputState`의 필드와 상태 전이는 `state.rs`에 있다.
- Text·Hex·Trace 계산은 `views.rs`에 있지만 interface가 많은 helper로 노출된다.
- `render.rs`가 Output 필드와 helper를 직접 조합하고 draw 중 viewport를 갱신한다.
- resize·navigation·표시 변경을 이해하려면 세 module과 여러 테스트를 함께 읽어야 한다.

현재 `views.rs` implementation의 Unicode·4 KiB budget·Hex·Trace 계산은 보존할 가치가
있다. 새 구조는 이 계산을 다시 나누지 않고 Output 수명주기와 함께 깊게 만든다.

## 3. 범위

### 포함

- `views.rs`를 `output.rs`로 교체
- `OutputState`, `Artifact`, `View`, 결과 cache와 traces 이동
- effective View, viewport reflow와 navigation 이동
- 표시할 Text·Hex·Trace 범위를 만드는 presentation 이동
- Output interface 기준으로 관련 테스트 교체
- `AGENTS.md` 구조와 `CONTEXT.md` 용어 현행화

### 제외

- Ratatui widget, 색상, 표와 문자열 배치 변경
- Input editor, Pipeline 편집, shortcut command와 clipboard workflow 변경
- Preview worker, request ID와 `Effect` 구조 변경
- 새 trait, adapter와 dependency 추가
- 사용자 동작 또는 성능 한도 변경

## 4. module과 seam

`src/tui/output.rs`는 in-process deep module이다. implementation은 하나뿐이므로 trait이나
adapter를 만들지 않는다.

```text
App ── lifecycle facts / navigation ──> Output
                                         │
                                  private implementation
                                         │
render.rs <──── presentation / summary ──┘
clipboard.rs <──── copyable Artifact ────┘
```

### Output이 소유하는 것

- final 또는 선택한 Transform의 source
- 요청한 `View`와 effective View 판정
- idle, debouncing, running, ready, failed, cancelled 상태
- final snapshot과 현재 snapshot의 `Artifact`·traces
- Text byte offset, Hex·Trace row offset와 마지막 viewport
- View 순환, line·page·home·end navigation
- resize 시 보이는 byte 또는 trace 위치 보존
- 표시 byte budget과 visible range
- copy 가능 조건

### App이 계속 소유하는 것

- Input과 Pipeline
- request ID 증가와 오래된 Preview 결과 거부
- debounce 시간이 끝났을 때 `PreviewJob` 생성
- Preview worker submit·cancel `Effect`
- dirty flag, transient status와 modal
- key·mouse event를 semantic navigation으로 변환하는 일

### render.rs가 계속 소유하는 것

- Ratatui layout과 widget
- title, 색상, 열 너비와 안전한 화면 문자열 배치
- presentation을 Text·Hex·Trace 화면으로 그리는 일

## 5. interface

Output의 모든 상태 필드는 private으로 만든다. interface는 세 종류의 동작만 노출한다.

1. **Lifecycle**
   - Pipeline 변경으로 final snapshot을 무효화하고 debouncing 상태로 전환
   - Final 또는 선택 단계 실행 시작
   - App이 검증한 Preview 결과 적용
   - final snapshot 복원, 실행 취소와 long-running notice 갱신
2. **Navigation**
   - 다음 View
   - line·page 전후 이동
   - home·end 이동
3. **Read model**
   - `present(&mut self, Viewport)`로 reflow와 표시 데이터 생성을 원자적으로 수행
   - footer·Pipeline·inspector용 read-only summary
   - copy 가능한 현재 `Artifact`

`Viewport`는 `rows`와 `columns`만 가진다. Ratatui `Rect`는 Output seam을 넘지 않는다.
`present`의 mutation은 viewport와 offset 보정에만 사용하며 module 밖에서 관찰 가능한 중간
상태를 만들지 않는다.

presentation은 다음 중 하나다.

- 빈 화면
- Text window
- visible Hex rows
- visible Trace range와 실패 상세 위치
- Cancelled, Text unavailable 또는 Pipeline failure message

구체적인 Ratatui 타입과 style은 presentation에 포함하지 않는다.

## 6. 불변 조건

- Ready 상태는 항상 현재 `Artifact`를 가진다.
- Failed와 Cancelled 상태는 현재 `Artifact`를 가지지 않는다.
- final `Artifact`와 final traces는 하나의 snapshot으로 함께 저장·삭제한다.
- Final 성공만 final snapshot을 교체한다.
- 선택 단계 실행은 final snapshot을 바꾸지 않는다.
- Input 또는 Pipeline 변경은 final snapshot을 무효화하되 기존 현재 Output은 실행 전까지 유지한다.
- 새 Preview 결과와 View 변경은 navigation offset을 0으로 초기화한다.
- Text offset은 UTF-8 grapheme 경계에만 놓인다.
- Hex resize는 기존에 보이던 첫 byte를 가능한 한 유지한다.
- Trace는 작은 viewport에서 첫 실패가 보이는 기존 규칙을 유지한다.
- 표시 데이터는 기존 4 KiB budget과 `MAX_STEPS` 제한을 지킨다.
- 0×0 viewport와 navigation 끝은 정상적인 no-op이다.

## 7. 데이터 흐름

### Input 또는 Pipeline 변경

1. App이 request ID를 증가시킨다.
2. App이 Output에 변경 deadline을 전달한다.
3. Output은 final snapshot을 무효화하고 debouncing 상태가 된다.
4. App은 기존 request cancel `Effect`를 유지한다.

### Preview 실행과 완료

1. App이 deadline을 확인하고 Output을 running 상태로 전환한다.
2. App이 기존 Input·Pipeline으로 `PreviewJob`을 만든다.
3. App이 request ID가 현재 값인 결과만 Output에 전달한다.
4. Output이 source, snapshot, traces, 상태와 offset을 원자적으로 갱신한다.

### Render

1. `render.rs`가 기존 layout으로 Output 내부 영역을 계산한다.
2. `Output::present`가 같은 viewport로 reflow와 visible data를 만든다.
3. `render.rs`가 presentation을 그린다.
4. 다른 pane은 read-only summary만 사용한다.

## 8. 오류와 안전성

- Output module은 I/O를 하지 않으므로 새 recoverable error를 만들지 않는다.
- 오래된 Preview 결과 거부는 App의 기존 request ID 검사에 남긴다.
- final snapshot이 없을 때 복원 요청은 상태를 바꾸지 않고 기존 안내를 유지한다.
- request ID 고갈은 기존과 같이 invariant 위반으로 처리한다.
- error summary와 control escaping 규칙은 이번 범위에서 바꾸지 않는다.
- clipboard는 기존 `Artifact` 내용을 clone해 사용하며 Output 내부 offset에는 접근하지 않는다.

## 9. 테스트 전략

interface가 test surface다.

### Output interface 테스트

- Final 성공·선택 단계 성공·Final 복원
- Final과 선택 단계 실패·취소의 snapshot 규칙
- Input·Pipeline 변경 시 기존 현재 Output 유지와 final cache 무효화
- Smart·Text·Hex·Trace 판정
- View 변경 시 offset 초기화
- Unicode Text line·page·home·end navigation
- Hex resize 시 첫 visible byte 보존
- Trace failure, 작은 viewport와 page·end navigation
- 4 KiB 표시 budget과 0×0 viewport
- copy 가능 조건

동일한 동작을 새 interface에서 검증하면 기존 helper와 직접 필드 조작 테스트는 삭제한다.

### 유지할 테스트

- Ratatui 화면의 의미·제목·열·색상·작은 화면 테스트
- App key·mouse event에서 semantic navigation까지의 통합 테스트
- CLI 전체 테스트
- `tests/shell-smoke.sh`

## 10. 완료 기준

- `views.rs`가 없고 `output.rs`가 Output interface와 implementation을 소유한다.
- Output 상태 필드는 module 밖에서 직접 변경되지 않는다.
- `state.rs`에 View·viewport·offset 계산이 남지 않는다.
- `render.rs`가 effective View와 navigation 범위를 직접 계산하지 않는다.
- 사용자에게 보이는 기존 동작과 제한이 유지된다.
- `AGENTS.md`와 `CONTEXT.md`가 새 구조와 용어를 반영한다.
- fmt, Clippy, 전체 test, release build와 Shell smoke가 통과한다.
