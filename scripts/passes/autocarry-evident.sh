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
# FAILURE PROPAGATION: each stage's registries are statically bounded
# with loud overflow exits (codes 61-64 analyze / 70-79 fix / 81-83
# apply — the table lives in each program's MODULE header). A nonzero
# stage aborts the pass: its stderr diagnostic (minus [functionizer]
# noise) is surfaced and the stage's exit code is propagated, so an
# overflow can never silently truncate the expanded source.
#
# Usage: scripts/passes/autocarry-evident.sh < in.ev > out.ev
# Env:   EVIDENT_KERNEL (default <repo>/kernel/target/release/kernel)

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
KERNEL="${EVIDENT_KERNEL:-$ROOT/kernel/target/release/kernel}"

[ -x "$KERNEL" ] || { echo "autocarry-evident: kernel not executable: $KERNEL" >&2; exit 2; }

T="$(mktemp -t acpass.XXXXXX.ev)" || exit 1
trap 'rm -f "$T" "$T.edits" "$T.e1" "$T.e2" "$T.e3"' EXIT
cat > "$T"

stage_fail() { # <errfile> <stage> <code>
    grep -v '^\[functionizer\]' "$1" >&2 || true
    echo "autocarry-evident: $2 stage failed (exit $3)" >&2
    exit "$3"
}

"$KERNEL" "$DIR/autocarry_analyze.smt2" < "$T" 2>"$T.e1" \
    | "$KERNEL" "$DIR/autocarry_fix.smt2" 2>"$T.e2" > "$T.edits"
ST=("${PIPESTATUS[@]}")
[ "${ST[0]}" -eq 0 ] || stage_fail "$T.e1" analyze "${ST[0]}"
[ "${ST[1]}" -eq 0 ] || stage_fail "$T.e2" fix "${ST[1]}"
{ cat "$T.edits"; cat "$T"; } | "$KERNEL" "$DIR/autocarry_apply.smt2" 2>"$T.e3"
RC=$?
[ "$RC" -eq 0 ] || stage_fail "$T.e3" apply "$RC"
