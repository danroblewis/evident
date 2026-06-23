#!/usr/bin/env python3
"""Test the reachability_tree rooting upgrade (diagram-review: scored NO).

The review said: 'root the reachability tree from the SET of initial conditions, not one
seed' + 'use closing_depth()/reachable() to show the tree CLOSING instead of capping at
depth 8 for finite discrete systems'. The old renderer rooted from initial_state() (one
seed) and stopped at a hard MAX_DEPTH=8 cap.

This pins the upgraded behavior:

  - traffic (cyclic, bounded int + enum) → all-conditions mode: the forest roots off the
    synthetic ∅ root over the initial-condition SET, the BFS CLOSES (complete=True) at the
    saturation depth from Model.closing_depth — NOT a hard depth-8 cap — and reaches the
    whole discrete reachable set.
  - terminating counter (climbs to a fixed point) → all-conditions, closes at its true
    depth, and the terminal fixed point is marked absorbing.
  - free-init traffic (no is_first_tick seed) → a real FOREST: every discrete start state
    is an initial condition, so the synthetic root fans to MANY roots, not one.
  - real-valued → fallback mode: the reachable set is infinite, so it does NOT claim to
    close and falls back to the single-seed depth-capped sample (the honesty gate).

Run from repo root: `python3 ide/test_reachability_tree.py` (exit non-zero on any failure).
"""
import os
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from evident_viz import load as load_model                 # noqa: E402
import render_reachability_tree as RT                       # noqa: E402
import reachability_forest as RF                             # noqa: E402

# Cyclic: Red→Green→Yellow, each held 3 ticks. Bounded → finite reachable set that CLOSES.
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

# Terminating: count climbs 0..5 then HOLDS at 5 (a fixed point / absorbing goal).
COUNTER = (
    "fsm counter\n"
    "    0 ≤ count ∈ Int ≤ 5\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 5 : _count + 1)\n")

# Free initial condition (NO is_first_tick seed) → every discrete state is an init → a forest.
TRAFFIC_FREE = (
    "enum Light = Red | Green | Yellow\n"
    "fsm traffic2\n"
    "    light ∈ Light\n"
    "    0 ≤ timer ∈ Int ≤ 2\n"
    "    (¬is_first_tick ∧ _timer < 2) ⇒ (light = _light ∧ timer = _timer + 1)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Red) ⇒ (light = Green ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Green) ⇒ (light = Yellow ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Yellow) ⇒ (light = Red ∧ timer = 0)\n")

# Real-valued: infinite reachable set → must NOT claim to close; falls back to one seed.
REAL = (
    "fsm decay\n"
    "    x ∈ Real\n"
    "    is_first_tick ⇒ x = 100\n"
    "    ¬is_first_tick ⇒ x = _x / 2\n")


def _load(src, work):
    ok, prefix, dropped, msg = _export(src, work)
    if not ok:
        raise RuntimeError(f"export failed: {msg.splitlines()[0][:80]}")
    return prefix, load_model(prefix + ".smt2", prefix + ".schema.json")


def _build(m):
    """The renderer's rooting decision (mode + closing + graph), exercised directly."""
    (G, states, depth, absorbing, root_k, truncated,
     mode, closing_k, complete, seed_src) = RF.build(m)
    real = [nk for nk in G.nodes() if states.get(nk) is not None] if G else []
    roots = sum(1 for _ in G.successors(RF.ROOT)) if (G and RF.ROOT in G) else 0
    return G, states, absorbing, mode, closing_k, complete, truncated, real, roots


def _render_nonempty(prefix, name, fails):
    out = prefix + f".{name}.reach.png"
    RT.render(prefix + ".smt2", prefix + ".schema.json", out)
    if not (os.path.exists(out) and os.path.getsize(out) > 0):
        fails.append(f"{name}: renderer produced no PNG")


