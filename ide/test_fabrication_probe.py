#!/usr/bin/env python3
"""Differential FABRICATION PROBE (#330) — the abstract-view soundness lint at CI time.

The terminal_map / "rest states" verdicts come from a Z3 quantified absorbing-set query
(terminal_states.absorbing_states). This probe cross-checks that query against an INDEPENDENT
brute-force: enumerate the reachable graph and call a state absorbing iff EVERY out-edge is a
self-edge. There is NO hardcoded oracle — it asserts the two methods AGREE, so a fabrication in
EITHER is caught (it would have caught #322 head-on: the old query said all 7 bistable states
absorbing, the brute-force graph says {0,6}). Disagreement on a bounded-discrete model = a soundness
bug, full stop.

Run from repo root: python3 ide/test_fabrication_probe.py
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                          # noqa: E402
from evident_viz import load as load_model              # noqa: E402
from terminal_states import absorbing_states            # noqa: E402
from reachable_region import bounding_box               # noqa: E402

MODELS = [
    ("terminating counter", "fsm c\n    0 ≤ count ∈ Int ≤ 5 := 0\n"
     "    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 5 : _count + 1)"),
    ("cyclic counter", "fsm c\n    0 ≤ count ∈ Int ≤ 5 := 0\n"
     "    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 0 : _count + 1)"),
    ("deterministic bistable", "fsm bistable\n    x ∈ Int\n    is_first_tick ⇒ x = 1\n"
     "    ¬is_first_tick ⇒\n        0 ≤ x\n        x ≤ 6\n"
     "        x = (_x < 3 ? (_x = 0 ? 0 : _x - 1) : (_x > 3 ? (_x = 6 ? 6 : _x + 1) : 3))"),
    ("NONDETERMINISTIC bistable (free step)", "fsm bistable\n    x ∈ Int\n    step ∈ Int\n"
     "    -1 ≤ step ≤ 1\n    is_first_tick ⇒ x = 3\n    0 ≤ x\n    x ≤ 6\n"
     "    Δx = (_x = 0 ? 0 : (_x = 6 ? 0 : step))"),
    ("2-var settle to (2,2)", "fsm s\n    0 ≤ x ∈ Int ≤ 4 := 3\n    0 ≤ y ∈ Int ≤ 4 := 1\n"
     "    ¬is_first_tick ⇒ (x = (_x > 2 ? _x - 1 : (_x < 2 ? _x + 1 : 2)) ∧ "
     "y = (_y > 2 ? _y - 1 : (_y < 2 ? _y + 1 : 2)))"),
    ("NONDETERMINISTIC drift in [0,4] (free d, sticks at 0/4)", "fsm drift\n    0 ≤ x ∈ Int ≤ 4 := 2\n"
     "    d ∈ Int\n    -1 ≤ d ≤ 1\n    ¬is_first_tick ⇒ x = (_x = 0 ? 0 : (_x = 4 ? 4 : _x + d))"),
]


def _key(s):
    return tuple(sorted((k.split(".")[-1], v) for k, v in s.items()))


def _brute_absorbing(m):
    """Absorbing set + reachable set + capped flag from the EXPLORED graph: a state is absorbing iff
    its out-edges are exactly the single self-edge (it can stay AND has no other exit). `capped` means
    the BFS hit its limit, so the reachable set is incomplete and the cross-check can't be trusted."""
    states, edges = m.reachable(limit=500)
    out = {}
    for (i, j) in edges:
        out.setdefault(i, set()).add(j)
    absorbing = {_key(states[i]) for i, js in out.items() if js == {i}}
    return absorbing, {_key(s) for s in states}, len(states) >= 500


def _box_violations(m):
    """reachable_region soundness: a k-induction box claimed BOUNDED must CONTAIN every reachable
    state (it is an over-approximation). A reachable state outside the proven box = an unsound box."""
    r = bounding_box(m)
    if r["verdict"] != "bounded":
        return []
    states, _ = m.reachable(limit=500)
    if len(states) >= 500:
        return []                                          # reachable incomplete — can't cross-check
    bad = []
    for s in states:
        for v, (lo, hi) in r["box"].items():
            val = s.get(v)
            if val is not None and not (lo <= val <= hi):
                bad.append((v, val, (lo, hi)))
    return bad


def main():
    fails, checked = [], 0
    for name, src in MODELS:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, dropped, msg = _export(src, w)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            if any(v.get("kind") == "real" for v in m.carried):
                continue                                  # real-valued — not exactly enumerable
            for (v, val, rng) in _box_violations(m):      # reachable_region: brute-reachable ⊆ proven box
                fails.append(f"{name}: reachable {v}={val} OUTSIDE the proven box {rng}")
            abs_states, decided = absorbing_states(m)
            if not decided:
                continue                                  # Z3 unknown — nothing to cross-check
            brute, reachable, capped = _brute_absorbing(m)
            if capped:
                continue                                  # reachable BFS hit the cap — brute incomplete
            abstract = {_key(s) for s in abs_states}
            # Compare on the reachable domain: the abstract query may also surface UNREACHABLE
            # absorbing states, which the explored graph can't see.
            if (abstract & reachable) != brute:
                fails.append(f"{name}: abstract∩reachable={sorted(abstract & reachable)} "
                             f"!= brute-force={sorted(brute)}")
            checked += 1
    if checked == 0:
        fails.append("probe checked 0 models — the enumerable gate is too strict (a vacuous pass)")
    if fails:
        print("FABRICATION PROBE — abstract Z3 absorbing-set DISAGREES with brute-force (a soundness bug):")
        for f in fails:
            print("  ✗", f)
        return 1
    print(f"✓ fabrication_probe (#330): on {checked} bounded-discrete FSMs (det + nondeterministic) the "
          f"abstract Z3 absorbing-set == brute-force reachable graph AND every k-induction reachable box "
          f"CONTAINS its reachable states — no fabrication in either abstract dynamical view")
    return 0


if __name__ == "__main__":
    sys.exit(main())
