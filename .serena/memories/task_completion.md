# toc task completion gate

For a coding task, obtain fresh output for all relevant focused tests and then run:

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all-targets --all-features`
4. `cargo test --release dirty_redraw_release_measurement -- --ignored --nocapture` for TUI/render changes
5. `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features`
6. `cargo package --locked` on a clean tree, or `--allow-dirty` only for the documented candidate phase
7. `bash tests/shell-smoke.sh` and `zsh tests/shell-smoke.sh` for CLI/TUI/platform-facing changes
8. Fresh locked offline install to a temporary root, then `toc --version`
9. `git diff --check` and `git status --short --branch`
10. `ccc index` after significant source/module changes

Additional gates:
- Confirm TDD evidence: each new behavior’s test was observed failing for the expected missing behavior before production code.
- Preserve CLI compatibility with `cargo test --test cli` when shared transforms/pipeline change.
- Run independent task review and final whole-diff review for planned feature work; fix Critical/Important findings before completion.
- Update README/PRD/design in the same logical feature commit when observable behavior changes.
- Only report platforms/clipboard modes actually executed; unavailable environments remain explicitly unverified.