<div align="center">
  <h1>toc</h1>
  <p><strong>TUI Object Converter</strong></p>
  <p>텍스트와 바이트를 여러가지 포맷으로 변환하는 TUI·CLI 도구입니다.</p>
  <p><code>TUI</code> · <code>CLI</code> · <code>Local-only</code> · <code>24 transforms</code></p>
</div>

## 30초 만에 시작

저장소 루트에서 설치한 뒤 TUI를 열거나 CLI 명령을 실행하세요.

```bash
cargo install --locked --path .
printf '%s' 'hello' | toc base64-encode
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | toc url-decode --then format-json
toc tui
```

## TUI 사용법

TUI는 화면 상에서 입력 원문을 유지한 채 변환 단계를 만들고 결과를 확인할 수 있습니다. Input에 원문을 입력하고 Pipeline에 변환을 추가한 뒤 Output에서 결과를 확인하세요.

### 실행 화면

[![toc TUI 실행 녹화](https://asciinema.org/a/tqmZAslTwfglLSfj.svg)](https://asciinema.org/a/tqmZAslTwfglLSfj)

### 4단계로 시작

| 단계 | 작업 | 방법 |
|---:|---|---|
| 1 | 입력 | Input에 원문 작성 |
| 2 | 추가 | <kbd>Ctrl</kbd> + <kbd>p</kbd>로 변환 선택 |
| 3 | 실행 | <kbd>s</kbd>로 선택 단계 실행 |
| 4 | 확인 | Output에서 결과 확인 |

### Output View

| View | 용도 |
|---|---|
| `SMART` | 결과 형식에 맞는 View 자동 선택 |
| `TEXT` | UTF-8 텍스트 확인 |
| `HEX` | 바이트를 Offset·Hex·ASCII 열로 확인 |
| `TRACE` | Pipeline 단계별 상태와 안전한 실패 요약 확인 |

### 키 한눈에 보기

| 구역 | 키 | 동작 |
|---|---|---|
| 전역 | <kbd>Tab</kbd><br><kbd>Shift</kbd> + <kbd>Tab</kbd> | 패널 이동 |
|  | <kbd>Ctrl</kbd> + <kbd>p</kbd> | 변환 추가 |
|  | <kbd>F1</kbd> | 도움말 |
|  | <kbd>Ctrl</kbd> + <kbd>q</kbd> | 정상 종료 |
|  | <kbd>Esc</kbd> | 창·확대 닫기 또는 실행 취소 |
| Pipeline | <kbd>↑</kbd><br><kbd>↓</kbd> | 단계 선택 |
|  | <kbd>Shift</kbd> + <kbd>↑</kbd><br><kbd>Shift</kbd> + <kbd>↓</kbd> | 단계 이동 |
|  | <kbd>Space</kbd> | 단계 활성화 전환 |
|  | <kbd>Backspace</kbd> | 단계 삭제 |
|  | <kbd>Enter</kbd> | 단계 검사 |
|  | <kbd>s</kbd> | 선택 단계 실행 |
|  | <kbd>f</kbd> | 최종 결과 복원 |
|  | <kbd>z</kbd> | Pipeline 확대 |
| Output | <kbd>Enter</kbd> | Pretty Copy |
|  | <kbd>Shift</kbd> + <kbd>Enter</kbd> | Raw Copy |
|  | <kbd>v</kbd> | View 전환 |
|  | <kbd>z</kbd> | Output 확대 |

`Shift+Enter`를 구분하지 못하는 터미널에서는 Raw Copy가 제한될 수 있습니다.

## CLI 사용법

```console
# 문자열 Base64 인코딩
$ printf '%s' 'hello' | toc base64-encode
aGVsbG8=

# URL Decode 후 JSON 정리
$ printf '%s' '%7B%22name%22%3A%22toc%22%7D' \
  | toc url-decode --then format-json
{
  "name": "toc"
}

# 파일의 JSON 정리
$ toc format-json --input input.json

# Binary Gzip 결과 저장
$ toc gzip-compress --input input.txt > output.gz
```

- CLI는 표준 입력이나 `--input PATH`로 입력을 받습니다.
- 성공 결과 끝에는 줄바꿈을 임의로 붙이지 않습니다.
- Binary 결과는 터미널에 직접 쓰지 말고 파일로 리디렉션합니다.

## 지원 변환

| 기능군 | 변환 ID |
|---|---|
| 인코딩 | `base64-encode`,<br>`base64-decode`,<br>`base64url-encode`,<br>`base64url-decode`,<br>`base32-encode`,<br>`base32-decode`,<br>`url-encode`,<br>`url-decode`,<br>`hex-encode`,<br>`hex-decode`,<br>`html-encode`,<br>`html-decode` |
| 데이터·텍스트 | `format-json`,<br>`minify-json`,<br>`rot13`,<br>`sort-lines`,<br>`remove-duplicate-lines` |
| 보안 분석 | `url-defang`,<br>`url-refang`,<br>`jwt-decode` |
| 해시·압축 | `sha256`,<br>`sha512`,<br>`gzip-compress`,<br>`gzip-decompress` |

- Base64URL 인코딩은 패딩을 붙이지 않으며 `url-decode`는 `+`를 그대로 둡니다.
- `jwt-decode`는 서명을 검증하지 않습니다. Gzip 압축은 같은 입력에 항상 같은 결과를 만듭니다.

## 한도

| 실행 경로 | 입력 | 단계별 출력 |
|---|---:|---:|
| CLI | 64 MiB | 256 MiB |
| TUI | 1 MiB | 64 MiB |

한 Pipeline에는 변환을 최대 32단계까지 연결할 수 있습니다.

## 라이선스

[MIT](LICENSE)
