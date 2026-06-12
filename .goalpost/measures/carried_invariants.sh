#!/usr/bin/env bash
# Self-enforced budget: hard-cap this script at 55s regardless of runner.
[ -z "${GP_TIMEBOXED:-}" ] && exec env GP_TIMEBOXED=1 timeout 55 bash "$0" "$@"
# Static-safety clause: the compiler's carried-state TYPE INVARIANTS are
# proven inductive (Z3 k-induction), not just run-and-prayed. scripts/
# invariant-gate.sh pins the RESULTS.md baseline of the four real carried
# types (the three zinit latch banks PROVEN with their ordering lemma; the
# FtiBuffer overrun documented as the runtime net). This measures how many
# of those carried-type checks still hold — the STATIC half of the
# carried-invariant safety net (functionization-gate is the dynamic half).
# Runs the gate directly (~15s); no artifact. See tests/proof/RESULTS.md.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TOTAL=4
out="$(scripts/invariant-gate.sh 2>/dev/null || true)"
pass="$(printf '%s' "$out" | grep -cE '^\s*PASS ' || true)"
gate="$(printf '%s' "$out" | grep -q 'invariant-gate: PASS' && echo 1 || echo 0)"

printf '{"goal":"compiler2-selfhost","measure":"carried_invariants_proven","kind":"trend","value":%s,"target":%s,"higher_is_better":true,"unit":"count","rung":"deterministic","period_s":300,"label":"carried-type invariants matching the proven RESULTS.md baseline (Z3 k-induction)"}\n' "$pass" "$TOTAL"
printf '{"goal":"compiler2-selfhost","measure":"invariant_gate","kind":"gate","value":%s,"rung":"deterministic","period_s":300,"label":"scripts/invariant-gate.sh passes (all carried-type invariants hold)"}\n' "$gate"
