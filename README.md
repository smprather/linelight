# linelight

A standalone, unprivileged output colorizer for plain command output and logs.

`linelight` adds readable emphasis to text that has no adequate native color.
It is deliberately explicit: it does not wrap commands, rewrite argv, change
PATH, export color environment variables, or select replacement utilities.

**Status: v0.1, filter-first.** The filter, embedded profiles, strict TOML
schema, `profiles`, `explain`, and `check` commands are implemented. A command
runner, PTY support, and automatic shell adapters are not yet shipped; they
need a complete process-group and job-control contract first.

## Build

```sh
cargo build --release
```

The result is one executable, `target/release/linelight`. All default profiles
are embedded, so the binary works with no adjacent files.

## Install

```sh
./install.sh
```

The default prefix is `$HOME/.local`; no sudo, `/etc`, package manager, service,
setuid bit, or capability is required. Override with `PREFIX`, `DESTDIR`,
`BINDIR`, or `MANDIR`.

## Use

Explicit filter, always-colour for a pager pipeline:

```sh
df -h | linelight filter --profile df --color always | less -R
journalctl -n 200 --no-pager | linelight filter --profile log --color always
diff -u old new | linelight filter --profile diff --color always
```

When stdout is not a tty, the default `--color auto` adds no colour. `NO_COLOR`
(non-empty) and `TERM=dumb` also disable auto colour. Explicit `always` and
`never` win.

Profiles are explicitly requested. `linelight` never decides on its own to
color a command or a stream.

## Profiles

Embedded IDs:

| ID | Target | Native-color note |
|---|---|---|
| `df` | GNU df | native colour off by default |
| `free` | procps free | no native colour |
| `ps` | procps ps | `PS_COLORS` unsupported |
| `ss` | iproute2 ss | no native colour for table output |
| `ping` | iputils ping | ordinary line output only |
| `dig` | BIND dig | human answer output only |
| `sensors` | lm-sensors | display only |
| `ls` | GNU ls -l | native colour is opt-in |
| `diff` | GNU diff | native colour is opt-in |
| `lsblk` | util-linux lsblk | `--highlight` is opt-in |
| `log` | explicit logs | custom filter use case |

### Native-color policy

A command is eligible for a fallback profile only when its ordinary invocation
does not already supply adequate color, including default auto/TTY detection.
Opt-in flags, format strings, loggers, and enabling environment variables do
not count. Build-dependent defaults require a verified build.

Commands with adequate default native color are not profiled. That is why
there are no profiles here for tools such as `journalctl`, `systemctl`, `dnf`,
`mvn`, `iostat`, `sar`, or default-auto builds of `ip`, `gcc`, and `fdisk`.

## Profile format

Profiles are versioned TOML, data-only, and strict: unknown fields, duplicate
keys, unknown semantic styles, missing capture groups, and invalid regexes are
errors.

```toml
schema = 1
id = "diff"
name = "GNU diff"
description = "Unified and normal diff output."

[[rule]]
regex = '^\+'
style = "success"

[[rule]]
regex = '^@@'
style = "heading"
```

Styles are semantic: `heading`, `muted`, `accent`, `identifier`, `path`,
`number`, `success`, `warning`, `error`. Rules apply in order; named groups can
override the whole-match style:

```toml
[[rule]]
regex = '^(?P<user>\S+)\s+(?P<pid>\d+)\s'
groups = { user = "identifier", pid = "number" }
```

No profile can execute commands, write files, import from the current
directory, replace or discard text, or emit arbitrary terminal control
sequences. The only bytes added are SGR style runs.

## Palette

The shared palette is configurable with
`${XDG_CONFIG_HOME:-$HOME/.config}/linelight/palette.toml` or `--palette`:

```toml
[palette]
error = "bold red"
warning = "yellow"
```

Palette values are token lists such as `bold red` or `dim`; raw escape
sequences are rejected.

## Resolution

Profile lookup, highest precedence first:

1. an explicitly named `.toml` file passed to `--profile`
2. user overrides in `${XDG_CONFIG_HOME:-$HOME/.config}/linelight/profiles`
3. each `--profile-dir` (highest precedence first)
4. embedded defaults

Overrides replace a profile by stable ID as a whole. A user or `--profile-dir`
file must be named `<id>.toml` and declare that same ID; a broken override fails
closed instead of silently falling back. There is no system-wide or implicit
current-directory lookup.

Palette lookup:

1. `--palette PATH`
2. `${XDG_CONFIG_HOME:-$HOME/.config}/linelight/palette.toml`
3. built-in palette

## Commands

```text
linelight filter --profile ID [--color auto|always|never] [--max-record N]
linelight profiles [--profile-dir DIR]
linelight explain --profile ID [--profile-dir DIR] [--palette PATH]
linelight check [PATH...] [--profile-dir DIR] [--palette PATH]
```

`check` with no paths validates every effective profile and the palette.
`explain` reports the source, effective palette, rules, and styles.

## Byte-preservation policy

Removing only the SGR sequences inserted by `linelight` yields the original
bytes. The filter:

- preserves LF, CRLF, and a final line without a newline;
- passes through entire records containing ESC/ANSI, NUL, other control bytes,
  lone carriage returns/progress updates, or invalid UTF-8;
- bounds records to `--max-record` (default 65536); an oversized record is
  streamed unchanged until the next LF;
- flushes on control/progress markers instead of waiting indefinitely for LF;
- holds ordinary partial lines until LF, EOF, or `--max-record`; there is no
  timer-based flush in v0.1;
- never splits a UTF-8 scalar to insert a style.

## Development

```sh
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
./tests/deploy.sh
target/debug/linelight check
```

Fixtures live in `tests/fixtures`. The suite covers profile compilation,
fixture text preservation, style semantics, CLI resolution, user and directory
overrides, palette overrides, ANSI/control bypass, CRLF, invalid UTF-8,
oversized records, progress-marker flushes, shell pipeline composition, and
byte-exact `--color never` output.

`tests/deploy.sh` verifies an unprivileged install into a prefix containing
spaces, a clean HOME/XDG environment, and a read-only executable.

## Measured locally

Release build on x86_64 Linux, 20–30 runs or a 27 MB log corpus:

| Case | Result |
|---|---|
| `linelight --version` startup | 0.72 ms mean |
| `log` profile, `--color always` | 26.1 MB/s |
| `log` profile, `--color never` | 3.5 GB/s (byte copy) |
| RSS at startup | ~3.3 MiB |

These are local measurements, not release gates.

## License

MIT. See `LICENSE`.
