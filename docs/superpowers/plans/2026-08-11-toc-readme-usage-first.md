# toc README Usage-First Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `toc 0.2.1`을 처음 접한 사용자가 소개부터 CLI·TUI 실전 사용법까지 순서대로 읽고 바로 실행할 수 있도록 README를 개편한다.

**Architecture:** 제품 코드는 바꾸지 않고 `README.md` 한 파일의 정보 순서와 문장을 다시 구성한다. 현재 바이너리의 명령·변환·출력 계약을 기준으로 예제를 작성하고, 한국어 설명은 보수적으로 윤문한 뒤 전체 Rust 검증으로 회귀가 없음을 확인한다.

**Tech Stack:** GitHub Flavored Markdown, Rust/Cargo, 기존 `target/debug/toc`, `humanize-korean`

## Global Constraints

- README 순서는 소개, 30초 시작, 자주 쓰는 방법, TUI 사용법, 지원 변환, 안전 경계와 한도, 문서와 라이선스를 따른다.
- 외부 배지·이미지·GIF·가짜 TUI 캡처를 추가하지 않는다.
- E2E 보고서와 해당 링크, 개발 검증 명령, 시험 수를 README에 포함하지 않는다.
- 제품 코드, CLI 문법, TUI 키, 변환, 의존성을 변경하지 않는다.
- `toc 0.2.1`의 공개 변환 ID 24개, 입력·출력 계약, 한도와 보안 경계를 유지한다.
- Binary 결과는 실제 터미널이 아니라 파일로 리디렉션하는 예제만 제공한다.
- CLI, TUI, Pipeline, Trace, View와 모든 명령·ID·키·수치·링크는 윤문 중 원형을 유지한다.
- 윤문용 `_workspace` 산출물과 `HUMANIZE-SUMMARY`는 커밋하거나 README에 넣지 않는다.

---

### Task 1: README 사용 흐름 재작성과 윤문

**Files:**
- Modify: `README.md`
- Reference: `docs/superpowers/specs/2026-08-11-toc-readme-usage-first-design.md`
- Reference: `docs/prd/v0.2.1-prd.md`
- Reference: `docs/superpowers/specs/2026-08-08-toc-0.2.1-design.md`

**Interfaces:**
- Consumes: `target/debug/toc --version`, `target/debug/toc --help`, `target/debug/toc <transform-id> --help`, `target/debug/toc --list`
- Produces: 소개와 CLI·TUI 사용법이 중심인 `README.md`

- [ ] **Step 1: 현재 공개 명령을 다시 확인한다**

Run:

```bash
target/debug/toc --version
target/debug/toc --help
target/debug/toc base64-encode --help
target/debug/toc --list
```

Expected: 버전은 `toc 0.2.1`, Root help에는 `tui`와 `--list`, 변환 help에는 `--then`, 목록에는 공개 ID 24개가 표시된다.

- [ ] **Step 2: README를 사용 흐름 중심으로 재작성한다**

첫 화면은 제품명과 다음 설명만 둔다.

````markdown
<div align="center">
  <h1>toc</h1>
  <p><strong>TUI Object Converter</strong></p>
  <p>텍스트와 바이트를 로컬에서 변환하고, 여러 작업을 Pipeline으로 연결합니다.</p>
  <p><code>CLI</code> · <code>TUI</code> · <code>Local-only</code> · <code>24 transforms</code></p>
</div>
````

이후 제목은 다음 순서로 제한한다.

```text
30초 시작
자주 쓰는 방법
TUI 사용법
지원 변환
안전 경계와 한도
문서
라이선스
```

`30초 시작`에는 설치, 단일 변환, Pipeline, TUI 실행을 이 순서로 둔다.

```bash
cargo install --locked --path .
printf '%s' 'hello' | toc base64-encode
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | toc url-decode --then format-json
toc tui
```

`자주 쓰는 방법`은 목적을 한 문장으로 설명한 뒤 다음 네 예제를 제공한다.

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

첫 예제의 결과 `aGVsbG8=`와 Pipeline의 두 칸 들여쓰기 JSON 결과를 명시한다. 성공 결과에 임의의 끝 줄바꿈이 없고 입력은 표준 입력과 `--input PATH` 중 하나만 사용한다는 설명은 예제 아래에 한 번만 둔다.

TUI는 `INPUT → PIPELINE → OUTPUT [SMART | TEXT | HEX | TRACE]`와 다음 조작 흐름을 설명한다.

```text
1. Input에 원문 입력
2. Ctrl+p로 변환 추가
3. Pipeline에서 단계 선택·전환·실행
4. Output에서 View 전환·확대·복사
```

