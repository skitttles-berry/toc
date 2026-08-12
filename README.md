<div align="center">
  <h1>toc</h1>
  <p><strong>TUI Object Converter</strong></p>
  <p>텍스트와 바이트를 로컬에서 변환하고, 여러 작업을 Pipeline으로 연결합니다.</p>
  <p><code>CLI</code> · <code>TUI</code> · <code>Local-only</code> · <code>24 transforms</code></p>
</div>

## 30초 시작

저장소 루트에서 설치한 뒤, 단일 변환과 Pipeline을 실행하거나 TUI를 엽니다.

```bash
cargo install --locked --path .
printf '%s' 'hello' | toc base64-encode
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | toc url-decode --then format-json
toc tui
```

## TUI 실행 화면

[![toc TUI 실행 녹화](https://asciinema.org/a/tqmZAslTwfglLSfj.svg)](https://asciinema.org/a/tqmZAslTwfglLSfj)

## 자주 쓰는 방법

```bash
# 문자열 Base64 인코딩
printf '%s' 'hello' | toc base64-encode

# URL Decode 후 JSON 정리
printf '%s' '%7B%22name%22%3A%22toc%22%7D' \
  | toc url-decode --then format-json

# 파일의 JSON 정리
toc format-json --input input.json

# Binary Gzip 결과 저장
toc gzip-compress --input input.txt > output.gz
```

첫 번째 명령의 결과는 `aGVsbG8=`입니다. Pipeline은 다음처럼 두 칸 들여쓴 JSON을 출력합니다.

```json
{
  "name": "toc"
}
```

CLI에서는 표준 입력 또는 `--input PATH` 중 하나로 입력을 전달하며, 성공한 결과에는 임의의 끝 줄바꿈을 더하지 않습니다. Binary 결과는 실제 터미널에 직접 쓰지 말고 파일로 리디렉션합니다.

## TUI 사용법

TUI는 원본을 덮어쓰지 않습니다. Input에서 시작해 Pipeline으로 변환을 연결하고 Output에서 결과를 확인합니다.

### 화면 구성

다음은 세 패널의 관계를 보여주는 개념 예시입니다.

```text
>_ TOC

┌ PIPELINE ────────────────────┐  ┌ INPUT ──────────────────────┐
│ > 01 Base64 Encode    5→8 B │  │ hello                       │
│   02 SHA-256         8→64 B │  └─────────────────────────────┘
└─────────────────────────────┘  ┌ OUTPUT [SMART] [64 B] ──────┐
                                  │ 333d6b3a3c1f…               │
                                  └─────────────────────────────┘
```

터미널 크기에 따라 실제 배치는 달라질 수 있습니다.

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
| `SMART` | 결과 형식에 알맞은 View 자동 선택 |
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

## 지원 변환

| 기능군 | 변환 ID |
|---|---|
| 인코딩 | `base64-encode`, `base64-decode`, `base64url-encode`, `base64url-decode`, `base32-encode`, `base32-decode`, `url-encode`, `url-decode`, `hex-encode`, `hex-decode`, `html-encode`, `html-decode` |
| 데이터·텍스트 | `format-json`, `minify-json`, `rot13`, `sort-lines`, `remove-duplicate-lines` |
| 보안 분석 | `url-defang`, `url-refang`, `jwt-decode` |
| 해시·압축 | `sha256`, `sha512`, `gzip-compress`, `gzip-decompress` |

Base64URL 인코딩은 무패딩을 사용하고, `url-decode`는 `+`를 바꾸지 않습니다. `jwt-decode`는 서명을 검증하지 않으며, Gzip 압축은 결정적인 결과를 만듭니다.

## 안전 경계와 한도

민감한 값은 셸 인자로 넘기지 마십시오. 셸 기록과 프로세스 목록에 남을 수 있으므로 대화형 파이프나 권한을 제한한 `--input` 파일을 사용합니다.

```bash
IFS= read -r -s TOC_INPUT
printf '%s' "$TOC_INPUT" | toc base64-encode
unset TOC_INPUT
```

파이프와 리디렉션은 원시 바이트를 보존합니다. 실제 터미널은 비 UTF-8 또는 위험 제어 문자를 출력 전에 거부합니다. `jwt-decode`는 서명을 검증하지 않고, URL Defang은 보안 경계가 아닙니다.

| 실행 경로 | 입력 | 단계별 출력 |
|---|---:|---:|
| CLI | 64 MiB | 256 MiB |
| TUI | 1 MiB | 64 MiB |

Pipeline은 최대 32단계입니다.

## 문서

- [v0.2.1 제품 요구사항](docs/prd/v0.2.1-prd.md)
- [v0.2.1 설계](docs/superpowers/specs/2026-08-08-toc-0.2.1-design.md)

## 라이선스

[MIT](LICENSE-MIT) 또는 [Apache-2.0](LICENSE-APACHE) 중 하나를 선택할 수 있습니다.
