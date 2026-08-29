# toc

제품 설명, 설치·사용법, 변환 목록과 제한은 [README.md](README.md)를 따른다.

## 구조

```text
toc/
├── Cargo.toml             # 패키지·의존성
├── src/
│   ├── main.rs            # CLI/TUI 진입점
│   ├── lib.rs             # 공용 모듈·제한
│   ├── cli.rs             # 인자·입출력
│   ├── error.rs           # 오류·종료 코드
│   ├── pipeline.rs        # 공용 Pipeline 실행
│   ├── transforms/
│   │   ├── mod.rs         # 변환 Registry
│   │   └── *.rs           # 변환 구현
│   ├── tui.rs             # 터미널 수명주기·이벤트 루프
│   └── tui/
│       ├── state.rs       # 상태·이벤트·Effect
│       ├── worker.rs      # Preview Worker
│       ├── render.rs      # 레이아웃·렌더링
│       ├── views.rs       # Text·Hex·Trace View
│       └── clipboard.rs   # 복사 준비·쓰기
├── tests/
│   ├── cli.rs             # CLI 통합
│   └── shell-smoke.sh     # Shell·PTY smoke
└── docs/                  # PRD·설계·구현 계획
```

## 재발 방지

- Serena 메모리, 과거 Plan·rollout, 화면 관찰을 현재 상태로 간주하지 않는다. 작업 전
  checkout, `Cargo.toml`, `toc --help`, `toc --list`, 관련 테스트를 다시 확인한다.
- Dirty worktree의 사용자 수정·미추적 파일을 덮어쓰거나 삭제·일괄 stage하지 않는다.
  대상 파일만 명시적으로 stage하고 staged diff를 확인한다.
- `AGENTS.md`는 Git 추적 대상이다. 작업 규칙이 바뀌면 코드·문서와 함께 현행화하고,
  ccc 인덱스만 믿지 말고 항상 직접 읽는다.
- CLI 예제는 `printf '%s'`로 byte-exact 입력을 만든다. 변환은 공용 Registry와
  Pipeline에 한 번만 추가하고 binary 허용·출력 제한을 지킨다. Zlib 해제는 스트림
  종료·checksum·후행 데이터를 엄격히 검증한다.
- TUI는 인접 ANSI 문자열로 검증하지 않는다. 화면의 의미를 확인하고 alternate-screen
  녹화는 PTY·raw event로, modifier key는 대상 터미널의 실제 입력으로 검증한다.
- 동작 변경은 README와 관련 설계·계획을 같은 논리적 변경에서 갱신한다. 완료 전 관련
  시험과 fmt·Clippy·전체 시험을 실행하고, TUI·플랫폼 변경은 release·Shell smoke도 확인한다.
- Push 403은 force-push하지 않는다. 활성 GitHub 계정과 credential helper를 확인하고,
  저장소 권한이 있는 계정으로 push한 뒤 기존 계정·설정을 복원한다.
- 토큰, 인증 URL, clipboard·입력 본문을 기록하거나 출력하지 않는다.

## 검증

```bash
cargo test --locked
```

@RTK.md
