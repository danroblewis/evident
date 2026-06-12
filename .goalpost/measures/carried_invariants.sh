#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Static-safety clause: the compiler's carried-state TYPE INVARIANTS are
# proven inductive (Z3 k-induction), not just run-and-prayed. scripts/
# invariant-gate.sh pins the RESULTS.md baseline of the four real carried
# types (the three zinit latch banks PROVEN with their ordering lemma; the
# FtiBuffer overrun documented as the runtime net). This is the STATIC
# half of the carried-invariant safety net (functionization-gate is the
# dynamic half).
#
# Artifact pattern: the gate is ~15s (flatten+emit+z3 ×4), over the <50ms
# measure budget — so .goalpost/bin/run-invariant-gate.sh runs it and
# drops compiler2-invariants.json; this script only READS it. See
# tests/proof/RESULTS.md.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
A="$ROOT/.goalpost/artifacts/compiler2-invariants.json"

if [ -f "$A" ]; then
    proven="$(jq -r '.proven' "$A")"
    total="$(jq -r '.total'  "$A")"
    gate="$(jq -r '.gate'    "$A")"
    ts="$(jq -r '.ts'        "$A")"
    age_h="$(awk -v now="$(date +%s)" -v ts="$ts" 'BEGIN{printf "%.1f",(now-ts)/3600}')"
else
    # Missing artifact = "never measured": honest zero + maximally-stale.
    proven=0; total=4; gate=0; age_h=999999
fi

printf '{"goal":"compiler2-selfhost","measure":"carried_invariants_proven","kind":"trend","value":%s,"target":%s,"higher_is_better":true,"unit":"count","rung":"deterministic","period_s":300,"label":"carried-type invariants matching the proven RESULTS.md baseline (Z3 k-induction)"}\n' "$proven" "$total"
printf '{"goal":"compiler2-selfhost","measure":"invariant_gate","kind":"gate","value":%s,"rung":"deterministic","period_s":300,"label":"scripts/invariant-gate.sh passes (all carried-type invariants hold)"}\n' "$gate"
printf '{"goal":"compiler2-selfhost","measure":"invariant_fresh","kind":"gate","value":%s,"target":168,"higher_is_better":false,"unit":"h","rung":"deterministic","period_s":300,"label":"age of last invariant-gate run"}\n' "$age_h"
