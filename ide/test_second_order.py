#!/usr/bin/env python3
"""Phase — the second-order shift register in the reachable graph.

A SEEDED two-tick model (velocity carried in the position history via `_x := x - v` + `Δx = Δ_x`)
must be DETERMINISTIC in the reachable graph, not fan out on a free `__x`. The fix captures the
seeded tick-0 `_x` as the (cur, prev) pair-graph's bootstrap prev (`__x(1) = _x(0)`, matching the
runtime's shift register). The forced-prev guard distinguishes a real `_x := …` seed from a free
`_var` — fib's unseeded `_n` (is_second_tick bootstrap) must stay unaffected, since pinning its
`__n` to an arbitrary Z3 pick would collapse fib's reachable graph.

Run from repo root: python3 ide/test_second_order.py
"""
import sys
import tempfile
from collections import Counter

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                          # noqa: E402
from evident_viz import load                            # noqa: E402

DVD = """fsm dvd
    x ∈ Real := 50.0
    _x ∈ Real := x - 3.0
    (0.0 < _x < 100.0) ⇒ Δx = Δ_x
    ¬(0.0 < _x < 100.0) ⇒ Δx = 0.0 - Δ_x"""


def _branch(src):
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export(src, w)
        assert ok, "export failed"
        m = load(prefix + ".smt2", prefix + ".schema.json")
        states, edges = m.reachable(limit=120)
        out_deg = Counter(a for a, _ in edges)
        return m.has_two_tick, len(states), (max(out_deg.values()) if out_deg else 1), states


def main():
    fails = []

    # Seeded second-order model: `_x := x - 3` bootstraps `__x = _x(0)`, so the (cur, prev)
    # pair-graph is single-valued — deterministic, velocity 3.
    two, n, mb, states = _branch(DVD)
    if not two:
        fails.append("seeded dvd should be detected as two-tick")
    if mb != 1:
        fails.append(f"seeded dvd should be DETERMINISTIC (max_branch 1), got {mb}")
    head = [round(s["x"]) for s in states[:5]]
    if head != [50, 53, 56, 59, 62]:
        fails.append(f"dvd trajectory wrong: {head} (expected velocity 3 from _x := x - 3)")

    # fib: UNSEEDED `_n`. The forced-prev guard returns None so the reachable graph is unchanged.
    with open("examples/test_24_fib.ev") as f:
        _, n2, _mb2, st2 = _branch(f.read())
    seq = [s["n"] for s in st2[:6]]
    if n2 < 10:
        fails.append(f"fib reachable collapsed to {n2} states (forced-prev guard regressed it)")
    if seq != [1, 1, 2, 3, 5, 8]:
        fails.append(f"fib sequence wrong: {seq}")

    if fails:
        print("SECOND-ORDER FAILURES:")
        for x in fails:
            print("  ✗", x)
        sys.exit(1)
    print("✓ second-order: a SEEDED two-tick model (`_x := x - 3` + `Δx = Δ_x`) is deterministic in "
          "the reachable graph (max_branch 1, velocity-3 trajectory) — the shift-register bootstrap "
          f"pins `__x = _x(0)`; fib's UNSEEDED `_n` is unaffected ({n2} states, 1 1 2 3 5 8) via the "
          "forced-prev guard")


if __name__ == "__main__":
    main()
