#!/usr/bin/env python3
"""Phase 2.21 — ABSTRACT terminal-state analysis (viz/terminal_states.py + render_terminal_map).

Pins that the absorbing/terminal set is solved ABSTRACTLY from the one-step relation via Z3
(no trajectory walking, no state-product enumeration): it agrees with brute-force on bounded
models AND decides the UNBOUNDED random walk — where full_state_graph() can't enumerate at all.
The headline is the daemon-vs-terminates verdict.

Run from repo root: python3 ide/test_terminal_map.py
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                          # noqa: E402
from evident_viz import load as load_model              # noqa: E402
from terminal_states import classify, stability                    # noqa: E402

CASES = [
    ("terminating counter (0→5, then stays)",
     "fsm c\n    0 ≤ count ∈ Int ≤ 5 := 0\n    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 5 : _count + 1)",
     "terminates", [{"count": 5}]),
    ("cyclic counter (0→5→0, daemon)",
     "fsm c\n    0 ≤ count ∈ Int ≤ 5 := 0\n    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 0 : _count + 1)",
     "daemon", []),
    ("bistable / gambler's ruin (walls 0,6 + saddle 3)",
     "fsm bistable\n    x ∈ Int\n    is_first_tick ⇒ x = 1\n    ¬is_first_tick ⇒\n        0 ≤ x\n"
     "        x ≤ 6\n        x = (_x < 3 ? (_x = 0 ? 0 : _x - 1) : (_x > 3 ? (_x = 6 ? 6 : _x + 1) : 3))",
     "terminates", [{"x": 0}, {"x": 3}, {"x": 6}]),
    ("random_walk (UNBOUNDED 2D — brute-force can't enumerate)",
     "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1",
     "daemon", []),
]


def _short(s):
    return {k.split(".")[-1]: v for k, v in s.items()}


def _set(states):
    return {frozenset(_short(s).items()) for s in states}


def main():
    fails = []
    for name, src, want_verdict, want_states in CASES:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, dropped, msg = _export(src, w)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            c = classify(m)
            if c["verdict"] != want_verdict:
                fails.append(f"{name}: verdict {c['verdict']!r} != {want_verdict!r}")
            elif _set(c["states"]) != _set(want_states):
                fails.append(f"{name}: terminal set {[_short(s) for s in c['states']]} != {want_states}")
    # #20: fixed-point stability — the bistable's walls 0,6 are stable, the saddle 3 is unstable.
    bist = next(c[1] for c in CASES if "bistable" in c[0])
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export(bist, w)
        m = load_model(prefix + ".smt2", prefix + ".schema.json")
        numeric = [v for v in m.carried if v["kind"] in ("int", "real")]
        got = {s[numeric[0]["name"]]: stability(m, s, numeric) for s in classify(m)["states"]}
        if got != {0: "stable", 3: "unstable", 6: "stable"}:
            fails.append(f"bistable stability {got} != {{0:stable, 3:unstable, 6:stable}}")

    if fails:
        print("TERMINAL-MAP FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ terminal_map: absorbing set decides daemon-vs-terminates (counter→{5}, cyclic→∅, "
          "bistable→{0,3,6}, random_walk→∅); + stability: bistable 0,6 stable, 3 unstable (saddle)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
