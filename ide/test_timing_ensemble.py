#!/usr/bin/env python3
"""Test that render_timing_diagram roots on ALL INITIAL CONDITIONS (diagram review).

A timing diagram historically followed ONE forward trajectory from ONE seed and drew
it as digital waveforms — for a deterministic FSM that is a single run, not the
program's behavior. The diagram review scored timing_diagram NO for exactly that. This
pins the fix: for a finitely-DISCRETE program the diagram roots on the GLOBAL
all-initial-conditions graph (`Model.full_state_graph` — the same root state_graph /
basin_map / transition_matrix use), follows EVERY valid starting state forward, and the
renderer draws the per-signal reachable ENVELOPE over that ensemble.

  - traffic (enum Light + bounded-int timer) / counter (bounded-int) — finitely
    enumerable → `build_ensemble` returns ONE timeline per valid initial carried
    assignment; the SET of roots equals `full_state_graph`'s state set (a SUPERSET of
    the single from-init seed), so the diagram is over all initial conditions, not one
    run. Each timeline is a forward successor chain (the REAL dynamics).
  - the per-signal band (`track_band`) at each tick is exactly the set of values the
    signal takes across the ensemble — proving the envelope is the real reachable spread,
    not a fabricated range. A 0/1-valued signal (bool) maps to {0,1} digital levels.
  - real-valued state (decay) → full_state_graph is discrete=False, so `build_ensemble`
    returns None and the renderer keeps the honest single-seed fallback.

Run from repo root: `python3 ide/test_timing_ensemble.py` (non-zero on fail)."""
import os
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                                # noqa: E402
from evident_viz import load as load_model                    # noqa: E402
import render_timing_diagram as RTD                           # noqa: E402
from timing_ensemble import build_ensemble, track_band        # noqa: E402

# Traffic light: 3 lights × 3 timer values = 9 states (enum + bounded int).
TRAFFIC = (
    "enum Light = Red | Green | Yellow\n"
    "fsm traffic\n"
    "    light ∈ Light\n"
    "    0 ≤ timer ∈ Int ≤ 2\n"
    "    is_first_tick ⇒ (light = Red ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer < 2) ⇒ (light = _light ∧ timer = _timer + 1)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Red) ⇒ (light = Green ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Green) ⇒ (light = Yellow ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Yellow) ⇒ (light = Red ∧ timer = 0)\n")

# Bounded cyclic counter 0→…→5→0: 6 states, a single int signal.
COUNTER = (
    "fsm counter\n"
    "    0 ≤ count ∈ Int ≤ 5\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 0 : _count + 1)\n")

# A bool toggle + bounded int: the bool signal must read as a 0/1 DIGITAL trace.
FLIP = (
    "fsm flip\n"
    "    on ∈ Bool\n"
    "    0 ≤ k ∈ Int ≤ 3\n"
    "    is_first_tick ⇒ (on = false ∧ k = 0)\n"
    "    ¬is_first_tick ⇒ (on = ¬_on ∧ k = (_k ≥ 3 ? 0 : _k + 1))\n")

# Real-valued state: not finitely enumerable → discrete=False → single-seed fallback.
REAL = (
    "fsm decay\n"
    "    x ∈ Real\n"
    "    is_first_tick ⇒ x = 100\n"
    "    ¬is_first_tick ⇒ x = _x / 2\n")

TICKS = RTD.TICKS


def _load(src, work):
    ok, prefix, _dropped, msg = _export(src, work)
    if not ok:
        raise RuntimeError(f"export failed: {msg.splitlines()[0][:80]}")
    return prefix, load_model(prefix + ".smt2", prefix + ".schema.json")


def _real_band_at(m, track, ensemble, t):
    """The set of values `track` takes at tick t, recomputed independently of the
    renderer from the ensemble — the ground truth the envelope must match."""
    return {track["get"](trace[t]) for trace in ensemble}


