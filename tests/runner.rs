use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::process::{Command, Output, Stdio};

use nix::pty::openpty;
use tempfile::TempDir;

fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_linelight"))
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

#[test]
fn run_styles_stdout_and_propagates_status() {
    let xdg = isolated();
    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--",
            "sh",
            "-c",
            "printf '+x\\n'",
        ],
        b"",
    );
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"\x1b[0;32m+\x1b[0mx\n");

    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--",
            "sh",
            "-c",
            "exit 42",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(42));

    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--",
            "sh",
            "-c",
            "kill -TERM $$",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(143));

    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--",
            "does-not-exist",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn run_inherits_stdin_and_can_filter_stderr() {
    let xdg = isolated();
    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--",
            "sh",
            "-c",
            "read x; echo got:$x",
        ],
        b"hello\n",
    );
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"got:hello\n");

    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--stderr",
            "--",
            "sh",
            "-c",
            "printf '+out\\n'; printf -- '-err\\n' >&2",
        ],
        b"",
    );
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"\x1b[0;32m+\x1b[0mout\n");
    assert_eq!(output.stderr, b"\x1b[0;1;31m-\x1b[0merr\n");
}

#[test]
fn run_pty_is_a_terminal_and_preserves_lf() {
    let xdg = isolated();
    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--pty",
            "--",
            "python3",
            "-c",
            "import os; print('tty', os.isatty(1))",
        ],
        b"",
    );
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("tty True"));
    assert!(!output.stdout.windows(2).any(|window| window == b"\r\n"));
}

#[test]
fn run_pty_reports_the_outer_terminal_size() {
    let xdg = isolated();
    let pair = openpty(None, None).unwrap();
    let master: File = pair.master.into();
    let slave: File = pair.slave.into();
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    size.ws_col = 100;
    size.ws_row = 40;
    unsafe {
        libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &size);
    }

    let mut child = Command::new(bin())
        .args([
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--pty",
            "--",
            "python3",
            "-c",
            "import os; print(os.get_terminal_size(1).columns, os.get_terminal_size(1).lines)",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("HOME", xdg.path())
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::from(slave))
        .spawn()
        .unwrap();
    drop(child.stdin.take());
    let mut output = Vec::new();
    let mut reader = master;
    loop {
        let mut buf = [0u8; 4096];
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => output.extend_from_slice(&buf[..n]),
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) => panic!("read pty: {}", error),
        }
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "{:?}", output);
    assert!(
        String::from_utf8_lossy(&output).contains("100 40"),
        "{:?}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn run_forwards_sigterm_to_the_child() {
    let xdg = isolated();
    let child = Command::new(bin())
        .args([
            "run",
            "--profile",
            "diff",
            "--color",
            "always",
            "--",
            "sh",
            "-c",
            "trap 'exit 7' TERM; echo READY; while :; do sleep 0.1; done",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("HOME", xdg.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(7), "{:?}", output);
}

#[test]
fn run_rejects_pty_with_stderr() {
    let xdg = isolated();
    let output = run(
        &xdg,
        &[
            "run",
            "--profile",
            "diff",
            "--pty",
            "--stderr",
            "--",
            "true",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(2));
}
