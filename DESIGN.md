# linelight — design

Date: 2026-10-30  
Status: 2026.10.31, implemented runner and adapters  
License: MIT

## Boundary

`linelight` adds readable emphasis to plain command output and user-controlled
logs that lack an adequate native renderer. It does not select replacement
utilities, elevate privileges, manage services, become a pager, or wrap
commands implicitly.

It is a standalone product: one executable with embedded profiles, optional
user profiles under XDG, and no system-wide or current-directory lookup.

## Non-goals

- no command matcher or argv rewriting;
- no Python runtime;
- no executable profile actions, replacements, or skips;
- no `/etc` installation or sudo requirement;
- no cache, daemon, service, network access, or persistent state;
- no implicit command wrapping or PATH shadowing;
- no runner or PTY without the documented process contract.

## Scope in 2026.10.31

Implemented:

- pure filter engine;
- embedded default profiles;
- strict, versioned TOML profile schema;
- semantic palette with user override;
- `filter`, `run`, `init`, `profiles`, `explain`, and `check`;
- stdout and optional stderr filtering with independent auto-colour decisions;
- an optional stdout-only PTY for `run`;
- byte-preserving record policy;
- unprivileged, arbitrary-prefix installation with no adjacent files.

Deferred:

- native capability probing at shell startup;
- enabling `ls` in generated adapters by default;
- further profile coverage;
- downstream package registration.

## Native-color admission

A command is eligible for a fallback profile only when its ordinary invocation
does not already supply adequate color, including default auto/TTY detection.
Opt-in flags, format strings, loggers, and enabling environment variables do
not count. Build-dependent defaults count only for a verified build.

The initial profile set is:

`df`, `free`, `ps`, `ss`, `ping`, `dig`, `sensors`, `ls`, `diff`, `lsblk`,
`log`.

`ls`, `diff`, and `lsblk` are eligible because their native color is opt-in.
Commands with default native color are not profiled. Build-dependent commands
such as `ip`, `gcc`, and `fdisk` are not embedded without a verified
default-on/auto build.

## Profile schema

```toml
schema = 1
id = "diff"
name = "GNU diff"
description = "Unified and normal diff output."
[[rule]]
regex = '^\+'
style = "success"
[[rule]]
regex = '^(?P<user>\S+)\s+(?P<pid>\d+)\s'
groups = { user = "identifier", pid = "number" }
```

Rules are ordered. A whole-match `style` is applied first; named groups can
override portions. Later rules override earlier spans. There is no `replace`,
`skip`, `count`, `command`, `concat`, `previous`, `block`, or raw SGR field.

The TOML parser rejects unknown fields and duplicate keys. Profile validation
rejects:

- an unsupported schema;
- an invalid or missing ID;
- no rules;
- an empty regexp;
- a rule with neither `style` nor `groups`;
- an unknown semantic style;
- a missing named capture group;
- an invalid regexp;
- executable or non-versioned constructs.

Profiles are data-only. They cannot import files, execute commands, write
files, or emit arbitrary terminal control strings.

## Semantic palette

The shared palette has stable semantic names:

`heading`, `muted`, `accent`, `identifier`, `path`, `number`, `success`,
`warning`, `error`.

Built-in values are conservative and configurable by
`${XDG_CONFIG_HOME:-$HOME/.config}/linelight/palette.toml` or `--palette`.
Palette values are token lists such as `bold red` or `dim`; raw escape
sequences are rejected.

## Resolution

Profiles, highest precedence first:

1. explicit `.toml` file;
2. user overrides under `${XDG_CONFIG_HOME:-$HOME/.config}/linelight/profiles`;
3. `--profile-dir` directories, highest precedence first;
4. embedded profiles.

An override replaces a profile by stable ID as a whole. User and
`--profile-dir` files must be named `<id>.toml` and declare that same ID; a
broken override fails closed. There is no merge of rule fragments, no implicit
cwd lookup, and no system-wide fallback.

Palette lookup:

1. `--palette PATH`;
2. `${XDG_CONFIG_HOME:-$HOME/.config}/linelight/palette.toml`;
3. built-in palette.

## Byte preservation and framing

Invariant: removing only the SGR sequences inserted by `linelight` yields the
exact input bytes.

The filter operates on bounded records:

- LF, CRLF, and a final line without a newline are preserved;
- a record containing ESC/ANSI, NUL, other control bytes, a lone `\r`, or
  invalid UTF-8 is passed through unchanged;
- a CR progress marker flushes pending bytes immediately and bypasses the rest
  of the record;
