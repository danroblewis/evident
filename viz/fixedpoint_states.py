#!/usr/bin/env python3
"""fixedpoint_states.py — channel assignment + state sampling for the
fixed-point map renderer.

The DATA layer for render_fixedpoint_map.py: which two vars become the phase-plane
axes (assign_channels), the per-var domain/label projection helpers, and the
representative state set to probe (the reachable graph when it's non-trivial, else
a phase-space grid scan). No plotting policy lives here.
"""


def assign_channels(model):
    """Map the ranked+deduped vars onto this viz's channels by type-effectiveness.

    AXES come first: if the system carries numeric vars, those ARE the geometry
    (a limit cycle only lives in a continuous phase plane), so numeric_vars drive
    x/y directly. Otherwise we let assign_channels rank a discrete projection
    (enum/bool ordinals). The leftover categoricals decorate the BACKGROUND
    basin — the derived attractor coloring (red fixed / blue cycle) is preserved,
    so color/shape/facet only enhance the sampled-state dots, never clobber it.

    Returns a dict: {x, y, color, shape, facet} -> var|None.
    """
    ch = {"x": None, "y": None, "color": None, "shape": None, "facet": None}
    numeric = model.numeric_vars
    if len(numeric) >= 2:
        # phase-plane viz: NEEDS numeric axes (continuous orbits).
        ch["x"], ch["y"] = numeric[0], numeric[1]
        used = {numeric[0]["name"], numeric[1]["name"]}
        cats = [v for v in model.categorical_vars if v["name"] not in used]
    elif len(numeric) == 1:
        # mixed: one numeric axis + the top categorical as the other ordinal axis.
        ch["x"] = numeric[0]
        cats = list(model.categorical_vars)
        if cats:
            ch["y"] = cats.pop(0)
    else:
        # purely discrete: rank a 2-D ordinal projection over the categoricals.
        cats = list(model.categorical_vars)
        if cats:
            ch["x"] = cats.pop(0)
        if cats:
            ch["y"] = cats.pop(0)

    # Facet ONLY by a variable that stays ~constant within a run (a config/regime
    # set once). Faceting by a var that CHANGES as the system runs would split the
    # trajectory across panels and hide the dynamics. facet_var() returns None when
    # no such variable exists -> don't facet (single panel). Claim it BEFORE the
    # secondary channels so the suitable facet var isn't stolen by color/shape.
    fvar = model.facet_var()
    if fvar is not None and fvar["name"] in {v["name"] for v in cats}:
        ch["facet"] = fvar
        cats = [v for v in cats if v["name"] != fvar["name"]]

    # Remaining categoricals -> secondary channels, by type-effectiveness order:
    # color (hue, excellent for categorical) > shape.
    for v in list(cats):
        if ch["color"] is None:
            ch["color"] = v
            cats.remove(v)
        elif ch["shape"] is None:
            ch["shape"] = v
            cats.remove(v)
    return ch


def _domain(model, var):
    if var["kind"] == "bool":
        return [False, True]
    if var["kind"] == "enum":
        return list(model.enum_variants[var["name"]])
    return [None]


def axis_label(var):
    return f"{var['name']}  [{var['kind']}]"


def _short(name):
    return name.split(".")[-1]


def _fmt(var, val):
    if var["kind"] == "bool":
        return "true" if val else "false"
    return str(val)


# --------------------------------------------------------------------------
# sampling: produce a list of state dicts to probe.
# --------------------------------------------------------------------------
def numeric_range(model, var, samples_estimate):
    """Heuristic grid range for a numeric var. Van der Pol fixed-point IR uses
    fixed-point ints scaled to ~±3000 with a limit cycle near r~2000, so a box
    of [-3200, 3200] with a coarse-ish grid is the right default."""
    init = model.initial_state()
    base = 3200.0
    if init is not None:
        v = abs(float(init[var["name"]]))
        if v > base:
            base = v * 1.2
    lo, hi = -base, base
    n = max(2, int(samples_estimate))
    step = (hi - lo) / (n - 1)
    return [lo + i * step for i in range(n)]


def grid_states(model, max_points=900):
    """Grid-scan numeric axes, enumerating discrete axes. Returns list of state
    dicts spanning a representative box of the state space."""
    numeric = [v for v in model.state_vars if v["kind"] in ("int", "real")]
    discrete = [v for v in model.state_vars if v["kind"] not in ("int", "real")]

    # Discrete combinations (bounded; bail to a single slice if it explodes).
    def disc_domain(v):
        if v["kind"] == "bool":
            return [False, True]
        if v["kind"] == "enum":
            return list(model.enum_variants[v["name"]])
        return [None]

    disc_combos = [{}]
    for v in discrete:
        dom = disc_domain(v)
        new = []
        for combo in disc_combos:
            for val in dom:
                c = dict(combo)
                c[v["name"]] = val
                new.append(c)
        disc_combos = new
        if len(disc_combos) > 64:
            disc_combos = disc_combos[:64]
            break

    if not numeric:
        # purely discrete -> the discrete combos ARE the sample (but we prefer
        # the exact reachable set; handled by caller). Return combos as states.
        return [dict(c) for c in disc_combos]

    # Budget grid resolution so total points stay under max_points.
    per_axis = max(2, int((max_points / max(1, len(disc_combos))) ** (1.0 / len(numeric))))
    per_axis = min(per_axis, 40)
    axis_vals = {v["name"]: numeric_range(model, v, per_axis) for v in numeric}

    states = []
    for combo in disc_combos:
        # cartesian product over numeric axes
        idxs = [0] * len(numeric)
        total = 1
        for v in numeric:
            total *= len(axis_vals[v["name"]])
        for flat in range(total):
            st = dict(combo)
            rem = flat
            for v in numeric:
                vals = axis_vals[v["name"]]
                st[v["name"]] = int(vals[rem % len(vals)]) if v["kind"] == "int" else vals[rem % len(vals)]
                rem //= len(vals)
            states.append(st)
            if len(states) >= max_points:
                return states
    return states


def sample_states(model):
    """Return (states, mode, edges).

    The reachable set from the initial state IS the real dynamics, so prefer it
    whenever it's non-trivial (vending's limit cycle, dungeon's graph). Only
    when it collapses to a point/pair AND the system carries numeric axes
    (vanderpol: reachable = the origin fixed point alone) do we fall back to a
    phase-space GRID scan to expose the surrounding orbits.

    `edges` are the (state, next_state) pairs of the reachable graph — faint
    connecting structure that turns the basin from scattered dots into a legible
    transition graph the fixed points sit in. Empty for grid mode."""
    # Cap the cloud at 800: the fixed-point/cycle map runs successor() + short-cycle detection on
    # EVERY sampled state, so reachable(5000) on a real-valued FSM (oscillator) meant ~70s and a held
    # server lock. A fixed-point map reads the same at 800 — the attractors and their basins are
    # unchanged; only redundant orbit dots are dropped. (Same fix as scatter_matrix #217.)
    reach, idx_edges = model.reachable(limit=800)
    has_numeric = any(v["kind"] in ("int", "real") for v in model.state_vars)
    if len(reach) > 2 or (reach and not has_numeric):
        edges = [(reach[i], reach[j]) for i, j in idx_edges]
        return reach, "reachable", edges
    if has_numeric:
        grid = grid_states(model)
        # keep the reachable point(s) too (the true fixed point), unioned in.
        keys = {model._key(s) for s in grid}
        for s in reach:
            if model._key(s) not in keys:
                grid.append(s)
        return grid, "grid", []
    return reach, "reachable", []
