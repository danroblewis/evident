#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# driver-decomp-gate.sh — the §9 per-step equivalence gate for the
# driver_main decomposition.
#
# After each module extraction, the emitted stage-1 SMT-LIB for the
# (now partly-split) driver must be observably identical to the frozen
# pre-decomposition baseline. This script runs the structural half of
# the §9 gate (steps 1 + 2); conformance (step 3) and the module
# isolation tests (step 4) are separate harnesses.
#
#   1. flatten driver.ev → oracle emit driver_main → now.smt2
#   2. (1) __callN-normalized diff vs BASE.smt2 must be EMPTY
#   3. (2) manifest state-fields line must be unchanged (the §31 eff_out
#          bridge is the ONE sanctioned addition; pass ALLOW_EFF_OUT=1
#          once that module lands and BASE has been re-frozen).
#
# The frozen baseline lives at .goalpost/artifacts/BASE.smt2. It is the
# emit of the UN-split driver and is regenerable at any time from
# `git show main:compiler2/driver.ev` (see --freeze).
#
# Usage:
#   scripts/driver-decomp-gate.sh           # run the gate, exit 0 iff EQUIV
#   scripts/driver-decomp-gate.sh --freeze  # (re)capture BASE.smt2 from
#                                            # the CURRENT driver.ev
#
# Exit: 0 = EQUIV (gate green), 1 = drift (gate red), 2 = build/setup error.

set -u -o pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/.." && pwd)"
ORACLE="${EVIDENT_ORACLE:-/usr/local/bin/evident-oracle}"
FLATTEN="$ROOT/scripts/flatten-evident.sh"
BASE="$ROOT/.goalpost/artifacts/BASE.smt2"
DRIVER="$ROOT/compiler2/driver.ev"

[ -x "$ORACLE" ] || { echo "gate: oracle not executable: $ORACLE" >&2; exit 2; }

build() { # build <src.ev> <out.smt2>
    local flat; flat="$(mktemp -t gate-flat.XXXXXX.ev)"
    "$FLATTEN" "$1" > "$flat" 2>/dev/null || { rm -f "$flat"; return 2; }
    "$ORACLE" emit "$flat" driver_main -o "$2" 2>/dev/null || { rm -f "$flat"; return 2; }
    rm -f "$flat"
}

if [ "${1:-}" = "--freeze" ]; then
    mkdir -p "$(dirname "$BASE")"
    build "$DRIVER" "$BASE" || { echo "gate: baseline build failed" >&2; exit 2; }
    echo "gate: froze baseline → $BASE ($(wc -l < "$BASE") lines)"
    exit 0
fi

[ -f "$BASE" ] || { echo "gate: no baseline at $BASE — run --freeze first" >&2; exit 2; }

NOW="$(mktemp -t gate-now.XXXXXX.smt2)"
trap 'rm -f "$NOW"' EXIT
build "$DRIVER" "$NOW" || { echo "gate: current driver build failed" >&2; exit 2; }

# (1) __callN-normalized structural diff
norm() { sed 's/__call[0-9]\+/__callN/g' "$1"; }
if diff <(norm "$BASE") <(norm "$NOW") > /tmp/gate-diff.txt 2>&1; then
    echo "gate (1): EQUIV — __callN-normalized emit byte-identical to baseline"
else
    echo "gate (1): DRIFT — $(wc -l < /tmp/gate-diff.txt) diff lines (see /tmp/gate-diff.txt)" >&2
    head -40 /tmp/gate-diff.txt >&2
    exit 1
fi

# (2) manifest state-fields
base_mf="$(grep '^;; manifest: state-fields' "$BASE")"
now_mf="$(grep '^;; manifest: state-fields' "$NOW")"
if [ "$base_mf" = "$now_mf" ]; then
    echo "gate (2): manifest state-fields UNCHANGED"
elif [ "${ALLOW_EFF_OUT:-0}" = 1 ]; then
    echo "gate (2): manifest changed — ALLOW_EFF_OUT=1, accepting (§31 eff_out bridge)"
else
    echo "gate (2): manifest state-fields DRIFT" >&2
    diff <(printf '%s\n' "$base_mf") <(printf '%s\n' "$now_mf") >&2
    exit 1
fi

echo "gate: GREEN"