def check_ensemble(name, src, expected_inits):
    fails = []
    with tempfile.TemporaryDirectory() as work:
        prefix, m = _load(src, work)

        # 1. The ensemble must exist and root on the GLOBAL all-conditions graph.
        ensemble = build_ensemble(m, TICKS)
        if ensemble is None:
            return [f"{name}: build_ensemble returned None (a finitely-discrete program "
                    f"must trace an ensemble over all initial conditions)"]

        gl_states, _gl_edges, info = m.full_state_graph(limit=5000)
        if not (info["discrete"] and not info["capped"]):
            return [f"{name}: full_state_graph not discrete/uncapped — fixture not enumerable"]

        # 2. One timeline per valid initial carried assignment: the SET of roots
        #    (timeline starts) must equal full_state_graph's state set — a SUPERSET of
        #    the single from-init seed. (No sampling for these small fixtures.)
        root_keys = {m.state_key(trace[0]) for trace in ensemble}
        gl_keys = {m.state_key(s) for s in gl_states}
        if root_keys != gl_keys:
            fails.append(f"{name}: ensemble roots ({len(root_keys)}) != global graph "
                         f"({len(gl_keys)}) — not rooted on all initial conditions")
        if len(root_keys) != expected_inits:
            fails.append(f"{name}: expected {expected_inits} initial conditions, "
                         f"got {len(root_keys)}")
        init = m.initial_state()
        if init is not None and m.state_key(init) not in root_keys:
            fails.append(f"{name}: the single from-init seed is not among the ensemble roots")

        # 3. Each timeline spans the full time axis and steps the REAL transition: every
        #    consecutive pair must be a real successor (or a held last-state at a fixed
        #    point / cycle entry).
        for ti, trace in enumerate(ensemble):
            if len(trace) != TICKS + 1:
                fails.append(f"{name}: timeline {ti} has {len(trace)} ticks, expected {TICKS + 1}")
                break
            bad = next((t for t in range(len(trace) - 1)
                        if m.state_key(trace[t]) != m.state_key(trace[t + 1])
                        and m.state_key(trace[t + 1])
                        not in {m.state_key(s) for s in m.successors(trace[t])}), None)
            if bad is not None:
                fails.append(f"{name}: timeline {ti} step {bad}→{bad + 1} is not a real "
                             f"transition edge")
                break

        # 4. The per-signal band == the real per-tick value spread (track_band is the
        #    envelope the renderer fills). Recompute independently and compare.
        tracks = RTD._expand_tracks(m.state_vars)
        for tr in tracks:
            bands = track_band(tr, ensemble, len(ensemble[0]))
            for t in (0, 1, TICKS // 2, TICKS):
                got = set(bands[t])
                want = _real_band_at(m, tr, ensemble, t)
                if got != want:
                    fails.append(f"{name}: signal {tr['name']} band at tick {t} = {got} "
                                 f"!= real value spread {want}")
                    break

        # 5. End-to-end render: a non-empty PNG with the all-conditions title.
        out = work + f"/{name}.timing.png"
        RTD.render(m, out)
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append(f"{name}: renderer produced no PNG")
    return fails


def check_digital_levels():
    """A 0/1-valued (bool) signal must map to {0.0, 1.0} digital lane levels — the
    review's ask (b): treat 0/1 signals as proper digital traces."""
    fails = []
    with tempfile.TemporaryDirectory() as work:
        _prefix, m = _load(FLIP, work)
        ensemble = build_ensemble(m, TICKS)
        if ensemble is None:
            return ["flip: build_ensemble returned None (bool+bounded-int is enumerable)"]
        tracks = RTD._expand_tracks(m.state_vars)
        on = next((t for t in tracks if t["name"] == "on"), None)
        if on is None:
            return ["flip: no 'on' bool track found"]
        bands = track_band(on, ensemble, len(ensemble[0]))
        observed = {v for band in bands for v in band}
        if observed - {True, False}:
            fails.append(f"flip: bool signal took non-bool values {observed}")
        level, lo, hi = RTD._ordinal_levels(m, on, list(observed))
        if (level(True), level(False)) != (1.0, 0.0) or (lo, hi) != ("0", "1"):
            fails.append(f"flip: bool levels not digital — True→{level(True)}, "
                         f"False→{level(False)}, labels=({lo},{hi}); expected 1/0 and '0'/'1'")
    return fails


def check_real_fallback():
    """A Real-valued program must NOT build an ensemble (full_state_graph discrete=False);
    the renderer keeps the honest single-seed trace."""
    fails = []
    with tempfile.TemporaryDirectory() as work:
        _prefix, m = _load(REAL, work)
        _s, _e, info = m.full_state_graph(limit=5000)
        if info["discrete"]:
            fails.append("real: full_state_graph discrete=True — a Real var must not enumerate")
        if build_ensemble(m, TICKS) is not None:
            fails.append("real: build_ensemble returned an ensemble on a Real-valued program "
                         "(must fall back to single-seed)")
        out = work + "/decay.timing.png"
        RTD.render(m, out)
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append("real: renderer produced no PNG")
    return fails


def main():
    fails = []
    fails += check_ensemble("traffic", TRAFFIC, expected_inits=9)
    fails += check_ensemble("counter", COUNTER, expected_inits=6)
    fails += check_digital_levels()
    fails += check_real_fallback()
    if fails:
        print("TIMING-DIAGRAM ALL-CONDITIONS TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ timing_diagram: traffic (9) + counter (6) trace an ensemble over ALL initial "
          "conditions (roots == full_state_graph ⊇ from-init), each timeline a real successor "
          "chain, the per-signal band == the real value envelope; bool signals are 0/1 digital; "
          "real falls back to single-seed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
