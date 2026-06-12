#!/usr/bin/env bash
# .goalpost/bin/run-invariant-gate.sh — the work half of the
# carried-invariants measure. Runs scripts/invariant-gate.sh (which
# k-induction-proves each carried-type invariant via Z3, ~15s) and drops
# a tiny tally into .goalpost/artifacts/compiler2-invariants.json. The
# measure (.goalpost/measures/carried_invariants.sh) only READS it, so
# every md refresh stays in the <50ms measure budget.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ART="$ROOT/.goalpost/artifacts"
mkdir -p "$ART"
cd "$ROOT"

out="$(scripts/invariant-gate.sh 2>/dev/null || true)"
proven="$(printf '%s' "$out" | grep -cE '^[[:space:]]*PASS ' || true)"
gate="$(printf '%s' "$out" | grep -q 'invariant-gate: PASS' && echo 1 || echo 0)"
total=4
ts="$(date +%s)"

cat > "$ART/compiler2-invariants.json" <<JSON
{"ts":$ts,"proven":$proven,"total":$total,"gate":$gate}
JSON
echo "wrote $ART/compiler2-invariants.json  (proven=$proven/$total gate=$gate)"
