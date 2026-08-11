# toc README TUI 사용법 고도화 설계

**작성일:** 2026-08-11
**상태:** 사용자 승인
**대상:** `README.md`의 `TUI 사용법` 섹션

## 1. 목표

README의 TUI 사용법을 처음 보는 사람도 화면 구조와 조작 순서를 빠르게 파악할 수 있게
바꾼다. 실제 TUI를 닮은 Terminal Blueprint, 단계별 시작 흐름, Output View 설명,
키캡 표를 차례로 배치한다.

제품 코드와 TUI 동작은 변경하지 않는다. README에서 사용자가 별도로 수정한 TUI 밖의
내용도 건드리지 않는다.

## 2. 시각 원칙

- 외부 이미지, GIF, 배지, 스타일시트를 추가하지 않는다.
- GitHub에서 그대로 렌더링되는 Markdown, `text` 코드 블록, `<kbd>`만 사용한다.
- 실제 화면 캡처로 오해하지 않도록 Terminal Blueprint를 개념 예시라고 설명한다.
- 키나 View 이름을 쉼표로 길게 이어 쓰지 않는다.
- 단계, View, 단축키는 항목당 한 행으로 표시한다.
- 한 문장에는 한 가지 동작만 설명한다.

## 3. 섹션 구조

`TUI 사용법`은 다음 순서로 구성한다.

1. 짧은 소개
2. 화면 구성
3. 4단계로 시작
4. Output View
5. 키 한눈에 보기
6. Raw Copy 호환성 안내

## 4. 화면 구성

다음 데이터 흐름을 Terminal Blueprint로 보여준다.

- Input: `hello`, 5 B
- Pipeline 1: Base64 Encode, 5 B에서 8 B
- Pipeline 2: SHA-256, 8 B에서 64 B
- Output View: `SMART`
- Output 크기: 64 B
- Output 시작값: `333d6b3a3c1f…`

`333d6b3a3c1f…`는 `aGVsbG8=`의 실제 SHA-256 결과
`333d6b3a3c1f5db6c9bdda5939b136986d170f4649172a68368d54ecb44c2ff2`를 줄인
표기다.

개념도는 `>_ TOC`, Pipeline, Input, `OUTPUT [SMART] [64 B]`의 관계를 보여준다.
터미널 크기에 따라 실제 배치는 달라질 수 있다는 문장을 코드 블록 아래에 둔다.

## 5. 4단계로 시작

각 단계는 한 행에 하나씩 기록한다.

| 단계 | 작업 | 방법 |
|---:|---|---|
| 1 | 입력 | Input에 원문 작성 |
| 2 | 추가 | `<kbd>Ctrl</kbd>`+`<kbd>p</kbd>`로 변환 선택 |
| 3 | 실행 | `<kbd>s</kbd>`로 선택 단계 실행 |
| 4 | 확인 | Output에서 결과 확인 |

## 6. Output View

View는 한 행에 하나씩 설명한다.

| View | 용도 |
|---|---|
| `SMART` | 결과 형식에 알맞은 View 자동 선택 |
| `TEXT` | UTF-8 텍스트 확인 |
| `HEX` | 바이트를 Offset·Hex·ASCII 열로 확인 |
| `TRACE` | Pipeline 단계별 상태와 안전한 실패 요약 확인 |

## 7. 키 한눈에 보기

키는 `구역 | 키 | 동작` 형식의 세 열 표로 만든다. 하나의 행에는 하나의 동작만 둔다.
서로 반대 방향인 키 조합은 같은 행에 함께 표시할 수 있다.

### 전역

- `<kbd>Tab</kbd>` / `<kbd>Shift</kbd>`+`<kbd>Tab</kbd>`: 패널 이동
- `<kbd>Ctrl</kbd>`+`<kbd>p</kbd>`: 변환 추가
- `<kbd>F1</kbd>`: 도움말
- `<kbd>Ctrl</kbd>`+`<kbd>q</kbd>`: 정상 종료
- `<kbd>Esc</kbd>`: 창·확대 닫기 또는 실행 취소

### Pipeline

- `<kbd>↑</kbd>` / `<kbd>↓</kbd>`: 단계 선택
- `<kbd>Shift</kbd>`+`<kbd>↑</kbd>` / `<kbd>Shift</kbd>`+`<kbd>↓</kbd>`: 단계 이동
- `<kbd>Space</kbd>`: 단계 활성화 전환
- `<kbd>Backspace</kbd>`: 단계 삭제
- `<kbd>Enter</kbd>`: 단계 검사
- `<kbd>s</kbd>`: 선택 단계 실행
- `<kbd>f</kbd>`: 최종 결과 복원
- `<kbd>z</kbd>`: Pipeline 확대

### Output

- `<kbd>Enter</kbd>`: Pretty Copy
- `<kbd>Shift</kbd>`+`<kbd>Enter</kbd>`: Raw Copy
- `<kbd>v</kbd>`: View 전환
- `<kbd>z</kbd>`: Output 확대

실제 README에서는 위 항목을 하나의 표로 합치되, 같은 구역 이름을 각 행에 반복하지
않도록 첫 행 이후에는 빈 셀을 사용한다. 키는 `<kbd>`로 표현하고 슬래시 또는 `<br>`로
시각적으로 분리한다.

`Shift+Enter`를 구분하지 못하는 터미널에서는 Raw Copy가 제한될 수 있다는 기존 안내를
표 아래에 유지한다.

## 8. 변경 경계

- `README.md`의 `TUI 사용법` 섹션만 수정한다.
- 현재 미커밋 변경에서 제거된 안전 경계 설명과 문서 링크를 복원하지 않는다.
- 현재 미커밋 변경에서 Apache-2.0 라이선스 링크를 복원하지 않는다.
- 변환 목록, 한도, MIT 라이선스 문구를 변경하지 않는다.
- 제품 코드, 키 바인딩, TUI 화면을 변경하지 않는다.

## 9. 검증

- `printf '%s' 'hello' | toc base64-encode --then sha256` 결과가 Blueprint의 전체
  SHA-256 값과 일치하는지 확인한다.
- README의 키와 View 설명을 현재 TUI 구현과 시험 이름으로 대조한다.
- 각 단계, View, 단축키가 한 행씩 표시되는지 확인한다.
- 외부 이미지와 E2E 보고서 링크가 추가되지 않았는지 확인한다.
- TUI 밖의 기존 미커밋 diff가 그대로 유지되는지 비교한다.
- `git diff --check`, Format, 경고 금지 Clippy, 전체 시험, rustdoc를 실행한다.

## 10. 범위 밖

- 실제 TUI 화면·키·색상 변경
- 스크린샷·GIF·외부 배지 추가
- README의 TUI 밖 섹션 재작성
- 안전 경계·문서·Apache-2.0 내용 복원
