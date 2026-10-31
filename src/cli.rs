use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crate::adapters;
use crate::error::{Error, Result};
use crate::framer;
use crate::profile::{parse_profile_file, resolve, CompiledProfile, ProfileSet};
use crate::runner::{self, RunOptions};
use crate::style::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColourMode {
    Auto,
    Always,
    Never,
}

struct FilterArgs {
    profile: String,
    colour: ColourMode,
    max_record: usize,
    palette: Option<PathBuf>,
    profile_dirs: Vec<PathBuf>,
}

struct ExplainArgs {
    profile: String,
    palette: Option<PathBuf>,
    profile_dirs: Vec<PathBuf>,
}

struct CheckArgs {
    paths: Vec<PathBuf>,
    palette: Option<PathBuf>,
    profile_dirs: Vec<PathBuf>,
}

enum Command {
    Help,
    Version,
    Filter(FilterArgs),
    Run(RunArgs),
    Init(InitArgs),
    Profiles(Vec<PathBuf>),
    Explain(ExplainArgs),
    Check(CheckArgs),
}

struct InitArgs {
    shell: String,
    profile_dirs: Vec<PathBuf>,
}

struct RunArgs {
    profile: String,
    colour: ColourMode,
    max_record: usize,
    stderr: bool,
    pty: bool,
    palette: Option<PathBuf>,
    profile_dirs: Vec<PathBuf>,
    command: Vec<OsString>,
}

