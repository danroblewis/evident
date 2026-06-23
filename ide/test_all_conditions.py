#!/usr/bin/env python3
"""Test the ALL-INITIAL-CONDITIONS transition graph (diagram task #1).

`Model.reachable()` walks the successor relation FORWARD from the single seeded init
(`is_first_tick = true`). For a DETERMINISTIC FSM that is one trajectory — it shows only
the basin the seed falls into. `Model.full_state_graph()` (viz/model_global.py) instead
enumerates EVERY valid carried assignment (the product of the bounded discrete carried
vars, ignoring the seed) and applies the SAME successor relation — the GLOBAL dynamics.

This pins the headline win and the honesty fallback:

  - bistable (two deterministic basins) → all-conditions ⊋ from-init: it has STRICTLY MORE
    states AND surfaces BOTH attractors (0 and 6), where from-init reaches only the seed's
    basin (0). This is the foundational difference the diagram review demanded.
  - counter / traffic (ergodic-ish) → all-conditions covers the discrete product; no crash,
    discrete=True, and the from-init reachable set is a SUBSET of it.
  - real-valued → full_state_graph returns discrete=False (no enumeration attempted), so the
    caller falls back to from-init. The honesty gate.

Run from repo root: `python3 ide/test_all_conditions.py` (exit non-zero on any failure)."""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from evident_viz import load as load_model                  # noqa: E402

# A DETERMINISTIC bistable: x flows to the nearest wall (0 or 6). x=3 is the saddle.
# From init x=1 the orbit reaches only {1,0} — the left basin. The GLOBAL graph over
# every x ∈ 0..6 surfaces BOTH attractors (0 and 6) and all seven states.
BISTABLE = (
    "fsm bistable\n"
    "    x ∈ Int\n"
    "    is_first_tick ⇒ x = 1\n"
    "    ¬is_first_tick ⇒\n"
    "        0 ≤ x\n"
    "        x ≤ 6\n"
    "        x = (_x < 3 ? (_x = 0 ? 0 : _x - 1)\n"
    "             : (_x > 3 ? (_x = 6 ? 6 : _x + 1) : 3))\n")

# A BOUNDED cyclic counter (wraps 0→5→0): its single orbit already visits every state, and
# the range is finite, so the all-conditions enumeration covers exactly the same set. (An
# UNBOUNDED `count ∈ Int` climbing forever is correctly NOT enumerable — that's the real/
# unbounded fallback case, covered by REAL below.)
COUNTER = (
    "fsm counter\n"
    "    0 ≤ count ∈ Int ≤ 5\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ count = (_count ≥ 5 ? 0 : _count + 1)\n")

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

# Real-valued state: not a finite enumerable graph → discrete=False, caller falls back.
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


def _attractors(m, states, edges):
    """The absorbing states (self-loop or no out-edge) in a (states, edges) graph,
    as a set of x-values — the basins' fixed points."""
    out = {i: [] for i in range(len(states))}
    for a, b in edges:
        out[a].append(b)
    return {states[i]["x"] for i, succ in out.items()
            if not succ or succ == [i]}


def check_bistable():
    fails = []
    with tempfile.TemporaryDirectory() as work:
        m = _load(BISTABLE, work)
        fi_states, fi_edges = m.reachable()
        gl_states, gl_edges, info = m.full_state_graph()

        if not info["discrete"]:
            return ["bistable: full_state_graph discrete=False (bounded int should enumerate)"]
        if info["capped"]:
            return ["bistable: full_state_graph capped (7-state product should fit)"]

        fi_keys = {m.state_key(s) for s in fi_states}
        gl_keys = {m.state_key(s) for s in gl_states}
        # The headline: all-conditions ⊋ from-init — strictly more states, and a superset.
        if not (fi_keys < gl_keys):
            fails.append(f"bistable: all-conditions ({len(gl_keys)}) is not a strict superset "
                         f"of from-init ({len(fi_keys)}): {sorted(s['x'] for s in fi_states)} "
                         f"vs {sorted(s['x'] for s in gl_states)}")
        # from-init reaches only the LEFT basin (attractor 0); all-conditions surfaces BOTH.
        fi_attr = _attractors(m, fi_states, fi_edges)
        gl_attr = _attractors(m, gl_states, gl_edges)
        if 6 in fi_attr:
            fails.append(f"bistable: from-init unexpectedly reaches the right wall: {fi_attr}")
        if not ({0, 6} <= gl_attr):
            fails.append(f"bistable: all-conditions missing a basin attractor — "
                         f"expected {{0,6}} ⊆ {gl_attr}")
    return fails


