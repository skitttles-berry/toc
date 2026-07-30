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
smoke_expected=
smoke_clipboard_backup=
smoke_clipboard_verify=
smoke_clipboard_backed_up=false
smoke_expect_pid=
smoke_signal_pending=false
cleanup() {
    smoke_cleanup_status=$?
    trap - EXIT HUP INT TERM
    if [ "$smoke_clipboard_backed_up" = true ]; then
        if ! pbcopy <"$smoke_clipboard_backup"; then
            printf 'shell smoke failed: could not restore macOS clipboard\n' >&2
            smoke_cleanup_status=1
        elif ! pbpaste >"$smoke_clipboard_verify"; then
            printf 'shell smoke failed: could not verify restored macOS clipboard\n' >&2
            smoke_cleanup_status=1
        elif ! cmp -s "$smoke_clipboard_backup" "$smoke_clipboard_verify"; then
            printf 'shell smoke failed: restored macOS clipboard did not match backup\n' >&2
            smoke_cleanup_status=1
        fi
    fi
    if [ -n "$smoke_tmp" ] && [ -d "$smoke_tmp" ]; then
        [ -z "$smoke_input" ] || rm -f -- "$smoke_input"
        [ -z "$smoke_output" ] || rm -f -- "$smoke_output"
        [ -z "$smoke_error" ] || rm -f -- "$smoke_error"
        [ -z "$smoke_expected" ] || rm -f -- "$smoke_expected"
        [ -z "$smoke_clipboard_backup" ] || rm -f -- "$smoke_clipboard_backup"
        [ -z "$smoke_clipboard_verify" ] || rm -f -- "$smoke_clipboard_verify"
        rmdir -- "$smoke_tmp"
    fi
    exit "$smoke_cleanup_status"
}

handle_pending_signal() {
    smoke_signal_pending=true
}

handle_signal() {
    trap - HUP INT TERM
    if [ -n "$smoke_expect_pid" ]; then
        kill -TERM "$smoke_expect_pid" 2>/dev/null || :
        wait "$smoke_expect_pid" 2>/dev/null || :
        smoke_expect_pid=
    fi
    exit 1
}

smoke_tmp=$(mktemp -d "${TMPDIR:-/tmp}/doop-smoke.XXXXXX")
trap cleanup EXIT
trap handle_signal HUP INT TERM
smoke_input="$smoke_tmp/input.txt"
smoke_output="$smoke_tmp/output.txt"
smoke_error="$smoke_tmp/error.txt"
smoke_expected="$smoke_tmp/expected.txt"
smoke_clipboard_verify="$smoke_tmp/clipboard-verify.txt"
smoke_clipboard_expected=68656c6c6f
smoke_os=$(uname -s)

if [ -n "${BASH_VERSION:-}" ]; then
    smoke_shell_kind=bash
