# toc Asciinema Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record one representative, color-preserving TUI session and attach its hosted preview to README.

**Architecture:** Build the current release binary once, then drive the real TUI through `expect` and asciinema 3.2.1 with terminal colors enabled. Keep one asciicast source in `docs/asciinema/`, upload it as an unlisted recording, and use the returned URL for a GitHub-compatible SVG preview link.

**Tech Stack:** Rust/Cargo, asciinema 3.2.1, Expect 5.45, Markdown

## Global Constraints

- Preserve the user's existing `.gitignore`, `README.md`, and `docs/test-reports/` changes.
- Do not enable asciinema input capture or record secrets.
- Record TUI at 120×30 with `TERM=xterm-256color` and `NO_COLOR` unset.
- Store only `docs/asciinema/toc-tui.cast`; exclude the earlier CLI cast.
- Keep `$ toc tui`, then pause capture with `Ctrl+\` before terminating the shell so no `exit`
  text is recorded.
- Upload only the TUI recording to `https://asciinema.org` with `unlisted` visibility.
- Do not add a recording framework, GIF converter, dependency, or automation script.

---

### Task 1: Record the color TUI session

**Files:**
- Create: `docs/asciinema/toc-tui.cast`
- Exclude: `docs/asciinema/toc-cli.cast`

**Interfaces:**
- Consumes: `target/release/toc`, the current TUI key map, and terminal-native ANSI colors
- Produces: one asciicast v3 file accepted by `asciinema play` and `upload`

- [ ] **Step 1: Build the executable used by the recording**

Run: `rtk cargo build --locked --release`

Expected: exit 0 and `target/release/toc` exists.

- [ ] **Step 2: Record the TUI session**

Run `asciinema record` under Expect with a 120×30 window and a clean Bash `$ ` prompt. Add
`target/release` to `PATH`, set `TERM=xterm-256color`, remove `NO_COLOR` from the child
environment, type `toc tui`, then send the following actual key sequence with short pauses:

```text
hello
Ctrl+p, base64-encode, Enter
Ctrl+p, sha256, Enter
Tab, Tab, Up, s, f
Shift+Tab, v, v, v
Ctrl+q, y
```

Use `--overwrite`, `--idle-time-limit 1`, and title
`toc TUI — input, pipeline, and output views`; do not use `--capture-input`. When the shell prompt
returns, send `Ctrl+\` to pause capture, then terminate the shell outside the captured stream.

- [ ] **Step 3: Verify the TUI recording**

Run:

```bash
rtk asciinema play --speed 10 docs/asciinema/toc-tui.cast
rtk bat --plain --color=never --line-range 1:1 docs/asciinema/toc-tui.cast
rtk rg --color=never -F 'Base64 Encode' docs/asciinema/toc-tui.cast
rtk rg --color=never -F 'SHA-256' docs/asciinema/toc-tui.cast
rtk rg --color=never -F 'TRACE' docs/asciinema/toc-tui.cast
rtk rg --color=never -e '\\u001b\\[(32|36|38;5;(10|14))m' docs/asciinema/toc-tui.cast
rtk rg --color=never -F '"o", "exit' docs/asciinema/toc-tui.cast
```

Expected: playback, header read, and the first four searches exit 0; the final `exit` search prints
nothing and exits 1. The header reports 120×30 and `xterm-256color`. Search the raw cast because
asciinema's plain-text converter intentionally omits alternate-screen TUI frames.

### Task 2: Upload and attach the TUI recording

**Files:**
- Modify: `README.md`
- Verify: `docs/asciinema/toc-tui.cast`

**Interfaces:**
- Consumes: the verified TUI cast and the URL returned by asciinema.org
- Produces: one persistent unlisted recording page and one clickable README SVG preview

- [ ] **Step 1: Upload the TUI cast**

Run `asciinema upload --server-url https://asciinema.org --visibility unlisted` for the TUI cast
with title `toc TUI — input, pipeline, and output views`. Retain the exact recording URL printed
by the successful upload. If the CLI reports that the installation is not linked to an account, run
`asciinema auth --server-url https://asciinema.org` and complete that account link before
continuing so the recording is not deleted after seven days. Assign the returned URL to
`TUI_RECORDING_URL` for the remaining checks.

- [ ] **Step 2: Add the TUI preview to README**

Immediately after the `30초 시작` code block, add `## TUI 실행 화면`. Use the official Markdown
image-link form: the link target is the exact recording URL and the image source is that URL with
`.svg` appended. Do not change any other README content.

- [ ] **Step 3: Verify hosted assets and repository changes**

Run:

```bash
rtk curl -fsSI "$TUI_RECORDING_URL"
rtk curl -fsSI "$TUI_RECORDING_URL.svg"
rtk git diff --check
rtk git status --short
```

Expected: both HTTP checks succeed, the diff check exits 0, only the TUI cast and intended README
edit are present, and the user's pre-existing changes remain present.

- [ ] **Step 4: Commit only this task's artifacts**

Stage the TUI cast normally. Use `git add -p README.md` to accept only the new `TUI 실행 화면`
hunk and reject the user's pre-existing README hunk. Stage this plan only if it is not already
committed. Verify the staged diff excludes `.gitignore`, the earlier README edits,
`docs/test-reports/`, and the CLI cast, then commit with:

```bash
rtk git commit -m "docs(readme): TUI 실행 녹화 첨부"
```
