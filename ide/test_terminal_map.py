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
from terminal_states import classify, stability, reach_path        # noqa: E402

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
    ("SHIPPED bistable (free step ±1 — NONDETERMINISTIC; the absorbing-soundness repro, Ana #322)",
     "fsm bistable\n    x ∈ Int\n    step ∈ Int\n    -1 ≤ step ≤ 1\n    is_first_tick ⇒ x = 3\n"
     "    0 ≤ x\n    x ≤ 6\n    Δx = (_x = 0 ? 0 : (_x = 6 ? 0 : step))",
     "terminates", [{"x": 0}, {"x": 6}]),
    ("random_walk (UNBOUNDED 2D — brute-force can't enumerate)",
     "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1",
     "daemon", []),
]


def _short(s):
    return {k.split(".")[-1]: v for k, v in s.items()}


def _set(states):
    return {frozenset(_short(s).items()) for s in states}


def _check_reach_path(fails):
    """#326: reach_path — BFS from init (index 0) to the nearest absorbing state → the trajectory to rest."""
    rp = [{"x": 0}, {"x": 1}, {"x": 2}]
    if reach_path(rp, [(0, 1), (1, 2), (2, 2)], {2}) != rp:
        fails.append("#326 reach_path: 0→1→2 (2 absorbing) should be the full path")
    if reach_path(rp, [(0, 1), (1, 2), (2, 2)], {0}) is not None:
        fails.append("#326 reach_path: init already absorbing → None")
    if reach_path([{"x": 0}, {"x": 1}], [(0, 0)], {1}) is not None:
        fails.append("#326 reach_path: no path to absorbing → None")


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
    # #20 stability + #323 soundness: the DETERMINISTIC bistable's walls 0,6 are stable, saddle 3
    # unstable; the NONDETERMINISTIC shipped bistable (free step) must claim NO stability — the
    # perturb-and-step direction is ambiguous, so every terminal is 'unknown'.
    det = next(c[1] for c in CASES if "gambler" in c[0])
    nd = next(c[1] for c in CASES if "free step" in c[0])
    for src, want in [(det, {0: "stable", 3: "unstable", 6: "stable"}),
                      (nd, {0: "unknown", 6: "unknown"})]:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, *_ = _export(src, w)
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            numeric = [v for v in m.carried if v["kind"] in ("int", "real")]
            got = {s[numeric[0]["name"]]: stability(m, s, numeric) for s in classify(m)["states"]}
            if got != want:
                fails.append(f"stability {got} != {want}")

    # #328 must-rest: deterministic bistable + counter — EVERY run reaches rest (non-rest is a DAG);
    # the nondeterministic bistable — a run can loop at x=3 forever (a cycle in non-rest), so NOT every
    # run rests.
    counter = next(c[1] for c in CASES if "terminating counter" in c[0])
    for src, want_mr in [(det, True), (nd, False), (counter, True)]:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, *_ = _export(src, w)
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            got_mr = classify(m).get("must_rest")
            if got_mr != want_mr:
                fails.append(f"must_rest {got_mr} != {want_mr}")

    # #333 witness: 'can rest, not always' must come with a concrete non-rest cycle the FSM can ride
    # forever — a closed loop of states none of which are absorbing (0,6 for the bistable).
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export(nd, w)
        m = load_model(prefix + ".smt2", prefix + ".schema.json")
        cyc = classify(m).get("rest_cycle")
        xs = [list(s.values())[0] for s in cyc] if cyc else []
        if not cyc or xs[0] != xs[-1] or any(x in (0, 6) for x in xs):
            fails.append(f"rest_cycle {xs} is not a closed non-absorbing loop")

    _check_reach_path(fails)

    if fails:
        print("TERMINAL-MAP FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ terminal_map: absorbing set SOUND on nondeterministic FSMs (shipped free-step "
          "bistable→{0,6}, not all 7); counter→{5}, cyclic→∅, det-bistable→{0,3,6}, random_walk→∅; "
          "stability: det 0,6 stable/3 unstable, nondeterministic→unknown; must-rest (#328): "
          "det+counter every-run-rests, nondet can-loop-at-3 (+ #333 witness cycle); #326 reach_path "
          "init→terminal")
    return 0


if __name__ == "__main__":
    sys.exit(main())