키 표는 전역, Pipeline, Output 세 행으로 유지한다. `Shift+Enter`를 구분하지 못하는 터미널에서 Raw Copy가 제한된다는 문장을 표 아래에 둔다.

지원 변환 표에는 다음 ID를 모두 코드 표기로 기록한다.

```text
인코딩: base64-encode, base64-decode, base64url-encode, base64url-decode,
         base32-encode, base32-decode, url-encode, url-decode,
         hex-encode, hex-decode, html-encode, html-decode
데이터·텍스트: format-json, minify-json, rot13, sort-lines, remove-duplicate-lines
보안 분석: url-defang, url-refang, jwt-decode
해시·압축: sha256, sha512, gzip-compress, gzip-decompress
```

안전 경계와 한도에는 민감한 셸 인자 금지, 파이프의 원시 바이트 보존, 실제 터미널의 위험 출력 거부, JWT 서명 미검증, URL Defang의 비보안 경계, CLI 64 MiB·256 MiB, TUI 1 MiB·64 MiB, Pipeline 32단계만 남긴다.

- [ ] **Step 3: 한국어 설명을 보수적으로 윤문한다**

Read completely:

```text
/Users/ruffin/gdrive/repo/im-not-ai/codex/skills/humanize-korean/SKILL.md
/Users/ruffin/gdrive/repo/im-not-ai/codex/skills/humanize-korean/references/quick-rules.md
```

README의 한국어 설명을 공적 문서, 보수 강도로 점검한다. 번역투, 광고성 표현, 반복 문장만 고치고 명령·ID·키·수치·링크·Markdown은 바꾸지 않는다. skill 절차가 만든 `_workspace/2026-08-11-NNN/final.md`의 윤문 결과만 README에 반영하고 `HUMANIZE-SUMMARY`와 `_workspace`는 최종 변경에서 제외한다.

- [ ] **Step 4: 예제를 실제 실행하고 결과를 확인한다**

Run:

```bash
printf '%s' 'hello' | target/debug/toc base64-encode
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | target/debug/toc url-decode --then format-json
toc_readme_tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/toc-readme.XXXXXX")
target/debug/toc base64-encode --input README.md > "$toc_readme_tmp_dir/readme.b64"
target/debug/toc base64-decode --input "$toc_readme_tmp_dir/readme.b64" > "$toc_readme_tmp_dir/readme.md"
cmp README.md "$toc_readme_tmp_dir/readme.md"
target/debug/toc gzip-compress --input Cargo.toml > "$toc_readme_tmp_dir/cargo.toml.gz"
target/debug/toc gzip-decompress --input "$toc_readme_tmp_dir/cargo.toml.gz" > "$toc_readme_tmp_dir/cargo.toml"
cmp Cargo.toml "$toc_readme_tmp_dir/cargo.toml"
unlink "$toc_readme_tmp_dir/readme.b64" "$toc_readme_tmp_dir/readme.md"
unlink "$toc_readme_tmp_dir/cargo.toml.gz" "$toc_readme_tmp_dir/cargo.toml"
rmdir "$toc_readme_tmp_dir"
```

Expected: Base64는 `aGVsbG8=`, JSON은 두 칸 들여쓰기 결과를 출력한다. 파일 Base64와 Gzip 왕복은 `cmp`가 출력 없이 성공하며 임시 디렉터리가 제거된다.

- [ ] **Step 5: README 계약과 범위를 검사한다**

Run:

```bash
missing=0
while IFS= read -r line; do
  id=${line%%[[:space:]]*}
  rg --color=never --fixed-strings --quiet "\`$id\`" README.md || {
    printf 'missing transform: %s\n' "$id"
    missing=1
  }
done < <(target/debug/toc --list)
test "$missing" -eq 0

for doc_path in \
  docs/prd/v0.2.1-prd.md \
  docs/superpowers/specs/2026-08-08-toc-0.2.1-design.md \
  LICENSE-MIT LICENSE-APACHE; do
  test -f "$doc_path" || exit 1
done

! rg --color=never --quiet 'docs/test-reports|E2E 시험 보고서|shields\.io|<img|!\[' README.md
! rg --color=never --quiet 'cargo test|개 통과|개 무시|X11|Wayland' README.md
```

Expected: 출력 없이 종료 코드 0.

- [ ] **Step 6: 전체 문서·Rust 검증을 실행한다**

Run:

```bash
git diff --check -- README.md
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features
```

Expected: Markdown 공백 오류가 없고 Format, Clippy, 전체 시험, rustdoc가 모두 성공한다.

- [ ] **Step 7: README만 커밋한다**

```bash
git add README.md
git diff --cached --check
git commit -m "docs(readme): 사용 흐름 정리"
```

Expected: 커밋에는 `README.md`만 포함되고 제품 코드와 `_workspace`는 변경되지 않는다.
