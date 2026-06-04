#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# run-kernel-tests.sh — drive every tests/kernel/test_*.ev through
# `evident emit` + `kernel`, checking the `-- expect:` header comments.
# Faithful Bash port of the former scripts/run-kernel-tests.py.
#
# Each fixture has a header comment block describing expected stdout +
# exit code:
#
#     -- expect: stdout = "hello world"
#     -- expect: exit = 0
#
# Multiple `expect: stdout` lines stack into a multi-line expected output.
# Missing `expect:` lines default to "stdout = '', exit = 0". When the
# expected stdout is empty the stdout check is skipped (only exit checked).
#
# Conventions:
# - Each .ev file declares a top-level claim named after the file
#   (`test_hello.ev` -> claim `hello`, drop the `test_` prefix).
# - Or the file may use `main`; both are tried (also `hello`, `app`).

set -u -o pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EVIDENT="$("$ROOT/scripts/evident-self" bin)"
KERNEL="$ROOT/kernel/target/release/kernel"
TESTS="$ROOT/tests/kernel"

if [ ! -x "$EVIDENT" ]; then
    echo "evident binary missing at $EVIDENT" >&2
    exit 1
fi
if [ ! -x "$KERNEL" ]; then
    echo "kernel binary missing at $KERNEL" >&2
    exit 1
fi

# Parse the `-- expect:` headers of a file.
# Sets globals: EXP_STDOUT (joined with newlines) and EXP_EXIT.
parse_expectations() {
    local src_file="$1"
    EXP_STDOUT=""
    EXP_EXIT=0
    local first_stdout=1
    local stdout_re='^--[[:space:]]*expect:[[:space:]]*stdout[[:space:]]*=[[:space:]]*(.*)$'
    local exit_re='^--[[:space:]]*expect:[[:space:]]*exit[[:space:]]*=[[:space:]]*(-?[0-9]+)'
    local line val
    while IFS= read -r line || [ -n "$line" ]; do
        if [[ $line =~ $stdout_re ]]; then
            val="${BASH_REMATCH[1]}"
            # strip trailing whitespace (.strip() in the Python)
            val="${val%"${val##*[![:space:]]}"}"
            # strip one matched pair of surrounding double quotes
            if [[ ${#val} -ge 2 && ${val:0:1} == '"' && ${val: -1} == '"' ]]; then
                val="${val:1:${#val}-2}"
            fi
            if [ "$first_stdout" -eq 1 ]; then
                EXP_STDOUT="$val"
                first_stdout=0
            else
                EXP_STDOUT="$EXP_STDOUT"$'\n'"$val"
            fi
        elif [[ $line =~ $exit_re ]]; then
            EXP_EXIT="${BASH_REMATCH[1]}"
        fi
    done < "$src_file"
}

# Pick the top-level claim to emit.
guess_claim_name() {
    local src_file="$1" stem="$2"
    local natural="${stem#test_}"
    local c
    for c in "$natural" main hello app; do
        if grep -qE "^[[:space:]]*(claim|fsm|type|schema)[[:space:]]+${c}([^a-zA-Z0-9_]|\$)" "$src_file"; then
            echo "$c"
            return
        fi
    done
    echo "$natural"
}

# Per-fixture input-file / stdin setup, mirroring the Python harness.
setup_fixture() {
    local name="$1"
    STDIN_TEXT=""
    HAS_STDIN=0
    case "$name" in
        test_file_io.ev)
            printf 'file roundtrip\n' > /tmp/evident_kernel_io_input.txt
            rm -f /tmp/evident_kernel_io_output.txt ;;
        test_echo_lines.ev)
            STDIN_TEXT=$'alpha\nbeta\ngamma\n'; HAS_STDIN=1 ;;
        test_file_lexer.ev)
            printf '(7+3)\n' > /tmp/evident_lex_input.txt ;;
        test_multichar_ident.ev)
            printf 'abc def\n' > /tmp/evident_multichar_input.txt ;;
        test_multichar_int.ev)
            printf '12+345\n' > /tmp/evident_digits_input.txt ;;
        test_keyword_lexer.ev)
            printf 'claim hello type fsm\n' > /tmp/evident_kw_input.txt ;;
        test_full_keywords.ev)
            printf 'claim type schema fsm enum import match subclaim external matches in true false mapsto\n' \
                > /tmp/evident_full_kw_input.txt ;;
        test_comment_lexer.ev)
            printf 'x = 5 -- this is a comment\ny = 7\n' > /tmp/evident_comment_input.txt ;;
        test_consolidated_lexer.ev)
            printf 'claim x = 1\n' > /tmp/evident_consolidated_input.txt ;;
        test_eof_edges.ev)
            printf 'abc' > /tmp/evident_eof_edge_input.txt ;;
        test_crlf.ev)
            printf 'a\r\nb\n' > /tmp/evident_crlf_input.txt ;;
        test_utf8_state_carry.ev)
            printf 'a∈b' > /tmp/evident_utf8_state_carry.txt ;;
        test_compiler_driver_readfile.ev)
            printf 'claim main\n    x ∈ Int = 5' > /tmp/evident_compiler_readfile_input.ev ;;
        test_sample_driver_marker_count.ev)
            # wave 4o: the sample driver reads its source PATH from stdin
            # line 1 (per-process, no shared /tmp file). The source is the
            # proven repro (tests/lang_tests/test_cons_chain_lit.ev shape):
            # 1 enum + 1 claim whose `nums = ⟨…⟩` / `nums = ICons(…)` body
            # the walk stops on, leaving leftover tokens. Pre-4o that yielded
            # SIX `;; claim:` markers (4 empty-name + a stray `ICons`); the
            # head_is_claimkw gate + skip_junk drain make it exactly ONE.
            printf 'enum IntList = INil | ICons(Int, IntList)\nclaim sat_user_intlist\n    nums ∈ IntList\n    nums = ⟨10, 20, 30⟩\n    nums = ICons(10, ICons(20, ICons(30, INil)))\n' \
                > /tmp/evident_sample_marker_input.ev
            STDIN_TEXT=$'/tmp/evident_sample_marker_input.ev\n'; HAS_STDIN=1 ;;
    esac
}

