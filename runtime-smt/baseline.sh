#!/usr/bin/env bash
# Baseline: for each examples/test_*.ev, run the oracle (evident effect-run) and
# the hybrid (runtime-smt fsm), compare stdout+exit byte-for-byte. Print a
# per-example verdict. SDL/GL visual demos are skipped (they need a window).
#
# Usage: runtime-smt/baseline.sh [glob]
set -u
cd "$(dirname "$0")/.." || exit 2

ORACLE=runtime/target/release/evident
HYBRID=runtime-smt/target/debug/runtime-smt
MAXSTEPS="${MAXSTEPS:-30}"
GLOB="${1:-examples/test_*.ev}"

pass=0; fail=0; gap=0
for f in $GLOB; do
    name=$(basename "$f")
    # Oracle
    o_out=$(timeout 30 "$ORACLE" effect-run "$f" --max-steps "$MAXSTEPS" 2>/dev/null)
    o_code=$?
    # Hybrid
    h_out=$(timeout 30 "$HYBRID" fsm "$f" 2>/tmp/hyberr)
    h_code=$?
    h_err=$(cat /tmp/hyberr)

    if [ -n "$h_err" ] && [ "$h_code" != 0 ] && [ -z "$h_out" ]; then
        # transpile/load gap
        printf "GAP   %-32s | %s\n" "$name" "$(echo "$h_err" | head -1)"
        gap=$((gap+1))
        continue
    fi
    if [ "$o_out" == "$h_out" ] && [ "$o_code" == "$h_code" ]; then
        printf "PASS  %-32s | exit=%s\n" "$name" "$o_code"
        pass=$((pass+1))
    else
        printf "FAIL  %-32s | oracle(exit=%s) hybrid(exit=%s)\n" "$name" "$o_code" "$h_code"
        printf "      oracle stdout: %q\n" "$o_out"
        printf "      hybrid stdout: %q\n" "$h_out"
        [ -n "$h_err" ] && printf "      hybrid stderr: %s\n" "$(echo "$h_err" | head -1)"
        fail=$((fail+1))
    fi
done
echo "----"
echo "PASS=$pass FAIL=$fail GAP=$gap"
