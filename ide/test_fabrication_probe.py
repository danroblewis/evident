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
from soundness_check import soundness_report            # noqa: E402

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


def main():
    fails, checked = [], 0
    for name, src in MODELS:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, dropped, msg = _export(src, w)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            r = soundness_report(m)
            if not r["applicable"]:
                continue                                  # real-valued / capped — not exactly enumerable
            if r["absorbing_ok"] is False:
                fails.append(f"{name}: ABSORBING fabrication — {r['detail']}")
            if r["box_ok"] is False:
                fails.append(f"{name}: BOX unsound — {r['detail']}")
            checked += 1
    if checked == 0:
        fails.append("probe checked 0 models — the enumerable gate is too strict (a vacuous pass)")
    if fails:
        print("FABRICATION PROBE — abstract Z3 absorbing-set DISAGREES with brute-force (a soundness bug):")
        for f in fails:
            print("  ✗", f)
        return 1
    print(f"✓ fabrication_probe (#330): soundness_check.soundness_report finds NO fabrication on {checked} "
          f"bounded-discrete FSMs (abstract Z3 absorbing-set == brute graph, k-induction box ⊇ reachable) "
          f"— the SAME cross-check the on-demand 'verify soundness' button runs (#332)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
