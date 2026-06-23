#!/usr/bin/env python3
"""Test the LIVENESS-UNDER-FAIRNESS verdicts (Ana #269).

The plain lasso check (`check_temporal` without `fair`) refutes □◇P / ◇P / P⤳Q on ANY dodging
run — including UNFAIR ones that perpetually ignore an always-available path to P. Every branching
FSM has such a lasso, so liveness almost always 'fails'. `fair=True` switches to the WEAK-FAIRNESS
oracle: □◇/◇ hold iff every reachable state can reach a P-state; P⤳Q iff every reachable P-state
can; the only fair counterexample is a TRAP (a reachable state from which P is unreachable).

Two model shapes pin both directions:

  - DODGER: a nondeterministic FSM where □◇(mode = B) FAILS without fairness (an unfair lasso loops
    in A forever) but HOLDS under fairness (B is reachable from every reachable state) — fairness
    flips it to HOLDS, no trap.
  - TRAP: an FSM with a reachable ABSORBING region (a sink with no path to the goal) → FAILS even
    under fairness, witness = the trap state + the init→trap run.

Run from repo root: `python3 ide/test_fairness.py` (exit non-zero on any failure)."""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from evident_viz import load as load_model                  # noqa: E402

# DODGER: a free boolean `flip` makes the A-state NONDETERMINISTIC — from A the machine MAY stay in
# A (flip false) or step to B (flip true); from B it returns to A. The unfair run "always stay in A"
# dodges B forever — a real lasso (the A self-loop). But B IS reachable from A (and from B), so from
# EVERY reachable state B is reachable: under weak fairness □◇(mode = B) HOLDS — the always-available
# flip-to-B eventually fires. Without fairness the dodging A-loop refutes it.
DODGER = (
    "enum Mode = A | B\n"
    "fsm dodger\n"
    "    mode ∈ Mode\n"
    "    flip ∈ Bool\n"
    "    is_first_tick ⇒ mode = A\n"
    "    (¬is_first_tick ∧ _mode = A ∧ ¬flip) ⇒ mode = A\n"
    "    (¬is_first_tick ∧ _mode = A ∧ flip)  ⇒ mode = B\n"
    "    (¬is_first_tick ∧ _mode = B) ⇒ mode = A\n")

# TRAP: phase walks Idle → Working, then Working → (Done OR Stuck) nondeterministically. Done loops
# on itself (the goal recurs); Stuck is ABSORBING — it loops on itself with no edge out, and the
# goal (phase = Done) is UNREACHABLE from it. So □◇(phase = Done) FAILS even under fairness, and the
# fair counterexample is the Stuck trap + the run Idle→Working→Stuck.
TRAP = (
    "enum Phase = Idle | Working | Done | Stuck\n"
    "fsm pipeline\n"
    "    phase ∈ Phase\n"
    "    is_first_tick ⇒ phase = Idle\n"
    "    (¬is_first_tick ∧ _phase = Idle) ⇒ phase = Working\n"
    "    (¬is_first_tick ∧ _phase = Working ∧ choose) ⇒ phase = Done\n"
    "    (¬is_first_tick ∧ _phase = Working ∧ ¬choose) ⇒ phase = Stuck\n"
    "    (¬is_first_tick ∧ _phase = Done) ⇒ phase = Done\n"
    "    (¬is_first_tick ∧ _phase = Stuck) ⇒ phase = Stuck\n"
    "    choose ∈ Bool\n")


def _load(src, work):
    ok, prefix, dropped, msg = _export(src, work)
    if not ok:
        raise RuntimeError(f"export failed: {msg.splitlines()[0][:80]}")
    return load_model(prefix + ".smt2", prefix + ".schema.json")


def main():
    fails = []

    # ── DODGER: fairness flips the verdict ──────────────────────────────────
    with tempfile.TemporaryDirectory() as work:
        m = _load(DODGER, work)
        Q = [["mode", "=", "B"]]
        unfair = m.check_temporal(Q, modality="infinitely_often")
        fair = m.check_temporal(Q, modality="infinitely_often", fair=True)
        # Without fairness: a run dodges B forever → REFUTED, with a lasso.
        if unfair["holds"]:
            fails.append("dodger □◇(mode=B) without fairness: expected REFUTED (unfair lasso), got HOLDS")
        # Under fairness: B is reachable from every reachable state → HOLDS, no trap.
        if not fair["holds"]:
            fails.append(f"dodger □◇(mode=B) UNDER fairness: expected HOLDS, got refuted "
                         f"(trap={fair.get('counterexample')})")
        if not fair.get("fair"):
            fails.append("dodger fair check: expected fair=True flag on the verdict")
        if fair.get("trap"):
            fails.append("dodger fair HOLDS: expected no trap flag")
        # ◇ under fairness ≡ □◇ under fairness (every state reaches Q).
        ev_fair = m.check_temporal(Q, modality="eventually", fair=True)
        if not ev_fair["holds"]:
            fails.append("dodger ◇(mode=B) UNDER fairness: expected HOLDS")

    # ── TRAP: fails even under fairness, witness = the trap + its run ────────
    with tempfile.TemporaryDirectory() as work:
        m = _load(TRAP, work)
        Q = [["phase", "=", "Done"]]
        fair = m.check_temporal(Q, modality="infinitely_often", fair=True)
        if fair["holds"]:
            fails.append("trap □◇(phase=Done) UNDER fairness: expected REFUTED (Stuck trap), got HOLDS")
        if not fair.get("trap"):
            fails.append("trap fair refutation: expected trap=True flag")
        cex = fair.get("counterexample") or {}
        if "phase" not in cex or cex["phase"] != "Stuck":
            fails.append(f"trap witness: expected the Stuck trap state, got {cex!r}")
        tr = fair.get("trace") or []
        # The run reaches the trap from init: Idle → Working → Stuck (3 states), ending on Stuck.
        if not tr or (tr[-1].get("phase") if isinstance(tr[-1], dict) else None) != "Stuck":
            fails.append(f"trap trace: expected init→trap run ending on Stuck, got {tr!r}")
        # leads_to under fairness, REFUTED side: Stuck ⤳ Done fails — Stuck is a reachable P-state
        # that cannot reach Done (the trap is itself the offending P-state).
        lt_bad = m.check_temporal(Q, modality="leads_to", p_terms=[["phase", "=", "Stuck"]], fair=True)
        if lt_bad["holds"]:
            fails.append("trap (phase=Stuck ⤳ phase=Done) UNDER fairness: expected REFUTED")
        if not lt_bad.get("trap"):
            fails.append("trap leads_to fair: expected trap=True flag")
        if (lt_bad.get("counterexample") or {}).get("phase") != "Stuck":
            fails.append(f"trap leads_to witness: expected Stuck, got {lt_bad.get('counterexample')!r}")
        # leads_to under fairness, HOLDS side: Working ⤳ Done holds — from every reachable Working
        # state Done IS reachable (Working → Done), even though Working can ALSO reach Stuck. Weak
        # fairness excludes the dodging Working→Stuck run.
        lt_ok = m.check_temporal(Q, modality="leads_to", p_terms=[["phase", "=", "Working"]], fair=True)
        if not lt_ok["holds"]:
            fails.append("trap (phase=Working ⤳ phase=Done) UNDER fairness: expected HOLDS "
                         "(Done reachable from Working)")

    if fails:
        print("FAIRNESS-LIVENESS TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ fairness liveness: dodger flips REFUTED→HOLDS under fairness; trap FAILS even under "
          "fairness with the trap state + its run")
    return 0


if __name__ == "__main__":
    sys.exit(main())
