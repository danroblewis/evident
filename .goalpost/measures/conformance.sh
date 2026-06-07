#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Goal clause (1): compiler2 correctly compiles the conformance corpus
# (tests/conformance/features/), emitted units running correctly.
#
# Artifact pattern: .goalpost/bin/run-conformance.sh actually compiles
# every fixture through kernel+compiler2 (minutes per fixture — far
# over the measure budget) and drops compiler2-conformance.json; this
# script only parses it.
#
# The TARGET is the LIVE corpus size, counted from the tree at measure
# time — so adding fixtures raises the bar, and a stale artifact that
# predates new fixtures can never report them as passed (failing =
# live_total − artifact_passed). Timeouts count as not-passed.
# Freshness is its own gate: a week-old artifact goes red on its own.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
A="$ROOT/.goalpost/artifacts/compiler2-conformance.json"

live_total="$(ls -d "$ROOT"/tests/conformance/features/[0-9]*/ 2>/dev/null | wc -l | tr -d ' ')"
[ "$live_total" -gt 0 ] || { echo "no conformance corpus found" >&2; exit 1; }

# Missing artifact = "never measured": emit the honest zero state with a
# maximally-stale freshness gate (panel goes red, ruler stays unbroken).
if [ -f "$A" ]; then
    passed="$(jq -r .passed "$A")"
    ts="$(jq -r .ts "$A")"
    age_h="$(awk -v now="$(date +%s)" -v ts="$ts" 'BEGIN{printf "%.1f",(now-ts)/3600}')"
else
    passed=0
    age_h=999999
fi
failing=$(( live_total - passed )); [ "$failing" -lt 0 ] && failing=0

printf '{"goal":"compiler2-selfhost","measure":"conformance_pass","kind":"gate","value":%s,"target":%s,"unit":"count","rung":"deterministic","period_s":300,"label":"conformance fixtures compiled+run correctly via compiler2"}\n' "$passed" "$live_total"
printf '{"goal":"compiler2-selfhost","measure":"conformance_failing","kind":"trend","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"conformance fixtures not yet passing under compiler2"}\n' "$failing"
printf '{"goal":"compiler2-selfhost","measure":"conformance_fresh","kind":"gate","value":%s,"target":72,"higher_is_better":false,"unit":"h","rung":"deterministic","period_s":300,"label":"age of last full conformance run"}\n' "$age_h"
