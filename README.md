<div align="center">

# Linelight

**Put your output in the limelight.**

A command-line filter that makes plain logs and command output easier to scan.

[Get started](#get-started) · [Examples](#examples) · [Profiles](#profiles) · [Customization](#customization)

</div>

![The same three log lines before and after Linelight: INFO in green, WARN in yellow, ERROR in red, with addresses and paths highlighted.](docs/images/log-demo.svg)

Pipe in text and choose a profile. Linelight highlights useful details while
preserving the original text. All 11 built-in profiles travel with the binary,
which runs on its own on a compatible remote host.

## Get started

Build from source on Linux with Git, a [stable Rust toolchain](https://rust-lang.org/tools/install/),
and a system linker:

```sh
git clone https://github.com/smprather/linelight.git
cd linelight
cargo build --release --locked
./install.sh
```

Installation needs no sudo: the executable goes in `~/.local/bin`, the manual
in `~/.local/share/man`. Add `~/.local/bin` to your `PATH` for the examples below.

Try it in your terminal:

```sh
printf '%s\n' \
    'INFO  listening on 127.0.0.1:8080' \
    'WARN  retrying /api/search' \
    'ERROR database unavailable' |
    ~/.local/bin/linelight filter --profile log
```

Rust is needed only for building; the installed binary runs offline.

<details>
<summary>Use a different install location</summary>

```sh
env PREFIX="$HOME/tools/linelight" ./install.sh
"$HOME/tools/linelight/bin/linelight" --version
```

The installer also accepts `BINDIR`, `MANDIR`, and `DESTDIR`. To try Linelight
without installing, use `./target/release/linelight`.

</details>

## Examples

**Read command output:**

```sh
df -h | linelight filter --profile df
ps aux | linelight filter --profile ps
ss -ltn | linelight filter --profile ss
```

**Read a log file, or follow it as it grows:**

```sh
linelight filter --profile log < app.log
tail -f app.log | linelight filter --profile log
```

**Keep color when using a pager:**

```sh
diff -u old.txt new.txt |
    linelight filter --profile diff --color always |
    less -R
```

> [!TIP]
> Auto mode checks Linelight's **output**. When piping to `less -R`, use
> `--color always`; when redirecting to a file, leave auto mode on for plain text.

In Bash or Zsh scripts, use `set -o pipefail` when the source command's exit
status matters. The shell determines the status of the pipeline.

## Profiles

Every profile below is included in the binary; configuration is optional.

| Profile | Use with |
| --- | --- |
| [`log`](profiles/log.toml) | Line-oriented application logs |
| [`df`](profiles/df.toml) | GNU filesystem-usage tables |
| [`diff`](profiles/diff.toml) | Unified `diff -u` output |
| [`dig`](profiles/dig.toml) | BIND DNS answers |
| [`free`](profiles/free.toml) | procps memory summaries |
| [`ls`](profiles/ls.toml) | GNU `ls -l` with English date formatting |
| [`lsblk`](profiles/lsblk.toml) | util-linux block-device trees |
| [`ping`](profiles/ping.toml) | Ordinary iputils ping output |
| [`ps`](profiles/ps.toml) | procps process tables |
| [`sensors`](profiles/sensors.toml) | lm-sensors Celsius readings |
| [`ss`](profiles/ss.toml) | iproute2 socket tables |

Profiles target output that is plain by default; opt-in native color does not
disqualify a profile. Prefer native output when a command already colors its
ordinary terminal output well.

## Color and streaming

| Mode | Behavior |
| --- | --- |
| `--color auto` (default) | Adds color when stdout is a terminal and the environment allows it |
| `--color always` | Adds color even through a pipe or into a file |
| `--color never` | Copies input without adding color |

A nonempty `NO_COLOR` or `TERM=dumb` disables auto color. An explicit
`--color always` or `--color never` takes precedence.

Line endings and a missing final newline are preserved. Records containing
ANSI escapes, control bytes, or invalid UTF-8 pass through unchanged, as do
records reaching the default 64 KiB limit. Change the limit with `--max-record N`.

> [!NOTE]
> Linelight 2026.10.30 is a line-oriented filter. Ordinary partial lines wait for a
> newline, EOF, or the record limit. Progress displays that redraw the same line
> can be delayed; complete-line logs and tables are the intended inputs.

## Customization

User configuration lives in `~/.config/linelight`, or under your configured
`XDG_CONFIG_HOME` root.

### Add a profile

Profiles are TOML files pairing regular expressions with named styles. For a
tool that prints build-check results, save this as `build.toml`:

```toml
schema = 1
id = "build"
name = "Build checks"

[[rule]]
regex = '^PASS\b'
style = "success"

[[rule]]
regex = '^FAIL\b'
style = "error"
```

Validate it, then try it:

```sh
linelight check ./build.toml
printf 'PASS compile\nFAIL timing check\n' |
    linelight filter --profile ./build.toml
```

Keep it as `~/.config/linelight/profiles/build.toml` to select it by name with
`--profile build`.

Later rules override earlier styles. Named groups can style parts of a match;
the [built-in profiles](profiles/) show how.
Available styles are `heading`, `muted`, `accent`, `identifier`, `path`,
`number`, `success`, `warning`, and `error`. Profiles are data-only; they cannot
execute commands or rewrite text.

<details>
<summary>Profile lookup and overrides</summary>

Linelight looks for a requested profile in this order:

1. An explicit file, such as `--profile ./build.toml`.
2. The user profile directory under `XDG_CONFIG_HOME` or `~/.config`.
3. Directories supplied with `--profile-dir DIR`, highest priority first.
4. The embedded defaults.

Files in profile directories must be named `<id>.toml` and declare that ID.
Overrides replace whole profiles; a broken requested override reports an error.

</details>

### Change the colors

Create `~/.config/linelight/palette.toml`:

```toml
[palette]
error = "bold bright_red"
warning = "yellow"
path = "underline cyan"
```

Entries change styles across all profiles; omitted entries keep their defaults.
Combine named colors with attributes such as `bold`, `dim`, and `underline`.
Your terminal's palette determines how the colors look.

For a one-off palette, pass `--palette ./palette.toml` to `filter`, `explain`,
or `check`.

## Command reference

| Command | Purpose |
| --- | --- |
| `linelight filter --profile ID` | Read stdin and write styled text to stdout |
| `linelight profiles` | List effective profiles and their sources |
| `linelight explain --profile ID` | Show a profile's source, palette, and rules |
| `linelight check [PATH...]` | Validate profile files, or all effective profiles and the palette |

Run `linelight --help` for options, or `man linelight` after installation.
See the [manual source](linelight.1) and [design notes](DESIGN.md) for more detail.
