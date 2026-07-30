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
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
cargo install --locked --path . --root target/install-check
target/install-check/bin/doop --version
```

릴리스 렌더링 측정은 시간 합격 기준 없이 무조건 렌더 기준과 변경 기반
렌더링 경로의 실제 측정값만 출력합니다. 셸 점검에는 `expect`가 필요하며
호출한 Bash 또는 Zsh 자체를 의사 터미널 안에서 실행합니다.

고도화한 작업판의 macOS·Linux 셸 및 실제 클립보드 통합 검증은 다음
로컬 검증 작업에서 수행할 예정입니다.

### 2026-07-30 v0.1 릴리스 검증 기록(역사 기록)

다음은 v0.1 당시의 역사 기록이다. 프로덕션 코드 기준은 `3f336dc`이며 아래
검증 스크립트와 기록은 이 문서와 같은 검증 커밋에 포함된다.
Rust는 `1.97.1`이다.

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

### 2026-07-30 v0.2 전체 검증 기록

v0.2 검증 기준 코드는 `7ca79dc29aa925407bc95517def837691a4ae875`다.
이 기록은 그 기준 코드 다음의 문서 전용 커밋으로 추가되며 제품 코드,
Cargo 설정, 시험 스크립트와 CI는 변경하지 않는다.

| 환경 | 도구 체인 |
|---|---|
| macOS 26.5.2(25F84), Darwin 25.5.0, arm64 | Rust·Cargo 1.97.1, Bash 3.2.57, Zsh 5.9, Expect 5.45 |
| Debian 12 aarch64, `rust:1.97.1-bookworm` | Rust·Cargo 1.97.1, Bash 5.2.15, Zsh 5.9, Expect 5.45.4, sway 1.7, wl-clipboard 2.1.0 |

컨테이너 이미지 식별자는
`sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa`다.
저장소는 `/workspace`에 읽기 전용으로 연결했고 Cargo 대상·설치·설정과
화면 서버 런타임은 `/tmp`에 두었다.

macOS에서 다음 범주를 검증했다.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
cargo package --locked
cargo install --locked --offline --path . --root "$install_root"
"$install_root/bin/doop" --version
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=macos zsh tests/shell-smoke.sh
```

형식, 경고 금지 린트와 일반 시험 126개가 통과했고, ignored 릴리스 측정
1개도 별도로 통과했다. rustdoc 경고 금지 문서 빌드, 패키징과 오프라인
잠금 설치도 통과했다.
설치 결과는 `doop 0.2.0`이었다. 릴리스 렌더링 측정 결과는
`iterations=500, unconditional=28.677083ms, dirty=62.709µs, redraws=1`이었다.
Bash·Zsh의 기본 스모크와 실제 macOS 복사가 모두 통과했다. 각 실제 복사
전후에 별도 임시 파일로 원문을 백업·복원하고 `pbpaste`와 `cmp`로 동일성을
확인했다. 클립보드 내용 자체는 출력하거나 기록하지 않았다.

Linux 컨테이너에서는 다음 범주를 검증했다.

```bash
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
DOOP_SMOKE_CLIPBOARD_MODE=unavailable bash tests/shell-smoke.sh
xvfb-run -a env DOOP_SMOKE_CLIPBOARD_MODE=x11 bash tests/shell-smoke.sh
WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman sway
WAYLAND_DISPLAY=wayland-1 DOOP_SMOKE_CLIPBOARD_MODE=wayland \
  bash tests/shell-smoke.sh
cargo audit --color never --file /workspace/Cargo.lock
```

Bash·Zsh 기본 스모크와 `/dev/full` 출력 오류 코드 `5`, 화면 서버가 없을
때의 `Clipboard unavailable`, Xvfb/X11 실제 복사와 `xclip` 정확값 검증이
통과했다. 비권한 사용자와 권한 `0700`의 `XDG_RUNTIME_DIR`에서 wlroots
headless sway를 실행해 `wayland-1` 유닉스 소켓을 확인했고, 자료 제어 복사와
외부 `wl-paste --no-newline` 정확값 검증도 통과했다.

`cargo-audit 0.22.2`는 최신 RustSec 데이터베이스를 가져와 자문 1,173건을
불러왔고 `Cargo.lock`의 146개 의존성을 검사했다. 알려진 취약점과 경고는
각각 0건이었다. 위 v0.2 검증 범위의 미검증 항목은 없다.

자동 완성과 배포 패키지는 v0.2 범위에 포함하지 않습니다.
GitHub Actions와 다른 CI 설정은 사용하지 않습니다.

## 라이선스

MIT 또는 Apache-2.0 중 하나를 선택할 수 있습니다.
