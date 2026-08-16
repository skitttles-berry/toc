# toc INPUT 네이티브 커서 이동 설계

**작성일:** 2026-08-17

**상태:** 사용자 승인·구현 완료

**대상:** `toc tui` INPUT 편집기

## 1. 목적

INPUT 편집기에서 터미널과 운영체제의 일반 입력란에 익숙한 수평 커서 이동을 지원한다.
macOS의 Command·Option 조합을 추가하면서 Linux와 Windows에서 쓰는 기존 Home·End,
Control 조합은 그대로 유지한다.

현재 의존성인 `tui-textarea-2`가 줄 시작·끝, 단어 이동, Shift 선택을 이미 제공하므로 새
편집기나 의존성을 추가하지 않는다. Crossterm 키 이벤트를 INPUT 경계에서 기존 편집기
키로 정규화하는 최소 변경으로 구현한다.

## 2. 확정 동작

### 2.1 키 바인딩

| 입력 | 동작 | 상태 |
|---|---|---|
| `←` / `→` | 문자 단위 이동 | 기존 유지 |
| `Home` / `End` | 현재 논리 줄의 시작 / 끝 | 기존 유지 |
| `Ctrl+A` / `Ctrl+E` | 현재 논리 줄의 시작 / 끝 | 기존 유지 |
| `Ctrl+←` / `Ctrl+→` | 이전 / 다음 단어 경계 | 기존 유지 |
| `Cmd+←` / `Cmd+→` | 현재 논리 줄의 시작 / 끝 | 신규 별칭 |
| `Option+←` / `Option+→` | 이전 / 다음 단어 경계 | 신규 별칭 |

`Cmd`는 Crossterm의 `SUPER`, `Option`은 `ALT`로 들어오는 이벤트를 뜻한다. 일부
터미널이 Option을 `META`로 보고할 수 있으므로 `META`도 같은 단어 이동 별칭으로 받는다.
각 조합에 `Shift`를 함께 누르면 이동 구간을 선택한다. `Shift` 없는 이동은 기존 선택을
해제하는 `tui-textarea-2` 동작을 따른다.

Home·End는 화면에서 줄바꿈된 시각 행이 아니라 입력 데이터의 `\n`으로 구분한 현재 논리
줄을 기준으로 한다. 설치된 `tui-textarea-2 0.12.1`의 `Head`와 `End` 동작도 같은 줄 안에서
멱등적으로 움직인다.

단어 이동은 `tui-textarea-2`의 기존 `WordBack`·`WordForward` 의미를 그대로 사용한다.
Unicode 공백은 구분자로, ASCII 문장 부호는 별도 구간으로, 밑줄과 나머지 Unicode 문자는
단어 구간으로 취급한다. 줄에 더 이동할 단어가 없으면 이전 줄의 끝이나 다음 줄의 시작으로
이동한다. 별도 Unicode 분절 의존성이나 운영체제별 단어 판정은 추가하지 않는다.

### 2.2 적용 범위

새 별칭은 포커스가 INPUT에 있을 때만 적용한다. OUTPUT, Pipeline, Picker, Help와 다른
Modal의 키 처리는 바꾸지 않는다. 전역 키와 Modal 키가 먼저 처리되는 현재 우선순위도
유지한다.

운영체제를 컴파일 시점에 분기하지 않는다. 같은 이벤트 정규화를 macOS, Linux, Windows와
로컬·SSH 세션에 적용한다. 따라서 Super·Alt 조합을 전송하는 터미널에서는 어느
운영체제에서도 별칭을 사용할 수 있고, 기존 Home·End와 Control 조합은 계속 동작한다.

## 3. 구현 구조

`src/tui/state.rs`의 INPUT 키 처리 직전에 작은 순수 정규화 함수를 둔다. 이벤트의 기본
modifier에서 선택용 `SHIFT`를 제외한 뒤 아래 조합만 정확히 변환한다.

- `SUPER+Left` / `SUPER+Right` → `Home` / `End`
- `ALT+Left` / `ALT+Right` → `Ctrl+Left` / `Ctrl+Right`
- `META+Left` / `META+Right` → `Ctrl+Left` / `Ctrl+Right`

변환된 이벤트에는 원래의 `SHIFT`, 키 종류와 키 상태를 보존한다. 정규화 대상에 Control,
Alt, Super, Meta가 추가로 섞인 조합은 원본 그대로 두어 기존 또는 터미널 고유 동작을
침범하지 않는다. 다른 키도 모두 원본 그대로 전달한다.

