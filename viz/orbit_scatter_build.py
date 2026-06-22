#!/usr/bin/env python3
"""orbit_scatter_build.py — channel selection + orbit construction for
render_orbit_scatter.py.

The DATA layer: value->coordinate projection, the axis/color/facet channel
choice, building the orbits to plot (the honest autonomous chain, the reachable
set, or self-scaled seeds for a degenerate fixed point), and collision-only
jitter for discrete grids. No plotting policy lives here.
"""
import math

def _project(model, var, value):
    """Map a state value to a float coordinate. int/real pass through;
    bool -> 0/1; enum -> its ordinal index in the declared variant order."""
    k = var["kind"]
    if k in ("int", "real"):
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        return float(model.enum_variants[var["name"]].index(value))
    return 0.0


def _axis_label(var):
    return f'{var["name"]}  [{var["kind"]}]'


def _cat_key(model, var, value):
    """A stable label for a categorical value (color/facet grouping)."""
    k = var["kind"]
    if k == "bool":
        return f'{var["name"].split(".")[-1]}={"true" if value else "false"}'
    if k == "enum":
        return str(value)
    return str(value)


# ---- channel selection -------------------------------------------------------
def _select_channels(model):
    """Decide axes / color-var / facet-var from the ranked, typed interface vars.

    Returns (xvar, yvar, color_var, facet_var). color_var/facet_var may be None,
    in which case the renderer falls back to the time/depth gradient (color) or a
    single panel (facet)."""
    numeric = model.numeric_vars
    cats = model.categorical_vars

    # AXES: a numeric pair is the honest continuous phase plane (vanderpol).
    if len(numeric) >= 2:
        xvar, yvar = numeric[0], numeric[1]
    else:
        ch = model.assign_channels(["x", "y"])
        xvar, yvar = ch["x"], ch["y"]
        # assign_channels gives best two by type-effectiveness; if it left one
        # empty (single state var), reuse / bail later.
        if xvar is None:
            return None, None, None, None
        if yvar is None:
            yvar = xvar

    used = {v["name"] for v in (xvar, yvar) if v is not None}

    # FACET: only a var that stays ~constant within a run (a config/regime set
    # once) — faceting by a var that changes ON the trajectory cuts the dynamics
    # across panels. The shared guard returns such a var, or None -> don't facet.
    facet_var = model.facet_var()
    if facet_var is not None and facet_var["name"] in used:
        facet_var = None      # already an axis; don't double-use it
    if facet_var is not None:
        used.add(facet_var["name"])

    # COLOR: a remaining categorical reads best in color; else None -> gradient.
    color_var = None
    color_candidates = [v for v in cats if v["name"] not in used]
    if color_candidates:
        color_var = color_candidates[0]

    return xvar, yvar, color_var, facet_var


# ---- orbits ------------------------------------------------------------------
def _expanding_orbit_radius(model, xvar, yvar, center):
    """Probe OUTWARD from the fixed point `center` along +x to find the smallest
    radius at which the dynamics expand into a non-trivial orbit (length > 2),
    rather than sitting at a fixed point. Returns that radius, or None if no probe
    out to a generous cap expands — i.e. the system really is a fixed point and the
    seed-and-reveal trick has nothing to reveal. This is how we discover the
    attractor's SCALE from the program itself instead of hardcoding a ±3000 box:
    the limit cycle's own extent then sets the plot, self-scaling per program."""
    cx = center.get(xvar["name"], 0)
    cy = center.get(yvar["name"], 0)
    r = 1
    while r <= 1 << 16:          # generous cap; geometric so ~17 probes max
        seed = {v["name"]: center.get(v["name"], 0) for v in model.state_vars}
        seed[xvar["name"]] = int(round(cx + r))
        seed[yvar["name"]] = int(round(cy))
        if len(model.trajectory(start=seed, steps=64)) > 2:
            return r
        r *= 2
    return None