def check_traffic_closes():
    fails = []
    with tempfile.TemporaryDirectory() as work:
        prefix, m = _load(TRAFFIC, work)
        G, states, absorbing, mode, ck, complete, truncated, real, roots = _build(m)
        if mode != "all-conditions":
            fails.append(f"traffic: mode={mode!r}, expected 'all-conditions' "
                         f"(a bounded enum+int system roots from all inits)")
        if not complete or truncated:
            fails.append(f"traffic: did not CLOSE (complete={complete}, truncated={truncated}) "
                         f"— a finite cyclic system must close, not cap at depth 8")
        # The closing depth must match Model.closing_depth and be the REAL saturation point,
        # not the old hard MAX_DEPTH cap (8). For this traffic the cycle saturates at 8 too,
        # so assert it equals closing_depth AND is reported as a genuine close.
        cd_k, cd_complete = m.closing_depth()
        if ck != cd_k or not cd_complete:
            fails.append(f"traffic: closing depth {ck} != Model.closing_depth {cd_k} "
                         f"(complete={cd_complete})")
        # All 9 discrete states (3 lights × 3 timer values) reached.
        xs = {(states[nk]["light"], states[nk]["timer"]) for nk in real}
        if len(xs) != 9:
            fails.append(f"traffic: reached {len(xs)} states, expected the full 9 "
                         f"(Red/Green/Yellow × timer 0/1/2)")
        _render_nonempty(prefix, "traffic", fails)
    return fails


def check_counter_terminates():
    fails = []
    with tempfile.TemporaryDirectory() as work:
        prefix, m = _load(COUNTER, work)
        G, states, absorbing, mode, ck, complete, truncated, real, roots = _build(m)
        if mode != "all-conditions":
            fails.append(f"counter: mode={mode!r}, expected 'all-conditions'")
        if not complete or truncated:
            fails.append(f"counter: did not CLOSE (complete={complete}, truncated={truncated})")
        # count=5 is the terminal fixed point → must be marked absorbing.
        term = {nk for nk in real if states[nk]["count"] == 5}
        if not (term and term <= absorbing):
            fails.append(f"counter: terminal count=5 not marked absorbing (absorbing has "
                         f"{len(absorbing)} nodes)")
        # The tree height equals the climb length (5), proving it CLOSED at the true depth
        # rather than running to the old MAX_DEPTH=8 cap.
        if ck != 5:
            fails.append(f"counter: closing depth {ck}, expected 5 (the climb length) — "
                         f"a depth-8 cap would have over- or under-counted")
        _render_nonempty(prefix, "counter", fails)
    return fails


def check_free_init_forest():
    """No is_first_tick seed → every discrete start state is an initial condition, so the
    synthetic root must fan to MANY roots (a real forest), not one — the 'root from the SET
    of initial conditions' demand in its purest form."""
    fails = []
    with tempfile.TemporaryDirectory() as work:
        prefix, m = _load(TRAFFIC_FREE, work)
        inits = RF.initial_states(m)
        if len(inits) < 2:
            fails.append(f"free-init: _initial_states found {len(inits)} — a free-init FSM "
                         f"must enumerate MANY initial conditions, not one")
        G, states, absorbing, mode, ck, complete, truncated, real, roots = _build(m)
        if mode != "all-conditions":
            fails.append(f"free-init: mode={mode!r}, expected 'all-conditions'")
        if roots < 2:
            fails.append(f"free-init: synthetic root has {roots} children — a forest must "
                         f"root from the WHOLE init set, not a single seed")
        if roots != len(inits):
            fails.append(f"free-init: {roots} roots != {len(inits)} initial conditions")
        _render_nonempty(prefix, "free", fails)
    return fails


def check_real_fallback():
    """A real-valued system has an INFINITE reachable set — it must NOT claim to close, and
    must fall back to the single-seed depth-capped sample (the honesty gate)."""
    fails = []
    with tempfile.TemporaryDirectory() as work:
        prefix, m = _load(REAL, work)
        G, states, absorbing, mode, ck, complete, truncated, real, roots = _build(m)
        if mode != "fallback":
            fails.append(f"real: mode={mode!r}, expected 'fallback' — a real-valued reachable "
                         f"set is infinite and must NOT be rooted as a closing finite forest")
        if complete:
            fails.append("real: reported complete=True — an infinite reachable set was "
                         "dishonestly certified as fully enumerated")
        if RF.ROOT in (G.nodes() if G else []):
            fails.append("real: fallback tree carries the synthetic ∅ root (should be a "
                         "single real seed, no forest root)")
        _render_nonempty(prefix, "real", fails)
    return fails


def main():
    fails = []
    fails += check_traffic_closes()
    fails += check_counter_terminates()
    fails += check_free_init_forest()
    fails += check_real_fallback()
    if fails:
        print("REACHABILITY-TREE TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ reachability_tree: roots from ALL initial conditions (forest over the init SET); "
          "traffic/counter CLOSE at their true saturation depth (Model.closing_depth, not a "
          "hard depth-8 cap); free-init fans to many roots; real falls back to one seed (no "
          "false 'complete')")
    return 0


if __name__ == "__main__":
    sys.exit(main())
