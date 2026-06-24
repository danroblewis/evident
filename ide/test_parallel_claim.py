#!/usr/bin/env python3
"""#284: parallel_coords samples a CLAIM's solution space (z3 witnesses), like scatter_matrix — not just
FSM reachable states. The claim path detects a no-fsm schema and reuses claim_witnesses (the same witness
enumeration the solve panel + scatter_matrix use), then draws the parallel-coords over the satisfying
assignments. Pin: a feasible ≥2-numeric-var claim yields a real witness cloud where EVERY witness
satisfies the constraint (never a fabricated cloud); an UNSAT claim draws the honest N/A card.
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                            # noqa: E402
from render_parallel_coords import _claim_setup           # noqa: E402


def main():
    fails = []
    # feasible claim, 2 numeric vars → a real witness cloud, every witness satisfying y≤x ∧ x+y≤20
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ x ∈ Int ≤ 20\n    0 ≤ y ∈ Int ≤ 20\n"
                                 "    y ≤ x\n    x + y ≤ 20", w)
        setup = _claim_setup(prefix + ".smt2", prefix + ".schema.json", w + "/out.png")
        if setup is None:
            fails.append("feasible 2-var claim should yield witnesses, got the N/A card")
        else:
            _m, states, _note = setup
            if len(states) < 2:
                fails.append(f"feasible claim should have ≥2 witnesses, got {len(states)}")
            for s in states:
                g = {k.split(".")[-1]: v for k, v in s.items()}
                if "x" in g and "y" in g and not (g["y"] <= g["x"] and g["x"] + g["y"] <= 20):
                    fails.append(f"fabricated witness {g} violates y≤x ∧ x+y≤20")
                    break
    # UNSAT claim → the honest N/A card (return None), never a fabricated plot
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ x ∈ Int ≤ 5\n    0 ≤ y ∈ Int ≤ 5\n    x > y\n    x < y", w)
        if _claim_setup(prefix + ".smt2", prefix + ".schema.json", w + "/out.png") is not None:
            fails.append("UNSAT claim should draw the N/A card (return None)")
    if fails:
        print("PARALLEL-CLAIM FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ parallel_claim (#284): parallel_coords samples a claim's SOLUTION SPACE (z3 witnesses, every "
          "one satisfying the constraint) like scatter_matrix; UNSAT → honest N/A card")
    return 0


if __name__ == "__main__":
    sys.exit(main())
