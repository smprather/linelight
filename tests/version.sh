#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
root=$(pwd -P)

fail() {
    echo "version.sh: $*" >&2
    exit 1
}

version=$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)
printf '%s\n' "$version" |
    grep -Eq '^[0-9]{4}\.([1-9]|1[0-2])\.[1-9][0-9]*$' ||
    fail "Cargo.toml version '$version' is not YYYY.M.N"

grep -Fq "\"linelight $version\"" linelight.1 ||
    fail "linelight.1 does not name version $version"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git -C "$tmp" init -q
git -C "$tmp" -c user.email=version@test -c user.name=version \
    -c commit.gpgsign=false commit -q --allow-empty -m init

expect() {
    want=$1
    month=$2
    got=$(cd "$tmp" && "$root/scripts/next-version.sh" "$month")
    [ "$got" = "$want" ] ||
        fail "next-version.sh $month: got '$got', want '$want'"
}

expect 2026.11.1 2026.11

git -C "$tmp" tag v2026.11.1
git -C "$tmp" tag v2026.11.2
git -C "$tmp" tag v2026.11.9
git -C "$tmp" tag v2026.11.10-1
git -C "$tmp" tag v2026.12.4

expect 2026.11.10 2026.11
expect 2026.12.5 2026.12

git -C "$tmp" tag v2026.10.30
git -C "$tmp" tag v2026.10.31
git -C "$tmp" tag v2026.10.31-1

expect 2026.10.32 2026.10

if (cd "$tmp" && "$root/scripts/next-version.sh" 2026.13 >/dev/null 2>&1); then
    fail "next-version.sh accepted month 13"
fi

if (cd "$tmp" && "$root/scripts/next-version.sh" 2026.01 >/dev/null 2>&1); then
    fail "next-version.sh accepted a leading-zero month"
fi

# A shallow clone at an untagged HEAD sees none of the older release tags.
origin="$tmp/origin"
git init -q "$origin"
git -C "$origin" -c user.email=version@test -c user.name=version \
    -c commit.gpgsign=false commit -q --allow-empty -m one
git -C "$origin" tag v2026.10.31
git -C "$origin" -c user.email=version@test -c user.name=version \
    -c commit.gpgsign=false commit -q --allow-empty -m two

git clone -q --depth 1 "file://$origin" "$tmp/shallow"
if (cd "$tmp/shallow" && "$root/scripts/next-version.sh" 2026.10 >/dev/null 2>&1); then
    fail "next-version.sh accepted a shallow clone"
fi
(cd "$tmp/shallow" && "$root/scripts/next-version.sh" 2026.10 2>&1) |
    grep -q shallow ||
    fail "next-version.sh gave no shallow-clone diagnostic"

git clone -q --no-tags "file://$origin" "$tmp/notags"
if (cd "$tmp/notags" && "$root/scripts/next-version.sh" 2026.10 >/dev/null 2>&1); then
    fail "next-version.sh accepted a --no-tags clone"
fi
(cd "$tmp/notags" && "$root/scripts/next-version.sh" 2026.10 2>&1) |
    grep -q no-tags ||
    fail "next-version.sh gave no --no-tags diagnostic"
