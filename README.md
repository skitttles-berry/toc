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
Input·Output 분할을 사용하고, 40~89열에서는 Pipeline 30%, Input 30%, Output
40%의 세로 배치를 사용합니다. 높이 10~11행에서는 포커스된 패널 하나만 표시하고,
그보다 작으면 터미널 크기 안내를 표시합니다. 최하단 두 줄은 포커스별 도움말과
공통 도움말이며, 상태 메시지는 첫째 줄만 대체합니다.

Output은 `Smart`, `Text`, `Hex`, `Trace` 보기를 제공합니다. `p`는 선택
단계까지 다시 계산하고 `f`는 보관된 최종 결과로 돌아갑니다. 유효한 JSON 결과는
Pretty Copy에서 두 칸 들여쓰고 Raw Copy에서 구조 공백을 제거합니다. 그 밖의
UTF-8은 원문, 비 UTF-8은 공백 없는 소문자 Hex로 복사합니다. 복사는 표시 View가
아니라 현재 Output의 FINAL 또는 STEP 원본을 사용하며 Trace에서는 비활성입니다.
위험한 UTF-8 제어 문자는 복사 전에 확인합니다.

- 전역: `Tab`/`Shift+Tab` 패널 이동, `F3` Pretty Copy, `F4` Raw Copy,
  `Ctrl+P` 변환 추가, `F1` 도움말
- Pipeline: `j`/`k` 선택, `J`/`K` 이동, `Space` 전환, `d` 삭제, `Enter` 검사
- Output: `v`/`V` 보기, `p` 단계, `f` 최종, `Enter`/`y` Pretty Copy, `z` 확대
- `Esc`: 창·확대 닫기 또는 실행 취소, `Ctrl+Q`: 정상 종료, `Ctrl+C`: 강제 종료

256 KiB 이하 입력은 50 ms, 그보다 큰 입력은 200 ms 뒤에 단일 작업
스레드로 실행합니다. 최신 대기 요청 하나만 유지하며 오래된 결과는 폐기합니다.
화면은 변경 때만 다시 그리고, Text·Hex·Trace 렌더링은 보이는 범위와
렌더당 4 KiB 처리·출력 예산으로 제한합니다.

## 로컬 검증

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
cargo test --release max_input_edit_release_measurement -- --ignored --nocapture
cargo test --release utf8_validation_release_measurement -- --ignored --nocapture
cargo package --locked
install_root=$(mktemp -d "${TMPDIR:-/tmp}/doop-install.XXXXXX")
trap 'rm -rf -- "$install_root"' EXIT
cargo install --locked --offline --path . --root "$install_root"
"$install_root/bin/doop" --version
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
test "$(pbpaste)" = ff
DOOP_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
test "$(pbpaste)" = ff
```

### 최신 로컬 검증 요약

2026-07-31 `codex/tui-ux-refresh`를 macOS 26.5.2(25F84), Darwin 25.5.0 arm64,
Rust·Cargo 1.97.1에서 검증하고, 최종 리뷰에서
`cargo test --all-targets --all-features --locked`를 다시 실행했다.
전체 시험은 라이브러리 230개 통과·3개 무시, CLI 통합 15개 통과로
합계 245개 통과·3개 무시였고 실패는 없었다. 형식,
경고 금지 Clippy, rustdoc, 잠금 패키징, 임시 경로 오프라인 잠금 설치와
`doop 0.2.0` 실행도 통과했다.

세 릴리스 측정은 5회 준비 뒤 30표본의 중앙값을 대표값으로 사용했다.
표본당 500회 반복한 렌더링의 무조건 다시 그리기 최솟값·중앙값·최댓값은
`67.031625ms`·`68.970125ms`·`79.030709ms`, 변경 시 다시 그리기는
`125.791µs`·`135.791µs`·`208.875µs`였고 표본마다 실제 다시 그리기는 1회였다.
최대 입력 편집은 `2.788958ms`·`2.922958ms`·`3.025042ms`, 64 MiB
UTF-8 판정은 `2.15725ms`·`2.251334ms`·`2.575417ms`였다. 두 조건부
경로 중 변경 시 다시 그리기의 중앙값이 16 ms 이하이므로 현재 구현을 유지하며,
시간은 시험 성공 기준이 아니다.

Bash·Zsh 기본 PTY와 두 셸의 실제 macOS 복사 경로가 통과했고 `pbpaste`로
소문자 `ff`를 확인했다. 이전 클립보드 내용은 복원하지 않는다. 이 장비에는
Linux 로컬 환경이 없으므로 Linux 미지원·X11·Wayland 경로는 미검증이다.

자동 완성과 배포 패키지는 v0.2 범위에 포함하지 않습니다.
GitHub Actions와 다른 CI 설정은 사용하지 않습니다.

## 라이선스

MIT 또는 Apache-2.0 중 하나를 선택할 수 있습니다.
