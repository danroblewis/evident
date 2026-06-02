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

total=0
nfiles=0
fail_lines=()

shopt -s nullglob
files=("$LANG_TESTS"/*.ev)
IFS=$'\n' files=($(printf '%s\n' "${files[@]}" | sort))
unset IFS

for f in "${files[@]}"; do
    nfiles=$((nfiles + 1))
    name="$(basename "$f")"

    out="$("$BIN" sample "$f" --all --json 2>/tmp/evident_lang_stderr.$$)"
    rc=$?
    if [ "$rc" -ne 0 ]; then
        err="$(head -c 300 /tmp/evident_lang_stderr.$$ 2>/dev/null)"
        fail_lines+=("  FAIL ${name}::load: ${err}")
        continue
    fi

    # The JSON is a single flat object: {"name":true,"name2":false,...}.
    # Keys never contain a double quote, so this extraction is exact.
    pairs="$(printf '%s' "$out" | grep -oE '"[^"]*":(true|false)')"
    if [ -z "$pairs" ]; then
        # No claims (or unparseable output); mirror the JSON-decode failure path.
        if ! printf '%s' "$out" | grep -qE '^\s*\{'; then
            fail_lines+=("  FAIL ${name}::json: $(printf '%s' "$out" | head -c 300)")
        fi
        continue
    fi

    while IFS= read -r pair; do
        [ -z "$pair" ] && continue
        cname="${pair%\":*}"; cname="${cname#\"}"
        cval="${pair##*:}"
        total=$((total + 1))
        case "$cname" in
            sat_*)
                if [ "$cval" = "false" ]; then
                    fail_lines+=("  FAIL ${name}::${cname}: expected sat, got unsat")
                fi ;;
            unsat_*)
                if [ "$cval" = "true" ]; then
                    fail_lines+=("  FAIL ${name}::${cname}: expected unsat, got sat")
                fi ;;
        esac
    done <<< "$pairs"
done
rm -f /tmp/evident_lang_stderr.$$

echo "$nfiles files, $total claims, ${#fail_lines[@]} failed"
for line in "${fail_lines[@]:-}"; do
    [ -n "$line" ] && echo "$line"
done

[ "${#fail_lines[@]}" -eq 0 ]
