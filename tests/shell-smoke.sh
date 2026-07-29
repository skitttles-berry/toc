#!/usr/bin/env bash
set -eu

smoke_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
smoke_target_dir=${CARGO_TARGET_DIR:-"$smoke_root/target"}
case "$smoke_target_dir" in
    /*) ;;
    *) smoke_target_dir="$PWD/$smoke_target_dir" ;;
esac
smoke_bin="$smoke_target_dir/debug/doop"

smoke_tmp=
smoke_input=
smoke_output=
smoke_error=
cleanup() {
    if [ -n "$smoke_tmp" ] && [ -d "$smoke_tmp" ]; then
        [ -z "$smoke_input" ] || rm -f -- "$smoke_input"
        [ -z "$smoke_output" ] || rm -f -- "$smoke_output"
        [ -z "$smoke_error" ] || rm -f -- "$smoke_error"
        rmdir -- "$smoke_tmp"
    fi
}

smoke_tmp=$(mktemp -d "${TMPDIR:-/tmp}/doop-smoke.XXXXXX")
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
smoke_input="$smoke_tmp/input.txt"
smoke_output="$smoke_tmp/output.txt"
smoke_error="$smoke_tmp/error.txt"
smoke_input_marker="paste-smoke-$$"
smoke_os=$(uname -s)

if [ -n "${BASH_VERSION:-}" ]; then
    smoke_shell_kind=bash
elif [ -n "${ZSH_VERSION:-}" ]; then
    smoke_shell_kind=zsh
else
    printf 'shell smoke failed: invoke with Bash or Zsh\n' >&2
    exit 1
fi
smoke_shell=$(command -v "$smoke_shell_kind")

fail() {
    printf 'shell smoke failed: %s\n' "$1" >&2
    exit 1
}

assert_eq() {
    [ "$1" = "$2" ] || fail "$3: expected [$1], got [$2]"
}

pty_run() {
    export DOOP_SMOKE_PTY_COMMAND="$1"
    expect -c '
        log_user 1
        set timeout 15
        spawn -noecho $env(DOOP_SMOKE_SHELL) -c $env(DOOP_SMOKE_PTY_COMMAND)
        expect {
            eof {}
            timeout { exit 86 }
        }
        set result [wait]
        if {[lindex $result 2] != 0} {
            exit 87
        }
        exit [lindex $result 3]
    '
}

tui_run() {
    export DOOP_SMOKE_TUI_MODE="$1"
    expect -c '
        proc expect_exact {text eof_code timeout_code} {
            global spawn_id
            expect {
                -exact $text {}
                eof { exit $eof_code }
                timeout { exit $timeout_code }
            }
        }
        proc confirm_discard {} {
            global spawn_id timeout
            set previous_timeout $timeout
            set timeout 1
            for {set attempt 0} {$attempt < 3} {incr attempt} {
                send -- "\021"
                expect {
                    -exact "Discard" {
                        set timeout $previous_timeout
                        return
                    }
                    eof { exit 97 }
                    timeout {}
                }
            }
            set timeout $previous_timeout
            exit 98
        }
        proc smoke {} {
            global env spawn_id spawn_out
            log_user 0
            set timeout 15
            set command {
                case "$DOOP_SMOKE_SHELL_KIND" in
                    bash) [ -n "${BASH_VERSION:-}" ] || exit 88 ;;
                    zsh) [ -n "${ZSH_VERSION:-}" ] || exit 88 ;;
                esac
                stty rows 24 columns 120 || exit 89
                before=$(stty -g) || exit 89
                "$DOOP_SMOKE_BIN" tui
                doop_status=$?
                after=$(stty -g) || exit 89
                [ "$before" = "$after" ] || exit 90
                exit "$doop_status"
            }
            spawn -noecho $env(DOOP_SMOKE_SHELL) -c $command
            if {[info exists spawn_out(slave,name)]} {
                set replica $spawn_out(slave,name)
            } else {
                set replica $spawn_out(replica,name)
            }
            expect_exact "Input" 91 92

            set mode $env(DOOP_SMOKE_TUI_MODE)
            if {$mode eq "normal"} {
                send -- "\033\[200~$env(DOOP_SMOKE_INPUT_MARKER)\033\[201~"
                expect_exact $env(DOOP_SMOKE_INPUT_MARKER) 93 94
                stty rows 5 columns 30 < $replica
                expect_exact "Increase" 95 96
                stty rows 24 columns 120 < $replica
                expect_exact $env(DOOP_SMOKE_INPUT_MARKER) 109 110
                confirm_discard
                send -- "y"
            } elseif {$mode eq "interrupt"} {
                send -- "\003"
            } else {
                send -- "\033\[200~clipboard-smoke\033\[201~"
                expect_exact "clipboard-smoke" 93 94
                expect_exact "clipboard-smoke" 119 120
                send -- "\t\r"
                if {$mode eq "unavailable"} {
                    expect_exact "Clipboard" 101 102
                    expect_exact "unavailable" 101 102
                } elseif {$mode eq "x11"} {
                    expect_exact "Copied" 103 104
                    if {[catch {exec timeout 5s xclip -selection clipboard -o} copied]} {
                        exit 105
                    }
                    if {$copied ne "clipboard-smoke"} {
                        exit 106
                    }
                } else {
                    exit 111
                }
                send -- "\003"
            }

            expect_exact "\033\[?2004l" 112 113
            expect_exact "\033\[?1049l" 114 115
            expect {
                eof {}
                timeout { exit 116 }
            }
            set result [wait]
            if {[lindex $result 2] != 0} {
                exit 117
            }
            return [lindex $result 3]
        }
        if {[catch {smoke} result]} {
            puts stderr $result
            exit 118
        }
        exit $result
    '
}

case "$smoke_os" in
    Darwin | Linux) ;;
    *) fail "unsupported smoke-test operating system" ;;
esac

command -v expect >/dev/null 2>&1 || fail "expect command is required"
export CARGO_TARGET_DIR="$smoke_target_dir"
export DOOP_SMOKE_SHELL="$smoke_shell"
export DOOP_SMOKE_SHELL_KIND="$smoke_shell_kind"
cargo build --manifest-path "$smoke_root/Cargo.toml" >/dev/null
[ -x "$smoke_bin" ] || fail "cargo did not create the expected binary"

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
export DOOP_SMOKE_ERROR="$smoke_error"
export DOOP_SMOKE_INPUT_MARKER="$smoke_input_marker"
pty_run '
    case "$DOOP_SMOKE_SHELL_KIND" in
        bash) [ -n "${BASH_VERSION:-}" ] || exit 88 ;;
        zsh) [ -n "${ZSH_VERSION:-}" ] || exit 88 ;;
    esac
    "$DOOP_SMOKE_BIN" base64-encode --input "$DOOP_SMOKE_INPUT" >"$DOOP_SMOKE_OUTPUT"
' \
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
    set -o pipefail
    head -c 8388608 /dev/zero |
        "$smoke_bin" base64-encode |
        head -c 1 >/dev/null
)
exit_status=$?
set -e
assert_eq '0' "$exit_status" "broken pipe status"

if [ "$smoke_os" = Linux ]; then
    : >"$smoke_output"
    : >"$smoke_error"
    set +e
    pty_run '"$DOOP_SMOKE_BIN" base64-encode --input "$DOOP_SMOKE_INPUT" > /dev/full 2>"$DOOP_SMOKE_ERROR"' \
        >"$smoke_output" </dev/null
    exit_status=$?
    set -e
    assert_eq '5' "$exit_status" "/dev/full output error status"
    [ ! -s "$smoke_output" ] || fail "/dev/full output error wrote unnecessary stdout"
    actual=$(<"$smoke_error")
    assert_eq 'Could not write output' "$actual" "/dev/full output error message"
fi

set +e
tui_run normal
exit_status=$?
set -e
assert_eq '0' "$exit_status" "TUI normal exit and terminal restoration"

set +e
tui_run interrupt
exit_status=$?
set -e
assert_eq '130' "$exit_status" "TUI interrupt and terminal restoration"

case "${DOOP_SMOKE_CLIPBOARD_MODE:-skip}" in
    skip) ;;
    unavailable | x11)
        [ "$smoke_os" = Linux ] || fail "clipboard smoke is isolated to Linux"
        if [ "$DOOP_SMOKE_CLIPBOARD_MODE" = x11 ]; then
            command -v xclip >/dev/null 2>&1 || fail "xclip command is required"
        fi
        set +e
        tui_run "$DOOP_SMOKE_CLIPBOARD_MODE"
        exit_status=$?
        set -e
        assert_eq '130' "$exit_status" "$DOOP_SMOKE_CLIPBOARD_MODE clipboard path"
        ;;
    *) fail "unknown clipboard smoke mode" ;;
esac

printf 'shell smoke passed\n'
