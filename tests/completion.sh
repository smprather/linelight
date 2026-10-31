#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

BIN=${1:-target/release/linelight}
test -x "$BIN"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/home" "$tmp/xdg"
ln -s "$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")" "$tmp/bin/linelight"

for shell in bash zsh tcsh fish; do
    command -v "$shell" >/dev/null 2>&1 || {
        echo "completion.sh: $shell is required" >&2
        exit 1
    }
done

"$BIN" init bash > "$tmp/init.bash"
HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/xdg" PATH="$tmp/bin:$PATH" \
    LL_INIT="$tmp/init.bash" bash --noprofile --norc -c '
        _ll_complete() { COMPREPLY=("MARKER"); }
        complete -F _ll_complete df
        complete -F _ll_complete ps
        source "$LL_INIT"
        declare -F df >/dev/null || exit 1
        declare -F ps >/dev/null || exit 1
        complete -p df | grep -q -- "-F _ll_complete" || exit 1
        complete -p ps | grep -q -- "-F _ll_complete" || exit 1
    '

"$BIN" init zsh > "$tmp/init.zsh"
HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/xdg" PATH="$tmp/bin:$PATH" \
    LL_INIT="$tmp/init.zsh" zsh -f -c '
        autoload -Uz compinit
        compinit -C
        _ll_complete() { _arguments; }
        compdef _ll_complete df
        compdef _ll_complete ps
        source "$LL_INIT"
        (( $+functions[df] )) || exit 1
        (( $+functions[ps] )) || exit 1
        [[ ${_comps[df]} == _ll_complete ]] || exit 1
        [[ ${_comps[ps]} == _ll_complete ]] || exit 1
    '

"$BIN" init tcsh > "$tmp/init.tcsh"
HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/xdg" PATH="$tmp/bin:$PATH" \
    LL_BIN="$tmp/bin" LL_INIT="$tmp/init.tcsh" tcsh -f -c '
        set path = ($LL_BIN $path)
        complete df "p/1/(MARKER_DF)/"
        complete ps "p/1/(MARKER_PING)/"
        source "$LL_INIT"
        complete df | grep -q MARKER_DF || exit 1
        complete ps | grep -q MARKER_PING || exit 1
    '

# README documents `eval "`linelight init tcsh`"` in ~/.tcshrc. Exercise that
# exact form: tcsh joins the substitution into one line before eval parses it.
HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/xdg" PATH="$tmp/bin:$PATH" \
    LL_BIN="$tmp/bin" tcsh -f -c '
        set path = ($LL_BIN $path)
        complete df "p/1/(MARKER_DF)/"
        complete ps "p/1/(MARKER_PING)/"
        eval "`linelight init tcsh`"
        alias df | grep -q -- "--profile df" && alias ps | grep -q -- "--profile ps" && complete df | grep -q MARKER_DF && complete ps | grep -q MARKER_PING
    '

"$BIN" init fish > "$tmp/init.fish"
HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/xdg" PATH="$tmp/bin:$PATH" \
    LL_BIN="$tmp/bin" LL_INIT="$tmp/init.fish" fish --no-config -c '
        set -gx PATH $LL_BIN $PATH
        complete -c df -a MARKER_DF
        complete -c ps -a MARKER_PING
        source "$LL_INIT"
        complete -C "df " | string match -q "*MARKER_DF*"; or exit 1
        complete -C "ps " | string match -q "*MARKER_PING*"; or exit 1
    '

echo "completion test ok"
