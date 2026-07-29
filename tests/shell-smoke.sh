#!/usr/bin/env bash
set -eu

smoke_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
smoke_bin="$smoke_root/target/debug/doop"
smoke_tmp=$(mktemp -d "${TMPDIR:-/tmp}/doop-smoke.XXXXXX")
smoke_input="$smoke_tmp/input.txt"
smoke_output="$smoke_tmp/output.txt"
smoke_error="$smoke_tmp/error.txt"

cleanup() {
    if [ -d "$smoke_tmp" ]; then
        rm -f -- "$smoke_input" "$smoke_output" "$smoke_error"
        rmdir -- "$smoke_tmp"
    fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

fail() {
    printf 'shell smoke failed: %s\n' "$1" >&2
    exit 1
}

assert_eq() {
    [ "$1" = "$2" ] || fail "$3: expected [$1], got [$2]"
}

pty_run() {
    case "$(uname -s)" in
        Darwin)
            script -q /dev/null sh -c "$1"
            ;;
        Linux)
            script -qec "$1" /dev/null
            ;;
        *)
            fail "unsupported smoke-test operating system"
            ;;
    esac
}

command -v script >/dev/null 2>&1 || fail "script command is required"
cargo build --manifest-path "$smoke_root/Cargo.toml" >/dev/null

actual=$(printf 'hello' | "$smoke_bin" base64-encode)
assert_eq 'aGVsbG8=' "$actual" "pipe input"

actual=$(
    printf '%s' '%7B%22a%22%3A1%7D' |
        "$smoke_bin" url-decode --then format-json
)
expected='{
  "a": 1
}'
assert_eq "$expected" "$actual" "transform chain"

printf 'file' >"$smoke_input"
export DOOP_SMOKE_BIN="$smoke_bin"
export DOOP_SMOKE_INPUT="$smoke_input"
export DOOP_SMOKE_OUTPUT="$smoke_output"
pty_run '"$DOOP_SMOKE_BIN" base64-encode --input "$DOOP_SMOKE_INPUT" >"$DOOP_SMOKE_OUTPUT"' \
    >/dev/null </dev/null
actual=$(<"$smoke_output")
assert_eq 'ZmlsZQ==' "$actual" "file input"

set +e
printf '!' |
    "$smoke_bin" base64-decode >"$smoke_output" 2>"$smoke_error"
exit_status=$?
set -e
assert_eq '4' "$exit_status" "transform error status"
[ ! -s "$smoke_output" ] || fail "transform error wrote stdout"
[ -s "$smoke_error" ] || fail "transform error did not write stderr"

set +e
(
    sleep 1
    printf '\003'
) | (pty_run '
    "$DOOP_SMOKE_BIN" tui
    doop_status=$?
    terminal_state=$(stty -a)
    case " $terminal_state " in
        *" -echo "*|*" -icanon "*) exit 90 ;;
    esac
    exit "$doop_status"
') >/dev/null
exit_status=$?
set -e
assert_eq '130' "$exit_status" "TUI interrupt and terminal restoration"

printf 'shell smoke passed\n'
