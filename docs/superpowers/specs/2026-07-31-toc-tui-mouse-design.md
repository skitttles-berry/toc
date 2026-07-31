# toc TUI 마우스 탐색 지원 설계

* **작성일:** 2026-07-31
* **상태:** 사용자 승인·구현 대기
* **기준 문서:** `docs/superpowers/specs/2026-07-31-toc-tui-workbench-design.md`
* **대상:** `toc tui`
* **제품 UI 언어:** 영어
* **프로젝트 문서 언어:** 한국어

---

# 1. 목적과 범위

이 설계는 현재 키보드 중심 TUI에 안전한 마우스 탐색을 추가한다. 사용자는
마우스로 패널 포커스를 바꾸고, Pipeline과 Add Transform 항목을 선택하고,
Output·Pipeline·Add Transform을 스크롤하고, Modal의 명시적 동작을 실행할 수
있다.

마우스는 기존 키보드 조작을 대체하지 않는다. 키보드 단축키, Input 편집,
Pipeline 실행, Output 원본과 View, 복사, 작업자, 반응형 배치와 보안 규약은
유지한다. 새 Cargo 의존성, 마우스 설정, 사용자 매핑과 별도 UI 프레임워크는
추가하지 않는다.

# 2. 조사 결과와 기술 선택

현재 의존성만으로 구현할 수 있다.

* Crossterm 0.29의 `EnableMouseCapture`와 `DisableMouseCapture`는 마우스 이벤트
  캡처를 시작하고 종료한다.
* `Event::Mouse(MouseEvent)`는 버튼 누름·해제·드래그·이동과 상하좌우 휠을
  제공한다.
* `MouseEvent`의 `column`과 `row`는 터미널 셀 좌표다.
* Ratatui 0.30.2의 `Rect`는 왼쪽 위 `(0, 0)` 기준 셀 좌표를 사용하고
  `Rect::contains(Position)`으로 영역 포함 여부를 판정한다.
* Ratatui는 렌더링 때 계산한 위치를 보관해 후속 hit testing에 사용하는
  방식을 예시로 제공한다.
* `tui-textarea-2`는 커서 이동·선택·스크롤 API를 제공하지만, 렌더된 셀 좌표를
  줄바꿈·그래핌·탭을 반영한 편집 커서 위치로 직접 변환하는 마우스 API는
  제공하지 않는다.

참고 자료:

