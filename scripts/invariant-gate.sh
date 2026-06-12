#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# invariant-gate.sh — the STATIC complement to functionization-gate.sh.
#
# functionization-gate.sh catches the ≠-disequality class DYNAMICALLY (a hot
# carried invariant that fell off the functionizer → nonzero ms z3). This gate
# catches the LOGIC class STATICALLY: it pins the documented k-induction baseline
# of the compiler's real carried-type invariants (tests/proof/RESULTS.md) and
# fails if any of them stops holding — a weakened/broken type invariant, or an
# emit-shape change that breaks the proof.
#
# Per carried type it asserts:
#   - base : the invariant holds at tick 0                      (always required)
#   - step : the transition preserves it                       (required form below)
# The three zinit latch banks are 1-inductive only WITH their step↔field ordering
# lemma (tests/proof/lemmas/*), so they must prove `step: unsat` with the lemma.
# The FtiBuffer is a RUNTIME net (unguarded count++), so its documented baseline
# is `step: sat` + `totality: STUCK` — the invariant IS the exit-2 overrun guard.
#
# A green run = the carried invariants still mean what RESULTS.md says. Run it
# after any change to a carried type's body (alongside functionization-gate.sh).

set -euo pipefail
cd "$(dirname "$0")/.."
PI=scripts/prove-invariants.sh
L=tests/proof/lemmas
T=tests/compiler2_units/types

# fixture-stem | prefix | lemma (or -) | expected-step | expected-totality (or -)
ROWS=(
  "z3_solverctx_carry|z3ctx_|$L/z3_solverctx_latch_order.smt2|unsat|-"
  "z3_sorts_carry|z3sorts_|$L/z3_sorts_latch_order.smt2|unsat|-"
  "z3_numerals_carry|z3nums_|$L/z3_numerals_latch_order.smt2|unsat|-"
  "fti_buffer_carry|buf_|-|sat|STUCK"
)

fails=0
for row in "${ROWS[@]}"; do
  IFS='|' read -r stem pfx lemma estep etot <<<"$row"
  args=("$T/$stem.ev" main "$pfx"); [ "$lemma" != - ] && args+=("$lemma")
  out="$("$PI" "${args[@]}" 2>&1)"
  gstep="$(printf '%s' "$out" | sed -n 's/^  step: *\([a-z]*\).*/\1/p')"
  gbase="$(printf '%s' "$out" | sed -n 's/^  base: *\([a-z]*\).*/\1/p')"
  gtot="$(printf '%s' "$out" | grep -oE 'totality:  [A-Za-z-]+' | awk '{print $2}' || true)"

  msg="$stem"
  ok=1
  [ "$gbase" = unsat ] || { ok=0; msg="$msg  base=$gbase(want unsat)"; }
  [ "$gstep" = "$estep" ] || { ok=0; msg="$msg  step=$gstep(want $estep)"; }
  if [ "$etot" != - ]; then
    [ "$gtot" = "$etot" ] || { ok=0; msg="$msg  totality=$gtot(want $etot)"; }
  fi

  if [ "$ok" = 1 ]; then
    printf '  PASS  %-22s base=%s step=%s%s\n' "$stem" "$gbase" "$gstep" \
      "$([ "$etot" != - ] && echo " totality=$gtot")"
  else
    printf '  FAIL  %s\n' "$msg"
    fails=$((fails+1))
  fi
done

echo
if [ "$fails" -eq 0 ]; then
  echo "invariant-gate: PASS — all carried-type invariants match the RESULTS.md baseline"
else
  echo "invariant-gate: FAIL — $fails carried-type invariant(s) drifted from baseline"
  exit 1
fi
