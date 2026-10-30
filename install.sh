#!/bin/sh
set -eu

PREFIX=${PREFIX:-${HOME:-/tmp}/.local}
BINDIR=${BINDIR:-$PREFIX/bin}
MANDIR=${MANDIR:-$PREFIX/share/man}
DESTDIR=${DESTDIR:-}
BIN=${BIN:-target/release/linelight}

if [ ! -x "$BIN" ]; then
    echo "install.sh: $BIN not found; run cargo build --release first" >&2
    exit 1
fi

install -d "$DESTDIR$BINDIR" "$DESTDIR$MANDIR/man1"
install -m 0755 "$BIN" "$DESTDIR$BINDIR/linelight"
install -m 0644 linelight.1 "$DESTDIR$MANDIR/man1/linelight.1"

echo "installed linelight under $DESTDIR$PREFIX"
echo "profiles are embedded; optional overrides live in \${XDG_CONFIG_HOME:-$HOME/.config}/linelight/profiles"