# Run one fixture. Echoes a status line; returns 0 on pass, 1 on fail.
run_one() {
    local path="$1"
    local name stem claim smt rc actual
    name="$(basename "$path")"
    stem="${name%.ev}"

    parse_expectations "$path"
    claim="$(guess_claim_name "$path" "$stem")"
    setup_fixture "$name"

    smt="$(mktemp -t evident_kernel.XXXXXX.smt2)"
    # mktemp on macOS may ignore the suffix; that's fine — the path is passed through.

    local emit_err
    emit_err="$("$EVIDENT" emit "$path" "$claim" -o "$smt" 2>&1 >/dev/null)"
    rc=$?
    if [ "$rc" -ne 0 ]; then
        rm -f "$smt"
        printf '  \xe2\x9c\x97 %s: emit failed: %s\n' "$name" "$(printf '%s' "$emit_err" | head -c 400)"
        return 1
    fi

    if [ "$HAS_STDIN" -eq 1 ]; then
        actual="$(printf '%s' "$STDIN_TEXT" | "$KERNEL" "$smt" 2>/tmp/evident_kernel_stderr.$$)"
    else
        actual="$("$KERNEL" "$smt" 2>/tmp/evident_kernel_stderr.$$ </dev/null)"
    fi
    rc=$?
    local kstderr; kstderr="$(cat /tmp/evident_kernel_stderr.$$ 2>/dev/null)"
    rm -f "$smt" /tmp/evident_kernel_stderr.$$

    # $(...) strips trailing newlines from actual, matching rstrip("\n").
    if [ -n "$EXP_STDOUT" ] && [ "$actual" != "$EXP_STDOUT" ]; then
        printf '  \xe2\x9c\x97 %s: stdout mismatch:\n    expected: %q\n    got:      %q\n    stderr:   %q\n' \
            "$name" "$EXP_STDOUT" "$actual" "$kstderr"
        return 1
    fi
    if [ "$rc" -ne "$EXP_EXIT" ]; then
        printf '  \xe2\x9c\x97 %s: exit mismatch: expected %s, got %s; stderr: %q\n' \
            "$name" "$EXP_EXIT" "$rc" "$kstderr"
        return 1
    fi
    printf '  \xe2\x9c\x93 %s\n' "$name"
    return 0
}

shopt -s nullglob
files=("$TESTS"/test_*.ev)
IFS=$'\n' files=($(printf '%s\n' "${files[@]}" | sort))
unset IFS

# Parallelize by fixture. Each `run_one` invocation is independent —
# it shells out to evident emit + kernel and prints a single ✓/✗ line.
# We export every helper run_one needs so they're visible in the child shells.
export EVIDENT KERNEL TESTS ROOT
export -f parse_expectations guess_claim_name setup_fixture run_one

PAR="${EVIDENT_KERNEL_PAR:-$(sysctl -n hw.activecpu 2>/dev/null || echo 4)}"
if [ "$PAR" -gt "${#files[@]}" ]; then PAR=${#files[@]}; fi

# Per-file invocation writes the ✓/✗ line and returns 0/1 via exit code.
# Aggregate failures by line-checking the stdout (✗ markers) so we don't
# need to sum codes across processes.
output_file="$(mktemp -t evident_kernel_results.XXXXXX)"
printf '%s\n' "${files[@]}" \
    | xargs -P "$PAR" -I{} bash -c 'run_one "$@" || true' _ {} \
    > "$output_file" 2>&1

cat "$output_file"
nfiles=${#files[@]}
failed=$(grep -c $'\xe2\x9c\x97' "$output_file" 2>/dev/null) || true
[ -z "$failed" ] && failed=0
rm -f "$output_file"

echo "$nfiles kernel tests, $failed failed"
[ "$failed" -eq 0 ]
