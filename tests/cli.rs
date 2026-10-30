use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_linelight"))
}

fn isolated() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn run(xdg: &TempDir, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(bin())
        .args(args)
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("HOME", xdg.path())
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn write_profile(xdg: &TempDir, name: &str, text: &str) {
    let dir = xdg.path().join("linelight/profiles");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), text).unwrap();
}

#[test]
fn lists_and_checks_embedded_profiles() {
    let xdg = isolated();
    let output = run(&xdg, &["profiles"], b"");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    for id in ["diff", "ls", "log", "ping", "df"] {
        assert!(text.contains(id), "missing {}", id);
    }
    let output = run(&xdg, &["check"], b"");
    assert!(output.status.success(), "{:?}", output);
}

#[test]
fn filter_always_styles_and_auto_does_not_on_a_pipe() {
    let xdg = isolated();
    let output = run(
        &xdg,
        &["filter", "--profile", "diff", "--color", "always"],
        b"+added\n",
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\x1b[0;32m+\x1b[0madded\n");

    let output = run(&xdg, &["filter", "--profile", "diff"], b"+added\n");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"+added\n");
}

#[test]
fn preserves_ansi_control_and_crlf() {
    let xdg = isolated();
    let input = b"\x1b[31mred\x1b[0m\r\n";
    let output = run(
        &xdg,
        &["filter", "--profile", "log", "--color", "always"],
        input,
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, input);

    let input = b"10%\r20%\n";
    let output = run(
        &xdg,
        &["filter", "--profile", "log", "--color", "always"],
        input,
    );
    assert_eq!(output.stdout, input);
}

#[test]
fn user_profile_overrides_embedded() {
    let xdg = isolated();
    write_profile(
        &xdg,
        "log.toml",
        "schema = 1\nid = \"log\"\n[[rule]]\nregex = 'ERROR'\nstyle = \"success\"\n",
    );
    let output = run(
        &xdg,
        &["filter", "--profile", "log", "--color", "always"],
        b"level=ERROR\n",
    );
    assert!(output.status.success(), "{:?}", output);
    assert!(
        output
            .stdout
            .windows(7)
            .any(|window| window == b"\x1b[0;32m"),
        "{:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn palette_override_changes_semantic_style() {
    let xdg = isolated();
    let dir = xdg.path().join("linelight");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("palette.toml"), "[palette]\nerror = \"blue\"\n").unwrap();
    let output = run(
        &xdg,
        &["filter", "--profile", "log", "--color", "always"],
        b"level=ERROR\n",
    );
    assert!(output.status.success(), "{:?}", output);
    assert!(
        output
            .stdout
            .windows(7)
            .any(|window| window == b"\x1b[0;34m"),
        "{:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn bad_user_profile_fails_check_but_not_an_unrelated_filter() {
    let xdg = isolated();
    write_profile(
        &xdg,
        "bad.toml",
        "schema = 1\nid = \"bad\"\nunknown = true\n",
    );
    let output = run(&xdg, &["check"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));

    let output = run(
        &xdg,
        &["filter", "--profile", "diff", "--color", "always"],
        b"+x\n",
    );
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"\x1b[0;32m+\x1b[0mx\n");
}

#[test]
fn explicit_profile_file_and_explain_work() {
    let xdg = isolated();
    let path = xdg.path().join("custom.toml");
    std::fs::write(
        &path,
        "schema = 1\nid = \"custom\"\n[[rule]]\nregex = 'foo'\nstyle = \"accent\"\n",
    )
    .unwrap();
    let path = path.to_string_lossy().into_owned();
    let output = run(
        &xdg,
        &["filter", "--profile", &path, "--color", "always"],
        b"foo\n",
    );
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"\x1b[0;1;36mfoo\x1b[0m\n");

    let output = run(&xdg, &["explain", "--profile", "diff"], b"");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("id: diff"));
}

#[test]
fn oversized_records_bypass_styling() {
    let xdg = isolated();
    let mut input = vec![b'+'; 100];
    input.push(b'\n');
    let output = run(
        &xdg,
        &[
            "filter",
            "--profile",
            "diff",
            "--color",
            "always",
            "--max-record",
            "16",
        ],
        &input,
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, input);
}

#[test]
fn profile_dirs_are_high_precedence_first() {
    let xdg = isolated();
    let high = xdg.path().join("high");
    let low = xdg.path().join("low");
    std::fs::create_dir_all(&high).unwrap();
    std::fs::create_dir_all(&low).unwrap();
    std::fs::write(
        high.join("log.toml"),
        "schema = 1\nid = \"log\"\n[[rule]]\nregex = 'ERROR'\nstyle = \"success\"\n",
    )
    .unwrap();
    std::fs::write(
        low.join("log.toml"),
        "schema = 1\nid = \"log\"\n[[rule]]\nregex = 'ERROR'\nstyle = \"warning\"\n",
    )
    .unwrap();
    let high = high.to_string_lossy().into_owned();
    let low = low.to_string_lossy().into_owned();
    let output = run(
        &xdg,
        &[
            "filter",
            "--profile",
            "log",
            "--color",
            "always",
            "--profile-dir",
            &high,
            "--profile-dir",
            &low,
        ],
        b"level=ERROR\n",
    );
    assert!(output.status.success(), "{:?}", output);
    assert!(
        output
            .stdout
            .windows(7)
            .any(|window| window == b"\x1b[0;32m"),
        "high precedence profile did not win: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn never_is_byte_exact_for_binary_input() {
    let xdg = isolated();
    let input: &[u8] = b"abc\xff\x00\r\n\x1b[31mred\x1b[0m";
    let output = run(
        &xdg,
        &["filter", "--profile", "log", "--color", "never"],
        input,
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, input);
}

#[test]
fn broken_override_for_requested_profile_fails_closed() {
    let xdg = isolated();
    write_profile(
        &xdg,
        "log.toml",
        "schema = 1\nid = \"log\"\nunknown = true\n",
    );
    let output = run(
        &xdg,
        &["filter", "--profile", "log", "--color", "always"],
        b"x\n",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));

    let output = run(
        &xdg,
        &["filter", "--profile", "diff", "--color", "always"],
        b"+x\n",
    );
    assert!(output.status.success(), "{:?}", output);
}

#[test]
fn user_override_beats_profile_dir() {
    let xdg = isolated();
    let dir = xdg.path().join("extra");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("log.toml"),
        "schema = 1\nid = \"log\"\n[[rule]]\nregex = 'ERROR'\nstyle = \"success\"\n",
    )
    .unwrap();
    write_profile(
        &xdg,
        "log.toml",
        "schema = 1\nid = \"log\"\n[[rule]]\nregex = 'ERROR'\nstyle = \"warning\"\n",
    );
    let dir = dir.to_string_lossy().into_owned();
    let output = run(
        &xdg,
        &[
            "filter",
            "--profile",
            "log",
            "--color",
            "always",
            "--profile-dir",
            &dir,
        ],
        b"level=ERROR\n",
    );
    assert!(output.status.success(), "{:?}", output);
    assert!(
        output
            .stdout
            .windows(7)
            .any(|window| window == b"\x1b[0;33m"),
        "user override did not win: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn shell_pipeline_composes_with_filter() {
    let xdg = isolated();
    let script = format!(
        "printf '+x\\n' | '{}' filter --profile diff --color always | cat",
        bin().display()
    );
    let output = Command::new("sh")
        .arg("-c")
        .arg(script)
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("HOME", xdg.path())
        .env_remove("NO_COLOR")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"\x1b[0;32m+\x1b[0mx\n");
}
