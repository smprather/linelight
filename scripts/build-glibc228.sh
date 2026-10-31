#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

IMAGE=${IMAGE:-linelight-build:glibc228}
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    docker build -f Dockerfile.glibc228 -t "$IMAGE" .
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
docker run --rm \
    -v "$PWD":/src:ro \
    -v "$tmp":/out \
    "$IMAGE" sh -c '
        mkdir /work
        cd /src
        tar --exclude=target --exclude=.git -cf - . | (cd /work && tar xf -)
        cd /work
        cargo build --release --locked
        cp target/release/linelight /out/linelight
    '

binary="$tmp/linelight"
test -x "$binary"
max=$(objdump -T "$binary" | grep -o 'GLIBC_[0-9.]*' | sed 's/^GLIBC_//' | sort -V | tail -1)
test -n "$max"
test "$(printf '%s\n' "$max" 2.28 | sort -V | tail -1)" = "2.28"
allowed='libc.so.6 libm.so.6 libpthread.so.0 libdl.so.2 libgcc_s.so.1 ld-linux-x86-64.so.2'
for needed in $(objdump -p "$binary" | awk '/NEEDED/{print $2}'); do
    case " $allowed " in
        *" $needed "*) ;;
        *) echo "unexpected NEEDED: $needed" >&2; exit 1 ;;
    esac
done
mkdir -p target/release
cp "$binary" target/release/linelight-glibc228
version=$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)
./scripts/package-release.sh target/release/linelight-glibc228 "$version" dist
echo "glibc 2.28 build ok (max GLIBC_$max)"
