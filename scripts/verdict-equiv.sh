#!/usr/bin/env bash
# TODO: rewrite in Evident
# scripts/verdict-equiv.sh — behavioural equivalence of two sample/sat-check
# drivers: run BOTH over each input .ev and compare (claim, sat/unsat)
# verdict sequences.
#
#   scripts/verdict-equiv.sh <ref-driver.smt2> <cand-driver.smt2> [input.ev …]
#
# With no inputs, runs every tests/lang_tests/*.ev. Sample wire protocol:
# stdin line 1 = flattened input path; the driver prints a check-sat
# program; `z3 -in` yields one sat/unsat/unknown per claim, paired with
# the `;; claim: <name>` headers. Exit 0 = all inputs agree.
#
# env: VE_RUN_TIMEOUT  per driver run, s (default 300)

set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL="$ROOT/kernel/target/release/kernel"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
RUN_TIMEOUT="${VE_RUN_TIMEOUT:-300}"

[ $# -ge 2 ] || { echo "usage: $0 <ref.smt2> <cand.smt2> [input.ev …]" >&2; exit 2; }
REF="$1"; CAND="$2"; shift 2
[ -f "$REF" ]  || { echo "verdict-equiv: missing $REF" >&2; exit 2; }
[ -f "$CAND" ] || { echo "verdict-equiv: missing $CAND" >&2; exit 2; }
[ -x "$KERNEL" ] || { echo "verdict-equiv: kernel not built" >&2; exit 2; }
command -v z3 >/dev/null || { echo "verdict-equiv: z3 not on PATH" >&2; exit 2; }

if [ $# -eq 0 ]; then set -- "$ROOT"/tests/lang_tests/*.ev; fi

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

# verdicts <driver.smt2> <flat-input> → "name=sat" lines (claim order)
verdicts() {
    local driver="$1" flat="$2" prog="$TMP/prog.smt2"
    printf '%s\n' "$flat" \
        | timeout "$RUN_TIMEOUT" "$KERNEL" "$driver" 2>/dev/null \
        | grep -v '^\[functionizer\]' > "$prog"
    [ "${PIPESTATUS[1]}" -eq 0 ] || return 1
    [ -s "$prog" ] || return 1
    local names verds
    names="$(grep '^;; claim: ' "$prog" | sed 's/^;; claim: //')"
    [ -n "$names" ] || return 1
    verds="$(z3 -in < "$prog" 2>/dev/null | grep -E '^(sat|unsat|unknown)$')"
    paste -d= <(printf '%s\n' "$names") <(printf '%s\n' "$verds")
}

agree=0; differ=0; ref_fail=0; cand_fail=0
for input in "$@"; do
    base="$(basename "$input")"
    flat="$TMP/in.ev"
    "$FLATTEN" "$input" > "$flat" 2>/dev/null \
        || { echo "SKIP  $base (flatten failed)"; continue; }
    ref_v="$(verdicts "$REF" "$flat")" \
        || { echo "REF-FAIL  $base"; ref_fail=$((ref_fail+1)); continue; }
    cand_v="$(verdicts "$CAND" "$flat")" \
        || { echo "CAND-FAIL $base"; cand_fail=$((cand_fail+1)); continue; }
    if [ "$ref_v" = "$cand_v" ]; then
        echo "AGREE $base ($(printf '%s\n' "$ref_v" | grep -c .) claims)"
        agree=$((agree+1))
    else
        echo "DIFFER $base"
        diff <(printf '%s\n' "$ref_v") <(printf '%s\n' "$cand_v") | sed 's/^/    /'
        differ=$((differ+1))
    fi
done

echo "---"
echo "verdict-equiv: $agree agree, $differ differ, $ref_fail ref-fail, $cand_fail cand-fail"
[ "$differ" -eq 0 ] && [ "$cand_fail" -eq 0 ] && [ "$agree" -gt 0 ]
