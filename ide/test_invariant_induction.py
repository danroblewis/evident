#!/usr/bin/env python3
"""#306 — k-INDUCTION upgrades an UNBOUNDED safety invariant from SAMPLED to PROVEN, end-to-end
through check_invariant. The whole point is a GENUINE proof where the BFS can only sample, so the
ONE bug to prevent above all is a false 'proven' for a FALSE invariant. This test is the soundness
gate: it pins that a true unbounded invariant is proven by induction AND that a false one is NEVER
proven (it must be refuted by the BFS counterexample, or stay inconclusive — never proof_method set).
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                            # noqa: E402
from evident_viz import load as load_model                # noqa: E402

# Unbounded counter: grows forever, so the BFS caps → exhaustive=False (only sampled).
UNB = ("fsm m\n    count ∈ Int\n    is_first_tick ⇒ count = 0\n"
       "    ¬is_first_tick ⇒ count = _count + 1\n")
# Clamped counter: saturates at 5, so the BFS exhausts (exhaustive=True) — induction must NOT run.
SAT = ("fsm m\n    0 ≤ count ∈ Int ≤ 5\n    is_first_tick ⇒ count = 0\n"
       "    ¬is_first_tick ⇒ count = (_count < 5 ? _count + 1 : _count)\n")
# Unbounded counter with a DERIVED var (no prev twin) — an invariant over it must DECLINE induction.
DRV = ("fsm m\n    count ∈ Int\n    done ∈ Bool = (count ≥ 0)\n"
       "    is_first_tick ⇒ count = 0\n    ¬is_first_tick ⇒ count = _count + 1\n")


def _load(src, w):
    ok, prefix, *_ = _export(src, w)
    return load_model(prefix + ".smt2", prefix + ".schema.json")


def main():
    fails = []
    with tempfile.TemporaryDirectory() as w:
        m = _load(UNB, w)
        # (A) TRUE unbounded invariant → PROVEN by k-induction (was exhaustive=False, now a real proof).
        r = m.check_invariant("count", ">=", 0)
        if not (r["holds"] and r["exhaustive"] is False and r.get("proof_method") == "k-induction"):
            fails.append(f"(A) count≥0 must be proven by k-induction over the unbounded set, got {r}")
        # (B) THE CRITICAL CASE: a FALSE unbounded invariant must NEVER be proven. The BFS finds
        # count=101 (a real counterexample) and induction is never consulted — proof_method ABSENT.
        r = m.check_invariant("count", "<=", 100)
        if r["holds"] or r.get("proof_method") is not None:
            fails.append(f"(B) count≤100 is FALSE — must NEVER be proven; got holds={r['holds']} "
                         f"proof_method={r.get('proof_method')} (a false proof is catastrophic)")
        if (r["counterexample"] or {}).get("count") != 101:
            fails.append(f"(B) count≤100 should be refuted by the BFS at count=101, got {r['counterexample']}")
        # implication under induction: count≥5 ⇒ count≥0 (trivially true) → proven.
        r = m.check_invariant_predicate(antecedent=[["count", ">=", 5]], consequent=[["count", ">=", 0]])
        if r.get("proof_method") != "k-induction":
            fails.append(f"(E) unbounded implication c≥5⇒c≥0 should be proven by induction, got {r}")

    with tempfile.TemporaryDirectory() as w:
        m = _load(SAT, w)
        # (C) BOUNDED cross-check: count≤5 is exhaustive (a proof already) — induction must NOT run.
        r = m.check_invariant("count", "<=", 5)
        if not (r["holds"] and r["exhaustive"] is True and r.get("proof_method") is None):
            fails.append(f"(C) bounded count≤5 should be exhaustive=True with NO induction, got {r}")
        # count≤3 is false even on the bounded set — the BFS counterexample (count=4) stands.
        r = m.check_invariant("count", "<=", 3)
        if r["holds"] or (r["counterexample"] or {}).get("count") != 4:
            fails.append(f"(C) bounded count≤3 should be refuted at count=4, got {r}")

    with tempfile.TemporaryDirectory() as w:
        m = _load(DRV, w)
        # (D) An invariant over a DERIVED var (no prev twin) must DECLINE induction — never a false proof.
        r = m.check_invariant("done", "=", True)
        if r.get("proof_method") is not None:
            fails.append(f"(D) a derived-var invariant must DECLINE induction (no _done twin), got {r}")

    if fails:
        for f in fails:
            print("✗", f)
        sys.exit(1)
    print("✓ invariant_induction (#306): a TRUE unbounded invariant (count≥0) is PROVEN by k-induction "
          "where the BFS only samples; a FALSE one (count≤100) is NEVER proven — refuted by the BFS at "
          "count=101 (the critical no-false-proof case); bounded models stay exhaustive; derived-var "
          "invariants decline induction")


if __name__ == "__main__":
    main()
