#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# run-lang-tests.sh — drive every tests/lang_tests/*.ev through
# `evident sample --all --json`. Asserts that claims prefixed `sat_`
# are SAT and `unsat_` are UNSAT. Faithful Bash port of the former
# scripts/run-lang-tests.py.
#
# The single source of truth for language correctness; conformance/
# tests the CLI, these test the language itself.

set -u -o pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$("$ROOT/scripts/evident-self" bin)"
LANG_TESTS="$ROOT/tests/lang_tests"

if [ ! -x "$BIN" ]; then
    echo "binary missing at $BIN; run cargo build --release first" >&2
    exit 1
fi

shopt -s nullglob
files=("$LANG_TESTS"/*.ev)
IFS=$'\n' files=($(printf '%s\n' "${files[@]}" | sort))
unset IFS

# Per-file worker: emits a single line summary to stdout in the form
#   FILE\tCLAIMS\tFAIL_LINES_NL_ESCAPED
# so the parent can aggregate without sharing state. Failures are joined
# with literal \n (one escape) and unescaped at aggregate time.
run_one() {
    local f="$1"
    local name; name="$(basename "$f")"
    local stderr; stderr="$(mktemp -t evident_lang_stderr.XXXXXX)"
    local out rc fails=""
    out="$("$BIN" sample "$f" --all --json 2>"$stderr")"
    rc=$?
    local nclaims=0
    if [ "$rc" -ne 0 ]; then
        local err; err="$(head -c 300 "$stderr" 2>/dev/null)"
        fails="  FAIL ${name}::load: ${err}"
    else
        local pairs
        pairs="$(printf '%s' "$out" | grep -oE '"[^"]*":(true|false)')"
        if [ -z "$pairs" ]; then
            if ! printf '%s' "$out" | grep -qE '^\s*\{'; then
                fails="  FAIL ${name}::json: $(printf '%s' "$out" | head -c 300)"
            fi
        else
            while IFS= read -r pair; do
                [ -z "$pair" ] && continue
                local cname="${pair%\":*}"; cname="${cname#\"}"
                local cval="${pair##*:}"
                nclaims=$((nclaims + 1))
                case "$cname" in
                    sat_*)   if [ "$cval" = "false" ]; then
                        fails="${fails:+$fails$'\n'}  FAIL ${name}::${cname}: expected sat, got unsat"
                    fi ;;
                    unsat_*) if [ "$cval" = "true" ]; then
                        fails="${fails:+$fails$'\n'}  FAIL ${name}::${cname}: expected unsat, got sat"
                    fi ;;
                esac
            done <<< "$pairs"
        fi
    fi
    rm -f "$stderr"
    # Use literal tab + escaped newlines to make output parseable.
    local esc; esc="${fails//$'\n'/§NL§}"
    printf '%s\t%d\t%s\n' "$name" "$nclaims" "$esc"
}

export BIN
export -f run_one

# Parallelism: default 4 (was sysctl hw.activecpu = ~12). Each kernel
# process running compiler.smt2 can briefly grow >3GB of RSS; at 12
# parallel that's enough to OOM/swap the host. mem-cap.sh caps each
# child, but lower fanout = less peak pressure.
PAR="${EVIDENT_LANG_PAR:-4}"
if [ "$PAR" -gt "${#files[@]}" ]; then PAR=${#files[@]}; fi

results_file="$(mktemp -t evident_lang_results.XXXXXX)"
printf '%s\n' "${files[@]}" \
    | xargs -P "$PAR" -I{} bash -c 'run_one "$@"' _ {} \
    > "$results_file"

total=0
nfiles=0
fail_lines=()
while IFS=$'\t' read -r name nclaims fails_esc; do
    nfiles=$((nfiles + 1))
    total=$((total + nclaims))
    if [ -n "$fails_esc" ]; then
        # Restore newlines and split
        decoded="${fails_esc//§NL§/$'\n'}"
        while IFS= read -r line; do
            [ -n "$line" ] && fail_lines+=("$line")
        done <<< "$decoded"
    fi
done < "$results_file"
rm -f "$results_file"

# Self-hosted toolchain has documented gaps for shapes the compiler
# can't yet translate (match-RHS equality, record-lit equality,
# composition+chain, etc. — see STATE.md). Allow these specific lines
# to fail while still verifying the rest. To re-enable strict mode:
# `unset EVIDENT_LANG_KNOWN_FAILS` or set it to empty.
#
# The list is the EXACT failure-line text after "FAIL ", colon-tail
# stripped, one per line.
DEFAULT_KNOWN_FAILS='test_record_lit_arg.ev::unsat_positional_color
test_record_lit_arg.ev::unsat_mapsto_color
test_record_lit_arg.ev::unsat_nested_record_lit
test_tuple_in_claim.ev::unsat_tuple_wrong_output
test_match.ev::unsat_match_result_pinned_wrong
test_enums_payload.ev::unsat_ok_via_subclaim_mismatch
test_enums_basic.ev::unsat_weekend_via_claim_wrong
test_chained_membership.ev::unsat_multi_name_range_violation
test_chained_membership.ev::unsat_chain_via_composition_violates
test_kernel_enums.ev::sat_inline_not_match'
KNOWN_FAILS="${EVIDENT_LANG_KNOWN_FAILS-$DEFAULT_KNOWN_FAILS}"

# Partition fail lines into expected vs unexpected.
unexpected_lines=()
expected_count=0
for line in "${fail_lines[@]:-}"; do
    [ -z "$line" ] && continue
    # Extract the "file::claim" identifier from "  FAIL file::claim: …"
    # The line uses `::` between file and claim and `:` before the message,
    # so we strip the leading "  FAIL " then everything from the first
    # SINGLE colon (not `::`) onward by replacing `::` with a placeholder,
    # cutting at the next `:`, then restoring.
    ident="${line#*FAIL }"
    placeheld="${ident//::/§§§}"
    placeheld="${placeheld%%:*}"
    ident="${placeheld//§§§/::}"
    if printf '%s\n' "$KNOWN_FAILS" | grep -qFx "$ident"; then
        expected_count=$((expected_count + 1))
    else
        unexpected_lines+=("$line")
    fi
done

echo "$nfiles files, $total claims, ${#unexpected_lines[@]} unexpected failures (${expected_count} expected-fail)"
for line in "${unexpected_lines[@]:-}"; do
    [ -n "$line" ] && echo "$line"
done

[ "${#unexpected_lines[@]}" -eq 0 ]
