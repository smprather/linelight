#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

tmp=$(mktemp -d)
trap 'chmod -R u+w "$tmp" 2>/dev/null || true; rm -rf "$tmp"' EXIT

PREFIX="$tmp/prefix with spaces" ./install.sh
bin="$tmp/prefix with spaces/bin/linelight"

test -x "$bin"
test -f "$tmp/prefix with spaces/share/man/man1/linelight.1"

"$bin" --version >/dev/null
"$bin" check >/dev/null
"$bin" init bash > "$tmp/init.bash"
bash -n "$tmp/init.bash"
LINELIGHT_INIT=0 "$bin" init bash > "$tmp/init.off"
test ! -s "$tmp/init.off"

printf '+x\n' > "$tmp/in"
env -i HOME=/nonexistent XDG_CONFIG_HOME=/nonexistent PATH=/usr/bin:/bin \
    "$bin" filter --profile diff --color always < "$tmp/in" > "$tmp/out"
grep -q "$(printf '\033')\[0;32m" "$tmp/out"

chmod 0555 "$tmp/prefix with spaces/bin" "$bin"
env -i HOME=/nonexistent XDG_CONFIG_HOME=/nonexistent PATH=/usr/bin:/bin \
    "$bin" filter --profile log --color always < /dev/null > /dev/null

echo "deploy test ok"
