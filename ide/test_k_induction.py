#!/usr/bin/env python3
"""#327 — k-induction depth: reachable_region.k_induction_box(m, k) deepens the unrolling. Pin the
HONESTY: a SATURATING counter's box CLOSES (proven inductive — provably contains every reachable state)
once k reaches the saturation; an UNBOUNDED counter's box NEVER closes (it grows with every k). `closed`
is the proven/sampled signal the badge reads, so a wrong closed=True would be dishonest — pin it.
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                            # noqa: E402
from evident_viz import load as load_model                # noqa: E402
from reachable_region import k_induction_box              # noqa: E402

SAT = ("fsm c\n    0 ≤ x ∈ Int ≤ 5\n    is_first_tick ⇒ x = 0\n"
       "    ¬is_first_tick ⇒ x = (_x ≥ 5 ? 5 : _x + 1)")
UNB = "fsm c\n    x ∈ Int\n    is_first_tick ⇒ x = 0\n    ¬is_first_tick ⇒ x = _x + 1"


def main():
    fails = []
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export(SAT, w)
        m = load_model(prefix + ".smt2", prefix + ".schema.json")
        if k_induction_box(m, 1)["closed"]:
            fails.append("saturating: k=1 is too shallow — must NOT yet be proven closed")
        deep = k_induction_box(m, 8)
        if not deep["closed"]:
            fails.append(f"saturating: k=8 box should be CLOSED (proven inductive), got {deep}")
        if deep["box"].get("x") != [0, 5]:
            fails.append(f"saturating: the closed box should be [0, 5], got {deep['box']}")
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export(UNB, w)
        m = load_model(prefix + ".smt2", prefix + ".schema.json")
        for k in (1, 4, 8):
            if k_induction_box(m, k)["closed"]:
                fails.append(f"unbounded: must NEVER be proven closed, but k={k} claimed closed")
        if not (k_induction_box(m, 8)["box"]["x"][1] > k_induction_box(m, 4)["box"]["x"][1]):
            fails.append("unbounded: the box upper bound should GROW with k (never converge)")
    if fails:
        print("K-INDUCTION FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ k_induction (#327): a saturating counter's box CLOSES (proven inductive [0,5]) at depth k, "
          "an unbounded counter NEVER closes (box grows with every k) — honest proven-vs-open")
    return 0


if __name__ == "__main__":
    sys.exit(main())
