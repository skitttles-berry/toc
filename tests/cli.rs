use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_doop"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn transforms_piped_input_without_final_newline() {
    let output = run(&["base64-encode"], b"hello");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"aGVsbG8=");
    assert!(output.stderr.is_empty());
}

#[test]
fn chains_in_written_order() {
    let output = run(
        &["url-decode", "--then", "format-json"],
        b"%7B%22a%22%3A1%7D",
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"{\n  \"a\": 1\n}");
}

#[test]
fn transform_error_writes_only_stderr_and_code_four() {
    let output = run(&["base64-decode"], b"!");
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn help_version_and_list_are_successful_english_output() {
    for args in [
        &[][..],
        &["--help"][..],
        &["--version"][..],
        &["--list"][..],
    ] {
        let output = run(args, b"");
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
        assert!(output.stdout.is_ascii());
    }
}

#[test]
fn unknown_transform_is_single_usage_error_without_stdout() {
    let output = run(&["unknown-transform"], b"");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.matches("error:").count(), 1);
    assert!(!stderr.contains("Invalid usage"));
}

#[test]
fn piped_input_and_input_path_conflict_with_code_two() {
    let path = std::env::temp_dir().join(format!("doop-cli-input-{}", std::process::id()));
    std::fs::write(&path, b"file").unwrap();
    let output = run(
        &["base64-encode", "--input", path.to_str().unwrap()],
        b"pipe",
    );
    std::fs::remove_file(path).unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn thirty_three_step_chain_is_pipeline_error() {
    let mut args = vec!["base64-encode"];
    for _ in 0..32 {
        args.extend(["--then", "base64-encode"]);
    }
    let output = run(&args, b"");
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn tui_has_explicit_temporary_code_one_path() {
    let output = run(&["tui"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}
