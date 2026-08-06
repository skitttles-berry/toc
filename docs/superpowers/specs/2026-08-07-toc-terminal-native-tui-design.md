# toc 터미널 기본 테마·TUI 정돈 설계

**작성일:** 2026-08-07
**상태:** 사용자 승인·구현 완료
**대상:** `toc tui`
**기준 커밋:** `c4ef3b2`
**대체 범위:** 기존 TUI 설계의 색상, 현재 키 바인딩, Add Transform, Output 제목 계약

## 1. 목적

현재 TUI의 고정 Quiet Prism RGB 팔레트를 제거하고 사용자의 터미널 기본 전경·배경과
ANSI 팔레트를 따른다. 동시에 Pipeline과 Output의 키 역할을 분명히 하고, Add Transform
정보 구조와 Pipeline·Trace 열 정렬을 정돈한다.

현재 `Pipeline + Input + Output`, 비동기 복사, 변환 실행, Output View와 반응형 레이아웃은
유지한다. 범용 테마·키맵·알림 계층은 만들지 않고 기존 `state.rs`와 `render.rs` 경계에서
직접 수정한다.

## 2. 승인 결정

| 항목 | 결정 |
|---|---|
| App Bar | `>_ TOC` 유지 |
| 기본 테마 | 터미널 기본 전경·배경 사용 |
| 강조색 | 터미널 ANSI Cyan·Green·Yellow·Red |
| 선택·키 캡·커서 | 고정 배경 대신 반전·굵게 |
| Output 제목 | `[VIEW]`와 전체 바이트 크기, 위치 카운터 제거 |
| Pipeline 삭제 | `Backspace`만 지원 |
| Pipeline Add | `a`·`ㅁ` 제거, 전역 `Ctrl+p`·`Ctrl+ㅔ` 유지 |
| Step 실행 | Pipeline의 `s`·`ㄴ`으로 이동 |
| Add Transform 목록 | 표시 이름만 한 줄 |
| Add Transform 상세 | ID·설명·입력·동작·TUI 정책 표시 |
| Picker Backspace | 검색어 편집 유지, 하단 안내만 제거 |
| 삭제 알림 | 2초 또는 다음 조작까지 반전·굵게 표시 |
| Pipeline 크기 | 넓은 화면에서 오른쪽 정렬 |
| Trace Operation | 실제 표시 이름 길이에 맞춘 고정 열 폭 |

## 3. 범위

### 3.1 포함

- 고정 RGB 제거와 터미널 기본색·ANSI 역할색 적용
- `NO_COLOR` 포커스 구분 보완
- Pipeline·Output 키와 모든 Dock·Help·문서 동기화
- Add Transform 목록·상세·작은 화면·마우스 행 정돈
- 삭제 상태의 강조와 실제 PTY 만료 검증
- Pipeline 바이트 변화와 Trace 열 정렬
- Output 제목의 `[VIEW]` 표기와 위치 카운터 제거

### 3.2 제외

- 변환 엔진, 공개 CLI와 변환 ID 변경
- 비동기 복사 작업자와 위험 문자 확인 변경
- Output 64 MiB 한도와 4 KiB 렌더 예산 변경
- 사용자 설정형 테마·키맵과 설정 파일
- OSC 터미널 배경 감지
- 범용 상태·알림 자료형
- 스크롤바, 신규 View와 신규 의존성
- App Bar 아이콘 변경

## 4. 터미널 기본 테마

### 4.1 색상 역할

전체 프레임에 RGB 전경·배경을 덮지 않는다. 기본 본문과 배경은 Ratatui 기본 스타일로
두어 터미널의 기본 전경·배경을 그대로 사용한다. 의미 강조에만 ANSI 명명색을 사용한다.

| 역할 | 스타일 |
|---|---|
| 기본 본문·배경 | 터미널 기본값 |
| 포커스·Offset | ANSI Cyan |
| 성공 | ANSI Green |
| 처리·경고 | ANSI Yellow |
| 실패 | ANSI Red |
| 비활성·보조 | DIM 또는 터미널 기본값 |
| 선택 행·키 캡·입력 커서 | REVERSED + BOLD |
| 입력 선택 영역 | REVERSED |
| Modal Shadow | DIM |

터미널이 정의한 ANSI 팔레트가 실제 색조를 결정하므로 밝은·어두운 배경을 별도로 감지하지
않는다. 색상은 상태 문자열과 `✓`, `×`, `·`, `−` 같은 기존 문자 의미를 대체하지 않는다.

### 4.2 `NO_COLOR`

`NO_COLOR`에서는 ANSI 전경색을 사용하지 않는다. 반전·굵게·흐리게와 구조 기호는
유지한다. 특히 포커스된 패널 제목은 굵게 표시해 색상 없이도 현재 위치를 구분한다.

## 5. App Bar와 Output 제목

아이콘 비교에서 현재 표기가 선택되었으므로 App Bar는 다음과 같이 유지한다.

```text
>_ TOC
```

Output View는 대괄호로 표시한다. Ready 상태이고 Artifact가 있으면 위치가 아닌 전체
크기만 붙인다.

