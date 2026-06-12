#!/usr/bin/env bash
# TODO: rewrite in Evident
# lowerseq-equiv.sh — byte-equivalence gate for the Evident bounded-Seq
# lowering port (lowerseq_scan/plan/emit) against the awk reference
# (lower-bounded-seq.sh).
#
# For each fixture it feeds the SAME input to (a) the awk reference and
# (b) the three-program Evident pipeline (scan → plan → emit), and diffs
# the stdout. Identity = pass. The fixtures live in
# tests/compiler2_units/seq_lowering_port/ and exercise only the PORTED
# rule subset (R0/R1/R5/R7 decl-side + the R7 bound rewrite); fixtures
# whose body has surviving seq index/card USES (R16/R18, deferred) are
# expected to diverge and are NOT in this gate yet.
#
# The .ev passes must be compiled to .smt2 first via build-lowerseq.sh
# (or this script builds them when missing).
#
# Usage: scripts/passes/lowerseq-equiv.sh [fixture.ev ...]
# Env:   EVIDENT_KERNEL, EVIDENT_ORACLE

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
AWK="$DIR/lower-bounded-seq.sh"

[ -x "$KERNEL" ] || { echo "lowerseq-equiv: kernel not executable: $KERNEL" >&2; exit 2; }

# build artifacts if missing
build_one() {
    local prog="$1"
    [ -f "$DIR/$prog.smt2" ] && [ "$DIR/$prog.smt2" -nt "$DIR/$prog.ev" ] && return 0
    local flat; flat="$(mktemp)"
    EVIDENT_AUTOCARRY=awk "$ROOT/scripts/flatten-evident.sh" "$DIR/$prog.ev" > "$flat" 2>/dev/null
    "$ORACLE" emit "$flat" "$prog" -o "$DIR/$prog.smt2" 2>/dev/null
    rm -f "$flat"
    [ -f "$DIR/$prog.smt2" ] || { echo "lowerseq-equiv: failed to build $prog" >&2; exit 2; }
}
build_one lowerseq_scan
build_one lowerseq_plan
build_one lowerseq_emit

run_port() {
    local in="$1" recs reg
    recs="$("$KERNEL" "$DIR/lowerseq_scan.smt2" < "$in" 2>/dev/null)"
    reg="$(printf '%s\n' "$recs" | "$KERNEL" "$DIR/lowerseq_plan.smt2" 2>/dev/null)"
    { printf '%s\n' "$reg"; cat "$in"; } | "$KERNEL" "$DIR/lowerseq_emit.smt2" 2>/dev/null
}

if [ "$#" -gt 0 ]; then
    FIXTURES=("$@")
else
    FIXTURES=("$ROOT"/tests/compiler2_units/seq_lowering_port/*.ev)
fi

pass=0; fail=0
for fx in "${FIXTURES[@]}"; do
    name="$(basename "$fx")"
    awk_out="$("$AWK" < "$fx" 2>/dev/null)"
    port_out="$(run_port "$fx")"
    if [ "$awk_out" = "$port_out" ]; then
        echo "PASS  $name"
        pass=$((pass + 1))
    else
        echo "FAIL  $name"
        diff <(printf '%s\n' "$awk_out") <(printf '%s\n' "$port_out") | sed 's/^/    /'
        fail=$((fail + 1))
    fi
done
echo "── $pass passed, $fail failed ──"
[ "$fail" -eq 0 ]
