#!/usr/bin/env bash
# Goal clause (3): compiler2 correctly compiles the legacy sat-check
# driver source (compiler/sample.ev). "Correctly" = the compiler2-built
# unit, run as a sample driver, produces the same (claim, sat/unsat)
# verdict sequences as the committed known-good sample.smt2 on the
# reference inputs — behavioural equivalence, not just "it emitted
# something".
#
# Artifact pattern: .goalpost/bin/run-sample.sh does the compile +
# z3 comparison and drops compiler2-sample.json; this script parses it.
# The gate value is 1 only when compiled && equiv both hold.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
A="$ROOT/.goalpost/artifacts/compiler2-sample.json"
# Missing artifact = "never measured": honest zero + maximally-stale freshness.
if [ -f "$A" ]; then
    ok="$(jq -r 'if .compiled and .equiv then 1 else 0 end' "$A")"
    ts="$(jq -r .ts "$A")"
    age_h="$(awk -v now="$(date +%s)" -v ts="$ts" 'BEGIN{printf "%.1f",(now-ts)/3600}')"
else
    ok=0
    age_h=999999
fi

printf '{"goal":"compiler2-selfhost","measure":"sample_ev_equiv","kind":"gate","value":%s,"rung":"deterministic","period_s":300,"label":"compiler2-built sample.ev driver matches committed sample.smt2 verdicts"}\n' "$ok"
printf '{"goal":"compiler2-selfhost","measure":"sample_ev_fresh","kind":"gate","value":%s,"target":168,"higher_is_better":false,"unit":"h","rung":"deterministic","period_s":300,"label":"age of last sample.ev equivalence run"}\n' "$age_h"
