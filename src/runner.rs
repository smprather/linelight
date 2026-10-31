use std::ffi::{CString, OsString};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};

use nix::pty::openpty;
use nix::sys::termios::{tcgetattr, tcsetattr, OutputFlags, SetArg};
use signal_hook::consts::{SIGHUP, SIGQUIT, SIGTERM, SIGWINCH};
use signal_hook::iterator::Signals;

use crate::error::{Error, Result};
use crate::framer::filter;
use crate::profile::CompiledProfile;
use crate::style::Palette;

pub struct RunOptions {
    pub command: Vec<OsString>,
    pub profile: CompiledProfile,
    pub palette: Palette,
    pub max_record: usize,
    pub stderr: bool,
    pub pty: bool,
    pub color_stdout: bool,
    pub color_stderr: bool,
}

pub fn run(options: RunOptions) -> Result<i32> {
    if options.command.is_empty() {
        return Err(Error::usage("run requires a command after `--`"));
    }
    if options.pty && options.stderr {
        return Err(Error::usage("--pty cannot be combined with --stderr"));
    }

    if !options.color_stdout && !options.color_stderr {
        return Err(exec(&options.command));
    }

    let mut cmd = Command::new(&options.command[0]);
    cmd.args(&options.command[1..]);
    cmd.stdin(Stdio::inherit());
    if let Some(width) = terminal_columns() {
        if std::env::var_os("COLUMNS").is_none() {
            cmd.env("COLUMNS", width.to_string());
        }
    }

    let pty_master = if options.color_stdout && options.pty {
        Some(open_pty_stdout(&mut cmd)?)
    } else {
        if options.color_stdout {
            cmd.stdout(Stdio::piped());
        }
        None
    };
    if options.color_stderr {
        cmd.stderr(Stdio::piped());
    }

    unsafe {
        cmd.pre_exec(|| {
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGHUP, libc::SIG_DFL);
            libc::signal(libc::SIGQUIT, libc::SIG_DFL);
            Ok(())
        });
    }

    let mut child = cmd
        .spawn()
        .map_err(|error| spawn_error(&options.command[0], error))?;
    let child_pid = child.id() as i32;
    drop(cmd);
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }

    let pty_fd = pty_master.as_ref().map(|file| file.as_raw_fd());
    install_signals(child_pid, pty_fd)?;

    let mut readers: Vec<JoinHandle<Result<()>>> = Vec::new();
    if options.color_stdout {
        let reader: Box<dyn Read + Send> = if let Some(master) = pty_master {
            Box::new(EioReader(master))
        } else {
            Box::new(child.stdout.take().expect("stdout pipe"))
        };
        let profile = options.profile.clone();
        let palette = options.palette.clone();
        let max = options.max_record;
        readers.push(thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let stdout = io::stdout();
            filter(
                &mut reader,
                &mut stdout.lock(),
                &profile,
                &palette,
                true,
                max,
                true,
            )
        }));
    }
    if options.color_stderr {
        let pipe = child.stderr.take().expect("stderr pipe");
        let profile = options.profile.clone();
        let palette = options.palette.clone();
        let max = options.max_record;
        readers.push(thread::spawn(move || {
            let mut reader = BufReader::new(pipe);
            let stderr = io::stderr();
            filter(
                &mut reader,
                &mut stderr.lock(),
                &profile,
                &palette,
                true,
                max,
                true,
            )
        }));
    }

    let status = child.wait()?;
    let mut reader_error = None;
    for handle in readers {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                if reader_error.is_none() {
                    reader_error = Some(error);
                }
            }
            Err(_) => {
                if reader_error.is_none() {
                    reader_error = Some(Error::new("output filter panicked"));
                }
            }
        }
    }

    let code = status_code(status);
    if code == 0 {
        if let Some(error) = reader_error {
            return Err(error);
        }
    }
    Ok(code)
}

fn open_pty_stdout(cmd: &mut Command) -> Result<File> {
    let pair = openpty(None, None).map_err(|error| Error::new(format!("openpty: {}", error)))?;
    let master: File = pair.master.into();
    let slave: File = pair.slave.into();

    let mut termios =
        tcgetattr(&slave).map_err(|error| Error::new(format!("tcgetattr: {}", error)))?;
    termios
        .output_flags
        .remove(OutputFlags::OPOST | OutputFlags::ONLCR);
    tcsetattr(&slave, SetArg::TCSANOW, &termios)
        .map_err(|error| Error::new(format!("tcsetattr: {}", error)))?;
    if let Some(size) = terminal_size() {
        set_winsize(slave.as_raw_fd(), &size);
    }
    cmd.stdout(Stdio::from(slave));
    Ok(master)
}

fn install_signals(pid: i32, pty_fd: Option<RawFd>) -> Result<()> {
    let mut signals = Signals::new([SIGTERM, SIGHUP, SIGQUIT, SIGWINCH])
        .map_err(|error| Error::new(format!("signal setup: {}", error)))?;
    thread::spawn(move || {
        for signal in signals.forever() {
            if signal == SIGWINCH {
                if let Some(fd) = pty_fd {
                    if let Some(size) = terminal_size() {
                        set_winsize(fd, &size);
                    }
                }
            } else {
                unsafe {
                    libc::kill(pid, signal);
                }
            }
        }
    });
    Ok(())
}

fn terminal_columns() -> Option<u16> {
    terminal_size().map(|size| size.ws_col)
}

fn terminal_size() -> Option<libc::winsize> {
    unsafe {
        for fd in [1, 0, 2] {
            let mut size: libc::winsize = std::mem::zeroed();
            if libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) == 0 && size.ws_col > 0 {
                return Some(size);
            }
        }
    }
    None
}

fn set_winsize(fd: RawFd, size: &libc::winsize) {
    unsafe {
        libc::ioctl(fd, libc::TIOCSWINSZ, size);
    }
}

fn status_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        code
    } else if let Some(signal) = status.signal() {
        128 + signal
    } else {
        1
    }
}

fn exec(command: &[OsString]) -> Error {
    let mut args = Vec::with_capacity(command.len());
    for arg in command {
        match CString::new(arg.as_encoded_bytes()) {
            Ok(value) => args.push(value),
            Err(_) => return Error::new("command contains a NUL byte"),
        }
    }
    let mut pointers: Vec<*const libc::c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
    pointers.push(std::ptr::null());
    unsafe {
        libc::execvp(args[0].as_ptr(), pointers.as_ptr());
    }
    spawn_error(&command[0], io::Error::last_os_error())
}

fn spawn_error(command: &OsString, error: io::Error) -> Error {
    let code = match error.raw_os_error() {
        Some(libc::ENOENT) => 127,
        Some(libc::EACCES) => 126,
        _ => 1,
    };
    Error::with_code(format!("{}: {}", command.to_string_lossy(), error), code)
}

struct EioReader<R>(R);

impl<R: Read> Read for EioReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            Err(error) if error.raw_os_error() == Some(libc::EIO) => Ok(0),
            other => other,
        }
    }
}
