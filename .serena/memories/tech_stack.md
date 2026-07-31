# toc tech stack

- Rust stable pinned by `rust-toolchain.toml`/Cargo metadata to 1.97.1; edition 2024; package version 0.2.0.
- Build/package manager: Cargo with committed `Cargo.lock`; licenses MIT OR Apache-2.0.
- CLI: Clap 4.6.4.
- TUI: Ratatui 0.30.2 + Crossterm 0.29.0 + `tui-textarea-2` 0.12.1.
- Clipboard: arboard 3.6.1, default features disabled, Wayland data-control enabled.
- Transforms/data: base64 0.23.0, serde 1.0.229, serde_json 1.0.151.
- Text rendering: unicode-segmentation 1.13.3, unicode-width 0.2.2.
- Platform verification: macOS and Linux; Bash and Zsh; Expect-driven PTY smoke; optional macOS/X11/Wayland clipboard paths.
- No async runtime or Tokio. Avoid new dependencies unless explicitly approved.