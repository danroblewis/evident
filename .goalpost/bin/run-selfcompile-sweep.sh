#!/usr/bin/env bash
# .goalpost/bin/run-selfcompile-sweep.sh — the EXPENSIVE half of the
# self-compile-coverage measure. Runs scripts/selfcompile-sweep.sh (each
# tests/compiler2_units/*.ev module self-compiled through stage1 under the
# kernel, in parallel) and drops a machine-readable tally into
# .goalpost/artifacts/compiler2-selfcompile-sweep.json. The measure script
# (.goalpost/measures/selfcompile.sh) only READS that artifact.
#
# rc legend (from selfcompile-sweep.sh): 0 clean, 7/9 unresolved-ident
# (use-before-decl / bodyless-record cluster), 70/71 null-handle/crash,
# 1/3 other, FLATFAIL = refusal fixture (expect: flatten-error).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ART="$ROOT/.goalpost/artifacts"
mkdir -p "$ART"
cd "$ROOT"

LOG="$(mktemp -t gp-sweep.XXXXXX)"
trap 'rm -f "$LOG"' EXIT
# The sweep returns nonzero unless ALL clean; we want the tally regardless.
scripts/selfcompile-sweep.sh > "$LOG" 2>&1 || true

# Final line: "selfcompile-sweep: <clean>/<total> fixtures self-compile clean"
clean="$(grep -oE 'selfcompile-sweep: [0-9]+/[0-9]+' "$LOG" | grep -oE '[0-9]+/[0-9]+' | cut -d/ -f1 | tail -1)"
total="$(grep -oE 'selfcompile-sweep: [0-9]+/[0-9]+' "$LOG" | grep -oE '[0-9]+/[0-9]+' | cut -d/ -f2 | tail -1)"
: "${clean:=0}"; : "${total:=0}"

# Per-rc tallies from the per-fixture lines ("... rc=N ...").
# NB: `grep -c` already prints "0" on no match (and exits 1) — so use `|| true`,
# NOT `|| echo 0`, or the count double-prints "0\n0" and corrupts the JSON.
rc_count() { grep -cE "rc=$1( |\$)" "$LOG" 2>/dev/null || true; }
rc7="$(rc_count 7)"; rc9="$(rc_count 9)"; rc71="$(rc_count 71)"; rc70="$(rc_count 70)"
rc1="$(rc_count 1)"; rc3="$(rc_count 3)"
flatfail="$(grep -cE 'FLATFAIL' "$LOG" 2>/dev/null || true)"
ts="$(date +%s)"

cat > "$ART/compiler2-selfcompile-sweep.json" <<JSON
{"ts":$ts,"clean":$clean,"total":$total,"rc7":$rc7,"rc9":$rc9,"rc1":$rc1,"rc3":$rc3,"rc70":$rc70,"rc71":$rc71,"flatfail":$flatfail}
JSON
echo "wrote $ART/compiler2-selfcompile-sweep.json  ($clean/$total clean; rc7=$rc7 rc9=$rc9 rc71=$rc71)"