def check_ergodic(name, src):
    fails = []
    with tempfile.TemporaryDirectory() as work:
        m = _load(src, work)
        fi_states, _ = m.reachable()
        gl_states, _, info = m.full_state_graph()
        if not info["discrete"]:
            return [f"{name}: full_state_graph discrete=False (bounded/enum should enumerate)"]
        fi_keys = {m.state_key(s) for s in fi_states}
        gl_keys = {m.state_key(s) for s in gl_states}
        # The orbit already covers the space, so from-init ⊆ all-conditions (every reachable
        # state is an enumerated initial condition too). No crash, honest discrete flag.
        if not (fi_keys <= gl_keys):
            fails.append(f"{name}: from-init not a subset of all-conditions "
                         f"({len(fi_keys - gl_keys)} states only reachable, not enumerated)")
    return fails


def check_basin_map_global():
    """The basin_map renderer's DISCRETE path must root on the GLOBAL graph, not the
    from-init orbit — a basin map's purpose is to PARTITION the state space by which
    attractor each STARTING state flows to. Drive _discrete_basins (the renderer entry
    the numeric finite-reachable route also calls), then SCC-condense the same root graph
    it used and assert it (a) saw all 7 bistable states and (b) surfaces BOTH wall
    attractors {0,6} — the from-init version saw only the left basin (2 states, attractor 0)."""
    import render_basin_map as RB                              # noqa: E402
    fails = []
    with tempfile.TemporaryDirectory() as work:
        m = _load(BISTABLE, work)
        out = work + "/basin.png"
        note = RB._discrete_basins(m, out)

        # The renderer must have taken the all-initial-conditions root (the honest caption),
        # NOT the from-init fallback — for a bounded-int bistable the global graph fits.
        if "all initial conditions" not in note:
            fails.append(f"basin_map: discrete path did not use the global graph "
                         f"(note={note!r}) — expected the all-initial-conditions root")

        # Re-derive the exact graph the renderer rooted on and check the basin partition:
        # every one of the 7 states present, BOTH wall attractors {0,6} terminal.
        states, edges, info = m.full_state_graph(limit=5000)
        n = len(states)
        _eset, sccs, scc_of, term_ids, _ti, _rt = RB._condense_terminals(n, edges)
        term_x = {states[sccs[s][0]]["x"] for s in term_ids}
        if {st["x"] for st in states} != set(range(7)):
            fails.append(f"basin_map: global graph missing states — "
                         f"got {sorted(st['x'] for st in states)}, expected 0..6")
        if not ({0, 6} <= term_x):
            fails.append(f"basin_map: global basin partition missing a wall attractor — "
                         f"expected {{0,6}} ⊆ terminal x-values {sorted(term_x)}")
        # Two DISTINCT basins at minimum: x=0 and x=6 land in different terminal SCCs.
        if scc_of[next(i for i, st in enumerate(states) if st["x"] == 0)] == \
           scc_of[next(i for i, st in enumerate(states) if st["x"] == 6)]:
            fails.append("basin_map: x=0 and x=6 collapsed into one terminal SCC — "
                         "the two basins are not partitioned")
    return fails


def check_fixedpoint_map_global():
    """The fixedpoint_map renderer must SEED from all initial conditions and color each
    state by the attractor it converges to — across the WHOLE discrete state space, not
    the from-init orbit (which for bistable is one trajectory into the LEFT basin).

    Drive sample_all_conditions (the renderer's seeding) + basin_colors (its attractor
    coloring), and assert it (a) took the all-conditions root, (b) saw all 7 bistable
    states, (c) found BOTH wall fixed points {0,6}, and (d) partitioned the space into
    two DISTINCT basin colors split at the saddle. Then render the PNG end-to-end."""
    import os
    sys.path.insert(0, "viz")
    import render_fixedpoint_map as RF                          # noqa: E402
    from fixedpoint_basins import sample_all_conditions, basin_colors  # noqa: E402
    from fixedpoint_attractors import find_attractors           # noqa: E402
    fails = []
    with tempfile.TemporaryDirectory() as work:
        ok, prefix, _dropped, msg = _export(BISTABLE, work)
        if not ok:
            return [f"fixedpoint_map: export failed: {msg.splitlines()[0][:80]}"]
        m = load_model(prefix + ".smt2", prefix + ".schema.json")

        states, mode, edges = sample_all_conditions(m)
        if mode != "all-conditions":
            fails.append(f"fixedpoint_map: seeding mode={mode!r}, expected 'all-conditions' "
                         f"(a 7-state bounded-int bistable enumerates)")
        if {st["x"] for st in states} != set(range(7)):
            fails.append(f"fixedpoint_map: seeded states missing — got "
                         f"{sorted(st['x'] for st in states)}, expected 0..6")

        # BOTH fixed points (0 and 6) surface from the all-conditions sample; the from-init
        # orbit would have found only the left wall (0).
        fixed, _cycles = find_attractors(m, states, mode)
        fixed_x = {s["x"] for s in fixed}
        if not ({0, 6} <= fixed_x):
            fails.append(f"fixedpoint_map: all-conditions missing a fixed point — "
                         f"expected {{0,6}} ⊆ {sorted(fixed_x)}")

        # Basin coloring partitions the space: x=0 and x=6 get DIFFERENT colors, and at least
        # two distinct real-basin colors exist (the two walls' basins).
        colors, n_term = basin_colors(states, edges)
        cof = {states[i]["x"]: colors[i] for i in range(len(states))}
        if n_term < 2:
            fails.append(f"fixedpoint_map: basin_colors found {n_term} terminals, expected ≥2")
        if cof.get(0) is None or cof.get(6) is None or cof[0] == cof[6]:
            fails.append(f"fixedpoint_map: x=0 and x=6 not in distinct basins "
                         f"(colors {cof.get(0)!r} vs {cof.get(6)!r})")

        out = work + "/fixedpoint_map.png"
        RF.render(prefix + ".smt2", prefix + ".schema.json", out)
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append("fixedpoint_map: renderer produced no PNG")
    return fails


