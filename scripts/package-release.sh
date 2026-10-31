#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

binary=${1:?usage: package-release.sh BINARY VERSION [OUTDIR]}
version=${2:?usage: package-release.sh BINARY VERSION [OUTDIR]}
out=${3:-dist}
version=${version#v}

test -x "$binary"
name="linelight-$version-x86_64-unknown-linux-gnu"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/$name/bin" "$out"
cp "$binary" "$tmp/$name/bin/linelight"
cp README.md DESIGN.md LICENSE install.sh linelight.1 "$tmp/$name/"
cp -a docs profiles "$tmp/$name/"
tar -C "$tmp" -czf "$out/$name.tar.gz" "$name"
(
    cd "$out"
    sha256sum "$name.tar.gz" > "$name.tar.gz.sha256"
)
echo "$out/$name.tar.gz"
