# toc README 개편 설계

**작성일:** 2026-08-10
**상태:** 사용자 승인
**대상:** `README.md`

## 1. 목표

README 첫 화면에서 `toc`의 용도와 실행 방법을 바로 이해할 수 있게 한다. 현재 문서의
정확한 CLI·TUI·보안 계약은 유지하되, 구현 세부사항과 과거 검증 기록을 덜어내고 관련
설계 문서와 E2E 보고서로 연결한다.

GitHub에서 별도 스타일시트나 외부 이미지 없이도 선명하게 보이는 터미널 중심 구성을
사용한다. 외부 배지, 생성 이미지, 실제 화면처럼 보이는 가짜 TUI 캡처는 넣지 않는다.

## 2. 독자와 성공 기준

주요 독자는 처음 저장소를 방문한 사용자와 로컬 변환 도구를 찾는 개발자다.

- 첫 화면에서 로컬 전용 CLI·TUI와 24개 변환을 파악할 수 있다.
- 설치부터 첫 변환까지 필요한 명령이 한 화면 안에 나온다.
- CLI 입력·Pipeline·이진 출력 안전 경계를 오해하지 않는다.
- TUI 핵심 키와 View를 README만 보고 사용할 수 있다.
- 현재 `toc 0.2.1`, 24개 ID, 시험 결과와 문서 링크가 실제 저장소와 일치한다.

## 3. 시각 원칙

- 중앙 정렬 Hero에는 제품명, 한 문장 설명, 네 가지 짧은 표식만 둔다.
- 표식은 `CLI`, `TUI`, `Local-only`, `24 transforms`이며 외부 배지를 사용하지 않는다.
- 색상이나 이모지에 의미를 맡기지 않는다. 제목, 코드, 표, 여백으로 위계를 만든다.
- 터미널 흐름은 `INPUT → PIPELINE → OUTPUT` 한 줄로 표현한다.
- 긴 표는 피하되 명령 ID와 고정 동작을 찾을 수 있어야 한다.
- 제목 단계는 `h1` 하나와 `h2` 중심으로 제한한다.

## 4. 정보 구조

### 4.1 Hero

- `toc`와 `TUI Object Converter`
- 로컬에서 텍스트와 바이트를 연결해 변환하고 단계별 결과를 살펴보는 도구라는 한 문장
- `CLI · TUI · Local-only · 24 transforms`
- `toc tui`, 기본 CLI 변환, `--then` Pipeline 세 명령

### 4.2 핵심 장점

세 항목만 사용한다.

1. 네트워크로 입력을 전송하지 않는 로컬 실행
2. 하나의 변환 레지스트리를 공유하는 CLI·TUI·Trace
3. 비 UTF-8과 위험 제어 문자를 구분하는 출력 안전 경계

### 4.3 설치와 Quick Start

- Rust 1.97.1과 `cargo install --locked --path .`
- 표준 입력, `--input`, `--then`, `toc tui` 대표 예시
- 성공 결과에 임의 줄바꿈을 붙이지 않는다는 짧은 주의

### 4.4 변환 목록

24행 상세 표 대신 다음 네 기능군 표를 사용한다.

- 인코딩: Base64, Base64URL, Base32, URL, Hex, HTML의 Encode·Decode
- 데이터·텍스트: JSON Prettify·Minify, ROT13, Sort Lines, Remove Duplicate Lines
- 보안 분석: URL Defang·Refang, JWT Decode
- 해시·압축: SHA-256·SHA-512, Gzip Compress·Decompress

표에는 실제 명령 ID를 모두 한 번씩 기록한다. Base64URL 무패딩, URL Decode의 `+`
보존, JWT 서명 미검증, 결정적 Gzip처럼 오해하기 쉬운 계약만 짧게 덧붙인다.

### 4.5 TUI

- `INPUT → PIPELINE → OUTPUT [SMART | TEXT | HEX | TRACE]` 흐름
- 전역, Pipeline, Output 세 묶음의 핵심 키 표
- 비파괴 결과, 단계 실행·최종 복원, View·Zoom, Pretty·Raw Copy 설명
- 터미널 테마 상속, 반응형 Layout, 키보드·마우스 지원은 한 문단으로 압축
- 상세 렌더링 예산, Debounce 시간, Trace 열 계산식은 설계 문서로 이동

### 4.6 안전과 한도

- 민감한 값을 셸 인자로 전달하지 않는 예시
- 실제 터미널의 비 UTF-8·위험 제어 문자 거부와 리디렉션 원시 바이트 보존
- CLI 64 MiB 입력·단계별 256 MiB 출력, TUI 1 MiB 입력·64 MiB 출력, 32단계 한도
- JWT Decode는 서명을 검증하지 않고 URL Defang은 보안 경계가 아님을 명시

### 4.7 검증·문서·라이선스

- 핵심 개발 명령 네 개만 본문에 둔다.
- 353개 통과·3개 일반 실행 무시, release 무시 시험 3개 별도 통과를 구분한다.
- `docs/test-reports/2026-08-09-e2e.md`와 v0.2.1 PRD·설계 문서로 연결한다.
- X11·Wayland 미검증 상태와 MIT 또는 Apache-2.0 라이선스를 기록한다.

## 5. 삭제·이동할 내용

- 버전별 과거 검증 로그와 세부 성능 측정값
- 클립보드 백업·복원 전체 셸 스크립트
- 렌더당 바이트 예산, Debounce 시간, Trace 열 너비 같은 내부 구현 설명
- Boop·CyberChef와 호환되지 않는다는 장문 설명
- 같은 동작을 여러 문단에서 반복하는 TUI 안내

해당 내용의 역사적 근거는 기존 PRD·설계 문서에서 보존한다. 현재 사용자가 알아야 할
호환성 경계만 README에 한두 문장으로 남긴다.

## 6. 검증

- `toc --version`, `toc --help`, `toc --list`와 README의 버전·명령을 대조한다.
- 공개 변환 ID 24개가 README에 정확히 한 번 이상 등장하는지 검사한다.
- 설치·Quick Start 명령을 실제 실행해 출력을 확인한다.
- README의 로컬 링크가 모두 존재하는지 검사한다.
- `git diff --check`와 Markdown 구조 검사를 실행한다.
- 문서 변경이므로 제품 코드는 수정하지 않는다.

## 7. 범위 밖

- 로고·스크린샷·GIF·외부 배지 제작
- 자동 완성, 패키지 배포, CI 추가
- CLI·TUI 동작이나 키 바인딩 변경
- PRD와 기존 역사 문서 재작성