def _numeric_seeds(model, xvar, yvar):
    """Seed points for a 2D numeric system whose REACHABLE set from the initial
    state is degenerate (a single fixed point the integer/continuous dynamics sit
    at). We DON'T guess a box: we probe outward from the fixed point to find the
    radius at which the dynamics come alive, then place a small spread of starts at
    that self-discovered scale so the attractor / limit cycle reveals itself. Seeds
    whose orbit does NOT expand are dropped by the caller, so a wide plane is never
    carpeted with dead fixed dots. Returns [] when nothing expands (true fixed
    point) — the caller then renders an honest N/A."""
    center = model.initial_state() or {v["name"]: 0 for v in model.state_vars}
    cx = center.get(xvar["name"], 0)
    cy = center.get(yvar["name"], 0)
    r = _expanding_orbit_radius(model, xvar, yvar, center)
    if r is None:
        return []
    # A spread of starts at the discovered scale: along each axis + an off-axis
    # diagonal, so a closed orbit is sampled from several entry angles.
    offsets = [(r, 0), (0, r), (-r, r), (r // 2, 0)]
    seeds = []
    for dx, dy in offsets:
        full = {v["name"]: center.get(v["name"], 0) for v in model.state_vars}
        full[xvar["name"]] = int(round(cx + dx))
        full[yvar["name"]] = int(round(cy + dy))
        seeds.append(full)
    return seeds


def _reachable_with_depth(model, limit=400):
    """BFS the reachable set, returning (states, depths) parallel lists where
    depths[i] is the minimum number of steps from the initial state. Used for
    nondeterministic discrete systems where a single chain dead-ends."""
    init = model.initial_state()
    if init is None:
        return [], []
    states = [init]
    index = {model._key(init): 0}
    depth = [0]
    frontier = [0]
    while frontier and len(states) < limit:
        i = frontier.pop(0)
        for nxt in model.successors(states[i]):
            k = model._key(nxt)
            if k not in index:
                index[k] = len(states)
                states.append(nxt)
                depth.append(depth[i] + 1)
                frontier.append(index[k])
    return states, depth


def _build_orbits(model, xvar, yvar):
    """Return (orbits, point_time, mode) where orbits is a list of state-dict
    lists, point_time[oi] is a parallel list of time/depth values per point, and
    mode is one of 'numeric' | 'autonomous' | 'reachable'.

    We ALWAYS prefer the program's actual reachable states / orbit and plot THOSE
    directly — never a hardcoded wide box. Multi-seed 'numeric' mode is used ONLY
    when the reachable set from the initial state is degenerate (a single fixed
    point the integer/continuous dynamics sit at), so that an attractor / limit
    cycle has a chance to reveal itself; even then the seeds are derived from the
    reachable extent (axis_bounds), and dead (non-expanding) seeds are dropped."""
    # 1. The honest autonomous orbit: one successor chain from the initial state.
    init = model.initial_state()
    orb = model.trajectory(start=init, steps=400) if init is not None else []
    if len(orb) > 2:
        return [orb], [list(range(len(orb)))], "autonomous"

    # 2. The full reachable set (handles branching dynamics a single chain dead-ends
    #    on — e.g. a terminating counter like wc, whose 0..10 states fan out).
    states, depths = _reachable_with_depth(model)
    if len(states) > 2:
        return [states], [depths], "reachable"

    # 3. Reachable set is degenerate (<=2 distinct states): the initial state is a
    #    fixed point and nothing expands from it. ONLY for a genuinely-continuous
    #    numeric 2D system do we seed across the reachable extent to expose an
    #    attractor; each kept seed must produce an orbit that actually expands, so a
    #    dead seed never becomes a lone fabricated dot on a wide plane.
    numeric_2d = (xvar["kind"] in ("int", "real")
                  and yvar["kind"] in ("int", "real")
                  and xvar["name"] != yvar["name"])
    if numeric_2d:
        orbits, times = [], []
        for seed in _numeric_seeds(model, xvar, yvar):
            o = model.trajectory(start=seed, steps=400)
            if len(o) > 2:                  # drop dead seeds (single fixed dot)
                orbits.append(o)
                times.append(list(range(len(o))))
        if orbits:
            return orbits, times, "numeric"

    # 4. Whatever small reachable set we have (1-2 states) — render it honestly;
    #    the caller routes a too-small set to an N/A card.
    if states:
        return [states], [depths], "reachable"
    return [], [], "autonomous"


# ---- collision-only offset for discrete axes ---------------------------------
def _offset_collisions(pts, xvar, yvar):
    """Keep every point on its true integer/categorical grid node; only when 2+
    points share a node, spread that group on a tiny ring AROUND the node so the
    coincidence is visible. Distinct points are untouched (no fabricated spread),
    and the ring radius is small enough to stay between grid lines."""
    groups = {}
    for p in pts:
        groups.setdefault((p["gx"], p["gy"]), []).append(p)
    # An enum/bool axis is categorical (its floor is variant 0); an int axis must
    # never show a dot below its true minimum value. Clamp ring offsets to that.
    xmin = min(p["gx"] for p in pts)
    ymin = min(p["gy"] for p in pts)
    for (gx, gy), group in groups.items():
        if len(group) < 2:
            continue                      # distinct: leave exactly on the grid node
        # One representative stays dead-on the grid node; the rest fan out on a tiny
        # ring around it (small enough to stay between grid lines, clamped above each
        # discrete axis's true minimum so nothing dips below the floor).
        rad = 0.16
        for i, p in enumerate(group[1:], start=1):
            ang = 2 * math.pi * (i - 1) / (len(group) - 1)
            p["x"] = max(xmin, gx + rad * math.cos(ang))
            p["y"] = max(ymin, gy + rad * math.sin(ang))