def check_real_fallback():
    with tempfile.TemporaryDirectory() as work:
        m = _load(REAL, work)
        states, edges, info = m.full_state_graph()
        fails = []
        if info["discrete"]:
            fails.append("real: full_state_graph discrete=True — a Real var must NOT be enumerated")
        if states or edges:
            fails.append(f"real: expected empty graph on fallback, got {len(states)} states")
        return fails


def check_time_series_ensemble():
    """time_series must show an ENSEMBLE over all initial conditions, not one from-init chain.

    For bistable, a single from-init run (seeded x=1) reaches only the LEFT basin (0) — it would
    plot one line decaying to 0, hiding the right attractor at 6. The ensemble forward-simulates
    EVERY enumerated init via the SAME successor relation; the per-tick reachable ENVELOPE's final
    spread must include BOTH 0 and 6. This pins the headline diagram-review fix.

    Also renders the PNG (asserting a non-empty file) so the renderer's ensemble path is exercised
    end-to-end, and confirms a from-init single run would NOT reach 6 (the contrast the fan shows)."""
    import numpy as np
    import os
    sys.path.insert(0, "viz")
    from time_series_ensemble import ensemble_inits, step_trajectory  # noqa: E402
    import render_time_series as RT                                    # noqa: E402
    fails = []
    with tempfile.TemporaryDirectory() as work:
        ok, prefix, _dropped, msg = _export(BISTABLE, work)
        if not ok:
            return [f"time_series: export failed: {msg.splitlines()[0][:80]}"]
        m = load_model(prefix + ".smt2", prefix + ".schema.json")

        # The single from-init run (the OLD behavior) reaches only the left basin {0,1}.
        from_init = {s["x"] for s in RT.walk(m, m.initial_state(), 60)}
        if 6 in from_init:
            fails.append(f"time_series: a from-init run unexpectedly reaches 6 — the contrast "
                         f"the ensemble shows would be moot ({sorted(from_init)})")

        inits, kind, note = ensemble_inits(m)
        if kind != "discrete":
            fails.append(f"time_series: bistable ensemble kind={kind!r}, expected 'discrete' "
                         f"(a 7-state bounded int enumerates)")
        if "initial conditions" not in note:
            fails.append(f"time_series: ensemble note {note!r} should mention 'initial conditions'")

        # Forward-simulate every init; the per-tick envelope's FINAL spread must span both walls.
        trajs = [step_trajectory(m, init, 60, m.is_discrete()) for init in inits]
        nt = max(len(t) for t in trajs)
        mat = np.full((len(trajs), nt), np.nan)
        for r, t in enumerate(trajs):
            for c, s in enumerate(t):
                mat[r, c] = s["x"]
        final = mat[:, -1]
        reached = set(final[~np.isnan(final)])
        if not ({0.0, 6.0} <= reached):
            fails.append(f"time_series: ensemble final-tick envelope missing a basin — "
                         f"expected {{0,6}} ⊆ {sorted(reached)} (the fan must reach BOTH attractors, "
                         f"where a single from-init run reaches only 0)")
        # The envelope band must genuinely SPREAD (a single line would have lo==hi every tick).
        with np.errstate(all="ignore"):
            band = float(np.nanmax(final) - np.nanmin(final))
        if band < 6.0:
            fails.append(f"time_series: final-tick envelope spread {band} < 6 — not a fan")

        out = work + "/time_series.png"
        rnote = RT.render(prefix + ".smt2", prefix + ".schema.json", out)
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append(f"time_series: renderer produced no PNG (note={rnote!r})")
        elif "ensemble" not in rnote:
            fails.append(f"time_series: render note {rnote!r} is not the ensemble path")
    return fails


