# toc INPUT 줄 시작 삭제 설계

**작성일:** 2026-08-17

**상태:** 사용자 승인·구현 완료

**대상:** `toc tui` INPUT 편집기

## 1. 목적

INPUT에서 macOS의 `Cmd+Backspace`처럼 선택 영역 또는 커서부터 현재 논리 줄 시작까지를
삭제한다. Windows와 Linux에서도 같은 기능에 접근할 수 있도록 `Ctrl+Backspace`를 함께
지원한다.

현재 의존성인 `tui-textarea-2 0.12.1`이 필요한 삭제, 다중 바이트 문자 처리, 삭제 기록과
실행 취소를 이미 제공한다. 새 편집기나 의존성, 운영체제별 구현 없이 INPUT 키 이벤트를
기존 편집기 동작으로 정규화한다.

## 2. 확정 동작

### 2.1 키 바인딩

| 입력 | 주 사용 환경 | 동작 |
|---|---|---|
| `Cmd+Backspace` (`SUPER+Backspace`) | macOS | 줄 시작 방향 삭제 |
| `Ctrl+Backspace` | Windows·Linux | 줄 시작 방향 삭제 |
| `Backspace` | 전체 | 기존 한 문자 삭제 유지 |
| `Alt+Backspace` | 전체 | 기존 이전 단어 삭제 유지 |

애플리케이션은 운영체제를 분기하지 않고 `SUPER+Backspace`와 `CONTROL+Backspace`를 어느
운영체제에서든 같은 별칭으로 받는다. `Shift`, `Alt`, `Meta` 등 다른 modifier가 추가로
섞인 조합은 매핑하지 않는다.

### 2.2 삭제 의미

- 선택 영역이 있으면 커서 위치와 관계없이 선택 영역만 삭제한다.
- 선택 영역이 없으면 커서부터 현재 논리 줄의 시작까지 삭제한다.
- 커서가 논리 줄 시작에 있으면 직전 줄바꿈을 삭제하여 이전 줄과 합친다.
- 문서의 첫 위치에서는 아무 내용도 바꾸지 않는다.
- 시각적으로 접힌 행이 아니라 입력 데이터의 `\n`으로 구분한 논리 줄을 기준으로 한다.
- 한 번의 실행 취소로 해당 삭제 전체를 복원한다.

새 별칭은 INPUT에 포커스가 있고 Modal이 열리지 않은 기존 INPUT 처리 경로에서만
적용한다. OUTPUT, Pipeline, Picker, Help와 다른 Modal의 키 동작은 바꾸지 않는다.

## 3. 구현 구조

`src/tui/state.rs`의 기존 INPUT 키 정규화 함수를 삭제까지 포괄하는 이름으로 일반화한다.
정규화 함수는 아래 두 정확한 조합만 편집기가 이미 이해하는 `Ctrl+J` 이벤트로 바꾼다.

- `SUPER+Backspace` → `CONTROL+Char('j')`
- `CONTROL+Backspace` → `CONTROL+Char('j')`

키 이벤트의 종류와 상태는 보존한다. 다른 키와 추가 modifier 조합은 원본 그대로
전달한다.

정규화 뒤에는 현재처럼 `TextArea::input`을 한 번 호출한다. `tui-textarea-2`의 기본
`Ctrl+J` 처리는 `delete_line_by_head()`를 실행하므로 다음 기존 기능을 그대로 재사용한다.

- 선택 영역 우선 삭제
- Unicode 문자 경계에 맞춘 삭제
- 줄 시작에서 직전 줄바꿈 삭제
- 삭제 내용 기록과 한 단계 실행 취소

직접 문자열을 자르거나 커서 위치를 계산하지 않는다. 별도 삭제 함수, 키맵 계층,
운영체제 조건부 컴파일과 신규 의존성도 추가하지 않는다.

## 4. 변경 전파

내용이 실제로 삭제되면 기존 `handle_input_key` 경로의 `changed()`가 호출된다. 따라서 일반
문자 삭제와 동일하게 debounce, request ID, 미리보기 재계산과 최종 출력 cache 무효화가
적용된다. 문서 첫 위치처럼 삭제 결과가 없으면 변경 상태나 재계산을 만들지 않는다.

## 5. 터미널 호환성

애플리케이션은 Crossterm의 keyboard enhancement를 요청하고, 전달받은 `SUPER`와
`CONTROL` 이벤트를 운영체제와 무관하게 처리한다. 합성 키 이벤트 시험으로 macOS,
Windows, Linux에서 공유하는 애플리케이션 경로를 검증한다.

