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

# Parallelism: default to active CPU count (cap at #files for sanity).
PAR="${EVIDENT_LANG_PAR:-$(sysctl -n hw.activecpu 2>/dev/null || echo 4)}"
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

echo "$nfiles files, $total claims, ${#fail_lines[@]} failed"
for line in "${fail_lines[@]:-}"; do
    [ -n "$line" ] && echo "$line"
done

[ "${#fail_lines[@]}" -eq 0 ]
