use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use clap::{
    Arg, ArgAction, Command, builder::PossibleValuesParser, error::ErrorKind, value_parser,
};

use crate::{
    error::{AppError, InputError, escape_external, hex_preview},
    transforms::{TransformDefinition, transform_by_id, transforms},
};

pub enum Invocation {
    List,
    Transform {
        first: &'static TransformDefinition,
        then: Vec<&'static TransformDefinition>,
        input: Option<PathBuf>,
    },
    Tui,
}

pub enum ParseOutcome {
    Run(Invocation),
    Print {
        text: String,
        stderr: bool,
        exit_code: i32,
    },
}

pub fn read_limited(reader: &mut dyn Read, limit: usize) -> Result<Vec<u8>, InputError> {
    let mut bytes = Vec::new();
    let read_limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    reader
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| InputError::Read)?;
    if bytes.len() > limit {
        return Err(InputError::TooLarge { limit });
    }
    Ok(bytes)
}

pub fn read_input(
    path: Option<&Path>,
    stdin: &mut dyn Read,
    stdin_is_terminal: bool,
    limit: usize,
) -> Result<Vec<u8>, InputError> {
    match (path, stdin_is_terminal) {
        (Some(_), false) => Err(InputError::ConflictingSources),
        (None, true) => Err(InputError::MissingSource),
        (Some(path), true) => {
            let mut file = File::open(path).map_err(|_| InputError::OpenFile {
                path: path.to_string_lossy().into_owned(),
            })?;
            read_limited(&mut file, limit)
        }
        (None, false) => read_limited(stdin, limit),
    }
}

pub fn write_result(
    writer: &mut dyn Write,
    stdout_is_terminal: bool,
    result: &[u8],
) -> Result<(), AppError> {
    if stdout_is_terminal {
        match std::str::from_utf8(result) {
            Ok(text) if crate::error::contains_dangerous_control(text) => {
                return Err(AppError::UnsafeTerminalOutput {
                    preview: escape_external(text, 64),
                });
            }
            Err(_) => {
                return Err(AppError::UnsafeTerminalOutput {
                    preview: format!("hex prefix: {}", hex_preview(result)),
                });
            }
            _ => {}
        }
    }
    match writer.write_all(result).and_then(|()| writer.flush()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(_) => Err(AppError::Output),
    }
}