pub fn run() -> i32 {
    let mut args = env::args_os();
    let name = args
        .next()
        .and_then(|arg| {
            PathBuf::from(arg)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "linelight".to_string());
    let rest: Vec<OsString> = args.collect();
    let result = match parse(&rest) {
        Ok(Command::Help) => {
            print_help();
            Ok(0)
        }
        Ok(Command::Version) => {
            println!("linelight {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Ok(Command::Filter(args)) => filter_main(args),
        Ok(Command::Run(args)) => run_main(args),
        Ok(Command::Init(args)) => init_main(args),
        Ok(Command::Profiles(dirs)) => profiles_main(&dirs),
        Ok(Command::Explain(args)) => explain_main(args),
        Ok(Command::Check(args)) => check_main(args),
        Err(error) => Err(error),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}: {}", name, error.message);
            error.code
        }
    }
}

fn filter_main(args: FilterArgs) -> Result<i32> {
    let palette = Palette::load(args.palette.as_deref())?;
    let (raw, warnings) = resolve(&args.profile, &args.profile_dirs)?;
    for warning in warnings {
        eprintln!("linelight: warning: {}", warning);
    }
    let profile = raw.compile(&palette)?;

    let colour = match args.colour {
        ColourMode::Always => true,
        ColourMode::Never => false,
        ColourMode::Auto => io::stdout().is_terminal() && auto_environment_allows(),
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    framer::filter(
        &mut stdin.lock(),
        &mut stdout.lock(),
        &profile,
        &palette,
        colour,
        args.max_record,
        false,
    )?;
    Ok(0)
}

fn run_main(args: RunArgs) -> Result<i32> {
    let palette = Palette::load(args.palette.as_deref())?;
    let (raw, warnings) = resolve(&args.profile, &args.profile_dirs)?;
    for warning in warnings {
        eprintln!("linelight: warning: {}", warning);
    }
    let profile = raw.compile(&palette)?;

    let stdout_colour = match args.colour {
        ColourMode::Always => true,
        ColourMode::Never => false,
        ColourMode::Auto => io::stdout().is_terminal() && auto_environment_allows(),
    };
    let stderr_colour = args.stderr
        && match args.colour {
            ColourMode::Always => true,
            ColourMode::Never => false,
            ColourMode::Auto => io::stderr().is_terminal() && auto_environment_allows(),
        };
    if args.pty && args.stderr {
        return Err(Error::usage("--pty cannot be combined with --stderr"));
    }
    runner::run(RunOptions {
        command: args.command,
        profile,
        palette,
        max_record: args.max_record,
        stderr: args.stderr,
        pty: args.pty,
        color_stdout: stdout_colour,
        color_stderr: stderr_colour,
    })
}

fn init_main(args: InitArgs) -> Result<i32> {
    if env::var("LINELIGHT_INIT")
        .map(|value| value == "0")
        .unwrap_or(false)
    {
        return Ok(0);
    }
    print!("{}", adapters::generate(&args.shell, &args.profile_dirs)?);
    Ok(0)
}

fn profiles_main(dirs: &[PathBuf]) -> Result<i32> {
    let set = ProfileSet::discover(dirs);
    for (id, profile) in &set.entries {
        println!("{}\t{}\t{}", id, profile.name, profile.source);
    }
    for error in &set.errors {
        eprintln!("linelight: {}", error);
    }
    Ok(if set.errors.is_empty() { 0 } else { 1 })
}

fn explain_main(args: ExplainArgs) -> Result<i32> {
    let palette = Palette::load(args.palette.as_deref())?;
    let (raw, warnings) = resolve(&args.profile, &args.profile_dirs)?;
    for warning in warnings {
        eprintln!("linelight: warning: {}", warning);
    }
    let profile = raw.compile(&palette)?;
    print_profile(&profile, &palette);
    Ok(0)
}

fn check_main(args: CheckArgs) -> Result<i32> {
    let palette = Palette::load(args.palette.as_deref())?;
    let mut failed = false;
    if args.paths.is_empty() {
        let set = ProfileSet::discover(&args.profile_dirs);
        for (id, raw) in &set.entries {
            match raw.compile(&palette) {
                Ok(_) => println!("{}: ok ({})", id, raw.source),
                Err(error) => {
                    eprintln!("{}", error.message);
                    failed = true;
                }
            }
        }
        for error in set.errors {
            eprintln!("{}", error);
            failed = true;
        }
    } else {
        for path in &args.paths {
            match parse_profile_file(path) {
                Ok(raw) => match raw.compile(&palette) {
                    Ok(_) => println!("{}: ok", path.display()),
                    Err(error) => {
                        eprintln!("{}", error.message);
                        failed = true;
                    }
                },
                Err(error) => {
                    eprintln!("{}", error.message);
                    failed = true;
                }
            }
        }
    }
    Ok(if failed { 1 } else { 0 })
}

fn print_profile(profile: &CompiledProfile, palette: &Palette) {
    println!("id: {}", profile.id);
    println!("name: {}", profile.name);
    println!("source: {}", profile.source);
    if !profile.description.is_empty() {
        println!("description: {}", profile.description);
    }
    println!("palette:");
    for (name, style) in palette.entries() {
        println!("  {}: {}", name, style_name(style));
    }
    println!("rules:");
    for (index, rule) in profile.rules.iter().enumerate() {
        let style = rule.style.map(|id| palette.name(id)).unwrap_or("-");
        print!("  {}. {} [{}]", index + 1, rule.regex.as_str(), style);
        for (name, id) in &rule.groups {
            print!(" {}={}", name, palette.name(*id));
        }
        println!();
    }
}

fn style_name(style: &crate::style::Style) -> String {
    if style.sgr.is_empty() {
        "default".to_string()
    } else {
        let text = String::from_utf8_lossy(&style.sgr);
        text.replace('\x1b', "ESC")
    }
}

fn auto_environment_allows() -> bool {
    if let Some(value) = env::var_os("NO_COLOR") {
        if !value.is_empty() {
            return false;
        }
    }
    if env::var("TERM").map(|term| term == "dumb").unwrap_or(false) {
        return false;
    }
    true
}

fn parse(args: &[OsString]) -> Result<Command> {
    let Some(first) = args.first() else {
        return Ok(Command::Help);
    };
    let first = first.to_string_lossy();
    match first.as_ref() {
        "--help" | "-h" | "help" => Ok(Command::Help),
        "--version" | "version" => Ok(Command::Version),
        "filter" => parse_filter(&args[1..]),
        "run" => parse_run(&args[1..]),
        "init" => parse_init(&args[1..]),
        "profiles" => parse_profiles(&args[1..]),
        "explain" => parse_explain(&args[1..]),
        "check" => parse_check(&args[1..]),
        other => Err(Error::usage(format!(
            "unknown command `{}`; expected filter, profiles, explain, or check",
            other
        ))),
    }
}

fn parse_filter(args: &[OsString]) -> Result<Command> {
    let mut profile = None;
    let mut colour = ColourMode::Auto;
    let mut max_record = 65536;
    let mut palette = None;
    let mut profile_dirs = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        if text == "--profile" {
            index += 1;
            profile = Some(required(args, index, "--profile")?);
        } else if let Some(value) = text.strip_prefix("--profile=") {
            profile = Some(value.to_string());
        } else if text == "--color" {
            index += 1;
            colour = parse_colour(&required(args, index, "--color")?)?;
        } else if let Some(value) = text.strip_prefix("--color=") {
            colour = parse_colour(value)?;
        } else if text == "--max-record" {
            index += 1;
            max_record = parse_usize(&required(args, index, "--max-record")?, "--max-record")?;
        } else if let Some(value) = text.strip_prefix("--max-record=") {
            max_record = parse_usize(value, "--max-record")?;
        } else if text == "--palette" {
            index += 1;
            palette = Some(PathBuf::from(required(args, index, "--palette")?));
        } else if let Some(value) = text.strip_prefix("--palette=") {
            palette = Some(PathBuf::from(value));
        } else if text == "--profile-dir" {
            index += 1;
            profile_dirs.push(PathBuf::from(required(args, index, "--profile-dir")?));
        } else if let Some(value) = text.strip_prefix("--profile-dir=") {
            profile_dirs.push(PathBuf::from(value));
        } else if text == "--" {
            return Err(Error::usage("filter does not accept positional arguments"));
        } else {
            return Err(Error::usage(format!("unknown filter option `{}`", text)));
        }
        index += 1;
    }
    let profile = profile.ok_or_else(|| Error::usage("filter requires --profile"))?;
    Ok(Command::Filter(FilterArgs {
        profile,
        colour,
        max_record,
        palette,
        profile_dirs,
    }))
}

fn parse_run(args: &[OsString]) -> Result<Command> {
    let mut profile = None;
    let mut colour = ColourMode::Auto;
    let mut max_record = 65536;
    let mut stderr = false;
    let mut pty = false;
    let mut palette = None;
    let mut profile_dirs = Vec::new();
    let mut command = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        if text == "--" {
            command = args[index + 1..].to_vec();
            break;
        }
        if text == "--profile" {
            index += 1;
            profile = Some(required(args, index, "--profile")?);
        } else if let Some(value) = text.strip_prefix("--profile=") {
            profile = Some(value.to_string());
        } else if text == "--color" {
            index += 1;
            colour = parse_colour(&required(args, index, "--color")?)?;
        } else if let Some(value) = text.strip_prefix("--color=") {
            colour = parse_colour(value)?;
        } else if text == "--max-record" {
            index += 1;
            max_record = parse_usize(&required(args, index, "--max-record")?, "--max-record")?;
        } else if let Some(value) = text.strip_prefix("--max-record=") {
            max_record = parse_usize(value, "--max-record")?;
        } else if text == "--stderr" {
            stderr = true;
        } else if text == "--pty" {
            pty = true;
        } else if text == "--palette" {
            index += 1;
            palette = Some(PathBuf::from(required(args, index, "--palette")?));
        } else if let Some(value) = text.strip_prefix("--palette=") {
            palette = Some(PathBuf::from(value));
        } else if text == "--profile-dir" {
            index += 1;
            profile_dirs.push(PathBuf::from(required(args, index, "--profile-dir")?));
        } else if let Some(value) = text.strip_prefix("--profile-dir=") {
            profile_dirs.push(PathBuf::from(value));
        } else if text.starts_with('-') {
            return Err(Error::usage(format!("unknown run option `{}`", text)));
        } else {
            command = args[index..].to_vec();
            break;
        }
        index += 1;
    }
    let profile = profile.ok_or_else(|| Error::usage("run requires --profile"))?;
    if command.is_empty() {
        return Err(Error::usage("run requires a command after `--`"));
    }
    Ok(Command::Run(RunArgs {
        profile,
        colour,
        max_record,
        stderr,
        pty,
        palette,
        profile_dirs,
        command,
    }))
}

