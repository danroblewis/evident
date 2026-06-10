#!/usr/bin/env bash
# autocarry-evident.sh — the fsm autocarry pass as Evident programs,
# a drop-in stdin→stdout replacement for scripts/expand-fsm-autocarry.sh.
#
#   analyze < src  ──record stream──▶  fix  ──2-line edit script──▶  apply
#        (concurrent pipe)                  (+ src again)
#
# Byte-identical to the awk pass on the full corpus gate (250 pipeline
# streams incl. compiler2/driver.ev with the headered DriverBroadcast,
# the counter_*_header fixtures, and conformance 142-148) and on
# self-application, 2026-06-10.
#
# WIRED into flatten-evident.sh as the production autocarry pass
# (EVIDENT_AUTOCARRY=awk falls back to the reference awk). Perf gate:
# 0.33-0.38 s wall on the 8610-line driver stream vs the ≤1 s budget —
# the kernel's lowered-IR interpreter (2b7312e) closed the prior 1.46 s
# wall; see docs/plans/passes-in-evident-walls.md.
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