```text
» OUTPUT [HEX] · 100 B
» OUTPUT / STEP 02 [TRACE] · 100 B
```

`BYTE 현재/전체`와 `ROW 현재/전체`는 모든 View에서 제거한다. 너비가 부족하면 먼저
`· N B`를 생략한 뒤 기본 제목을 유지한다. Debouncing·Running·Failed·Cancelled에는
이전 결과가 보이더라도 크기를 현재 결과처럼 붙이지 않는다.

## 6. 키 바인딩

### 6.1 전역

| 도움말 표기 | 실제 입력 | 동작 |
|---|---|---|
| `Tab`, `Shift+Tab` | 동일 | 패널 이동 |
| `Ctrl+p` | `Ctrl+p`, `Ctrl+ㅔ` | Add Transform 열기 |
| `F1`, `?` | 기존 범위 | 도움말 |
| `Ctrl+q` | `Ctrl+q`, `Ctrl+ㅂ` | 종료 |
| `Ctrl+c` | 동일 | 강제 종료 |
| `Esc` | 동일 | Modal·Zoom 닫기 또는 요청 취소 |

### 6.2 Pipeline

| 도움말 표기 | 실제 입력 | 동작 |
|---|---|---|
| `↑`, `↓` | 동일 | 선택 이동 |
| `Shift+↑`, `Shift+↓` | 동일 | 단계 재정렬 |
| `Space` | 동일 | 단계 활성화 전환 |
| `Backspace` | 동일 | 선택 단계 삭제 |
| `Enter` | 동일 | Inspector 열기 |
| `s` | `s`, `ㄴ` | 선택 Step 실행 |
| `z` | `z`, `ㅋ` | Pipeline 확대 |

`Delete`, `d`, `ㅇ`, `a`, `ㅁ`은 Pipeline에서 아무 동작도 하지 않는다. Input에서는
일반 문자와 편집 키를 계속 `tui-textarea`에 전달한다.

### 6.3 Output

`p`·`ㅔ` Step 실행을 제거한다. Pretty·Raw Copy, `v` View, `f` Final, 방향키·페이지
이동, `z` Zoom은 유지한다. 선택 Step 실행은 Pipeline 포커스에서만 노출된다.

`s`·`ㄴ`은 기존 `request_selected_step`을 호출한다. 따라서 입력과 Pipeline의 스냅샷,
요청 ID, 최종 결과 캐시, 취소와 작업자 결과 폐기 규칙은 바뀌지 않는다. 선택 단계가
없으면 기존 `No pipeline step selected` 상태를 사용한다.

### 6.4 Modal 우선순위

Modal 입력은 패널 키보다 우선한다. Add Transform이 열려 있으면 Backspace는 Pipeline
삭제가 아니라 검색어 한 글자 삭제로 동작한다. 검색어가 비어 있으면 무동작이다.

Dock, 전체 Help, 작은 화면 Help, README와 PTY 시험은 실제 키와 동시에 갱신한다.
한글 자판 별칭은 기존 정책대로 동작만 지원하고 도움말에는 표시하지 않는다.

## 7. Add Transform

### 7.1 일반 화면

첫 번째 목록 영역은 한 항목당 한 줄을 사용하고 표시 이름만 보여준다.

```text
Search: base

> Base64 Encode
  Base64 Decode
```

선택 항목의 등록 정보는 두 번째 상세 영역에 모은다.

```text
ID        base64-encode
ABOUT     Encode bytes using padded RFC 4648 Base64
INPUT     Bytes accepted
BEHAVIOR  padded RFC 4648 Base64 with canonical = padding ...
TUI       Result remains bytes; Smart selects Text or Hex
```

목록에서 `[base64-encode]`와 부가 설명을 제거한다. ID와 `description`, `behavior`, 입력
정책과 TUI 결과 정책은 상세 영역에서 안전하게 절단·줄바꿈해 표시한다.

### 7.2 작은 화면

작은 화면에서도 목록은 이름만 한 줄로 표시한다. 상세 영역에 선택 항목의 `description`을
1~2줄 확보하고, 공간이 부족한 나머지 메타데이터만 생략한다. 따라서 목록 정돈 때문에
선택 항목의 의미가 완전히 사라지지 않는다.

### 7.3 검색과 조작 안내

문자 입력 필터링과 Backspace 검색어 편집은 유지한다. 하단 안내는 다음과 같이 줄인다.

```text
↑/↓ Select · [Enter Add] · [Esc Cancel]
```

`Backspace Search` 문자열만 제거한다. 목록이 두 행에서 한 행으로 바뀌므로 키보드 가시
범위, 선택 스크롤과 마우스 클릭 영역도 한 행 단위로 계산한다.

## 8. Footer 삭제 알림

삭제는 기존 `delete_selected` 경로를 재사용한다.

```text
Backspace
  → 선택 단계 제거·선택 인덱스 보정
  → Removed <display name> 상태 설정
  → 기존 changed 경로로 최종 미리보기 예약
  → 2초 또는 다음 사용자 조작에서 상태 해제
```

