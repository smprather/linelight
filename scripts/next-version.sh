#!/bin/sh
set -eu

usage() {
    echo "usage: next-version.sh [YYYY.M]" >&2
    exit 2
}

# Versions are YYYY.M.N. M has no leading zero (semver forbids it) and N is a
# per-month counter that starts at 1 in a month with no release tags.
month=${1:-$(date -u +%Y.%-m)}
if ! printf '%s\n' "$month" | grep -Eq '^[0-9]{4}\.([1-9]|1[0-2])$'; then
    usage
fi

git rev-parse --git-dir >/dev/null 2>&1 || {
    echo "next-version.sh: not a git repository" >&2
    exit 1
}

# A shallow or tagless clone cannot see older release tags; computing from it
# would silently reuse or regress a version.
if [ "$(git rev-parse --is-shallow-repository)" = true ]; then
    echo "next-version.sh: shallow clone; run 'git fetch --unshallow --tags' first" >&2
    exit 1
fi
if [ "$(git config --get remote.origin.tagOpt || true)" = "--no-tags" ]; then
    echo "next-version.sh: remote.origin.tagOpt is --no-tags; fetch tags and unset it first" >&2
    exit 1
fi

max=0
for tag in $(git tag -l "v$month.*"); do
    n=${tag##*.}
    case $n in
        ''|0*|*[!0-9]*) continue ;;
    esac
    [ "$tag" = "v$month.$n" ] || continue
    if [ "$n" -gt "$max" ]; then
        max=$n
    fi
done

echo "$month.$((max + 1))"