터미널이나 운영체제가 조합을 먼저 소비하면 애플리케이션은 이를 복구할 수 없다. 또한
legacy 입력에서 `Ctrl+Backspace`가 일반 `Backspace`와 구분되지 않으면 한 문자 삭제로
처리될 수 있다. 도움말과 README에 수정키 전달을 지원하는 터미널이 필요하다고 명시하고,
터미널 설정 변경이나 escape sequence 직접 해석은 하지 않는다.

## 6. UI와 문서

- 전체 Input Help에 `Cmd+Backspace`와 `Ctrl+Backspace`를 추가한다.
- README INPUT 키 표에 운영체제별 주 사용 조합과 삭제 범위를 추가한다.
- README의 기존 터미널 수정키 전달 제한을 새 삭제 별칭에도 적용한다.
- 하단 INPUT dock의 `Text editing` 표시는 밀도를 유지하기 위해 바꾸지 않는다.

## 7. 시험

### 7.1 상태 단위 시험

- `SUPER+Backspace`와 `CONTROL+Backspace`가 같은 내부 편집 동작으로 정규화
- 한글과 이모지가 포함된 여러 줄 입력에서 커서부터 논리 줄 시작까지만 삭제
- 선택 영역이 있으면 선택 영역만 우선 삭제
- 논리 줄 시작에서는 이전 줄과 병합하고 문서 첫 위치에서는 변경 없음
- 삭제 한 번을 실행 취소 한 번으로 복원
- 추가 modifier 조합은 매핑하지 않음
- 일반 `Backspace`와 `ALT+Backspace` 동작 유지
- INPUT 이외의 pane과 Modal에서는 신규 별칭 미적용
- 실제 삭제 시 기존 문서 변경·cache 무효화 계약 유지

### 7.2 문서와 전체 검증

- 전체 Input Help 렌더링에 두 별칭과 동작 표시
- README 키 표와 터미널 제약이 구현과 일치
- Format, 경고 금지 Clippy, 전체 잠금 시험과 rustdoc 통과
- 기존 Bash와 Zsh shell smoke 통과

실제 수정키 전달은 터미널 설정과 protocol 지원에 의존하므로 자동 PTY 시험의 필수
조건으로 삼지 않는다. 지원 터미널에서 macOS의 Command 조합과 Windows·Linux의 Control
조합을 수동 확인 대상으로 남긴다.

## 8. 검토한 대안

### `SUPER+Backspace`만 지원

코드는 가장 작지만 Windows와 Linux에서 Super 키를 운영체제나 터미널이 소비할 가능성이
커 실용성이 낮아 제외했다.

### 운영체제별 조건부 바인딩

macOS 빌드에는 `SUPER`, Windows·Linux 빌드에는 `CONTROL`만 허용할 수 있으나 터미널이
보내는 이벤트 계약은 운영체제보다 터미널 기능에 좌우된다. 동일 동작을 위한 분기와 시험만
늘어나므로 제외했다.

### 직접 삭제 구현

INPUT 처리기에서 문자열과 커서를 직접 수정하면 기존 편집기의 Unicode 안전성, 선택 처리,
삭제 기록과 실행 취소를 중복 구현해야 하므로 제외했다.

## 9. 제외

- 현재 줄 전체와 줄바꿈을 일괄 삭제하는 별도 명령
- 커서부터 줄 끝까지 삭제하는 신규 별칭
- 사용자 정의 키 설정과 운영체제 자동 감지
- 터미널 단축키 설정 변경 또는 미전달 키 복구
- 편집기 crate 교체와 신규 의존성
- 하단 dock의 단축키 목록 확장

## 10. 완료 기준

INPUT에서 두 운영체제별 별칭이 승인된 줄 시작 삭제를 수행하고, 선택·Unicode·줄 병합·실행
취소·변경 전파가 기존 편집기 계약을 유지해야 한다. 일반·단어 단위 Backspace와 INPUT
외부 키 동작이 회귀하지 않고 Help와 README가 지원 범위와 터미널 제약을 정확히 설명하면
완료로 본다.

## 11. 구현 검증

2026-08-17에 Format, 경고 금지 Clippy, 전체 잠금 시험, rustdoc와 기존 Bash·Zsh shell
smoke를 통과했다.
