# toc asciinema 녹화 설계

## 목표

README에서 CLI와 TUI의 대표 사용 흐름을 각각 바로 확인할 수 있게 한다. 두 녹화는
짧고 독립적으로 재생되며, 현재 README의 CLI/TUI 균형을 유지한다.

## 산출물

- `docs/asciinema/toc-cli.cast`: CLI 단일 변환과 Pipeline 녹화
- `docs/asciinema/toc-tui.cast`: TUI 입력·변환 추가·실행·결과 확인 녹화
- `README.md`: 각 asciinema.org 녹화로 이동하는 SVG 미리보기 두 개

원본 asciicast는 재녹화와 로컬 재생을 위해 저장소에 보관한다. 공개 페이지는 검색
목록에 노출되지 않는 `unlisted`로 업로드한다.

## CLI 시나리오

100×22 터미널에서 다음 두 명령과 결과를 순서대로 보여준다.

1. `hello`를 `base64-encode`로 변환해 `aGVsbG8=` 확인
2. URL 인코딩된 JSON을 `url-decode --then format-json`으로 변환해 정리된 JSON 확인

명령은 README와 같은 `printf '%s'` 입력 방식을 사용한다. 결과 뒤에는 화면 구분을
위한 줄바꿈만 추가하며 `toc`가 출력에 줄바꿈을 붙이는 것처럼 보이지 않게 한다.

## TUI 시나리오

120×30 터미널에서 실제 `toc tui`를 열어 다음 흐름을 보여준다.

1. Input에 `hello` 입력
2. Picker에서 Base64 Encode와 SHA-256 추가
3. Pipeline의 선택 단계를 실행하고 최종 결과로 복귀
4. Output View를 전환해 Trace 확인 후 정상 종료

녹화는 실제 키 입력과 렌더링만 담고 설명용 오버레이나 별도 데모 코드는 추가하지
않는다.

## 안전성과 검증

- 키 입력 캡처 옵션은 사용하지 않고 통제된 예제 값만 입력한다.
- 현재 빌드한 release 실행 파일로 두 시나리오를 녹화한다.
- `asciinema play`로 두 원본을 재생하고 종료 상태, 터미널 크기, 주요 출력 문자열을
  확인한다.
- 업로드 후 두 페이지와 SVG 미리보기의 응답을 확인하고 README 링크를 검사한다.
- 코드 동작은 바꾸지 않으므로 기존 전체 시험 대신 release 빌드, 녹화 재생,
  `git diff --check`를 수행한다.

## 제외 범위

음성, 자막, GIF 변환, 별도 녹화 자동화 스크립트, 자체 플레이어 호스팅은 추가하지
않는다. README용 두 개의 짧은 재생 링크로 요구사항을 충족한다.
