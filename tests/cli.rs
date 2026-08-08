use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_toc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

const TRANSFORM_IDS: &[&str] = &[
    "base64-encode",
    "base64-decode",
    "url-encode",
    "url-decode",
    "format-json",
    "minify-json",
    "hex-encode",
    "hex-decode",
    "base64url-encode",
    "base64url-decode",
    "base32-encode",
    "base32-decode",
    "html-encode",
    "html-decode",
    "rot13",
    "url-defang",
    "url-refang",
    "jwt-decode",
    "sha256",
    "sha512",
    "gzip-compress",
    "gzip-decompress",
    "sort-lines",
    "remove-duplicate-lines",
];

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
fn newly_registered_transforms_run_directly_and_in_then_chains() {
    let output = run(&["rot13", "--then", "html-encode"], b"<N>");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"&lt;A&gt;");
    assert!(output.stderr.is_empty());
}

#[test]
fn transform_error_writes_only_stderr_and_code_four() {
    let output = run(&["base64-decode"], b"!");
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn piped_stdout_preserves_raw_non_utf8_and_control_bytes() {
    let output = run(&["base64-decode"], b"/wAb");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, [0xff, 0x00, 0x1b]);
    assert!(output.stderr.is_empty());
}

#[test]
fn binary_intermediate_reaches_a_byte_accepting_next_step() {
    let output = run(&["hex-decode", "--then", "base64-encode"], b"ff001b");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"/wAb");
    assert!(output.stderr.is_empty());
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
        if args.is_empty() || args == ["--help"] {
            let help = std::str::from_utf8(&output.stdout).unwrap();
            for token in [
                "TUI Object Converter",
                "Usage: toc [OPTIONS]",
                "Commands:",
                "tui",
                "--list",
                "Transform help: toc <transform-id> --help",
            ] {
                assert!(help.contains(token), "{args:?}: {token}");
            }
        }
    }
}

#[test]
fn version_reports_toc_v0_2_1() {
    let output = run(&["--version"], b"");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"toc 0.2.1\n");
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
    let path = std::env::temp_dir().join(format!("toc-cli-input-{}", std::process::id()));
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
fn list_exposes_the_exact_twenty_four_public_transform_ids_in_order() {
    let output = run(&["--list"], b"");
    assert_eq!(output.status.code(), Some(0));
    let list = std::str::from_utf8(&output.stdout).unwrap();
    let ids: Vec<_> = list
        .lines()
        .map(|line| line.split('\t').next().unwrap())
        .collect();
    assert_eq!(ids, TRANSFORM_IDS);
}

#[test]
fn root_help_exposes_each_public_transform_command_once() {
    let output = run(&["--help"], b"");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let help = std::str::from_utf8(&output.stdout).unwrap();
    let ids: Vec<_> = help
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|id| TRANSFORM_IDS.contains(id))
        .collect();
    assert_eq!(ids, TRANSFORM_IDS);
}

#[test]
fn tui_has_explicit_temporary_code_one_path() {
    let output = run(&["tui"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"TUI error: toc tui requires terminal stdin and stdout\n"
    );
}

#[test]
fn tui_rejects_additional_arguments() {
    for args in [
        &["tui", "--help"][..],
        &["tui", "format-json"][..],
        &["tui", "--input", "data.txt"][..],
    ] {
        let output = run(args, b"");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.contains(&b'\x1b'), "{args:?}");
        assert_eq!(
            output.stderr, b"error: tui does not accept trailing arguments\\x0a\n",
            "{args:?}"
        );
    }
}