정규화 뒤에는 현재처럼 `TextArea::input`을 한 번 호출한다. 직접 커서 위치나 선택 범위를
계산하지 않으므로 기존 편집기 동작과 문자 인덱스 안전성을 재사용한다. 새 키맵 계층,
운영체제별 설정, 사용자 설정은 만들지 않는다.

현재 `handle_input_key`는 `TextArea::input`이 내용 변경을 보고할 때만 `changed()`를
호출한다. 커서 또는 선택만 바뀌면 화면만 dirty로 표시한다. 이 구분을 유지하여 커서 이동이
Pipeline 실행, debounce 예약, request ID 증가, OUTPUT 교체나 결과 cache 무효화를 일으키지
않게 한다.

## 4. 터미널 호환성

애플리케이션은 이미 Crossterm의 `DISAMBIGUATE_ESCAPE_CODES` keyboard enhancement를
요청한다. 터미널이 Command를 `SUPER`, Option을 `ALT` 또는 `META`로 전달하면 새 별칭을
처리한다.

다만 터미널 자체가 Command 조합을 탭 전환 같은 단축키로 먼저 소비하거나 enhancement
protocol을 지원하지 않으면 애플리케이션에는 이벤트가 도착하지 않는다. 이 경우 오류나
대체 추측을 만들지 않으며 Home·End, `Ctrl+A`·`Ctrl+E`, `Ctrl+←`·`Ctrl+→`를 호환
경로로 안내한다. 터미널 설정을 자동 변경하거나 Escape byte sequence를 직접 해석하지
않는다.

## 5. UI와 문서

- 하단 INPUT dock의 `Text editing` 표시는 밀도를 유지하기 위해 바꾸지 않는다.
- 전체 Input Help에는 macOS 별칭과 Linux·Windows 호환 키를 함께 표시한다.
- README 키 표에는 INPUT 전용 줄 시작·끝, 단어 이동, Shift 선택을 추가한다.
- README에는 터미널이 조합을 가로채면 이벤트가 전달되지 않을 수 있다는 짧은 주석을 둔다.

## 6. 시험

### 6.1 상태 단위 시험

- `SUPER+Left`·`SUPER+Right`가 여러 줄 입력의 현재 논리 줄 시작·끝으로 이동
- `ALT+Left`·`ALT+Right`와 `META+Left`·`META+Right`가 ASCII, 문장 부호, 공백,
  한글을 포함한 입력에서 기존 단어 경계로 이동
- 각 신규 조합에 `SHIFT`를 더하면 같은 이동 구간을 선택
- `Ctrl+Alt`, `Super+Alt`처럼 추가 modifier가 있는 조합과 다른 키는 원본 유지
- 기존 Home·End, `Ctrl+A`·`Ctrl+E`, `Ctrl+←`·`Ctrl+→` 회귀 없음
- INPUT 이외의 pane과 Modal에서는 신규 별칭 미적용
- 기존 `cursor_and_selection_only_edits_keep_preview_ownership_and_cache` 시험에 신규 별칭을
  포함하여 이동 뒤 request ID, OUTPUT, Pipeline 결과와 cache 상태 유지

### 6.2 렌더링과 통합 검증

- 전체 Input Help와 README에 확정 키 및 호환성 주석 표시
- 기존 INPUT 편집, 선택, 복사, Pipeline 갱신 시험 통과
- `cargo fmt --check`, 경고 금지 Clippy, 전체 잠금 시험, rustdoc 통과
- 기존 Bash와 Zsh shell smoke 통과(스크립트 변경 없음)

실제 Command 이벤트 전달은 터미널 설정과 protocol 지원에 의존하므로 자동 PTY 시험의
필수 조건으로 삼지 않는다. 지원 터미널에서 Command·Option 조합과 Shift 선택을 수동으로
확인한다.

## 7. 제외

- `Cmd+↑`·`Cmd+↓`, 문서 처음·끝 이동
- Option·Control 기반 단어 삭제
- 시각 행 기준 Home·End
- 운영체제별 단어 분절과 키 설정 화면
- 터미널 단축키 설정 변경 또는 미전달 키 복구
- 편집기 crate 교체와 신규 의존성
- 하단 dock의 단축키 목록 확장

## 8. 완료 기준

INPUT에서 전달받은 macOS식 수평 이동과 Shift 선택이 확정 동작대로 작동하고,
Linux·Windows식 기존 키가 회귀하지 않아야 한다. 이동만으로 변환 결과가 다시 계산되지
않고, Help와 README가 지원 범위와 터미널 제약을 정확히 설명하면 완료로 본다.

## 9. 구현 검증

2026-08-17에 Format, 경고 금지 Clippy, 전체 잠금 시험, rustdoc와 기존 Bash·Zsh shell
smoke를 통과했다.