elif [ -n "${ZSH_VERSION:-}" ]; then
    smoke_shell_kind=zsh
    unsetopt BG_NICE
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
    smoke_signal_pending=false
    trap handle_pending_signal HUP INT TERM
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
        proc prepare_hex_preview {} {
            global env spawn_id
            send -- "\033\[200~hello\033\[201~"
            expect_exact "hello" 93 94
            send -- "\020"
            expect_exact "Search:" 119 120
            send -- "hex-encode\r"
            expect_exact $env(DOOP_SMOKE_CLIPBOARD_EXPECTED) 121 122
        }
        proc read_clipboard {mode failure_code} {
            global spawn_id
            set timeout 15
            set tui_spawn_id $spawn_id
            if {$mode eq "macos"} {
                set command [list pbpaste]
            } else {
                set command [list wl-paste --no-newline]
            }
            if {[catch {spawn -noecho {*}$command} reader_pid]} {
                set spawn_id $tui_spawn_id
                exit $failure_code
            }
            expect {
                eof { set copied $expect_out(buffer) }
                timeout {
                    catch {exec kill -TERM $reader_pid}
                    after 100
                    catch {exec kill -KILL $reader_pid}
                    catch {close}
                    catch {wait}
                    set spawn_id $tui_spawn_id
                    exit $failure_code
                }
            }
            if {[catch {wait} result]} {
                set spawn_id $tui_spawn_id
                exit $failure_code
            }
            set spawn_id $tui_spawn_id
            if {[lindex $result 2] != 0 || [lindex $result 3] != 0} {
                exit $failure_code
            }
            return $copied
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
                prepare_hex_preview
                stty rows 5 columns 30 < $replica
                expect_exact "Increase" 95 96
                stty rows 24 columns 120 < $replica
                expect_exact $env(DOOP_SMOKE_CLIPBOARD_EXPECTED) 109 110
                confirm_discard
                send -- "y"
            } elseif {$mode eq "interrupt"} {
                send -- "\003"
            } else {
                prepare_hex_preview
                send -- "\t\r"
                if {$mode eq "unavailable"} {
                    expect_exact "Clipboard" 101 102
                    expect_exact "unavailable" 101 102
                } elseif {$mode eq "x11"} {
                    expect_exact "Copied" 103 104
                    if {[catch {exec timeout 5s xclip -selection clipboard -o} copied]} {
                        exit 105
                    }
                    if {$copied ne $env(DOOP_SMOKE_CLIPBOARD_EXPECTED)} {
                        exit 106
                    }
                } elseif {$mode eq "macos"} {
                    expect {
                        -exact "Copied" {}
                        -exact "Clipboard" {
                            expect_exact "unavailable" 123 123
                            exit 123
                        }
                        eof { exit 127 }
                        timeout { exit 128 }
                    }
                    set copied [read_clipboard macos 124]
                    if {$copied ne $env(DOOP_SMOKE_CLIPBOARD_EXPECTED)} {
                        exit 125
                    }
                } elseif {$mode eq "wayland"} {
                    expect {
                        -exact "Copied" {}
                        -exact "Clipboard" {
                            expect_exact "unavailable" 125 125
                            exit 125
                        }
                        eof { exit 128 }
                        timeout { exit 129 }
                    }
                    set copied [read_clipboard wayland 126]
                    if {$copied ne $env(DOOP_SMOKE_CLIPBOARD_EXPECTED)} {
                        exit 127
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
    ' &
    smoke_expect_pid=$!
    trap handle_signal HUP INT TERM
    if [ "$smoke_signal_pending" = true ]; then
        handle_signal
    fi
    wait "$smoke_expect_pid"
    smoke_expect_status=$?
    smoke_expect_pid=
    return "$smoke_expect_status"
}

case "$smoke_os" in
    Darwin | Linux) ;;
    *) fail "unsupported smoke-test operating system" ;;
esac

command -v expect >/dev/null 2>&1 || fail "expect command is required"
export CARGO_TARGET_DIR="$smoke_target_dir"
export DOOP_SMOKE_SHELL="$smoke_shell"
export DOOP_SMOKE_SHELL_KIND="$smoke_shell_kind"
export DOOP_SMOKE_BIN="$smoke_bin"
export DOOP_SMOKE_INPUT="$smoke_input"
export DOOP_SMOKE_OUTPUT="$smoke_output"
export DOOP_SMOKE_ERROR="$smoke_error"
export DOOP_SMOKE_CLIPBOARD_EXPECTED="$smoke_clipboard_expected"
cargo build --manifest-path "$smoke_root/Cargo.toml" >/dev/null
[ -x "$smoke_bin" ] || fail "cargo did not create the expected binary"

actual=$(printf 'hello' | "$smoke_bin" base64-encode)
assert_eq 'aGVsbG8=' "$actual" "pipe input"

actual=$(printf '4869' | "$smoke_bin" hex-decode)
assert_eq 'Hi' "$actual" "hex decode pipe input"

actual=$(printf 'Hi' | "$smoke_bin" hex-encode)
assert_eq '4869' "$actual" "hex encode pipe input"

printf '\xffA' >"$smoke_input"
pty_run '
    "$DOOP_SMOKE_BIN" hex-encode --input "$DOOP_SMOKE_INPUT" >"$DOOP_SMOKE_OUTPUT"
' \
    >/dev/null </dev/null
actual=$(<"$smoke_output")
assert_eq 'ff41' "$actual" "hex encode binary file input"

actual=$(
    printf '%s' '%7B%22a%22%3A1%7D' |
        "$smoke_bin" url-decode --then format-json
)
expected='{
  "a": 1
}'
assert_eq "$expected" "$actual" "transform chain"

printf 'file' >"$smoke_input"
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

printf '411b5b324a' >"$smoke_input"
: >"$smoke_output"
: >"$smoke_error"
set +e
pty_run '"$DOOP_SMOKE_BIN" hex-decode --input "$DOOP_SMOKE_INPUT" 2>"$DOOP_SMOKE_ERROR"' \
    >"$smoke_output" </dev/null
