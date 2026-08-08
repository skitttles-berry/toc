#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformError {
    InvalidUtf8Input,
    InvalidBase64 {
        position: Option<usize>,
    },
    InvalidBase32 {
        position: Option<usize>,
    },
    InvalidUrl {
        position: usize,
    },
    InvalidHex {
        position: usize,
    },
    OddHexDigitCount {
        digits: usize,
    },
    InvalidUtf8Output {
        preview_hex: String,
        total_bytes: usize,
    },
    InvalidJson {
        line: usize,
        column: usize,
        kind: JsonErrorKind,
    },
    InvalidJwtPart,
    OutputTooLarge {
        limit: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    TooManySteps {
        max: usize,
    },
    Step {
        step: usize,
        transform_id: &'static str,
        source: TransformError,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum InputError {
    MissingSource,
    ConflictingSources,
    TooLarge { limit: usize },
    OpenFile { path: String },
    Read,
}

#[derive(Debug)]
pub enum AppError {
    Usage,
    Input(InputError),
    Pipeline(PipelineError),
    UnsafeTerminalOutput { preview: String },
    Output,
    Tui(String),
    Interrupted,
}

impl AppError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Tui(_) => 1,
            Self::Usage
            | Self::Input(InputError::MissingSource | InputError::ConflictingSources) => 2,
            Self::Input(_) => 3,
            Self::Pipeline(_) | Self::UnsafeTerminalOutput { .. } => 4,
            Self::Output => 5,
            Self::Interrupted => 130,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonErrorKind {
    Syntax,
    DuplicateKey,
    Bom,
    DepthExceeded,
}

pub fn is_dangerous_control(character: char) -> bool {
    let code = character as u32;
    (code <= 0x1f && !matches!(character, '\t' | '\n' | '\r')) || (0x7f..=0x9f).contains(&code)
}

pub fn contains_dangerous_control(text: &str) -> bool {
    text.chars().any(is_dangerous_control)
}

pub fn escape_controls(text: &str, max_chars: usize) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    for character in text.chars().take(max_chars) {
        if is_dangerous_control(character) {
            write!(&mut output, "\\x{:02x}", character as u32)
                .expect("writing to String cannot fail");
        } else {
            output.push(character);
        }
    }
    output
}

pub fn escape_external(text: &str, max_chars: usize) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    for character in text.chars().take(max_chars) {
        if character.is_control() {
            let code = character as u32;
            if code <= 0xff {
                write!(&mut output, "\\x{code:02x}").expect("writing to String cannot fail");
            } else {
                write!(&mut output, "\\u{{{code:x}}}").expect("writing to String cannot fail");
            }
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) fn hex_preview(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut preview = String::with_capacity(bytes.len().min(64) * 2);
    for byte in bytes.iter().take(64) {
        write!(&mut preview, "{byte:02x}").expect("writing to String cannot fail");
    }
    preview
}

pub(crate) fn invalid_utf8_output(bytes: &[u8]) -> TransformError {
    TransformError::InvalidUtf8Output {
        preview_hex: hex_preview(bytes),
        total_bytes: bytes.len(),
    }
}

fn render_transform_error(error: &TransformError) -> String {
    match error {
        TransformError::InvalidUtf8Input => "input is not valid UTF-8".to_string(),
        TransformError::InvalidBase64 {
            position: Some(position),
        } => {
            format!("invalid Base64 at byte {position}")
        }
        TransformError::InvalidBase64 { position: None } => "invalid Base64 padding".to_string(),
        TransformError::InvalidBase32 {
            position: Some(position),
        } => {
            format!("invalid Base32 at byte {position}")
        }
        TransformError::InvalidBase32 { position: None } => "invalid Base32 padding".to_string(),
        TransformError::InvalidUrl { position } => {
            format!("invalid percent escape at byte {position}")
        }
        TransformError::InvalidHex { position } => {
            format!("invalid hex character at byte {position}")
        }
        TransformError::OddHexDigitCount { digits } => {
            format!("hex input has an odd number of digits: {digits}")
        }
        TransformError::InvalidUtf8Output {
            preview_hex,
            total_bytes,
        } => {
            if *total_bytes > 64 {
                format!(
                    "decoded bytes are not UTF-8 (hex prefix: {preview_hex}; bytes omitted; total: {total_bytes} bytes)"
                )
            } else {
                format!("decoded bytes are not UTF-8 (hex prefix: {preview_hex})")
            }
        }
        TransformError::InvalidJson { line, column, kind } => {
            let reason = match kind {
                JsonErrorKind::Syntax => "invalid JSON syntax",
                JsonErrorKind::DuplicateKey => "duplicate JSON object key",
                JsonErrorKind::Bom => "UTF-8 BOM is not allowed",
                JsonErrorKind::DepthExceeded => "JSON depth exceeds 128",
            };
            format!("{reason} at line {line}, column {column}")
        }
        TransformError::InvalidJwtPart => "invalid JWT part".to_string(),
        TransformError::OutputTooLarge { limit } => {
            format!("transform output exceeds {limit} bytes")
        }
    }
}

pub fn render_pipeline_error(error: &PipelineError) -> String {
    match error {
        PipelineError::TooManySteps { max } => {
            format!("chain exceeds {max} steps")
        }
        PipelineError::Step {
            step,
            transform_id,
            source,
        } => format!(
            "step {step} ({transform_id}) failed: {}",
            render_transform_error(source)
        ),
    }
}

pub fn render_app_error(error: &AppError) -> String {
    match error {
        AppError::Usage => "Invalid usage".to_string(),
        AppError::Input(InputError::MissingSource) => "Provide stdin or --input PATH".to_string(),
        AppError::Input(InputError::ConflictingSources) => {
            "Use stdin or --input PATH, not both".to_string()
        }
        AppError::Input(InputError::TooLarge { limit }) => {
            format!("Input exceeds {limit} bytes")
        }
        AppError::Input(InputError::OpenFile { path }) => {
            format!("Could not open input file: {}", escape_external(path, 256))
        }
        AppError::Input(InputError::Read) => "Could not read input".to_string(),
        AppError::Pipeline(error) => render_pipeline_error(error),
        AppError::UnsafeTerminalOutput { preview } => {
            format!(
                "Refusing unsafe terminal output (preview: {}); redirect stdout to preserve raw output",
                escape_external(preview, 256)
            )
        }
        AppError::Output => "Could not write output".to_string(),
        AppError::Tui(message) => {
            format!("TUI error: {}", escape_external(message, 512))
        }
        AppError::Interrupted => "Interrupted".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_and_escapes_dangerous_terminal_controls() {
        assert!(!contains_dangerous_control("a\t\n\r"));
        assert!(contains_dangerous_control("a\u{1b}[2J"));
        assert_eq!(escape_controls("a\u{1b}\u{85}", 32), "a\\x1b\\x85");
        assert_eq!(escape_external("a\n\u{1b}", 32), "a\\x0a\\x1b");
    }

    #[test]
    fn maps_error_categories_to_public_exit_codes() {
        assert_eq!(AppError::Usage.exit_code(), 2);
        assert_eq!(AppError::Input(InputError::MissingSource).exit_code(), 2);
        assert_eq!(
            AppError::Input(InputError::ConflictingSources).exit_code(),
            2
        );
        assert_eq!(
            AppError::Input(InputError::TooLarge { limit: 1 }).exit_code(),
            3
        );
        assert_eq!(
            AppError::Pipeline(PipelineError::TooManySteps { max: 32 }).exit_code(),
            4
        );
        assert_eq!(
            AppError::UnsafeTerminalOutput {
                preview: String::new()
            }
            .exit_code(),
            4
        );
        assert_eq!(AppError::Output.exit_code(), 5);
        assert_eq!(AppError::Tui("terminal unavailable".into()).exit_code(), 1);
        assert_eq!(AppError::Interrupted.exit_code(), 130);
    }

    #[test]
    fn external_error_text_cannot_inject_terminal_lines_or_escapes() {
        let rendered = render_app_error(&AppError::Tui("failed\n\u{1b}[2J".to_string()));
        assert_eq!(rendered, "TUI error: failed\\x0a\\x1b[2J");
        assert!(!rendered.contains('\n'));
        assert!(!rendered.contains('\u{1b}'));
    }

    #[test]
    fn pipeline_invalid_utf8_error_reports_omission_and_total_bytes_for_both_decoders() {
        let base64 = crate::transforms::transform_by_id("base64-decode").unwrap();
        let encoded = (crate::transforms::transform_by_id("base64-encode")
            .unwrap()
            .apply)(&[0xff; 65], 1024)
        .unwrap();
        let steps = [crate::pipeline::TransformStep {
            definition: base64,
            enabled: true,
        }];
        let rendered =
            render_pipeline_error(&crate::pipeline::execute(encoded, &steps, 1024).unwrap_err());
        assert!(rendered.contains("bytes omitted"));
        assert!(rendered.contains("total: 65 bytes"));

        let url = crate::transforms::transform_by_id("url-decode").unwrap();
        let steps = [crate::pipeline::TransformStep {
            definition: url,
            enabled: true,
        }];
        let rendered = render_pipeline_error(
            &crate::pipeline::execute("%FF".repeat(65).into_bytes(), &steps, 1024).unwrap_err(),
        );
        assert!(rendered.contains("bytes omitted"));
        assert!(rendered.contains("total: 65 bytes"));
    }

    #[test]
    fn invalid_utf8_output_bounds_the_preview_to_sixty_four_bytes() {
        assert_eq!(
            invalid_utf8_output(&[0xff; 65]),
            TransformError::InvalidUtf8Output {
                preview_hex: "ff".repeat(64),
                total_bytes: 65,
            }
        );
    }

    #[test]
    fn pipeline_errors_use_fixed_categories_without_input_content() {
        assert_eq!(
            render_pipeline_error(&PipelineError::TooManySteps { max: 32 }),
            "chain exceeds 32 steps"
        );
        assert_eq!(
            render_pipeline_error(&PipelineError::Step {
                step: 2,
                transform_id: "format-json",
                source: TransformError::InvalidJson {
                    line: 1,
                    column: 4,
                    kind: JsonErrorKind::DuplicateKey,
                },
            }),
            "step 2 (format-json) failed: duplicate JSON object key at line 1, column 4"
        );
    }

    #[test]
    fn renders_hex_format_errors_with_exact_positions_and_counts() {
        assert_eq!(
            render_pipeline_error(&PipelineError::Step {
                step: 2,
                transform_id: "hex-decode",
                source: TransformError::InvalidHex { position: 7 },
            }),
            "step 2 (hex-decode) failed: invalid hex character at byte 7"
        );
        assert_eq!(
            render_pipeline_error(&PipelineError::Step {
                step: 1,
                transform_id: "hex-decode",
                source: TransformError::OddHexDigitCount { digits: 3 },
            }),
            "step 1 (hex-decode) failed: hex input has an odd number of digits: 3"
        );
    }

    #[test]
    fn renders_base32_errors_without_input_content() {
        assert_eq!(
            render_pipeline_error(&PipelineError::Step {
                step: 1,
                transform_id: "base32-decode",
                source: TransformError::InvalidBase32 { position: Some(7) },
            }),
            "step 1 (base32-decode) failed: invalid Base32 at byte 7"
        );
        assert_eq!(
            render_pipeline_error(&PipelineError::Step {
                step: 1,
                transform_id: "base32-decode",
                source: TransformError::InvalidBase32 { position: None },
            }),
            "step 1 (base32-decode) failed: invalid Base32 padding"
        );
    }

    #[test]
    fn renders_jwt_part_errors_without_token_content() {
        assert_eq!(
            render_pipeline_error(&PipelineError::Step {
                step: 1,
                transform_id: "jwt-decode",
                source: TransformError::InvalidJwtPart,
            }),
            "step 1 (jwt-decode) failed: invalid JWT part"
        );
    }

    #[test]
    fn file_path_is_sanitized_and_bounded_at_render_boundary() {
        let rendered = render_app_error(&AppError::Input(InputError::OpenFile {
            path: format!("bad\n\u{1b}{}", "x".repeat(300)),
        }));
        assert!(rendered.starts_with("Could not open input file: bad\\x0a\\x1b"));
        assert!(!rendered.contains('\n'));
        assert!(!rendered.contains('\u{1b}'));
        assert!(rendered.len() < 300);
    }
}
