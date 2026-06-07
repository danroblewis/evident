#!/usr/bin/env bash
# Goal clause (4), the definition of "genuinely self-hosting":
# compiler2 compiles its own source (compiler2/driver.ev + imports)
# into a stage2 artifact, and that stage2 WORKS AS A COMPILER (it
# compiles smoke fixtures whose emitted units run correctly under the
# kernel). Two gates: built (non-stub, manifest-headed stage2 exists)
# and works (stage2's own compiles are correct) — the 12-line
# no-such-claim stub passes neither.
#
# Artifact pattern: .goalpost/bin/run-selfhost.sh does the multi-hour
# self-compile and drops compiler2-selfhost.json; this script parses it.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
A="$ROOT/.goalpost/artifacts/compiler2-selfhost.json"
# Missing artifact = "never measured": honest zero + maximally-stale freshness.
if [ -f "$A" ]; then
    built="$(jq -r 'if .stage2_built then 1 else 0 end' "$A")"
    works="$(jq -r 'if .stage2_built and .stage2_smoke then 1 else 0 end' "$A")"
    ts="$(jq -r .ts "$A")"
    age_h="$(awk -v now="$(date +%s)" -v ts="$ts" 'BEGIN{printf "%.1f",(now-ts)/3600}')"
else
    built=0
    works=0
    age_h=999999
fi

printf '{"goal":"compiler2-selfhost","measure":"selfhost_stage2_built","kind":"gate","value":%s,"rung":"deterministic","period_s":300,"label":"compiler2 compiles its own source to a non-stub stage2 artifact"}\n' "$built"
printf '{"goal":"compiler2-selfhost","measure":"selfhost_stage2_works","kind":"gate","value":%s,"rung":"deterministic","period_s":300,"label":"the self-compiled stage2 correctly compiles smoke fixtures"}\n' "$works"
printf '{"goal":"compiler2-selfhost","measure":"selfhost_fresh","kind":"gate","value":%s,"target":168,"higher_is_better":false,"unit":"h","rung":"deterministic","period_s":300,"label":"age of last self-compile attempt"}\n' "$age_h"
