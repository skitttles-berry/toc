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
doop --list
doop --help
```

변환 명령은 `base64-encode`, `base64-decode`, `url-encode`,
`url-decode`, `format-json`, `minify-json`입니다. `run`이나
`transform` 상위 명령 없이 변환 명령을 직접 실행합니다. 파이프 입력과
`--input PATH` 중 정확히 하나만 사용해야 합니다. 성공 결과에는
임의의 끝 줄바꿈을 추가하지 않습니다.

민감한 입력은 셸 인자에 직접 넣지 마십시오. 인자는 셸 기록과 프로세스
목록에 남을 수 있습니다. 기록에 값이 남지 않는 대화형 파이프를 사용하거나,
처음부터 접근 권한을 제한해 만든 파일을 `--input`으로 전달합니다.

```bash
IFS= read -r -s DOOP_INPUT
printf '%s' "$DOOP_INPUT" | doop base64-encode
unset DOOP_INPUT

(umask 077; some-local-command > secret.txt)
doop format-json --input secret.txt
```

## TUI

터미널에서 `doop tui`를 실행합니다. TUI는 빈 Input과 빈 Chain으로
시작하며 Preview 결과가 Input을 덮어쓰지 않습니다.
화면은 최초 표시와 입력·상태·작업 결과·터미널 크기 변경 때만 다시 그립니다.
미리보기는 확장 그래핌 군집을 나누지 않고 터미널 셀 폭에 맞춰 자르며,
렌더당 텍스트 처리·출력을 4 KiB로 제한합니다. 상한 경계의 군집은 생략될 수 있습니다.
좁은 상태줄에서는 클립보드 오류를 일반 도움말보다 먼저 표시합니다.

- `Ctrl+P`: 변환 추가
- `Tab`, `Shift+Tab`: 영역 이동
- Preview의 `Enter`: 결과 복사
- Chain의 방향키, `Space`, `Delete`: 선택·활성화·삭제
- `Ctrl+Q`: 정상 종료
- `Ctrl+C`: 강제 종료

## 로컬 검증

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
cargo install --locked --path . --root target/install-check
target/install-check/bin/doop --version
```

릴리스 렌더링 측정은 시간 합격 기준 없이 무조건 렌더 기준과 변경 기반
렌더링 경로의 실제 측정값만 출력합니다. 셸 점검에는 `expect`가 필요하며
호출한 Bash 또는 Zsh 자체를 의사 터미널 안에서 실행합니다.

### 2026-07-30 릴리스 검증 기록

프로덕션 코드 기준은 `3f336dc`이며 아래 검증 스크립트와 기록은 이 문서와
같은 검증 커밋에 포함된다. Rust는 `1.97.1`이다.

| 환경 | 셸·CLI·TUI | 클립보드 |
|---|---|---|
| macOS 26.5.2 arm64 | Bash 3.2.57, Zsh 5.9 통과 | 호스트 보호를 위해 읽기·쓰기 미실행 |
| `rust:1.97.1-bookworm` Debian 12 arm64 | Bash 5.2.15, Zsh 5.9 통과, `/dev/full` 코드 `5` 확인 | 화면 서버 없음: `Clipboard unavailable` 확인 |
| 같은 컨테이너의 Xvfb/X11 | Bash TUI 통과 | 복사 성공과 `xclip`의 정확한 `clipboard-smoke` 확인 |
| Wayland | 미검증 | 미검증 |

macOS 셸 점검과 별도 대상 디렉터리 확인은 다음 명령으로 수행했다.

```bash
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
CARGO_TARGET_DIR=target/task5-custom zsh tests/shell-smoke.sh
```

컨테이너는 저장소를 읽기 전용으로 마운트하고 모든 빌드·설치 결과를
임시 파일 시스템에 기록했다.

```bash
docker run --rm -d --name doop-task5-validation \
  --mount type=bind,src="$PWD",dst=/workspace,readonly \
  --workdir /workspace \
  --env CARGO_TARGET_DIR=/tmp/target \
  --env CARGO_TERM_COLOR=never \
  --env NO_COLOR=1 \
  rust:1.97.1-bookworm sleep infinity
docker exec doop-task5-validation bash -c \
  'apt-get update && apt-get install -y --no-install-recommends expect zsh xvfb xclip xauth'
docker exec doop-task5-validation bash -c \
  'bash tests/shell-smoke.sh && zsh tests/shell-smoke.sh'
docker exec doop-task5-validation bash -c \
  'DOOP_SMOKE_CLIPBOARD_MODE=unavailable bash tests/shell-smoke.sh'
docker exec doop-task5-validation bash -c \
  'xvfb-run -a env DOOP_SMOKE_CLIPBOARD_MODE=x11 bash tests/shell-smoke.sh'
docker exec doop-task5-validation bash -c \
  'cargo install --locked --path . --root /tmp/install-check && /tmp/install-check/bin/doop --version'
docker exec doop-task5-validation bash -c \
  'cargo install cargo-audit --locked && cargo audit'
docker stop doop-task5-validation
```

이미지 식별자는
`sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa`다.
잠긴 설치 결과는 `doop 0.1.0`이었다. `cargo-audit 0.22.2`는 RustSec
자문 1,173건을 불러와 `Cargo.lock`의 146개 의존성을 검사했고 알려진
취약점을 보고하지 않았다.

릴리스 렌더링 측정 명령과 결과는 다음과 같다.

```text
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
iterations=500, unconditional=27.937084ms, dirty=55.75µs, redraws=1
```

자동 완성과 배포 패키지는 v0.1 범위에 포함하지 않습니다.
GitHub Actions와 다른 CI 설정은 사용하지 않습니다.

## 라이선스

MIT 또는 Apache-2.0 중 하나를 선택할 수 있습니다.