`Removed URL Encode` 같은 삭제 상태만 REVERSED + BOLD로 표시한다. 문자열은 기존
외부 텍스트 Escape 경계를 통과한다. 상태 수명, Footer 우선순위와 별도 타이머는
변경하지 않는다. 현재 단위시험에 더해 실제 Run Loop의 Tick으로 Dock이 복귀하는 PTY
회귀시험을 둔다.

## 9. Pipeline과 Trace 정렬

### 9.1 Pipeline 바이트 변화

넓은 화면에서 Pipeline 행의 왼쪽 상태·표시 이름과 오른쪽 크기 문자열을 분리한다.
`UnicodeWidthStr`로 현재 내부 너비에서 필요한 공백을 계산해 `3B→4B`를 내용 영역의
오른쪽 끝에 붙인다. 크기 정보가 없거나 좁은 화면이면 기존처럼 크기를 표시하지 않는다.

### 9.2 Trace Operation

Operation 폭은 남은 전체 화면 폭을 소비하지 않는다. 최대 32개 Trace의 안전한 표시
이름 폭과 `OPERATION` Header 폭 중 큰 값을 사용하고 현재 레이아웃이 허용하는 범위로
제한한다. Input·Output·Time·Status 또는 좁은 화면의 Size·Status 열은 Operation 바로
뒤에 배치한다.

전체 Trace를 기준으로 계산하므로 Page 이동 중 열 위치가 흔들리지 않는다. 기존 4 KiB
행 생성 예산, 실패 상세 영역과 상태별 스타일은 유지한다.

## 10. 코드 경계와 오류 처리

- `src/tui.rs`: 고정 RGB 상수를 터미널 기본값·ANSI 역할색으로 축소
- `src/tui/state.rs`: 실제 Pipeline·Output 키 분기만 변경
- `src/tui/render.rs`: Style, Dock·Help, Picker, 제목과 열 정렬 변경
- `src/tui/views.rs`: 기존 렌더 예산·페이지 계산 유지

빈 Pipeline의 Backspace는 무동작이다. 폭 계산은 포화 연산을 사용하고, 외부 문자열은
기존 Escape와 그래핌 단위 절단을 거친다. 변환·복사·작업자 오류는 현재 상태와 복구
경로를 유지한다.

## 11. 시험과 검증

### 11.1 상태·키

- Pipeline Backspace만 삭제하고 선택 위치를 보정함
- `Delete`·`d`·`ㅇ`·`a`·`ㅁ`이 무동작임
- Pipeline `s`·`ㄴ`이 같은 선택 Step 요청을 생성함
- Output `p`·`ㅔ`가 무동작임
- Input의 `a`, `s`, Backspace 편집 회귀 없음
- Modal Backspace가 검색어만 편집함

### 11.2 렌더링

- 기본 전경·배경을 강제하지 않고 ANSI 역할색만 사용함
- `NO_COLOR`가 ANSI 색상을 제거하면서 포커스 굵기와 구조를 유지함
- App Bar가 `>_ TOC`를 유지함
- Output 제목이 `[VIEW] · N B`를 사용하고 `BYTE`·`ROW`를 포함하지 않음
- Pipeline 크기가 오른쪽 끝에 정렬됨
- Trace 우측 열이 최장 Operation 바로 뒤에 정렬됨
- Add Transform 목록·상세·작은 화면·마우스 영역이 한 행 계약을 따름
- `Removed …`만 강조되고 2초 뒤 Dock으로 복귀함

### 11.3 전체 검증

- `cargo fmt --check`
- 경고 금지 잠금 Clippy
- 전체 잠금 시험과 rustdoc
- 잠금 패키징과 임시 경로 오프라인 설치
- Bash·Zsh PTY와 실제 실행 가능한 macOS Pasteboard 경로
- X11·Wayland는 실행 가능한 환경에서만 확인하고 미실행을 명시

2026-08-07 구현 트리에서 형식, 경고 금지 Clippy, rustdoc, 패키징과 오프라인 설치가
통과했다. 전체 시험은 라이브러리 288개 통과·3개 무시와 CLI 통합 15개 통과로 합계
303개 통과·3개 무시였고, 릴리스 측정 3종도 통과했다. Bash·Zsh 기본 PTY와 두 셸의
macOS Pasteboard 경로가 통과했으며 `pbpaste`는 `ff`를 반환했다. Darwin 환경에 X11·
Wayland 세션이 없어 해당 경로는 실행하지 않았다. 세부 측정값은 README의 최신 로컬
검증 요약에 기록했다.

## 12. 추가 권고와 보류

이번 범위에는 `NO_COLOR` 포커스 제목 굵기를 포함한다. 빈 Pipeline 본문의
`Ctrl+p Add transform` 한 줄 안내는 발견성을 높일 수 있으나, 현재 Footer가 같은 정보를
제공하므로 후속 후보로만 남긴다.

범용 키맵·테마 설정, 스크롤바, 신규 단축키, 상태 시스템, 외부 아이콘 폰트는 현재
변환 8개·Pipeline 32단계 규모에서 변경량만 늘리므로 추가하지 않는다.
