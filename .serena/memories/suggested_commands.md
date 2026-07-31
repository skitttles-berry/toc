# toc suggested commands

## Run
- CLI example: `printf 'hello' | cargo run -- base64-encode`
- Chained CLI: `printf 'hello' | cargo run -- hex-encode --then hex-decode`
- TUI: `cargo run -- tui`
- List/help: `cargo run -- --list`, `cargo run -- --help`

## Focused checks
- Single test/filter: `cargo test <one-filter> -- --nocapture` (Cargo accepts only one TESTNAME filter).
- CLI integration: `cargo test --test cli`.
- TUI module tests: `cargo test tui::`.

## Full local checks
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture`
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features`
- `cargo package --locked` (use `--allow-dirty` only for a documented pre-commit candidate check)
- `bash tests/shell-smoke.sh`
- `zsh tests/shell-smoke.sh`
- `cargo install --locked --offline --path . --root <fresh-temp-root>` then `<fresh-temp-root>/bin/toc --version`

## Code search/index
- `ccc search <semantic query>`; `ccc index` after significant code/module changes. If uninitialized: `ccc init`, then `ccc index`.

## Darwin notes
- Real macOS clipboard smoke is opt-in: `TOC_SMOKE_CLIPBOARD_MODE=macos bash tests/shell-smoke.sh`; script must back up, restore and verify the existing text clipboard.
- Use repository-required safe CLI forms for shell fallback: `lsd --color=never --icon=never`, `bat --plain --color=never`, `rg --color=never`, `fd --color=never`.