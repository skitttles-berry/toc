use std::io::{self, IsTerminal as _, Write as _};

use doop::{
    cli::{Invocation, ParseOutcome},
    error::{AppError, escape_external},
    transforms::transforms,
};

fn render_list() -> String {
    let mut output = String::new();
    for transform in transforms() {
        output.push_str(transform.id);
        output.push('\t');
        output.push_str(transform.display_name);
        output.push('\t');
        output.push_str(transform.description);
        output.push('\n');
    }
    output
}

fn run() -> Result<(), AppError> {
    match doop::cli::parse_from(std::env::args_os()) {
        ParseOutcome::Print {
            text,
            stderr,
            exit_code,
        } => {
            if stderr {
                let safe = escape_external(&text, 4096);
                let _ = writeln!(io::stderr().lock(), "{safe}");
            } else {
                let stdout_is_terminal = io::stdout().is_terminal();
                doop::cli::write_result(
                    &mut io::stdout().lock(),
                    stdout_is_terminal,
                    text.as_bytes(),
                )?;
            }
            if exit_code == 0 {
                Ok(())
            } else {
                Err(AppError::Usage(text))
            }
        }
        ParseOutcome::Run(Invocation::List) => {
            let output = render_list();
            let stdout_is_terminal = io::stdout().is_terminal();
            doop::cli::write_result(
                &mut io::stdout().lock(),
                stdout_is_terminal,
                output.as_bytes(),
            )
        }
        ParseOutcome::Run(Invocation::Transform { first, then, input }) => {
            let stdin_is_terminal = io::stdin().is_terminal();
            let stdout_is_terminal = io::stdout().is_terminal();
            let mut stdin = io::stdin().lock();
            let mut stdout = io::stdout().lock();
            doop::cli::run_transform(
                first,
                &then,
                input.as_deref(),
                &mut stdin,
                stdin_is_terminal,
                &mut stdout,
                stdout_is_terminal,
            )
        }
        ParseOutcome::Run(Invocation::Tui) => {
            doop::tui::check_terminal_entry(io::stdin().is_terminal(), io::stdout().is_terminal())?;
            match doop::tui::run()? {
                0 => Ok(()),
                130 => Err(AppError::Interrupted),
                code => Err(AppError::Tui(format!("TUI exited with code {code}"))),
            }
        }
    }
}

fn main() {
    if let Err(error) = run() {
        let code = error.exit_code();
        if !matches!(error, AppError::Usage(_)) {
            let message = doop::error::render_app_error(&error);
            let _ = writeln!(io::stderr().lock(), "{message}");
        }
        std::process::exit(code);
    }
}
