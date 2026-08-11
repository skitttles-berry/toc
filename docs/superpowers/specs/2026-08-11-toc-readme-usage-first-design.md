# toc README 사용 안내 중심 개편 설계

**작성일:** 2026-08-11
**상태:** 사용자 승인
**대상:** `README.md`

## 1. 목표

README를 기능 명세와 검증 기록 중심 문서에서 소개와 사용법 중심 문서로 바꾼다. 처음
접한 사용자가 `toc`의 용도, 설치 방법, CLI와 TUI의 기본 흐름을 차례로 읽고 바로
실행할 수 있어야 한다.

현재 `toc 0.2.1`의 명령, 변환 ID, 키, 한도, 보안 경계는 그대로 유지한다. 제품 코드,
CLI·TUI 동작, 의존성은 변경하지 않는다.

## 2. 정보 구조

README는 다음 순서를 사용한다.

1. 소개
2. 30초 시작
3. 자주 쓰는 방법
4. TUI 사용법
5. 지원 변환
6. 안전 경계와 한도
7. 문서와 라이선스

개발 검증 명령과 시험 수는 본문에서 제거한다. 상세 구현 설명은 기존 PRD와 설계
문서로 연결한다. E2E 보고서와 해당 링크는 포함하지 않는다.

## 3. 소개와 시작 흐름

첫 화면에는 제품명, `TUI Object Converter`, 로컬에서 텍스트와 바이트를 연결해
변환하는 도구라는 짧은 소개를 둔다. 외부 배지·이미지·GIF·가짜 TUI 캡처는 사용하지
않는다.

`30초 시작`은 다음 네 흐름을 한 화면 안에서 보여준다.

- `cargo install --locked --path .` 설치
- 표준 입력을 사용한 단일 변환
- `--then`을 사용한 Pipeline
- `toc tui` 실행

성공 결과에 임의의 끝 줄바꿈을 붙이지 않는다는 계약은 예제 가까이에 한 번만
설명한다.

## 4. 자주 쓰는 방법

각 예제는 `목적 → 명령 → 결과 또는 결과 위치` 순서로 읽히게 한다.

- 문자열 Base64 인코딩
- URL Decode 후 JSON 정리
- `--input`을 사용한 파일 변환
- Binary 결과를 리디렉션으로 파일에 저장
- TUI에서 Input·Pipeline·Output을 사용하는 흐름

셸 입력은 `printf '%s'`를 사용한다. Binary 결과는 실제 터미널에 직접 쓰는 예제로
안내하지 않고 파일로 리디렉션한다. 명령은 현재 바이너리로 실행 가능한 형태만 싣는다.

## 5. TUI 사용법

TUI는 `INPUT → PIPELINE → OUTPUT [SMART | TEXT | HEX | TRACE]` 흐름으로 소개한다.
설명은 원본을 덮어쓰지 않는 비파괴 동작, 변환 추가와 단계 실행, 최종 결과 복원,
View·Zoom·Copy에 집중한다.

키 표는 전역, Pipeline, Output 세 행으로 유지한다. 한글 별칭과 내부 렌더링 동작은
본문에서 설명하지 않는다. `Shift+Enter`를 구분하지 못하는 터미널에서는 Raw Copy가
제한될 수 있다는 호환성 안내는 유지한다.

## 6. 지원 변환

공개 변환 ID 24개를 다음 네 기능군으로 묶는다.

- 인코딩: Base64, Base64URL, Base32, URL, Hex, HTML Encode·Decode
- 데이터·텍스트: JSON Prettify·Minify, ROT13, Sort Lines, Remove Duplicate Lines
- 보안 분석: URL Defang·Refang, JWT Decode
- 해시·압축: SHA-256, SHA-512, Gzip Compress·Decompress

모든 실제 ID를 README에 한 번 이상 기록한다. Base64URL 무패딩, URL Decode의 `+`
보존, JWT 서명 미검증, 결정적 Gzip처럼 오해하기 쉬운 계약만 짧게 덧붙인다.

## 7. 안전 경계와 한도

다음 내용은 사용자가 잘못된 사용법을 선택하지 않을 만큼만 설명한다.

- 민감한 값을 셸 인자로 전달하지 않음
- 파이프·리디렉션의 원시 바이트 보존
- 실제 터미널의 비 UTF-8·위험 제어 문자 거부
- JWT Decode의 서명 미검증
- URL Defang은 보안 경계가 아님
- CLI 64 MiB 입력·단계별 256 MiB 출력
- TUI 1 MiB 입력·64 MiB 출력
- Pipeline 최대 32단계

## 8. 문체와 표현

- 한 문장에는 한 가지 정보만 담는다.
- 명령을 보여주기 전에 사용 목적을 설명한다.
- 번역투, 광고성 표현, 반복 문장을 제거한다.
- CLI, TUI, Pipeline, Trace, View 같은 제품 용어는 원형을 유지한다.
- 명령, 변환 ID, 키, 수치, 링크, 보안 경계는 윤문 중 바꾸지 않는다.
- 과도한 강조, 이모지, 영어 병기를 사용하지 않는다.

완성 문장은 `humanize-korean`의 보수적 규칙으로 점검한다. 윤문용 임시 산출물과 요약
주석은 README에 포함하지 않는다.

## 9. 검증

- `toc --version`, `toc --help`, 변환별 `--help`, `toc --list`를 README와 대조한다.
- 공개 변환 ID 24개가 README에 모두 있는지 검사한다.
- 단일 변환, Pipeline, 파일 입력, Binary 리디렉션 예제를 실제 실행한다.
- PRD·설계·라이선스 링크가 존재하는지 검사한다.
- E2E 보고서 링크와 외부 이미지가 없는지 검사한다.
- `git diff --check`, Format, 경고 금지 Clippy, 전체 시험, rustdoc를 실행한다.

## 10. 범위 밖

- 제품 기능, CLI 문법, TUI 키 변경
- 변환 추가·삭제
- 외부 배지·이미지·스크린샷 제작
- E2E 보고서 포함
- 개발자용 상세 검증 기록을 README에 재수록
