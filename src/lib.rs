pub mod cli;
pub mod error;
pub mod pipeline;
pub mod transforms;
pub mod tui;

pub const CLI_INPUT_LIMIT: usize = 64 * 1024 * 1024;
pub const CLI_OUTPUT_LIMIT: usize = 256 * 1024 * 1024;
pub const TUI_INPUT_LIMIT: usize = 1024 * 1024;
pub const TUI_INPUT_LINE_LIMIT: usize = 65_536;
pub const TUI_UNDO_HISTORY_LIMIT: usize = 8;
pub const TUI_OUTPUT_LIMIT: usize = 64 * 1024 * 1024;
pub const MAX_STEPS: usize = 32;
