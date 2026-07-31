# doop

`doop`은 로컬에서 동작하는 텍스트 변환 CLI이자 비파괴 TUI 작업판입니다.
입력과 변환 결과를 네트워크로 전송하지 않습니다.

## 설치

저장소가 고정한 Rust 1.97.1 도구 체인에서 다음 명령을 실행합니다.

```bash
cargo install --locked --path .
```

## CLI

```bash
printf 'hello' | doop base64-encode
doop base64-encode --input input.txt
printf '%s' '%7B%22a%22%3A1%7D' | doop url-decode --then format-json
printf '%s' '48 65 6c 6c 6f' | doop hex-decode
printf 'hello' | doop hex-encode --then hex-decode
doop --list
doop --help
```

변환 명령은 `base64-encode`, `base64-decode`, `url-encode`,
`url-decode`, `format-json`, `minify-json`, `hex-encode`,
`hex-decode`입니다. `run`이나 `transform` 상위 명령 없이 변환 명령을
직접 실행합니다. 파이프 입력과 `--input PATH` 중 정확히 하나만 사용해야
합니다. 성공 결과에는 임의의 끝 줄바꿈을 추가하지 않습니다.

민감한 입력은 셸 인자에 직접 넣지 마십시오. 인자는 셸 기록과 프로세스
목록에 남을 수 있습니다. 기록에 값이 남지 않는 대화형 파이프를 사용하거나,
처음부터 접근 권한을 제한해 만든 파일을 `--input`으로 전달합니다.

```bash
IFS= read -r -s DOOP_INPUT
printf '%s' "$DOOP_INPUT" | doop base64-encode
unset DOOP_INPUT

(
  secret_file=$(mktemp "${TMPDIR:-/tmp}/doop-secret.XXXXXX")
  trap 'rm -f -- "$secret_file"' EXIT
  chmod 600 "$secret_file" || exit
  "${EDITOR:-vi}" "$secret_file"
  doop format-json --input "$secret_file"
)
```

## TUI

터미널에서 `doop tui`를 실행합니다. 빈 Input과 빈 Pipeline으로 시작하며
실행 결과가 원문을 덮어쓰지 않습니다. 넓은 화면은 왼쪽 Pipeline과 오른쪽
Input·Output 분할을 사용하고, 40~89열에서는 포커스된 패널 하나만 표시합니다.

Output은 `Smart`, `Text`, `Hex`, `Trace` 보기를 제공합니다. `p`는 선택
단계까지 다시 계산하고 `f`는 보관된 최종 결과로 돌아갑니다. UTF-8 결과는
표시 View와 무관하게 원문 그대로 복사하며, 비 UTF-8 결과는 공백 없는 소문자
Hex로 복사합니다. 위험한 UTF-8 제어 문자는 복사 전에 확인합니다.

- 전역: `Tab`/`Shift+Tab` 패널 이동, `Ctrl+P` 변환 추가, `F1` 도움말
- Pipeline: `j`/`k` 선택, `J`/`K` 이동, `Space` 전환, `d` 삭제, `Enter` 검사
- Output: `v`/`V` 보기, `p` 단계, `f` 최종, `Enter`/`y` 복사, `z` 확대
- `Esc`: 창·확대 닫기 또는 실행 취소, `Ctrl+Q`: 정상 종료, `Ctrl+C`: 강제 종료

256 KiB 이하 입력은 50 ms, 그보다 큰 입력은 200 ms 뒤에 단일 작업
스레드로 실행합니다. 최신 대기 요청 하나만 유지하며 오래된 결과는 폐기합니다.
화면은 변경 때만 다시 그리고, Text·Hex·Trace 렌더링은 보이는 범위와
렌더당 4 KiB 처리·출력 예산으로 제한합니다.

## 로컬 검증

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
cargo test --release max_input_edit_release_measurement -- --ignored --nocapture
cargo test --release utf8_validation_release_measurement -- --ignored --nocapture
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
cargo install --locked --path . --root target/install-check
target/install-check/bin/doop --version
```

### 최신 로컬 검증 요약

현재 `main`에서 형식, 경고 금지 Clippy, 전체 단위·CLI 시험, rustdoc,
패키징, 오프라인 잠금 설치, Bash·Zsh PTY를 검증한다. macOS 실제 복사는
`pbpaste`로 소문자 `ff`를 확인하며 이전 클립보드 내용을 복원하지 않는다.
Linux의 미지원·X11과 Wayland 경로는 사용할 수 있는 로컬 환경에서 별도로
실행하고, 실행하지 못한 환경은 미검증으로 명시한다.

릴리스 측정은 렌더링, 최대 입력 편집과 64 MiB UTF-8 판정의 실제 값만
기록하며 시간 자체를 시험 성공 기준으로 사용하지 않는다.

2026-07-31 macOS 26.5.2(25F84), Darwin 25.5.0 arm64, Rust·Cargo 1.97.1에서
5회 준비 실행 뒤 30표본 중앙값은 최대 입력 편집 `2.82475ms`, 64 MiB UTF-8
판정 `2.262584ms`였다. 두 경로 모두 16 ms 이하이므로 조건부 최적화를 적용하지
않고 현재 구현을 유지한다.

자동 완성과 배포 패키지는 v0.2 범위에 포함하지 않습니다.
GitHub Actions와 다른 CI 설정은 사용하지 않습니다.

## 라이선스

MIT 또는 Apache-2.0 중 하나를 선택할 수 있습니다.
