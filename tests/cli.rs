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
fn decoders_report_the_same_bounded_invalid_utf8_details() {
    let expected = format!(
        "decoded bytes are not UTF-8 (hex prefix: {}; bytes omitted; total: 65 bytes)\n",
        "ff".repeat(64)
    );
    let inputs = [
        ("base64-decode", format!("{}//8=", "/".repeat(84))),
        ("url-decode", "%FF".repeat(65)),
        ("hex-decode", "ff".repeat(65)),
    ];

    for (transform, input) in inputs {
        let output = run(&[transform], input.as_bytes());
        assert_eq!(output.status.code(), Some(4), "{transform}");
        assert!(output.stdout.is_empty(), "{transform}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            stderr.strip_prefix(&format!("step 1 ({transform}) failed: ")),
            Some(expected.as_str()),
            "{transform}"
        );
    }
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
fn version_reports_v0_2_0() {
    let output = run(&["--version"], b"");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"doop 0.2.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn list_cannot_be_combined_with_transform_or_tui() {
    for args in [
        &["--list", "format-json", "--input", "data.json"][..],
        &["--list", "tui"][..],
    ] {
        let output = run(args, b"");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
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
fn hex_cli_handles_binary_input_whitespace_chains_and_atomic_errors() {
    let output = run(&["hex-encode"], &[0x00, 0xff, b'A']);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"00ff41");
    assert!(output.stderr.is_empty());

    let output = run(&["hex-decode"], b"48 65\n6C6c6F");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"Hello");
    assert!(output.stderr.is_empty());

    let output = run(&["hex-encode", "--then", "hex-decode"], "한글".as_bytes());
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, "한글".as_bytes());

    for (input, expected) in [
        (
            &b"0x"[..],
            "step 1 (hex-decode) failed: invalid hex character at byte 1\n",
        ),
        (
            &b"0 a f"[..],
            "step 1 (hex-decode) failed: hex input has an odd number of digits: 3\n",
        ),
    ] {
        let output = run(&["hex-decode"], input);
        assert_eq!(output.status.code(), Some(4));
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8(output.stderr).unwrap(), expected);
    }
}

#[test]
fn list_exposes_the_exact_eight_public_transform_ids() {
    let output = run(&["--list"], b"");
    assert_eq!(output.status.code(), Some(0));
    let ids: std::collections::HashSet<_> = std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.split('\t').next().unwrap())
        .collect();
    assert_eq!(
        ids,
        std::collections::HashSet::from([
            "base64-encode",
            "base64-decode",
            "url-encode",
            "url-decode",
            "format-json",
            "minify-json",
            "hex-encode",
            "hex-decode",
        ])
    );
}

#[test]
fn tui_has_explicit_temporary_code_one_path() {
    let output = run(&["tui"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}
