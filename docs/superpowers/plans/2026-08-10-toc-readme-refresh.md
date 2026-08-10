# toc README Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 처음 방문한 사용자가 `toc 0.2.1`의 로컬 CLI·TUI, 24개 변환, 안전 경계를 짧고 정확하게 이해하도록 README를 개편한다.

**Architecture:** 제품 코드는 건드리지 않고 `README.md`만 승인된 터미널 중심 구조로 다시 쓴다. 현재 바이너리와 저장소 문서를 기준으로 명령·변환 ID·키·한도·시험 수를 대조하고, 상세 기록은 기존 PRD·설계·E2E 문서 링크로 분리한다.

**Tech Stack:** GitHub Flavored Markdown, Rust/Cargo, 기존 `target/debug/toc`

## Global Constraints

- 외부 배지·이미지·GIF·가짜 TUI 캡처를 추가하지 않는다.
- CLI·TUI 동작, 키 바인딩, 제품 코드, 의존성은 변경하지 않는다.
- `toc 0.2.1`과 공개 변환 ID 24개를 현재 실행 결과와 일치시킨다.
- 성공 출력에 임의 줄바꿈이 없고, 실제 터미널과 파이프의 출력 안전 경계가 다름을 명시한다.
- JWT 서명 미검증, URL Defang의 비보안 경계, CLI·TUI·Pipeline 한도를 유지한다.
- 사용자 소유 변경인 `.gitignore`와 `docs/test-reports/2026-08-09-e2e.md`는 수정하거나 함께 커밋하지 않는다.

---

### Task 1: README 재작성과 계약 검증

**Files:**
- Modify: `README.md`
- Reference: `docs/superpowers/specs/2026-08-10-toc-readme-refresh-design.md`
- Reference: `docs/test-reports/2026-08-09-e2e.md`
- Reference: `docs/prd/v0.2.1-prd.md`
- Reference: `docs/superpowers/specs/2026-08-08-toc-0.2.1-design.md`

**Interfaces:**
- Consumes: `target/debug/toc --version`, `target/debug/toc --help`, `target/debug/toc --list`, 현재 TUI 키·한도·출력 계약
- Produces: GitHub에서 외부 자산 없이 렌더링되는 간결한 `README.md`

- [ ] **Step 1: 현재 공개 계약을 다시 기록한다**

Run:

```bash
target/debug/toc --version
target/debug/toc --help
target/debug/toc --list
```

Expected: 버전은 `toc 0.2.1`, 목록은 24개이며 Root help에는 `tui`, 직접 변환 명령, `--then`, `--list`가 표시된다.

- [ ] **Step 2: README를 승인된 정보 구조로 교체한다**

첫 화면은 다음 구조와 문구를 사용한다.

````markdown
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
````

이후 제목은 `왜 toc인가`, `설치`, `Quick Start`, `변환`, `TUI`, `안전 경계`, `한도`, `검증`, `문서`, `라이선스` 순서로 둔다. 변환 표는 아래 네 행으로 제한하되 모든 ID를 코드 표기로 기록한다.

```text
인코딩: base64-encode/decode, base64url-encode/decode, base32-encode/decode,
         url-encode/decode, hex-encode/decode, html-encode/decode
데이터·텍스트: format-json, minify-json, rot13, sort-lines, remove-duplicate-lines
보안 분석: url-defang, url-refang, jwt-decode
해시·압축: sha256, sha512, gzip-compress, gzip-decompress
```

TUI에는 `INPUT → PIPELINE → OUTPUT [SMART | TEXT | HEX | TRACE]`와 전역·Pipeline·Output 핵심 키 표만 남긴다. 상세 설명은 아래 링크로 연결한다.

```markdown
- [E2E 시험 보고서](docs/test-reports/2026-08-09-e2e.md)
- [v0.2.1 제품 요구사항](docs/prd/v0.2.1-prd.md)
- [v0.2.1 설계](docs/superpowers/specs/2026-08-08-toc-0.2.1-design.md)
```

- [ ] **Step 3: README의 명령 예제를 실제 실행한다**

Run:

```bash
printf '%s' 'hello' | target/debug/toc base64-encode
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | target/debug/toc url-decode --then format-json
```

Expected: 첫 결과는 줄바꿈 없이 `aGVsbG8=`, 둘째 결과는 다음 두 칸 들여쓰기 JSON이다.

```json
{
  "name": "toc"
}
```

- [ ] **Step 4: 24개 ID와 로컬 링크를 검사한다**

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

for path in \
  docs/test-reports/2026-08-09-e2e.md \
  docs/prd/v0.2.1-prd.md \
  docs/superpowers/specs/2026-08-08-toc-0.2.1-design.md \
  LICENSE-MIT LICENSE-APACHE; do
  test -f "$path" || exit 1
done
```

Expected: 출력 없이 종료 코드 0.

- [ ] **Step 5: 문서와 제품 회귀 검증을 실행한다**

Run:

```bash
git diff --check -- README.md
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features
cargo test --release --locked -- --ignored
```

Expected: Markdown 공백 오류가 없고, 형식·Clippy·일반 시험·rustdoc·release ignored 시험이 모두 성공한다. README에는 이번 실행에서 확인한 일반 시험 통과·무시 수와 release ignored 시험 수만 기록한다.

- [ ] **Step 6: README만 커밋한다**

```bash
git add README.md
git diff --cached --check
git commit -m "docs(readme): 사용 안내 개편"
```

Expected: 커밋에는 `README.md`만 포함되고 `.gitignore`, `docs/test-reports/2026-08-09-e2e.md`는 기존 상태로 남는다.