pub fn run_transform(
    first: &'static TransformDefinition,
    then: &[&'static TransformDefinition],
    path: Option<&Path>,
    stdin: &mut dyn Read,
    stdin_is_terminal: bool,
    stdout: &mut dyn Write,
    stdout_is_terminal: bool,
) -> Result<(), AppError> {
    if then.len() >= crate::MAX_STEPS {
        return Err(AppError::Pipeline(
            crate::error::PipelineError::TooManySteps {
                max: crate::MAX_STEPS,
            },
        ));
    }
    let input = read_input(path, stdin, stdin_is_terminal, crate::CLI_INPUT_LIMIT)
        .map_err(AppError::Input)?;
    let mut steps = Vec::with_capacity(then.len().saturating_add(1));
    steps.push(crate::pipeline::TransformStep {
        definition: first,
        enabled: true,
    });
    steps.extend(
        then.iter()
            .map(|definition| crate::pipeline::TransformStep {
                definition,
                enabled: true,
            }),
    );
    let result = crate::pipeline::execute_allow_binary(input, &steps, crate::CLI_OUTPUT_LIMIT)
        .map_err(AppError::Pipeline)?;
    write_result(stdout, stdout_is_terminal, &result)
}

pub fn command() -> Command {
    let ids = || transforms().iter().map(|transform| transform.id);
    let mut command = Command::new("toc")
        .version(env!("CARGO_PKG_VERSION"))
        .about("TUI Object Converter")
        .after_help("Transform help: toc <transform-id> --help")
        .disable_help_subcommand(true)
        .args_conflicts_with_subcommands(true)
        .arg(
            Arg::new("list")
                .long("list")
                .help("List available transforms")
                .action(ArgAction::SetTrue)
                .exclusive(true),
        )
        .subcommand(
            Command::new("tui")
                .about("Open the non-destructive transform workbench")
                .disable_help_flag(true),
        );

    for transform in transforms() {
        let input_help = if transform.accepts_binary {
            "Input: arbitrary bytes."
        } else {
            "Input: UTF-8 text."
        };
        command = command.subcommand(
            Command::new(transform.id)
                .about(transform.description)
                .after_help(format!(
                    "{input_help} Use exactly one of stdin or --input PATH.\nBehavior: {}.",
                    transform.behavior
                ))
                .arg(
                    Arg::new("input")
                        .long("input")
                        .value_name("PATH")
                        .value_parser(value_parser!(PathBuf)),
                )
                .arg(
                    Arg::new("then")
                        .long("then")
                        .value_name("TRANSFORM")
                        .action(ArgAction::Append)
                        .value_parser(PossibleValuesParser::new(ids())),
                ),
        );
    }
    command
}

pub fn parse_from<I, T>(args: I) -> ParseOutcome
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.len() > 2 && args[1].as_os_str() == OsStr::new("tui") {
        return ParseOutcome::Print {
            text: "error: tui does not accept trailing arguments\n".to_owned(),
            stderr: true,
            exit_code: 2,
        };
    }
    if args.len() == 1 {
        let mut command = command();
        return ParseOutcome::Print {
            text: command.render_help().to_string(),
            stderr: false,
            exit_code: 0,
        };
    }

    let matches = match command().try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => {
            let display = matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            );
            return ParseOutcome::Print {
                text: error.to_string(),
                stderr: !display,
                exit_code: if display { 0 } else { 2 },
            };
        }
    };

    if matches.get_flag("list") {
        return ParseOutcome::Run(Invocation::List);
    }
    let Some((name, submatches)) = matches.subcommand() else {
        unreachable!("root command without arguments is handled before Clap");
    };
    if name == "tui" {
        return ParseOutcome::Run(Invocation::Tui);
    }

    let first = transform_by_id(name).expect("Clap subcommands come from the registry");
    let then = submatches
        .get_many::<String>("then")
        .into_iter()
        .flatten()
        .map(|id| transform_by_id(id).expect("Clap values come from the registry"))
        .collect();
    let input = submatches.get_one::<PathBuf>("input").cloned();
    ParseOutcome::Run(Invocation::Transform { first, then, input })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PanicReader;

    impl std::io::Read for PanicReader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("step validation must happen before input is read");
        }
    }

    #[test]
    fn too_many_steps_does_not_read_input() {
        let first = crate::transforms::transform_by_id("base64-encode").unwrap();
        let then = vec![first; crate::MAX_STEPS];
        let mut stdin = PanicReader;
        let mut stdout = Vec::new();

        let error =
            run_transform(first, &then, None, &mut stdin, false, &mut stdout, false).unwrap_err();

        assert!(matches!(
            error,
            AppError::Pipeline(crate::error::PipelineError::TooManySteps {
                max: crate::MAX_STEPS
            })
        ));
        assert!(stdout.is_empty());
    }

    #[test]
    fn requires_exactly_one_input_source() {
        let mut stdin = std::io::Cursor::new(b"pipe");
        assert!(matches!(
            read_input(Some(std::path::Path::new("x")), &mut stdin, false, 64),
            Err(InputError::ConflictingSources)
        ));
        assert!(matches!(
            read_input(None, &mut stdin, true, 64),
            Err(InputError::MissingSource)
        ));
    }

    #[test]
    fn reads_only_limit_plus_one_byte() {
        let mut input = std::io::Cursor::new(b"123456789");
        assert_eq!(
            read_limited(&mut input, 4).unwrap_err(),
            InputError::TooLarge { limit: 4 }
        );
        assert_eq!(input.position(), 5);
    }

    #[test]
    fn maximum_limit_does_not_overflow() {
        let mut input = std::io::Cursor::new(b"x");
        assert_eq!(read_limited(&mut input, usize::MAX).unwrap(), b"x");
    }

    #[test]
    fn reads_file_when_stdin_is_a_terminal() {
        let path = std::env::temp_dir().join(format!("toc-input-{}", std::process::id()));
        std::fs::write(&path, b"file").unwrap();
        let mut stdin = std::io::Cursor::new(Vec::<u8>::new());
        let result = read_input(Some(&path), &mut stdin, true, 64).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(result, b"file");
    }

    #[test]
    fn terminal_output_refuses_escape_but_pipe_output_preserves_it() {
        let result = b"x\x1b[2J";
        let mut terminal = Vec::new();
        let error = write_result(&mut terminal, true, result).unwrap_err();
        let rendered = crate::error::render_app_error(&error);
        assert!(matches!(error, AppError::UnsafeTerminalOutput { .. }));
        assert!(rendered.contains("x\\x1b[2J"));
        assert!(rendered.contains("redirect stdout"));
        assert!(!rendered.contains('\u{1b}'));
        assert!(terminal.is_empty());
        assert!(matches!(
            write_result(&mut terminal, true, "\u{85}".as_bytes()),
            Err(AppError::UnsafeTerminalOutput { .. })
        ));
        let error = write_result(&mut terminal, true, b"\x9b").unwrap_err();
        let rendered = crate::error::render_app_error(&error);
        assert!(matches!(error, AppError::UnsafeTerminalOutput { .. }));
        assert!(rendered.contains("hex prefix: 9b"));
        assert!(rendered.contains("redirect stdout"));
        assert!(terminal.is_empty());

        let mut pipe = Vec::new();
        write_result(&mut pipe, false, result).unwrap();
        assert_eq!(pipe, result);
    }

    struct FailingWriter(std::io::ErrorKind);

    impl std::io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(self.0))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn broken_pipe_is_success_but_other_output_failure_is_code_five() {
        write_result(
            &mut FailingWriter(std::io::ErrorKind::BrokenPipe),
            false,
            b"x",
        )
        .unwrap();
        let error =
            write_result(&mut FailingWriter(std::io::ErrorKind::Other), false, b"x").unwrap_err();
        assert_eq!(error.exit_code(), 5);
    }

    #[test]
    fn buffered_flush_failure_is_output_error_with_code_five() {
        let mut writer = std::io::BufWriter::new(FailingWriter(std::io::ErrorKind::Other));
        let error = write_result(&mut writer, false, b"x").unwrap_err();
        assert!(matches!(&error, AppError::Output));
        assert_eq!(error.exit_code(), 5);
    }

    #[test]
    fn buffered_flush_broken_pipe_is_success() {
        let mut writer = std::io::BufWriter::new(FailingWriter(std::io::ErrorKind::BrokenPipe));
        write_result(&mut writer, false, b"x").unwrap();
    }

    #[test]
    fn failed_pipeline_writes_no_stdout() {
        let mut stdin = std::io::Cursor::new(b"!");
        let mut stdout = Vec::new();
        let error = run_transform(
            crate::transforms::transform_by_id("base64-decode").unwrap(),
            &[],
            None,
            &mut stdin,
            false,
            &mut stdout,
            false,
        )
        .unwrap_err();
        assert_eq!(error.exit_code(), 4);
        assert!(stdout.is_empty());
    }

    #[test]
    fn no_arguments_prints_root_help_to_stdout_with_success() {
        let ParseOutcome::Print {
            text,
            stderr,
            exit_code,
        } = parse_from(["toc"])
        else {
            panic!("expected printable help");
        };
        assert!(!stderr);
        assert_eq!(exit_code, 0);
        assert!(text.contains("Usage:"));
        assert!(text.contains("tui"));
        assert!(text.contains("toc <transform-id> --help"));
    }

    #[test]
    fn parses_direct_transform_and_repeated_then_steps() {
        let ParseOutcome::Run(Invocation::Transform { first, then, input }) = parse_from([
            "toc",
            "url-decode",
            "--input",
            "data.txt",
            "--then",
            "format-json",
            "--then",
            "minify-json",
        ]) else {
            panic!("expected transform invocation");
        };
        assert_eq!(first.id, "url-decode");
        assert_eq!(
            then.iter().map(|step| step.id).collect::<Vec<_>>(),
            ["format-json", "minify-json"]
        );
        assert_eq!(input.unwrap(), std::path::PathBuf::from("data.txt"));
    }

    #[test]
    fn accepts_only_exact_tui_invocation() {
        assert!(matches!(
            parse_from(["toc", "tui"]),
            ParseOutcome::Run(Invocation::Tui)
        ));
        let ParseOutcome::Print {
            text,
            stderr,
            exit_code,
        } = parse_from(["toc", "tui", "--"])
        else {
            panic!("expected tui trailing token rejection");
        };
        assert!(stderr);
        assert_eq!(exit_code, 2);
        assert!(!text.is_empty());
        assert!(text.contains("does not accept trailing arguments"));
        assert!(!text.contains('\x1b'));
        for args in [
            vec!["toc", "tui", "--help"],
            vec!["toc", "tui", "format-json"],
            vec!["toc", "tui", "--input", "x"],
        ] {
            assert!(matches!(
                parse_from(args),
                ParseOutcome::Print {
                    stderr: true,
                    exit_code: 2,
                    ..
                }
            ));
        }
    }

    #[test]
    fn exposes_help_version_list_and_transform_help() {
        for args in [
            vec!["toc", "--help"],
            vec!["toc", "--version"],
            vec!["toc", "format-json", "--help"],
        ] {
            assert!(matches!(
                parse_from(args),
                ParseOutcome::Print {
                    stderr: false,
                    exit_code: 0,
                    ..
                }
            ));
        }
        assert!(matches!(
            parse_from(["toc", "--list"]),
            ParseOutcome::Run(Invocation::List)
        ));
    }

    #[test]
    fn transform_help_describes_input_and_fixed_behavior() {
        for (id, expected) in [
            ("base64-encode", "padded RFC 4648 Base64"),
            ("base64-decode", "ASCII whitespace"),
            ("url-encode", "uppercase %HH"),
            ("url-decode", "leaves plus signs unchanged"),
            ("format-json", "two-space indentation"),
            ("minify-json", "outside strings"),
        ] {
            let ParseOutcome::Print {
                text,
                stderr,
                exit_code,
            } = parse_from(["toc", id, "--help"])
            else {
                panic!("expected transform help");
            };
            assert!(!stderr);
            assert_eq!(exit_code, 0);
            assert!(text.contains("Use exactly one of stdin or --input PATH."));
            assert!(text.contains(expected), "{id} help was: {text}");
        }
    }

    #[test]
    fn hex_help_comes_from_registry_binary_and_behavior_metadata() {
        for (id, input_help, behavior) in [
            (
                "hex-encode",
                "Input: arbitrary bytes.",
                "lowercase hexadecimal",
            ),
            (
                "hex-decode",
                "Input: UTF-8 text.",
                "ASCII space, tab, CR, and LF",
            ),
        ] {
            let ParseOutcome::Print {
                text,
                stderr,
                exit_code,
            } = parse_from(["toc", id, "--help"])
            else {
                panic!("expected transform help");
            };
            assert!(!stderr);
            assert_eq!(exit_code, 0);
            assert!(text.contains(input_help), "{id} help was: {text}");
            assert!(text.contains(behavior), "{id} help was: {text}");
        }
    }

    #[test]
    fn has_no_run_transform_chain_script_or_doctor_command() {
        for name in ["run", "transform", "chain", "script", "doctor"] {
            assert!(matches!(
                parse_from(["toc", name]),
                ParseOutcome::Print {
                    stderr: true,
                    exit_code: 2,
                    ..
                }
            ));
        }
    }
}
