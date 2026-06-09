#!/usr/bin/env bash
# run.sh — equivalence driver for the bounded-Seq lowering (slice 1).
#
# For each scenario there is a pair:
#   <name>_hand.ev    — hand-written flat-scalar target form (no Seq)
#   <name>_seqsrc.ev  — same logic written in the bounded-Seq surface
#
# We build both and assert:
#   1. identical kernel exit code,
#   2. identical kernel stdout,
#   3. oracle emits byte-identical modulo the __callN counter
#      (sed 's/__call[0-9]\+/__callN/g', the driver-decomp-gate.sh
#      normalization).
#
# The seqsrc side is run THROUGH the lowering transform before the oracle;
# the hand side goes straight to the oracle. Pipeline per side:
#   flatten → [lower-bounded-seq, seqsrc only] → oracle emit main → kernel
#
# Usage:  tests/seq_lowering/run.sh
# Exit:   0 = all scenarios equivalent, 1 = a mismatch.

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
# Resolve the kernel: explicit override, this tree's build, then the main
# checkout's build (worktrees under .claude/worktrees/ carry no kernel/target).
KERNEL="${EVIDENT_KERNEL:-}"
if [ -z "$KERNEL" ]; then
    for c in "$ROOT/kernel/target/release/kernel" \
             "$(git -C "$ROOT" rev-parse --git-common-dir 2>/dev/null)/../kernel/target/release/kernel"; do
        [ -x "$c" ] && { KERNEL="$c"; break; }
    done
fi
FLATTEN="$ROOT/scripts/flatten-evident.sh"
LOWER="$ROOT/scripts/lower-bounded-seq.sh"
NORM='s/__call[0-9]\+/__callN/g'

[ -x "$ORACLE" ] || { echo "run: oracle not executable: $ORACLE" >&2; exit 2; }
[ -x "$KERNEL" ] || { echo "run: kernel not executable: $KERNEL" >&2; exit 2; }

TMP="$(mktemp -d -t seqlow.XXXXXX)"
trap 'rm -f "$TMP"/*; rmdir "$TMP" 2>/dev/null' EXIT

# emit_hand <src.ev> <out.smt2>
emit_hand() {
    local flat="$TMP/h.ev"
    "$FLATTEN" "$1" > "$flat" 2>/dev/null || return 2
    "$ORACLE" emit "$flat" main -o "$2" 2>/dev/null || return 2
}
# emit_seq <src.ev> <out.smt2>  (runs the lowering transform)
emit_seq() {
    local flat="$TMP/s0.ev" low="$TMP/s1.ev"
    "$FLATTEN" "$1" > "$flat" 2>/dev/null || return 2
    "$LOWER" < "$flat" > "$low" 2>/dev/null || return 2
    "$ORACLE" emit "$low" main -o "$2" 2>/dev/null || return 2
}

scenarios="forall_true forall_false member_hit member_miss carried"
fails=0

for sc in $scenarios; do
    hsm="$TMP/${sc}_hand.smt2"; ssm="$TMP/${sc}_seq.smt2"
    emit_hand "$DIR/${sc}_hand.ev"   "$hsm" || { echo "[$sc] hand build FAILED";  fails=$((fails+1)); continue; }
    emit_seq  "$DIR/${sc}_seqsrc.ev" "$ssm" || { echo "[$sc] seq build FAILED";   fails=$((fails+1)); continue; }

    ho="$("$KERNEL" "$hsm" 2>/dev/null)"; hx=$?
    so="$("$KERNEL" "$ssm" 2>/dev/null)"; sx=$?

    emit_eq="DIFF"
    if diff -q <(sed "$NORM" "$hsm") <(sed "$NORM" "$ssm") >/dev/null; then emit_eq="IDENTICAL"; fi

    status="ok"
    [ "$hx" = "$sx" ] || status="EXIT-MISMATCH"
    [ "$ho" = "$so" ] || status="STDOUT-MISMATCH"
    [ "$status" = "ok" ] || fails=$((fails+1))

    printf '[%-13s] hand(exit=%s) seq(exit=%s) stdout=%s emit=%-9s %s\n' \
        "$sc" "$hx" "$sx" \
        "$([ "$ho" = "$so" ] && echo same || echo DIFF)" \
        "$emit_eq" "$status"
    printf '    hand-stdout: %s\n' "$(printf '%s' "$ho" | tr '\n' '|')"
    printf '    seq -stdout: %s\n' "$(printf '%s' "$so" | tr '\n' '|')"
done

echo "----"
if [ "$fails" = 0 ]; then echo "ALL SCENARIOS EQUIVALENT"; exit 0
else echo "$fails scenario(s) FAILED"; exit 1; fi
