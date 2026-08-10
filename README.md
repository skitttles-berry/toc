<div align="center">
  <h1>toc</h1>
  <p><strong>TUI Object Converter</strong></p>
  <p>텍스트와 바이트를 로컬에서 연결해 변환하고, 단계별 결과를 살펴봅니다.</p>
  <p><code>CLI</code> · <code>TUI</code> · <code>Local-only</code> · <code>24 transforms</code></p>
</div>

```bash
toc tui
printf '%s' 'hello' | toc base64-encode
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | toc url-decode --then format-json
```

## 왜 toc인가

- 입력과 변환 결과를 네트워크로 전송하지 않는 로컬 도구입니다.
- CLI, TUI, Trace가 하나의 변환 레지스트리를 공유합니다.
- 비 UTF-8과 위험 제어 문자를 구분해 출력 경계를 지킵니다.

## 설치

Rust 1.97.1 환경의 저장소 루트에서 실행합니다.

```bash
cargo install --locked --path .
```

## Quick Start

표준 입력 또는 `--input PATH` 중 하나로 입력을 전달합니다. 성공한 결과에는 임의의 끝 줄바꿈을 더하지 않습니다.

```bash
printf '%s' 'hello' | toc base64-encode
toc base64-encode --input input.txt
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | toc url-decode --then format-json
toc tui
```

`--then`으로 변환을 최대 32단계까지 연결할 수 있습니다. 공개 변환 목록은 `toc --list`, 명령별 사용법은 `toc <transform-id> --help`에서 확인합니다.

## 변환

| 기능군 | 변환 ID |
|---|---|
| 인코딩 | `base64-encode`, `base64-decode`, `base64url-encode`, `base64url-decode`, `base32-encode`, `base32-decode`, `url-encode`, `url-decode`, `hex-encode`, `hex-decode`, `html-encode`, `html-decode` |
| 데이터·텍스트 | `format-json`, `minify-json`, `rot13`, `sort-lines`, `remove-duplicate-lines` |
| 보안 분석 | `url-defang`, `url-refang`, `jwt-decode` |
| 해시·압축 | `sha256`, `sha512`, `gzip-compress`, `gzip-decompress` |

Base64URL 인코딩은 무패딩을 사용하고, `url-decode`는 `+`를 바꾸지 않습니다. `jwt-decode`는 서명을 검증하지 않으며, Gzip 압축은 결정적인 결과를 만듭니다.

## TUI

```text
INPUT → PIPELINE → OUTPUT [SMART | TEXT | HEX | TRACE]
```

TUI는 원본을 덮어쓰지 않습니다. Pipeline에서 선택 단계를 실행하거나 보관된 최종 결과를 복원할 수 있고, Output에서는 보기 전환과 복사를 제공합니다.

| 구역 | 핵심 키 |
|---|---|
| 전역 | `Tab`/`Shift+Tab` 패널 이동, `Ctrl+p` 변환 추가, `F1` 도움말, `Ctrl+q` 정상 종료, `Esc` 창·확대 닫기 또는 실행 취소 |
| Pipeline | `↑`/`↓` 선택, `Shift+↑`/`Shift+↓` 이동, `Space` 전환, `Backspace` 삭제, `Enter` 검사, `s` 선택 단계 실행, `f` 최종 결과 복원, `z` 확대 |
| Output | `Enter` Pretty Copy, `Shift+Enter` Raw Copy, `v` 보기 전환, `z` 확대 |

터미널 테마를 따르고, 창 크기에 맞춰 배치를 조정합니다. 키보드와 마우스로 조작할 수 있습니다.

## 안전 경계

민감한 값은 셸 인자로 넘기지 마십시오. 셸 기록과 프로세스 목록에 남을 수 있으므로 대화형 파이프나 권한을 제한한 `--input` 파일을 사용합니다.

```bash
IFS= read -r -s TOC_INPUT
printf '%s' "$TOC_INPUT" | toc base64-encode
unset TOC_INPUT
```

파이프와 리디렉션은 원시 바이트를 보존합니다. 반면 실제 터미널은 비 UTF-8 또는 위험 제어 문자를 출력 전에 거부합니다. `jwt-decode`는 서명을 검증하지 않고, URL Defang은 보안 경계가 아닙니다.

## 한도

| 실행 경로 | 입력 | 단계별 출력 |
|---|---:|---:|
| CLI | 64 MiB | 256 MiB |
| TUI | 1 MiB | 64 MiB |

Pipeline은 최대 32단계입니다.

## 검증

다음 명령으로 개발 환경을 확인합니다.

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features
```

이번 README 갱신에서 일반 시험 353개 통과·3개 무시, release ignored 시험 3개 통과를 확인했습니다. X11·Wayland 실행 세션은 확인하지 않았습니다.

## 문서

- [v0.2.1 제품 요구사항](docs/prd/v0.2.1-prd.md)
- [v0.2.1 설계](docs/superpowers/specs/2026-08-08-toc-0.2.1-design.md)

## 라이선스

[MIT](LICENSE-MIT) 또는 [Apache-2.0](LICENSE-APACHE) 중 하나를 선택할 수 있습니다.
