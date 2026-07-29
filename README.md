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
```

릴리스 렌더링 측정은 시간 합격 기준 없이 무조건 렌더 기준과 변경 기반
렌더링 경로의 실제 측정값만 출력합니다.

| 환경 | 상태 |
|---|---|
| macOS + Bash/Zsh | 로컬 검증 |
| Linux + Bash/Zsh | 현재 환경에서 미검증 |

Linux 검증을 수행한 릴리스에서는 위 상태를 실제 결과에 맞게 갱신합니다.
자동 완성과 배포 패키지는 v0.1 범위에 포함하지 않습니다.
GitHub Actions와 다른 CI 설정은 사용하지 않습니다.

## 라이선스

MIT 또는 Apache-2.0 중 하나를 선택할 수 있습니다.
