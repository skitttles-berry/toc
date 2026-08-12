# toc README Output Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the asciinema preview into the TUI guide, replace the text mockup, and show CLI output directly after each command.

**Architecture:** Change only the presentation of `README.md`. Use one GitHub-compatible `console` transcript for CLI examples and reuse the existing clickable asciinema SVG in the TUI section without changing product behavior or URLs.

**Tech Stack:** GitHub Flavored Markdown, Bash examples, Rust/Cargo verification

## Global Constraints

- Preserve the user's current README introduction and license edits.
- Preserve every command, transform ID, shortcut, limit, and operational warning not explicitly removed by the approved design.
- Keep `https://asciinema.org/a/tqmZAslTwfglLSfj` and its `.svg` preview unchanged.
- Do not modify `.gitignore`, license files, `_workspace`, product code, or TUI behavior.
- Stage only this task's README hunks; do not stage pre-existing user changes.

---

### Task 1: Reorganize README examples and TUI preview

**Files:**
- Modify: `README.md:8-72`
- Reference: `docs/superpowers/specs/2026-08-12-toc-readme-output-layout-design.md`

**Interfaces:**
- Consumes: the current README wording, the existing asciinema URLs, and current `toc` CLI output
- Produces: one CLI transcript and one TUI preview inside `TUI 사용법`

- [ ] **Step 1: Capture the existing user-owned diff and verify example output**

Run:

```bash
rtk git diff -- README.md
rtk printf '%s' 'hello' | rtk cargo run --quiet --locked -- base64-encode
rtk printf '%s' '%7B%22name%22%3A%22toc%22%7D' \
  | rtk cargo run --quiet --locked -- url-decode --then format-json
```

Expected: the diff contains the user's introduction and TUI wording changes. The commands print
`aGVsbG8=` and the following JSON, without an extra trailing newline:

```json
{
  "name": "toc"
}
```

- [ ] **Step 2: Replace the CLI examples with one console transcript**

Replace the current CLI code block and the separate result paragraph/code block with exactly:

````markdown
```console
# 문자열 Base64 인코딩
$ printf '%s' 'hello' | toc base64-encode
aGVsbG8=

# URL Decode 후 JSON 정리
$ printf '%s' '%7B%22name%22%3A%22toc%22%7D' \
  | toc url-decode --then format-json
{
  "name": "toc"
}

# 파일의 JSON 정리
$ toc format-json --input input.json

# Binary Gzip 결과 저장
$ toc gzip-compress --input input.txt > output.gz
```
````

Keep the following paragraph about standard input, `--input PATH`, trailing newlines, and Binary
redirection unchanged.

- [ ] **Step 3: Move the asciinema preview into the TUI guide**

Delete the top-level `## TUI 실행 화면` section, including its click instruction. Immediately
after the existing `TUI 사용법` introductory paragraph, insert exactly:

````markdown
### 실행 화면

[![toc TUI 실행 녹화](https://asciinema.org/a/tqmZAslTwfglLSfj.svg)](https://asciinema.org/a/tqmZAslTwfglLSfj)
````

Delete `### 화면 구성`, its introductory sentence, the `>_ TOC` ASCII code block, and the terminal
size sentence. Keep `### 4단계로 시작` and every following table and compatibility note unchanged.

- [ ] **Step 4: Verify the README structure and live examples**

Run:

```bash
rtk rg --color=never -n '^## |^### ' README.md
rtk rg --color=never -c -F 'https://asciinema.org/a/tqmZAslTwfglLSfj.svg' README.md
rtk rg --color=never -n 'TUI 실행 화면|화면 구성|>_ TOC|첫 번째 명령|미리보기를 누르면' README.md
rtk printf '%s' 'hello' | rtk cargo run --quiet --locked -- base64-encode
rtk printf '%s' '%7B%22name%22%3A%22toc%22%7D' \
  | rtk cargo run --quiet --locked -- url-decode --then format-json
rtk curl -fsSIL -o /dev/null -w '%{http_code}\n' https://asciinema.org/a/tqmZAslTwfglLSfj
rtk curl -fsSIL -o /dev/null -w '%{http_code}\n' https://asciinema.org/a/tqmZAslTwfglLSfj.svg
rtk git diff --check
rtk cargo test --locked
```

Expected: the SVG count is `1`; the removed-text search exits `1` without output; both HTTP checks
print `200`; Base64 and JSON match Step 1; `git diff --check` exits `0`; and all Rust tests pass.

- [ ] **Step 5: Verify all 24 transform IDs remain documented**

Run:

```bash
rtk cargo run --quiet --locked -- --list \
  | rtk python3 -c 'import pathlib, sys
ids = [line.split("\t", 1)[0] for line in sys.stdin if "\t" in line]
readme = pathlib.Path("README.md").read_text(encoding="utf-8")
missing = [item for item in ids if f"`{item}`" not in readme]
print({"listed": len(ids), "documented": len(ids) - len(missing), "missing": missing})
sys.exit(0 if len(ids) == 24 and not missing else 1)'
```

Expected: `{'listed': 24, 'documented': 24, 'missing': []}` and exit `0`.

- [ ] **Step 6: Commit only the requested README hunks**

Stage only the CLI transcript, TUI preview move, and text-mockup removal hunks. Leave the user's
pre-existing README introduction changes, `.gitignore`, licenses, and `_workspace` unstaged.

Run:

```bash
rtk git diff --cached --name-status
rtk git diff --cached --check
rtk git commit -m 'docs(readme): CLI 출력과 TUI 녹화 정리'
```

Expected: only `README.md` is staged, the staged diff contains no unrelated introduction or license
change, the diff check exits `0`, and the commit succeeds.