fn parse_init(args: &[OsString]) -> Result<Command> {
    let Some(shell) = args.first() else {
        return Err(Error::usage("init requires bash, zsh, tcsh, or fish"));
    };
    let shell = shell.to_string_lossy().into_owned();
    let mut profile_dirs = Vec::new();
    let mut index = 1;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        if text == "--profile-dir" {
            index += 1;
            profile_dirs.push(PathBuf::from(required(args, index, "--profile-dir")?));
        } else if let Some(value) = text.strip_prefix("--profile-dir=") {
            profile_dirs.push(PathBuf::from(value));
        } else {
            return Err(Error::usage(format!("unknown init option `{}`", text)));
        }
        index += 1;
    }
    Ok(Command::Init(InitArgs {
        shell,
        profile_dirs,
    }))
}

fn parse_profiles(args: &[OsString]) -> Result<Command> {
    let dirs = parse_dirs(args)?;
    Ok(Command::Profiles(dirs))
}

fn parse_explain(args: &[OsString]) -> Result<Command> {
    let mut profile = None;
    let mut palette = None;
    let mut profile_dirs = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        if text == "--profile" {
            index += 1;
            profile = Some(required(args, index, "--profile")?);
        } else if let Some(value) = text.strip_prefix("--profile=") {
            profile = Some(value.to_string());
        } else if text == "--palette" {
            index += 1;
            palette = Some(PathBuf::from(required(args, index, "--palette")?));
        } else if let Some(value) = text.strip_prefix("--palette=") {
            palette = Some(PathBuf::from(value));
        } else if text == "--profile-dir" {
            index += 1;
            profile_dirs.push(PathBuf::from(required(args, index, "--profile-dir")?));
        } else if let Some(value) = text.strip_prefix("--profile-dir=") {
            profile_dirs.push(PathBuf::from(value));
        } else {
            return Err(Error::usage(format!("unknown explain option `{}`", text)));
        }
        index += 1;
    }
    let profile = profile.ok_or_else(|| Error::usage("explain requires --profile"))?;
    Ok(Command::Explain(ExplainArgs {
        profile,
        palette,
        profile_dirs,
    }))
}

