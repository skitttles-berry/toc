# doop project core

- Single Rust package and executable: local CLI + non-destructive terminal workbench; no network transfer.
- Source map: `src/main.rs` entrypoint; `src/cli.rs` parsing/input/output; `src/error.rs` typed errors and safe rendering; `src/pipeline.rs` shared transform execution; `src/transforms/` static registry and eight implementations; `src/tui.rs` terminal lifecycle/TUI (planned split under `src/tui/`); `tests/cli.rs` integration; `tests/shell-smoke.sh` real PTY/shell/clipboard paths.
- Product requirements: `docs/prd/init-prd.md`, `docs/prd/v0.2-prd.md`. Approved TUI design and implementation plan live under `docs/superpowers/specs/` and `docs/superpowers/plans/`.
- Public invariants: exactly eight transform IDs; commands are direct (no `run`/`transform` wrapper); `doop tui` is explicit; one shared registry/pipeline; preserve CLI byte output, errors and exit codes.
- Limits are centralized in `src/lib.rs`: CLI input 64 MiB, CLI output 256 MiB, TUI input 1 MiB/65,536 lines, TUI output 64 MiB, undo 8, pipeline steps 32.
- Read build/dependency pins in `mem:tech_stack`; commands in `mem:suggested_commands`; coding rules in `mem:conventions`; completion gate in `mem:task_completion`.