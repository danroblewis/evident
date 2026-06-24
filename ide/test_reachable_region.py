#!/usr/bin/env python3
"""Phase 2.22 — ABSTRACT reachable-region analysis (viz/reachable_region.py + render).

Pins the k-induction bounding box: a bounded daemon gets a box PROVEN 1-inductive over the one-step
relation (base: init ⊆ box; step: box closed under the transition — both UNSAT, no enumeration), and
an UNBOUNDED model is flagged — including the free random walk where full_state_graph can't run.

Run from repo root: python3 ide/test_reachable_region.py
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                          # noqa: E402
from evident_viz import load as load_model              # noqa: E402
from reachable_region import bounding_box               # noqa: E402

CASES = [
    ("cyclic counter",
     "fsm c\n    0 ≤ count ∈ Int ≤ 5 := 0\n    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 0 : _count + 1)",
     "bounded", True, {"count": (0, 5)}),
    ("random walk in a [0,10]² box",
     "fsm walk_box\n    0 ≤ x ∈ Int ≤ 10 := 5\n    0 ≤ y ∈ Int ≤ 10 := 5\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1",
     "bounded", True, {"x": (0, 10), "y": (0, 10)}),
    ("random_walk (UNBOUNDED — full_state_graph N/A)",
     "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1",
     "unbounded", False, None),
]


def _short(box):
    return {k.split(".")[-1]: tuple(v) for k, v in box.items()}


def main():
    fails = []
    for name, src, want_verdict, want_inductive, want_box in CASES:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, dropped, msg = _export(src, w)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            r = bounding_box(m)
            if r["verdict"] != want_verdict:
                fails.append(f"{name}: verdict {r['verdict']!r} != {want_verdict!r}")
            elif want_verdict == "bounded":
                if r["inductive"] != want_inductive:
                    fails.append(f"{name}: inductive {r['inductive']} != {want_inductive}")
                elif _short(r["box"]) != want_box:
                    fails.append(f"{name}: box {_short(r['box'])} != {want_box}")
    if fails:
        print("REACHABLE-REGION FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ reachable_region: k-induction bounds the reachable set — counter⊆[0,5], walk_box⊆[0,10]² "
          "(both PROVEN 1-inductive), UNBOUNDED random_walk flagged (brute-force N/A)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
