# toc conventions

- Preserve one shared static transform registry in `src/transforms/mod.rs`; each definition has public ID/name/description/behavior, `accepts_binary`, and function pointer.
- Preserve the public `pipeline::execute(Vec<u8>, &[TransformStep], usize)` signature and strict CLI behavior unless an approved spec explicitly changes it.
- Resource/security boundaries are product contracts: checked allocation, bounded previews, no lossy decode, no terminal control injection, no input/output/clipboard body in user errors or logs, terminal restoration on every exit path.
- TUI runs transforms off the event-loop thread and rejects stale results. Do not add Tokio, plugins, caches, options forms or speculative abstractions.
- Tests are colocated unit tests for internals plus `tests/cli.rs` and Expect PTY smoke. New behavior follows RED→GREEN→REFACTOR; expected values are hand-derived and tests exercise real code.
- Use standard Rust naming/formatting and `cargo fmt`; warnings are denied by Clippy completion checks.
- Keep code/doc diffs minimal; synchronize relevant README/PRD/design content in the same logical feature commit.
- Commit format: Conventional Commits, Korean subject, at most 50 characters, noun-ending; one logical change; use configured git identity; no co-author trailers.
- Do not commit `.env`, `node_modules`, ordinary build output or debug prints. Do not leave production TODO/FIXME markers.
- Shell fallback uses ANSI-free modern tools (`lsd`, `bat`, `rg`, `fd`) per repository instructions.