* [Crossterm MouseEvent](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.MouseEvent.html)
* [Crossterm MouseEventKind](https://docs.rs/crossterm/0.29.0/crossterm/event/enum.MouseEventKind.html)
* [Crossterm event 모듈](https://docs.rs/crossterm/0.29.0/crossterm/event/index.html)
* [Ratatui Rect](https://docs.rs/ratatui/0.30.2/ratatui/layout/struct.Rect.html)
* [Ratatui hit testing 예시](https://ratatui.rs/examples/apps/advanced-widget-impl/)

## 2.1 검토한 접근법

| 접근법 | 장점 | 단점 | 결정 |
|---|---|---|---|
| 렌더된 실제 영역 저장 | 화면과 판정이 정확히 일치, 작은 변경 | 임시 좌표 상태 필요 | 채택 |
| 이벤트마다 레이아웃 재계산 | 저장 상태 없음 | 렌더 계산 중복과 불일치 위험 | 제외 |
| 좌표 조건 직접 작성 | 초기 코드가 짧음 | 반응형·Zoom·Modal 분기 중복 | 제외 |

# 3. 승인된 사용자 경험

| 항목 | 결정 |
|---|---|
| 마우스 활성화 | `toc tui` 실행 중 항상 활성화 |
| 클릭 버튼 | 수정 키 없는 왼쪽 단일 누름 |
| 패널 클릭 | 해당 패널 포커스 |
| Pipeline 항목 클릭 | Pipeline 포커스와 단계 선택 |
| Input·Output 내용 클릭 | 포커스만 변경 |
| Output 휠 | 포인터가 Output에 있을 때 3단위 스크롤 |
| Pipeline 휠 | 포인터가 Pipeline에 있을 때 단계 1개 이동 |
| Palette 항목 클릭 | 선택만 변경 |
| Palette 휠 | 선택 1개 이동 |
| Modal 동작 | 표시된 Add·Confirm·Cancel·Close만 클릭 실행 |
| Modal 바깥 클릭 | 무시 |
| 휠과 포커스 | 휠은 포커스를 변경하지 않음 |
| Hover | 없음 |
| 무시 이벤트 | 이동·드래그·해제·오른쪽·가운데 클릭·가로 휠 |

# 4. 구조와 데이터 흐름

## 4.1 터미널 세션

`TerminalSession`은 기존 터미널 상태와 함께 마우스 캡처 활성화 여부를
추적한다.

진입 순서는 다음과 같다.

1. Raw Mode
2. Alternate Screen
3. Bracketed Paste
4. Mouse Capture
5. Cursor Hide

복구는 정확히 역순이다.

1. Cursor Show
2. Mouse Capture Disable
3. Bracketed Paste Disable
4. Alternate Screen Leave
5. Raw Mode Disable

마우스 활성화 명령은 기존 `execute_tracked` 규약을 사용한다. 명령 출력이나
flush가 부분 실패하더라도 활성 상태를 먼저 기록해 `Drop`이 비활성화를
시도한다. 패닉용 `best_effort_restore_terminal`에도
`DisableMouseCapture`를 포함한다.

## 4.2 렌더 영역 스냅샷

`App`은 현재 프레임에 대응하는 작은 `MouseRegions`를 가진다. 이 값은 제품
데이터가 아니라 다음 입력을 위한 일시적인 화면 좌표다.

렌더 시작 시 이전 영역을 모두 지우고, 실제로 그린 영역만 다시 기록한다.

* Pipeline·Input·Output 외곽 영역
* Pipeline의 내용 영역, 첫 표시 단계와 표시 행 수
* Output 내용 영역
* Add Transform의 보이는 각 2행 항목 영역
* Add·Confirm·Cancel·Close 동작 영역

숨겨진 패널, 빈 목록 행, 생략된 Modal 상세와 Tiny 화면에서 렌더하지 않은
요소는 영역을 기록하지 않는다. 각 영역은 `Option<Rect>` 또는 보이는 항목에
대응하는 작은 목록만 사용한다. 새 모듈, 범용 위젯 트리와 이벤트 버블링 체계는
만들지 않는다.

## 4.3 이벤트 흐름

```text
Crossterm Event::Mouse
  → AppEvent::Mouse
  → Modal 영역 우선 판정
  → 패널·항목·휠 영역 판정
  → 기존 App 상태 변경 함수 재사용
  → 필요한 경우에만 dirty 표시
```

렌더 루프는 매 반복 시작에 `draw_if_dirty`를 호출한다. 따라서 Resize,
Modal 전환 또는 Zoom 변경 뒤 다음 마우스 이벤트를 읽기 전에 새 영역이
기록된다. 첫 렌더 전이거나 유효한 영역이 없으면 이벤트를 무시한다.

# 5. 클릭 동작

## 5.1 패널과 Pipeline

수정 키 없는 `MouseEventKind::Down(MouseButton::Left)`만 클릭으로 처리한다.

* Pipeline·Input·Output의 테두리와 내용 영역을 클릭하면 해당 패널에
  포커스를 둔다.
* Pipeline 내용의 실제 단계 행을 클릭하면 포커스를 Pipeline으로 바꾸고
  `selected_step`을 그 행의 단계로 바꾼다.
* Pipeline의 빈 행이나 테두리는 포커스만 바꾸고 선택은 유지한다.
* Input 클릭은 편집 커서를 이동하거나 선택 영역을 만들지 않는다.
* Output 클릭은 복사, View 변경과 원본 전환을 실행하지 않는다.
* App Bar와 두 줄 Footer 클릭은 무시한다.

Zoom에서는 화면에 표시된 한 패널만 영역을 가진다. 높이 10~11행의 포커스
전용 화면도 동일하다. 40열 미만 또는 10행 미만에서는 일반 패널 영역을
기록하지 않는다.

## 5.2 Modal

Modal이 열려 있으면 Modal 영역만 처리한다. 뒤쪽 패널과 Footer는 클릭하거나
스크롤할 수 없다. Modal 바깥 클릭은 닫기나 상태 변경을 일으키지 않는다.

Add Transform의 보이는 항목은 각각 2행 전체가 하나의 선택 영역이다. 항목
클릭은 선택만 변경하고 변환을 즉시 추가하지 않는다.

명시적인 동작 영역은 다음 문자열로 표시한다.

```text
[Enter Add] · [Esc Cancel]
[Enter/y Confirm] · [n/Esc Cancel]
[Esc Close]
```

* Add는 현재 선택한 변환을 기존 Enter 경로로 추가한다.
* Confirm은 Quit 또는 위험한 복사의 기존 승인 경로를 사용한다.
* Cancel은 현재 Modal의 기존 취소 경로를 사용한다.
* Close는 Help 또는 Step Inspector를 닫는다.

이중 클릭 시간 판정, 선택 항목 재클릭 실행과 Modal 바깥 클릭 닫기는 추가하지
않는다.

# 6. 휠 동작

수정 키 없는 `ScrollUp`과 `ScrollDown`만 처리한다. 포인터가 있는 영역에만
적용하며 포커스는 변경하지 않는다.

## 6.1 Output

Output 내용 영역의 한 번의 휠 이벤트는 기존 방향키 스크롤 단위를 세 번
적용한다. Text는 기존 UTF-8 경계, Hex와 Trace는 기존 행 경계와 최대값을
그대로 사용한다. Home·End, View와 Artifact는 변경하지 않는다.

## 6.2 Pipeline

Pipeline 내용 영역의 휠은 `selected_step`을 한 단계씩 이동한다. 목록이
비어 있으면 아무 동작도 하지 않고, 첫 단계와 마지막 단계에서 포화한다.
선택이 바뀌면 기존 렌더가 새 선택을 보이는 범위로 맞춘다.

## 6.3 Add Transform

Palette 목록 영역의 휠은 필터된 목록 선택을 한 항목씩 이동하고 양 끝에서
포화한다. 검색 문자열과 Pipeline은 변경하지 않는다.

Input, App Bar, Footer, 일반 Modal 본문과 패널 테두리 위의 휠은 무시한다.
`ScrollLeft`와 `ScrollRight`도 무시한다.

# 7. 무시 이벤트와 다시 그리기

다음 이벤트는 상태, 효과와 dirty 여부를 변경하지 않는다.

* `MouseEventKind::Moved`
* `MouseEventKind::Drag`
* `MouseEventKind::Up`
* 오른쪽·가운데 버튼 누름
* 수정 키가 포함된 클릭과 휠
* 가로 휠
* 기록된 영역 밖 이벤트

Hover 상태를 저장하지 않는다. 마우스 이동만으로 다시 그리거나 상태 메시지를
표시하지 않는다.

# 8. 오류·보안·호환성

* 마우스 캡처 활성화가 실패하면 안전한 TUI 오류로 종료하고 이미 활성화된
  터미널 상태를 복구한다.
* 좌표 판정은 `u16` 셀 좌표와 `Rect::contains`만 사용하며 입력 바이트,
  Output Artifact와 클립보드 내용을 참조하거나 기록하지 않는다.
* 클릭은 기존 상태 변경 함수를 호출하므로 입력·줄·단계·출력 한도와 취소
  규약을 우회하지 않는다.
* 확인 Modal의 payload 소유권과 승인·취소 규약을 유지한다.
* Crossterm 문서에 명시된 플랫폼별 버튼·수정 키 차이를 피하기 위해 수정 키
  없는 왼쪽 누름과 버튼 없는 휠 종류만 사용한다.
* 마우스를 보고하지 않는 터미널에서는 기존 키보드 기능을 그대로 사용할 수
  있다.
* Mouse Capture 동안 터미널의 일반 드래그 텍스트 선택이 제한될 수 있음을
  README에 알린다. 런타임 전환 키와 비활성화 설정은 제공하지 않는다.

# 9. 도움말과 문서

일반 크기의 F1 도움말에 패널별 마우스 동작을 추가한다.

* Input: `Mouse Click  Focus only`
* Pipeline: `Mouse Click  Focus/select · Wheel  Move selection`
* Output: `Mouse Click  Focus only · Wheel  Scroll`

작은 Help Modal은 기존 키·닫기 정보를 가리지 않도록 마우스 설명을 추가하지
않는다. Modal 하단의 대괄호 동작 문자열 자체가 클릭 가능 영역을 설명한다.
두 줄 Footer는 좁은 화면 길이와 기존 공통 키를 유지하기 위해 변경하지 않는다.

README에는 지원 동작, 마우스 캡처 기간과 일반 드래그 선택 제한을 기록한다.
기존 TUI 작업판 설계의 이벤트 흐름, 터미널 복구, 화면 조작과 시험 전략도
현행화한다.

# 10. 시험 전략

새 시험 프레임워크 없이 기존 Rust 단위 시험, Ratatui `TestBackend`와 PTY
Shell Smoke를 확장한다.

## 10.1 터미널 수명주기

* Mouse Capture 활성 상태가 추적되는지 검증한다.
* 정상 종료와 오류·패닉 복구에 Disable 명령이 포함되는지 검증한다.
* PTY에서 마우스 활성화 Sequence와 종료 비활성화 Sequence를 확인한다.
* 종료 뒤 기존 `stty`와 Alternate Screen·Paste·Cursor 복구 검사를 유지한다.

## 10.2 클릭 영역

실제 렌더 뒤 저장된 영역에 합성 `MouseEvent`를 전달한다.

* Wide·Medium·40~89열 세로 배치
* 10~11행 포커스 전용과 Zoom
* Tiny 화면과 확인 Modal
* 각 패널 테두리·내용·빈 행
* Pipeline의 첫·중간·마지막 보이는 단계
* 일반·축약 Add Transform의 2행 항목
* Add·Confirm·Cancel·Close와 Modal 바깥

## 10.3 휠과 경계

* Output의 위·아래 3단위 이동과 UTF-8·행 최대 경계
* Pipeline과 Palette의 한 항목 이동, 빈 목록과 양 끝 포화
* 휠이 포커스, View, Artifact와 검색 문자열을 바꾸지 않는지 확인
* Modal이 뒤쪽 패널 휠을 차단하는지 확인

## 10.4 무시 이벤트와 회귀

* Moved·Drag·Up·오른쪽·가운데·수정 키·가로 휠이 상태와 dirty를 바꾸지
  않는지 확인한다.
* PTY에서 실제 SGR 마우스 Sequence로 포커스, 선택, 휠과 Modal 동작을
  검증한다.
* 기존 키보드, 복사, Clipboard, Resize, 반응형 배치, `NO_COLOR`, 안전한
  제어 문자와 렌더 성능 시험을 유지한다.

# 11. 완료 기준

다음을 모두 충족하면 구현 완료로 판단한다.

1. 마우스 캡처가 TUI 수명주기와 함께 활성화·복구된다.
2. 패널 포커스와 Pipeline·Palette 선택이 실제 렌더 좌표와 일치한다.
3. Output·Pipeline·Palette 휠 동작과 경계가 승인 규약을 따른다.
4. Modal이 이벤트 우선권을 가지며 명시된 동작만 클릭으로 실행한다.
5. 무시 이벤트는 상태 변경과 다시 그리기를 만들지 않는다.
6. 키보드만으로 기존 전체 기능을 계속 사용할 수 있다.
7. 새 의존성 없이 형식·Clippy·전체 시험·Shell Smoke·패키징이 통과한다.
8. 정상 종료·오류·패닉 뒤 터미널 마우스 캡처가 남지 않는다.

# 12. 제외 범위

다음은 이번 구현에 포함하지 않는다.

* Input 커서 위치 클릭과 드래그 텍스트 선택
* Output 텍스트 선택과 마우스 복사
* Pipeline 토글·이동·삭제의 직접 클릭
* 항목 이중 클릭 실행
* Hover 강조와 포인터 모양 변경
* Footer 단축키 클릭
* 마우스 켜기·끄기 단축키, 환경 변수와 사용자 설정
* 터치 제스처, 관성 스크롤과 픽셀 단위 스크롤
* 범용 컴포넌트·이벤트 전파 프레임워크
