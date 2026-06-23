#!/usr/bin/env python3
"""Test that the analyze STATS/BANNER follow the all_conditions toggle (#316 follow-up).

We shipped an `all_conditions` toggle: when on, the state_graph PNG renders the GLOBAL
transition graph (`Model.full_state_graph`) over every initial condition, instead of the
from-init reachable set. But the analyze stats/banner kept reporting the from-init set — so
the PNG showed 7 states while the banner read "2 reachable". Inconsistent.

This pins the fix: when `all_conditions` is on (on the state_graph view), `_reachable_stats`
and the analyze banner summarize the SAME global graph the PNG draws.

  - bistable (two deterministic basins, x ∈ 0..6) → all_conditions=False reports the 2 states
    reachable from the seed (x=1); all_conditions=True reports the 7-state GLOBAL graph. The
    stat FLIPS with the flag — the headline. And the banner says "7 states over all initial
    conditions" when on, never when off.
  - the flag is gated on view == "state_graph": on a non-state_graph view, all_conditions does
    NOT change the stats (the toggle is a state_graph affordance; other views are from-init).
  - a Real-valued model has no enumerable global product → full_state_graph returns
    discrete=False and `_reachable_stats` falls back to from-init even with the flag on, so the
    banner stays honest rather than silently empty.

Run from repo root: `python3 ide/test_all_conditions_stats.py` (exit non-zero on any failure)."""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from evident_viz import load as load_model                  # noqa: E402
from analysis import _reachable_stats                       # noqa: E402
from server import analyze, Source                          # noqa: E402

# The exact bistable from the task: x flows to the nearest wall (0 or 6). From init x=1 the
# orbit reaches only {1,0} (2 states); the GLOBAL graph over every x ∈ 0..6 is all 7 states.
BISTABLE = (
    "fsm bistable\n"
    "    0 ≤ x ∈ Int ≤ 6\n"
    "    is_first_tick ⇒ x = 1\n"
    "    (¬is_first_tick ∧ _x < 3) ⇒ Δx = (_x > 0 ? -1 : 0)\n"
    "    (¬is_first_tick ∧ _x ≥ 3) ⇒ Δx = (_x < 6 ? 1 : 0)\n")

# Real-valued state: no finite enumerable product → full_state_graph discrete=False, so the
# stats fall back to from-init even with the flag on (the honesty gate).
REAL = (
    "fsm decay\n"
    "    x ∈ Real\n"
    "    is_first_tick ⇒ x = 100\n"
    "    ¬is_first_tick ⇒ x = _x / 2\n")


def _load(src, work):
    ok, prefix, dropped, msg = _export(src, work)
    if not ok:
        raise RuntimeError(f"export failed: {msg.splitlines()[0][:80]}")
    return load_model(prefix + ".smt2", prefix + ".schema.json")


def check_stats_flip():
    """`_reachable_stats(all_conditions=…)` flips the state count from the from-init set (2)
    to the global graph (7) for the bistable — the unit-level core of the fix."""
    fails = []
    with tempfile.TemporaryDirectory() as work:
        m = _load(BISTABLE, work)
        off = _reachable_stats(m, 400, all_conditions=False)
        on = _reachable_stats(m, 400, all_conditions=True)
        n_off, n_on = off[2], on[2]
        if n_off != 2:
            fails.append(f"stats: from-init (all_conditions=False) reports {n_off} states, expected 2")
        if n_on != 7:
            fails.append(f"stats: global (all_conditions=True) reports {n_on} states, expected 7")
        if not (n_on > n_off):
            fails.append(f"stats: global ({n_on}) did not exceed from-init ({n_off}) — the flag did nothing")
    return fails


def check_analyze_banner():
    """The full /api/analyze flow: with all_conditions on (state_graph view) the response's
    `states` and `banner` reflect the global graph; with it off, EXACTLY the old behavior."""
    fails = []
    off = analyze(Source(source=BISTABLE, view="state_graph", all_conditions=False))
    on = analyze(Source(source=BISTABLE, view="state_graph", all_conditions=True))
    if not (off.get("ok") and on.get("ok")):
        return [f"analyze: not ok (off={off.get('error')}, on={on.get('error')})"]
    if off["states"] != 2:
        fails.append(f"analyze: all_conditions=False reports {off['states']} states, expected 2")
    if on["states"] != 7:
        fails.append(f"analyze: all_conditions=True reports {on['states']} states, expected 7")
    # Honest banner wording: the global verdict names the global root, the default never does.
    if "over all initial conditions" not in on["banner"]:
        fails.append(f"analyze: global banner missing 'over all initial conditions': {on['banner']!r}")
    if "over all initial conditions" in off["banner"]:
        fails.append(f"analyze: default banner leaked global wording: {off['banner']!r}")
    if "7 states" not in on["banner"]:
        fails.append(f"analyze: global banner should report 7 states: {on['banner']!r}")
    return fails


def check_non_state_graph_view_unaffected():
    """The toggle is a state_graph affordance: on a NON-state_graph view, all_conditions must
    NOT change the stats — they stay the from-init reachable set."""
    fails = []
    base = analyze(Source(source=BISTABLE, view="time_series", all_conditions=False))
    flagged = analyze(Source(source=BISTABLE, view="time_series", all_conditions=True))
    if not (base.get("ok") and flagged.get("ok")):
        return [f"analyze(time_series): not ok ({base.get('error')}, {flagged.get('error')})"]
    if base["states"] != flagged["states"]:
        fails.append(f"analyze: all_conditions changed stats on a non-state_graph view "
                     f"({base['states']} → {flagged['states']}) — it must be a state_graph-only toggle")
    if "over all initial conditions" in flagged["banner"]:
        fails.append(f"analyze: time_series banner leaked global wording: {flagged['banner']!r}")
    return fails


def check_real_fallback():
    """A Real-valued model has no enumerable global product: full_state_graph returns
    discrete=False, so `_reachable_stats` with the flag on falls back to from-init (no crash,
    no empty stats)."""
    fails = []
    with tempfile.TemporaryDirectory() as work:
        m = _load(REAL, work)
        off = _reachable_stats(m, 400, all_conditions=False)
        on = _reachable_stats(m, 400, all_conditions=True)
        if on[2] == 0:
            fails.append("stats(real): all_conditions=True produced 0 states — fallback to from-init failed")
        if on[2] != off[2]:
            fails.append(f"stats(real): all_conditions changed a non-enumerable model "
                         f"({off[2]} → {on[2]}) — should fall back to the from-init set")
    return fails


def main():
    fails = []
    fails += check_stats_flip()
    fails += check_analyze_banner()
    fails += check_non_state_graph_view_unaffected()
    fails += check_real_fallback()
    if fails:
        print("ALL-CONDITIONS STATS TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ all-conditions stats: bistable flips 2 (from-init) → 7 (global) with the flag; "
          "banner says 'over all initial conditions' only when on; non-state_graph views are "
          "unaffected; real-valued falls back to from-init")
    return 0


if __name__ == "__main__":
    sys.exit(main())
