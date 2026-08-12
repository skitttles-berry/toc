# toc Asciinema Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record separate, representative CLI and TUI sessions and attach both hosted previews to README.

**Architecture:** Build the current release binary once, then drive real terminal sessions through `expect` and asciinema 3.2.1. Keep the two asciicast sources in `docs/asciinema/`, upload them as unlisted recordings, and use the returned URLs for GitHub-compatible SVG preview links.

**Tech Stack:** Rust/Cargo, asciinema 3.2.1, Expect 5.45, Markdown

## Global Constraints

- Preserve the user's existing `.gitignore`, `README.md`, and `docs/test-reports/` changes.
- Do not enable asciinema input capture or record secrets.
- Record CLI at 100×22 and TUI at 120×30.
- Store `docs/asciinema/toc-cli.cast` and `docs/asciinema/toc-tui.cast`.
- Upload both recordings to `https://asciinema.org` with `unlisted` visibility.
- Do not add a recording framework, GIF converter, dependency, or automation script.

---

### Task 1: Record the CLI and TUI sessions

**Files:**
- Create: `docs/asciinema/toc-cli.cast`
- Create: `docs/asciinema/toc-tui.cast`

**Interfaces:**
- Consumes: `target/release/toc`, the current CLI command IDs, and the current TUI key map
- Produces: two asciicast v3 files accepted by `asciinema play`, `convert`, and `upload`

- [ ] **Step 1: Build the executable used by both recordings**

Run: `rtk cargo build --locked --release`

Expected: exit 0 and `target/release/toc` exists.

- [ ] **Step 2: Record the CLI session**

Run `asciinema record` under Expect with a 100×22 window and a clean `$ ` prompt. Add
`target/release` to the child shell's `PATH`, then type these exact commands with short pauses:

```bash
printf '%s' 'hello' | toc base64-encode; printf '\n'
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | toc url-decode --then format-json; printf '\n'
```

Exit the shell normally. Use `--overwrite`, `--idle-time-limit 1`, and title
`toc CLI — transforms and pipeline`; do not use `--capture-input`.

- [ ] **Step 3: Verify the CLI recording**

Run:

```bash
rtk asciinema play --speed 10 docs/asciinema/toc-cli.cast
rtk asciinema convert -f txt docs/asciinema/toc-cli.cast /tmp/toc-cli-asciinema.txt
rtk rg --color=never -F 'aGVsbG8=' /tmp/toc-cli-asciinema.txt
rtk rg --color=never -F '"name": "toc"' /tmp/toc-cli-asciinema.txt
```

Expected: every command exits 0 and both result strings are present.

- [ ] **Step 4: Record the TUI session**

Run `asciinema record` under Expect with a 120×30 window and the same controlled shell.
Type `toc tui`, then send the following actual key sequence with short pauses:

```text
hello
Ctrl+p, base64-encode, Enter
Ctrl+p, sha256, Enter
Tab, Tab, Up, s, f
Shift+Tab, v, v, v
Ctrl+q, y
exit
```

Use `--overwrite`, `--idle-time-limit 1`, and title
`toc TUI — input, pipeline, and output views`; do not use `--capture-input`.

- [ ] **Step 5: Verify the TUI recording**

Run:

```bash
rtk asciinema play --speed 10 docs/asciinema/toc-tui.cast
rtk asciinema convert -f txt docs/asciinema/toc-tui.cast /tmp/toc-tui-asciinema.txt
rtk rg --color=never -F 'Base64 Encode' /tmp/toc-tui-asciinema.txt
rtk rg --color=never -F 'SHA-256' /tmp/toc-tui-asciinema.txt
rtk rg --color=never -F 'TRACE' /tmp/toc-tui-asciinema.txt
```

Expected: every command exits 0 and all three rendered labels are present.

### Task 2: Upload and attach the recordings

**Files:**
- Modify: `README.md`
- Verify: `docs/asciinema/toc-cli.cast`
- Verify: `docs/asciinema/toc-tui.cast`

**Interfaces:**
- Consumes: the two verified casts and the two URLs returned by asciinema.org
- Produces: two persistent unlisted recording pages and two clickable README SVG previews

- [ ] **Step 1: Upload both casts**

Run `asciinema upload --server-url https://asciinema.org --visibility unlisted` once per cast,
with titles `toc CLI — transforms and pipeline` and
`toc TUI — input, pipeline, and output views`. Retain the exact recording URL printed by each
successful upload. If the CLI reports that the installation is not linked to an account, run
`asciinema auth --server-url https://asciinema.org` and complete that account link before
continuing so the recordings are not deleted after seven days. Assign the two returned URLs to
`CLI_RECORDING_URL` and `TUI_RECORDING_URL` for the remaining checks.

- [ ] **Step 2: Add both previews to README**

Immediately after `## 30초 시작`, add `## 실행 화면` with `### CLI` and `### TUI` subsections.
For each subsection, use the official Markdown image-link form: the link target is the exact
recording URL and the image source is that URL with `.svg` appended. Do not change any other
README content.

- [ ] **Step 3: Verify hosted assets and repository changes**

Run:

```bash
rtk curl -fsSI "$CLI_RECORDING_URL"
rtk curl -fsSI "$CLI_RECORDING_URL.svg"
rtk curl -fsSI "$TUI_RECORDING_URL"
rtk curl -fsSI "$TUI_RECORDING_URL.svg"
rtk git diff --check
rtk git status --short
```

Expected: all four HTTP checks succeed, the diff check exits 0, both casts and the intended
README edit are present, and the user's pre-existing changes remain present.

- [ ] **Step 4: Commit only this task's artifacts**

Stage both casts normally. Use `git add -p README.md` to accept only the new `실행 화면` hunk and
reject the user's pre-existing README hunk. Stage this plan only if it is not already committed.
Verify the staged diff excludes `.gitignore`, the earlier README edits, and `docs/test-reports/`,
then commit with:

```bash
rtk git commit -m "docs(readme): CLI·TUI 실행 녹화 첨부"
```