exit_status=$?
set -e
assert_eq '4' "$exit_status" "unsafe terminal output status"
[ ! -s "$smoke_output" ] || fail "unsafe terminal output wrote stdout"
actual=$(<"$smoke_error")
expected='Refusing unsafe terminal output (preview: A\x1b[2J); redirect stdout to preserve raw output'
assert_eq "$expected" "$actual" "unsafe terminal output message"

: >"$smoke_error"
set +e
pty_run '"$DOOP_SMOKE_BIN" hex-decode --input "$DOOP_SMOKE_INPUT" >"$DOOP_SMOKE_OUTPUT" 2>"$DOOP_SMOKE_ERROR"' \
    >/dev/null </dev/null
exit_status=$?
set -e
assert_eq '0' "$exit_status" "redirected unsafe output status"
[ ! -s "$smoke_error" ] || fail "redirected unsafe output wrote stderr"
printf 'A\033[2J' >"$smoke_expected"
cmp -s "$smoke_expected" "$smoke_output" ||
    fail "redirected unsafe output did not preserve control bytes"

smoke_missing=$(printf '%s/missing\n\033[2J' "$smoke_tmp")
export DOOP_SMOKE_MISSING="$smoke_missing"
: >"$smoke_output"
: >"$smoke_error"
set +e
pty_run '"$DOOP_SMOKE_BIN" hex-encode --input "$DOOP_SMOKE_MISSING" 2>"$DOOP_SMOKE_ERROR"' \
    >"$smoke_output" </dev/null
exit_status=$?
set -e
assert_eq '3' "$exit_status" "missing PTY file input status"
[ ! -s "$smoke_output" ] || fail "missing PTY file input wrote stdout"
actual=$(<"$smoke_error")
expected=$(printf 'Could not open input file: %s/missing\\x0a\\x1b[2J' "$smoke_tmp")
assert_eq "$expected" "$actual" "missing PTY file input message"

printf 'file' >"$smoke_input"
: >"$smoke_output"
: >"$smoke_error"
set +e
printf '' |
    "$smoke_bin" base64-encode --input "$smoke_input" >"$smoke_output" 2>"$smoke_error"
exit_status=$?
set -e
assert_eq '2' "$exit_status" "empty pipe and file input status"
[ ! -s "$smoke_output" ] || fail "empty pipe and file input wrote stdout"
actual=$(<"$smoke_error")
assert_eq 'Use stdin or --input PATH, not both' "$actual" "empty pipe and file input message"

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
    macos)
        [ "$smoke_os" = Darwin ] || fail "macOS clipboard smoke requires macOS"
        command -v pbpaste >/dev/null 2>&1 || fail "pbpaste command is required"
        command -v pbcopy >/dev/null 2>&1 || fail "pbcopy command is required"
        smoke_clipboard_backup="$smoke_tmp/clipboard.txt"
        if ! pbpaste >"$smoke_clipboard_backup"; then
            fail "macOS clipboard text backup failed"
        fi
        smoke_clipboard_backed_up=true
        set +e
        tui_run macos
        exit_status=$?
        set -e
        case "$exit_status" in
            123) fail "macOS clipboard product copy reported unavailable" ;;
            124) fail "macOS clipboard verification failed: pbpaste could not read copied text" ;;
            125) fail "macOS clipboard product copy did not match expected text" ;;
        esac
        assert_eq '130' "$exit_status" "macOS clipboard path"
        ;;
    wayland)
        [ "$smoke_os" = Linux ] || fail "Wayland clipboard smoke requires Linux"
        [ -n "${WAYLAND_DISPLAY:-}" ] ||
            fail "Wayland clipboard environment unavailable: WAYLAND_DISPLAY is not set"
        [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -d "$XDG_RUNTIME_DIR" ] ||
            fail "Wayland clipboard environment unavailable: XDG_RUNTIME_DIR is not usable"
        command -v wl-paste >/dev/null 2>&1 ||
            fail "Wayland clipboard environment unavailable: wl-paste is required"
        set +e
        tui_run wayland
        exit_status=$?
        set -e
        case "$exit_status" in
            130) ;;
            125)
                fail "Wayland clipboard product copy reported unavailable"
                ;;
            126)
                fail "Wayland clipboard environment failed: wl-paste data-control read unavailable"
                ;;
            127) fail "Wayland clipboard product copy did not match expected text" ;;
            *)
                fail "Wayland clipboard product screen/copy path failed with Expect code $exit_status"
                ;;
        esac
        ;;
    *) fail "unknown clipboard smoke mode" ;;
esac

printf 'shell smoke passed\n'