fn parse_check(args: &[OsString]) -> Result<Command> {
    let mut paths = Vec::new();
    let mut palette = None;
    let mut profile_dirs = Vec::new();
    let mut index = 0;
    let mut positional = false;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        if positional {
            paths.push(PathBuf::from(&args[index]));
        } else if text == "--" {
            positional = true;
        } else if text == "--palette" {
            index += 1;
            palette = Some(PathBuf::from(required(args, index, "--palette")?));
        } else if let Some(value) = text.strip_prefix("--palette=") {
            palette = Some(PathBuf::from(value));
        } else if text == "--profile-dir" {
            index += 1;
            profile_dirs.push(PathBuf::from(required(args, index, "--profile-dir")?));
        } else if let Some(value) = text.strip_prefix("--profile-dir=") {
            profile_dirs.push(PathBuf::from(value));
        } else if text.starts_with('-') {
            return Err(Error::usage(format!("unknown check option `{}`", text)));
        } else {
            paths.push(PathBuf::from(&args[index]));
        }
        index += 1;
    }
    Ok(Command::Check(CheckArgs {
        paths,
        palette,
        profile_dirs,
    }))
}

fn parse_dirs(args: &[OsString]) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        if text == "--profile-dir" {
            index += 1;
            dirs.push(PathBuf::from(required(args, index, "--profile-dir")?));
        } else if let Some(value) = text.strip_prefix("--profile-dir=") {
            dirs.push(PathBuf::from(value));
        } else {
            return Err(Error::usage(format!("unknown profiles option `{}`", text)));
        }
        index += 1;
    }
    Ok(dirs)
}

fn required(args: &[OsString], index: usize, option: &str) -> Result<String> {
    args.get(index)
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| Error::usage(format!("{} requires a value", option)))
}

fn parse_colour(value: &str) -> Result<ColourMode> {
    match value {
        "auto" => Ok(ColourMode::Auto),
        "always" => Ok(ColourMode::Always),
        "never" => Ok(ColourMode::Never),
        other => Err(Error::usage(format!(
            "invalid color mode `{}`; expected auto, always, or never",
            other
        ))),
    }
}

fn parse_usize(value: &str, option: &str) -> Result<usize> {
    value
        .parse()
        .map_err(|_| Error::usage(format!("{} requires a positive integer", option)))
}

fn print_help() {
    println!(
        "linelight {}\n\
         usage: linelight filter --profile ID [options]\n\
                linelight run --profile ID [options] -- COMMAND [ARG...]\n\
                linelight init bash|zsh|tcsh|fish [--profile-dir DIR]\n\
                linelight profiles [--profile-dir DIR]\n\
                linelight explain --profile ID [--profile-dir DIR]\n\
                linelight check [PATH...] [--profile-dir DIR]\n\
         \n\
         filter options:\n\
           --profile ID|PATH   profile id or explicit .toml file\n\
           --color MODE        auto (default), always, or never\n\
           --max-record N      bypass records longer than N bytes (default 65536)\n\
           --palette PATH      palette file\n\
           --profile-dir DIR   extra profile directory (highest precedence first)\n\
         \n\
         run options:\n\
           --profile ID|PATH   profile id or explicit .toml file\n\
           --color MODE        auto (default), always, or never\n\
           --max-record N      bypass records longer than N bytes (default 65536)\n\
           --stderr            also filter stderr (separate stream)\n\
           --pty               give the command a pty on stdout\n\
           --palette PATH      palette file\n\
           --profile-dir DIR   extra profile directory (highest precedence first)\n\
         \n\
         init emits shell wrappers for commands that have an effective profile
\
         and are on PATH. Existing aliases/functions are left alone; ls is off
\
         by default and log is never wrapped.
\
         \n\
         Profiles are embedded, overridden by $XDG_CONFIG_HOME/linelight/profiles,\n\
         and may be supplemented by --profile-dir. No configuration is read from\n\
         the current directory or /etc.",
        env!("CARGO_PKG_VERSION")
    );
}
