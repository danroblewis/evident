#!/usr/bin/env python3
"""Test that render_transition_matrix roots on ALL INITIAL CONDITIONS (diagram review).

A transition_matrix is a state×state incidence: cell (i,j) lit ⇔ state_i → state_j.
For that to be honest the rows/cols must be EVERY start state, not one z3 model's
from-init orbit, and the cells must come from the REAL transition relation — not a
fabricated linspace grid. The diagram review scored transition_matrix PARTIAL for
exactly those two leaks; this pins both fixes:

  - traffic / counter (finitely discrete) → the renderer's `_build_states_matrix`
    must take the GLOBAL root (mode == 'all initial conditions'), its state set must
    equal `full_state_graph`'s (every valid carried assignment, a SUPERSET of the
    from-init orbit), and the matrix's lit cells must be exactly the real transition
    edges over that global set — NO sampled-grid binning.
  - traffic's 9-state cyclic structure (3 lights × 3 timer values) must show as a
    permutation-with-self-loops: every row has ≥1 transition, total == #edges.
  - real-valued state (decay) → full_state_graph is discrete=False, so the renderer
    must NOT use the global path; it falls through to the exact-from-init / sampled
    route. The honesty fallback is preserved.

Run from repo root: `python3 ide/test_transition_matrix_global.py` (non-zero on fail)."""
import os
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

import numpy as np                                            # noqa: E402
from runtime_io import _export                                # noqa: E402
from evident_viz import load as load_model                    # noqa: E402
import render_transition_matrix as RTM                        # noqa: E402

# A bounded cyclic counter (0→…→5→0): one finite orbit covering all 6 states.
COUNTER = (
    "fsm counter\n"
    "    0 ≤ count ∈ Int ≤ 5\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 0 : _count + 1)\n")

# Traffic light: 3 lights × 3 timer values = 9 states, a single cycle.
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

# Real-valued state: not finitely enumerable → discrete=False → NOT the global path.
REAL = (
    "fsm decay\n"
    "    x ∈ Real\n"
    "    is_first_tick ⇒ x = 100\n"
    "    ¬is_first_tick ⇒ x = _x / 2\n")


def _load(src, work):
    ok, prefix, _dropped, msg = _export(src, work)
    if not ok:
        raise RuntimeError(f"export failed: {msg.splitlines()[0][:80]}")
    return prefix, load_model(prefix + ".smt2", prefix + ".schema.json")


def _real_edges(m, states):
    """The real transition edge set (as state-key pairs) over `states`, recomputed
    independently of the renderer via the model's own successor relation."""
    key = {m.state_key(s): s for s in states}
    edges = set()
    for s in states:
        for nxt in m.successors(s):
            k = m.state_key(nxt)
            if k in key:
                edges.add((m.state_key(s), k))
    return edges


def check_global(name, src, expected_states):
    fails = []
    with tempfile.TemporaryDirectory() as work:
        prefix, m = _load(src, work)

        # 1. The renderer must take the GLOBAL all-initial-conditions root.
        built = RTM._build_states_matrix(m, work + "/_unused.png")
        if built is None:
            return [f"{name}: _build_states_matrix returned None (emitted a placeholder)"]
        states, mat, _rv, _rvals, mode, _note, total = built
        if mode != "all initial conditions":
            fails.append(f"{name}: mode={mode!r}, expected 'all initial conditions' "
                         f"(a finitely-discrete program enumerates over the global graph)")

        # 2. Its state set must be the full global graph — a SUPERSET of the from-init
        #    orbit, equal to full_state_graph's state set (no fabricated grid).
        gl_states, gl_edges, info = m.full_state_graph(limit=5000)
        if not (info["discrete"] and not info["capped"]):
            return [f"{name}: full_state_graph not discrete/uncapped — fixture is not enumerable"]
        gl_keys = {m.state_key(s) for s in gl_states}
        built_keys = {m.state_key(s) for s in states}
        if built_keys != gl_keys:
            fails.append(f"{name}: matrix state set ({len(built_keys)}) != global graph "
                         f"({len(gl_keys)}) — not rooted on all initial conditions")
        if len(built_keys) != expected_states:
            fails.append(f"{name}: expected {expected_states} states over the global space, "
                         f"got {len(built_keys)}")
        fi_states, _ = m.reachable()
        fi_keys = {m.state_key(s) for s in fi_states}
        if not (fi_keys <= built_keys):
            fails.append(f"{name}: from-init orbit not a subset of the matrix state set")

        # 3. The matrix cells must be the REAL transition edges (recomputed
        #    independently), NOT a sampled-grid binning. Map matrix indices → keys,
        #    then compare the lit-cell key-pairs against the model's own successors.
        idx_key = [m.state_key(s) for s in states]
        lit = {(idx_key[i], idx_key[j])
               for i in range(len(states)) for j in range(len(states))
               if mat[i, j] > 0}
        # Restrict the real edges to within-set (full_state_graph self-contains, but be safe).
        real = {(a, b) for (a, b) in _real_edges(m, states)}
        if lit != real:
            fails.append(f"{name}: matrix lit cells ({len(lit)}) != real transition edges "
                         f"({len(real)}) — cells are not the true successor relation")
        # Every state must have at least one outgoing transition (a total FSM step).
        rows_with_edge = {i for i in range(len(states))
                          if any(mat[i, j] > 0 for j in range(len(states)))}
        if len(rows_with_edge) != len(states):
            fails.append(f"{name}: {len(states) - len(rows_with_edge)} states have NO outgoing "
                         f"transition — the matrix is missing real edges")

        # 4. End-to-end render: a non-empty PNG with the honest title.
        out = work + f"/{name}.transition_matrix.png"
        RTM.render(m, out)
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append(f"{name}: renderer produced no PNG")
    return fails


def check_real_fallback():
    """A Real-valued program must NOT take the global path (full_state_graph is
    discrete=False); the renderer falls through to the from-init / sampled route."""
    fails = []
    with tempfile.TemporaryDirectory() as work:
        prefix, m = _load(REAL, work)
        _s, _e, info = m.full_state_graph(limit=5000)
        if info["discrete"]:
            fails.append("real: full_state_graph discrete=True — a Real var must not enumerate")
        built = RTM._build_states_matrix(m, work + "/_unused.png")
        if built is not None:
            mode = built[4]
            if mode == "all initial conditions":
                fails.append("real: renderer took the global path on a Real-valued program "
                             "(must fall back to from-init / sampled)")
        out = work + "/decay.transition_matrix.png"
        RTM.render(m, out)
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append("real: renderer produced no PNG")
    return fails


def main():
    fails = []
    fails += check_global("counter", COUNTER, expected_states=6)
    fails += check_global("traffic", TRAFFIC, expected_states=9)
    fails += check_real_fallback()
    if fails:
        print("TRANSITION-MATRIX GLOBAL-ROOT TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ transition_matrix: counter (6) + traffic (9) root on the global graph "
          "(mode='all initial conditions'), state set == full_state_graph (⊇ from-init), "
          "lit cells == the real successor relation (no sampled grid); real falls back")
    return 0


if __name__ == "__main__":
    sys.exit(main())