def check_phase_portrait_field():
    """A phase portrait's value is the VECTOR FIELD over the whole plane — the flow at EVERY
    point — not one seeded trajectory. Pins both halves of the diagram-review fix:

      - the damped-spring oscillator (a 2-var Real-ish continuous system) must render a genuine
        QUIVER vector field (ax.quiver called over a grid), with trajectories OVERLAID — proved
        by counting quiver calls (the field) AND plotted lines (the overlay).
      - predator-prey (lotka) must NOT CRASH: a diverging successor produces a 1000+-digit z3
        numeral whose as_long() raises — the renderer must guard it (skip/clamp) so the field
        never crashes, where the unguarded path raised ValueError mid-render.

    Both render the daemon .ev files end-to-end through render_phase_portrait, asserting a
    non-empty PNG. The field is checked by patching matplotlib's quiver/plot to record calls."""
    import os
    sys.path.insert(0, "viz")
    import matplotlib
    matplotlib.use("Agg")
    from matplotlib.axes import Axes
    import render_phase_portrait as RP                                  # noqa: E402

    fails = []
    daemons = "examples/daemons"
    spring_src = open(os.path.join(daemons, "spring.ev")).read()
    lotka_src = open(os.path.join(daemons, "lotka.ev")).read()

    # --- spring: a real QUIVER field + overlaid trajectories -----------------
    with tempfile.TemporaryDirectory() as work:
        ok, prefix, _dropped, msg = _export(spring_src, work)
        if not ok:
            return [f"phase_portrait: spring export failed: {msg.splitlines()[0][:80]}"]
        out = work + "/spring.phase_portrait.png"
        counts = {"quiver": 0, "plot": 0}
        orig_q, orig_p = Axes.quiver, Axes.plot

        def _q(self, *a, **k):
            counts["quiver"] += 1
            return orig_q(self, *a, **k)

        def _p(self, *a, **k):
            counts["plot"] += 1
            return orig_p(self, *a, **k)

        Axes.quiver, Axes.plot = _q, _p
        try:
            RP.render(prefix + ".smt2", prefix + ".schema.json", out)
        except Exception as e:
            fails.append(f"phase_portrait: spring render crashed: {type(e).__name__}: {e}")
        finally:
            Axes.quiver, Axes.plot = orig_q, orig_p
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append("phase_portrait: spring produced no PNG")
        if counts["quiver"] < 1:
            fails.append("phase_portrait: spring drew NO quiver vector field "
                         "(a phase portrait must show the flow at every point, not one curve)")
        if counts["plot"] < 1:
            fails.append("phase_portrait: spring drew no overlaid trajectory line")

    # --- lotka (predator-prey): must NOT crash on divergence ------------------
    with tempfile.TemporaryDirectory() as work:
        ok, prefix, _dropped, msg = _export(lotka_src, work)
        if not ok:
            return fails + [f"phase_portrait: lotka export failed: {msg.splitlines()[0][:80]}"]
        out = work + "/lotka.phase_portrait.png"
        try:
            RP.render(prefix + ".smt2", prefix + ".schema.json", out)
        except (OverflowError, ValueError) as e:
            fails.append(f"phase_portrait: lotka (predator-prey) CRASHED on divergence — "
                         f"the field must guard a runaway successor: {type(e).__name__}: {e}")
        except Exception as e:
            fails.append(f"phase_portrait: lotka render crashed: {type(e).__name__}: {e}")
        if not (os.path.exists(out) and os.path.getsize(out) > 0):
            fails.append("phase_portrait: lotka produced no PNG")
    return fails


def main():
    fails = []
    fails += check_bistable()
    fails += check_ergodic("counter", COUNTER)
    fails += check_ergodic("traffic", TRAFFIC)
    fails += check_basin_map_global()
    fails += check_fixedpoint_map_global()
    fails += check_real_fallback()
    fails += check_time_series_ensemble()
    fails += check_phase_portrait_field()
    if fails:
        print("ALL-INITIAL-CONDITIONS TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ all-initial-conditions: bistable ⊋ from-init (both basins); "
          "counter/traffic enumerate; basin_map + fixedpoint_map partition on the "
          "global graph; time_series ensemble fans to BOTH attractors; phase_portrait "
          "draws the vector field + overlay and lotka doesn't crash; real falls back")
    return 0


if __name__ == "__main__":
    sys.exit(main())
