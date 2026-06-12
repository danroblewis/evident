#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Self-compile-coverage clause: the headline progress bar for Gate A of
# the self-host plan. How many tests/compiler2_units/*.ev modules compile
# their OWN source through the (oracle-emitted) compiler under the kernel.
# Burns up to total as the named gaps close (bodyless-record cluster,
# floor-ctor LibCall, pass-0 op-build gating). The rc7/rc9 measures are
# the remaining-gap breakdown — they burn DOWN to 0.
#
# Artifact pattern: .goalpost/bin/run-selfcompile-sweep.sh runs the
# (minutes-long) sweep and drops compiler2-selfcompile-sweep.json; this
# script only reads it. See docs/plans/full-self-host-plan.md (Gate A).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
A="$ROOT/.goalpost/artifacts/compiler2-selfcompile-sweep.json"

if [ -f "$A" ]; then
    clean="$(jq -r '.clean'   "$A")"
    total="$(jq -r '.total'   "$A")"
    rc7="$(jq -r '.rc7'       "$A")"
    rc9="$(jq -r '.rc9'       "$A")"
    ts="$(jq -r '.ts'         "$A")"
    age_h="$(awk -v now="$(date +%s)" -v ts="$ts" 'BEGIN{printf "%.1f",(now-ts)/3600}')"
else
    # Missing artifact = "never measured": honest zero + maximally-stale.
    clean=0; total=79; rc7=0; rc9=0; age_h=999999
fi

printf '{"goal":"compiler2-selfhost","measure":"selfcompile_modules_clean","kind":"trend","value":%s,"target":%s,"higher_is_better":true,"unit":"count","rung":"deterministic","period_s":300,"label":"compiler2_units modules that self-compile clean (rc=0) through the compiler"}\n' "$clean" "$total"
printf '{"goal":"compiler2-selfhost","measure":"selfcompile_rc7_bodyless","kind":"trend","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"modules blocked rc=7 (bodyless-record field-const cluster — A′ Steps 2-3 / pass-0 op-build gating)"}\n' "$rc7"
printf '{"goal":"compiler2-selfhost","measure":"selfcompile_rc9_unresolved","kind":"trend","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"modules blocked rc=9 (unresolved identifier / use-before-decl)"}\n' "$rc9"
printf '{"goal":"compiler2-selfhost","measure":"selfcompile_fresh","kind":"gate","value":%s,"target":168,"higher_is_better":false,"unit":"h","rung":"deterministic","period_s":300,"label":"age of last self-compile sweep"}\n' "$age_h"