- a record longer than `--max-record` (default 65536) is streamed unchanged
  until LF;
- ordinary final partial records are styled at EOF;
- ordinary partial lines are held until LF, EOF, or `--max-record`; there is no
  timer-based flush in this release, while progress/control markers flush immediately;
- matches run on valid UTF-8 strings, so SGR insertion never splits a scalar;
- `--color never` is a byte-exact copy.

Existing ANSI is never matchable and never receives resets. A later ANSI-aware
renderer would need explicit tokenization and style restoration; it is out of
scope.

## CLI

```text
linelight filter --profile ID [--color auto|always|never] [--max-record N]
linelight profiles [--profile-dir DIR]
linelight explain --profile ID [--profile-dir DIR] [--palette PATH]
linelight check [PATH...] [--profile-dir DIR] [--palette PATH]
```

`filter` defaults to auto. Auto colors only a tty stdout and is disabled by a
non-empty `NO_COLOR` or `TERM=dumb`. Explicit `always`/`never` win. A pager
pipeline uses `--color always`.

## Runner contract

`run --profile ID [--stderr] [--pty] -- CMD [ARG...]` keeps the child in the
caller's process group and stdin inherited. With colour off for both streams it
execs directly. Otherwise it preserves the child status (`128+N` for signal
`N`), forwards HUP/TERM/QUIT, ignores SIGINT in the parent so the filter can
drain the child, leaves Ctrl-Z/`fg` to the terminal's process group, sets
`COLUMNS` from the real terminal when unset, and treats EPIPE as a quiet
pipeline close. Descendants that keep stdout open keep the filter reading until
they close, as with a shell pipeline.

`--stderr` adds a separate stderr pipe and makes an independent auto-colour
decision from stderr's destination. stdout and stderr remain separate streams;
ordering between them is best effort. A background descendant that keeps
stdout open keeps the filter reading until it closes, as with a shell pipeline.
`--pty` gives only stdout a pseudoterminal, clears `OPOST`/`ONLCR`, copies
`TIOCGWINSZ` on resize, and leaves stdin, stderr, and the controlling terminal
alone. `--pty` and `--stderr` are separate modes in this release.

## Shell adapters

`init bash|zsh|tcsh|fish` prints wrappers for commands that have an effective
profile and are on `PATH` at init time. `log` is never wrapped; `ls` is off by
default. Existing aliases/functions win, collisions are skipped, and
`LINELIGHT_INIT=0` disables output. `~/.config/linelight/adapters.toml` can
disable names:

```toml
schema = 1
disable = ["ps", "diff"]
```

Wrappers call `linelight run --profile NAME -- NAME ARGS` and resolve the
target executable through the shell's ordinary path lookup, so colour-off
invocations exec directly without a pipe. Bash/Zsh keep name-based completion;
Fish copies it with `--wraps`.

## Installation

Single binary with embedded profiles. Default prefix `$HOME/.local`; no sudo,
`/etc`, package manager, service, setuid bit, capability, writable install
directory, cache, or network. Custom XDG roots and arbitrary prefixes
(including spaces) work. A read-only shared tree needs no writes beside the
executable.

## Acceptance tests

- all embedded profiles parse and compile;
- each fixture's text is byte-identical after stripping inserted SGR;
- semantic style assertions for additions, errors, warnings, and success;
- ANSI, NUL, control, lone CR, invalid UTF-8, CRLF, missing final newline, and
  oversized records bypass without mutation;
- progress-marker flushes occur before LF/EOF;
- user profile override by ID;
- palette override;
- broken unrelated user profile does not block an explicitly requested valid
  profile; a broken override for the requested profile fails closed;
- `check` fails on unknown fields, duplicate keys, missing groups, invalid
  regexes, and ID/filename mismatches;
- byte-exact `--color never`;
- explicit profile path;
- `profiles` and `explain` report sources;
- a real shell pipeline composes through the filter;
- runner status, signal, stdin, stderr, PTY isatty/window-size, and EPIPE
  behaviour;
- adapter generation, disable config, collision handling, and shell syntax;
- unprivileged install into a prefix with spaces, clean HOME/XDG, and a
  read-only executable.

## Future work

1. Add per-profile enable policy for adapters, including an explicit opt-in for
   `ls`.
2. Re-audit native defaults per executable build and add profiles only with
   current fixtures; grc's command list is not a roadmap.
3. Keep downstream packaging and integration out of the core; consume the
   released CLI and adapters through their public interfaces.
