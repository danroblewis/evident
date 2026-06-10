#!/usr/bin/env bash
# autocarry-evident.sh — the fsm autocarry pass as Evident programs,
# a drop-in stdin→stdout replacement for scripts/expand-fsm-autocarry.sh.
#
#   analyze < src  ──record stream──▶  fix  ──2-line edit script──▶  apply
#        (concurrent pipe)                  (+ src again)
#
# Byte-identical to the awk pass on the full corpus gate (236 pipeline
# streams incl. compiler2/driver.ev) and self-application, 2026-06-10.
#
# NOT wired into flatten-evident.sh: the perf budget gate failed —
# 1.49 s wall on the 8468-line driver stream vs the ≤1 s budget
# (awk: ~60 ms). Bottleneck: the functionizer interp evaluates ~0.6 us
# per step per tick across ~10k line-ticks × ~82 steps (analyze alone
# 0.79 s); see docs/plans/passes-in-evident-walls.md.
#
# Usage: scripts/passes/autocarry-evident.sh < in.ev > out.ev
# Env:   EVIDENT_KERNEL (default <repo>/kernel/target/release/kernel)

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"

[ -x "$KERNEL" ] || { echo "autocarry-evident: kernel not executable: $KERNEL" >&2; exit 2; }

T="$(mktemp -t acpass.XXXXXX.ev)" || exit 1
trap 'rm -f "$T" "$T.edits"' EXIT
cat > "$T"

"$KERNEL" "$DIR/autocarry_analyze.smt2" < "$T" 2>/dev/null \
    | "$KERNEL" "$DIR/autocarry_fix.smt2" 2>/dev/null > "$T.edits"
{ cat "$T.edits"; cat "$T"; } | "$KERNEL" "$DIR/autocarry_apply.smt2" 2>/dev/null
