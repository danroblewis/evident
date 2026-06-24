#!/usr/bin/env python3
"""The analyze-path fixed-point set (model_analysis.solution_structure → the solution_space
'rest states' / 'fixed point' sidebar).

A reachable state is a fixed point iff its ONLY successor is itself (absorbing) — NOT merely if it has
a self-edge. A free input (a nondeterministic `step`) gives every state a self-edge, so keying on
self-edges alone fabricates fixed points: the #322 soundness class, here in the solution_space sidebar
(Ana's scope note after the terminal_map re-check). This pins the absorbing-only notion.

Run from repo root: python3 ide/test_fixed_points.py
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                          # noqa: E402
from evident_viz import load as load_model              # noqa: E402

CASES = [
    ("nondeterministic bistable (free step ±1 — every state has a self-edge; only 0,6 are absorbing)",
     "fsm bistable\n    x ∈ Int\n    step ∈ Int\n    -1 ≤ step ≤ 1\n    is_first_tick ⇒ x = 3\n"
     "    0 ≤ x\n    x ≤ 6\n    Δx = (_x = 0 ? 0 : (_x = 6 ? 0 : step))",
     [0, 6]),
    ("terminating counter (0→5, then stays)",
     "fsm c\n    0 ≤ count ∈ Int ≤ 5 := 0\n    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 5 : _count + 1)",
     [5]),
    ("cyclic counter (0→5→0, no rest state)",
     "fsm c\n    0 ≤ count ∈ Int ≤ 5 := 0\n    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 0 : _count + 1)",
     []),
]


def main():
    fails = []
    for name, src, want in CASES:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, dropped, msg = _export(src, w)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            fps = m.solution_structure().get("fixed_points", [])
            got = sorted(list(s.values())[0] for s in fps)
            if got != want:
                fails.append(f"{name}: fixed_points {got} != {want}")
    if fails:
        print("FIXED-POINT FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ fixed_points (analyze 'rest states' sidebar): absorbing-only, not self-edge — "
          "nondeterministic bistable→{0,6} (not all 7), counter→{5}, cyclic→∅")
    return 0


if __name__ == "__main__":
    sys.exit(main())
