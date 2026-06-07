#!/usr/bin/env bash
# Goal clause (2): compiler2 correctly compiles the kernel fixture
# corpus (tests/kernel/test_*.ev), emitted units matching each
# fixture's `-- expect:` stdout/exit headers when run under the kernel.
#
# Artifact pattern: .goalpost/bin/run-kernel-corpus.sh does the
# expensive compiles and drops compiler2-kernel.json; this script
# parses it. Target = LIVE fixture count, so new fixtures raise the
# bar and stale artifacts can't cover them. Timeouts are not passes.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
A="$ROOT/.goalpost/artifacts/compiler2-kernel.json"

live_total="$(ls "$ROOT"/tests/kernel/test_*.ev 2>/dev/null | wc -l | tr -d ' ')"
[ "$live_total" -gt 0 ] || { echo "no kernel fixture corpus found" >&2; exit 1; }
# Missing artifact = "never measured": honest zero + maximally-stale freshness.
if [ -f "$A" ]; then
    passed="$(jq -r .passed "$A")"
    ts="$(jq -r .ts "$A")"
    age_h="$(awk -v now="$(date +%s)" -v ts="$ts" 'BEGIN{printf "%.1f",(now-ts)/3600}')"
else
    passed=0
    age_h=999999
fi
failing=$(( live_total - passed )); [ "$failing" -lt 0 ] && failing=0

printf '{"goal":"compiler2-selfhost","measure":"kernel_fixtures_pass","kind":"gate","value":%s,"target":%s,"unit":"count","rung":"deterministic","period_s":300,"label":"kernel fixtures compiled+run correctly via compiler2"}\n' "$passed" "$live_total"
printf '{"goal":"compiler2-selfhost","measure":"kernel_fixtures_failing","kind":"trend","value":%s,"target":0,"higher_is_better":false,"unit":"count","rung":"deterministic","period_s":300,"label":"kernel fixtures not yet passing under compiler2"}\n' "$failing"
printf '{"goal":"compiler2-selfhost","measure":"kernel_fixtures_fresh","kind":"gate","value":%s,"target":72,"higher_is_better":false,"unit":"h","rung":"deterministic","period_s":300,"label":"age of last full kernel-corpus run"}\n' "$age_h"
