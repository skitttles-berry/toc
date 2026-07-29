use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use clap::{
    Arg, ArgAction, Command, builder::PossibleValuesParser, error::ErrorKind, value_parser,
};

use crate::transforms::{TransformDefinition, transform_by_id, transforms};

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

pub fn command() -> Command {
    let ids = || transforms().iter().map(|transform| transform.id);
    let mut command = Command::new("doop")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Local text transformations")
        .disable_help_subcommand(true)
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
        command = command.subcommand(
            Command::new(transform.id)
                .about(transform.description)
                .after_help(if transform.accepts_binary {
                    "Input: arbitrary bytes. Use exactly one of stdin or --input PATH."
                } else {
                    "Input: UTF-8 text. Use exactly one of stdin or --input PATH."
                })
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

    #[test]
    fn no_arguments_prints_root_help_to_stdout_with_success() {
        let ParseOutcome::Print {
            text,
            stderr,
            exit_code,
        } = parse_from(["doop"])
        else {
            panic!("expected printable help");
        };
        assert!(!stderr);
        assert_eq!(exit_code, 0);
        assert!(text.contains("Usage:"));
        assert!(text.contains("tui"));
    }

    #[test]
    fn parses_direct_transform_and_repeated_then_steps() {
        let ParseOutcome::Run(Invocation::Transform { first, then, input }) = parse_from([
            "doop",
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
            parse_from(["doop", "tui"]),
            ParseOutcome::Run(Invocation::Tui)
        ));
        let ParseOutcome::Print {
            text,
            stderr,
            exit_code,
        } = parse_from(["doop", "tui", "--"])
        else {
            panic!("expected tui trailing token rejection");
        };
        assert!(stderr);
        assert_eq!(exit_code, 2);
        assert!(!text.is_empty());
        assert!(text.contains("does not accept trailing arguments"));
        assert!(!text.contains('\x1b'));
        for args in [
            vec!["doop", "tui", "--help"],
            vec!["doop", "tui", "format-json"],
            vec!["doop", "tui", "--input", "x"],
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
            vec!["doop", "--help"],
            vec!["doop", "--version"],
            vec!["doop", "format-json", "--help"],
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
            parse_from(["doop", "--list"]),
            ParseOutcome::Run(Invocation::List)
        ));
    }

    #[test]
    fn has_no_run_transform_chain_script_or_doctor_command() {
        for name in ["run", "transform", "chain", "script", "doctor"] {
            assert!(matches!(
                parse_from(["doop", name]),
                ParseOutcome::Print {
                    stderr: true,
                    exit_code: 2,
                    ..
                }
            ));
        }
    }
}
