#!/usr/bin/env python3
"""orbit_scatter_build.py — channel selection + orbit construction for
render_orbit_scatter.py.

The DATA layer: value->coordinate projection, the axis/color/facet channel
choice, building the orbits to plot, and collision-only jitter for discrete grids.
No plotting policy lives here.

The PRIMARY orbit-scatter mode samples MANY initial conditions (not one seeded
orbit): it seeds from `full_state_graph()` (discrete) / a `proven_range` grid
(continuous) via the SHARED ensemble seeder (`time_series_ensemble.ensemble_inits`,
the exact seed-set the all-conditions time-series uses), forward-simulates each with
the model's EXISTING successor relation (`time_series_ensemble.step_trajectory`,
clamping divergence), and DROPS the first few transient ticks so only the attractor /
limit-cycle structure is scattered. Each kept orbit is tagged with the attractor it
settles into, so a multi-attractor system (bistable) shows BOTH basins by color.

For a SINGLE numeric var an x-vs-x scatter is a useless 45° diagonal, so we DELAY-EMBED:
plot (x_t, x_{t+1}). The scatter then traces the map's graph (the logistic parabola, a
fixed point as a single dot on the diagonal, a 2-cycle as two off-diagonal dots).

If no honest ensemble box exists (some carried var is unbounded) we fall back to the old
single-orbit construction (autonomous chain / reachable set), so an unbounded model still
renders something faithful.
"""
import math

# Transient ticks dropped from the FRONT of each forward-simulated orbit, so the scatter
# shows the attractor the orbit settles ONTO, not the path it took to get there. Kept small
# (a short orbit that halts fast — a 2-state bistable basin — must still contribute its
# attractor point), and never more than the orbit minus one (always keep the final state).
_TRANSIENT = 4

# Forward-sim horizon per init. Long enough for a transient to die and a limit cycle to
# show, short enough to stay cheap across up to _MAX_INITS seeds.
_STEPS = 80


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

    Returns (xvar, yvar, color_var, facet_var). When there is exactly ONE expressive
    var, yvar is returned EQUAL to xvar — the renderer reads that (via `is_delay_embed`)
    as the single-numeric-var case and delay-embeds (x_t vs x_{t+1}) rather than plotting
    a degenerate x-vs-x diagonal. color_var/facet_var may be None."""
    numeric = model.numeric_vars
    cats = model.categorical_vars

    # AXES: a numeric pair is the honest continuous phase plane (vanderpol).
    if len(numeric) >= 2:
        xvar, yvar = numeric[0], numeric[1]
    else:
        ch = model.assign_channels(["x", "y"])
        xvar, yvar = ch["x"], ch["y"]
        # assign_channels gives best two by type-effectiveness; if it left one
        # empty (single state var), reuse it — the renderer delay-embeds when x==y.
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


def is_delay_embed(xvar, yvar):
    """True when the two chosen axes are the SAME numeric var — the single-numeric-var
    case where x-vs-x is a useless diagonal and we delay-embed (x_t vs x_{t+1}) instead."""
    return (xvar is not None and yvar is not None
            and xvar["name"] == yvar["name"]
            and xvar["kind"] in ("int", "real"))


# ---- ensemble orbits (the PRIMARY multi-initial-condition mode) --------------
def _attractor_key(model, orbit):
    """A hashable tag for the attractor an orbit settles into: the carried-leaf key of its
    LAST state (a fixed point) — or, when the orbit ends on a cycle, the smallest member key
    in the recurring tail (so two orbits on the same limit cycle share a tag regardless of
    phase). `step_trajectory` stops at the first revisit, so the tail from that revisit point
    to the end is the cycle; we key on the min of the visited keys in that recurrent set."""
    if not orbit:
        return None
    last_key = model._key(orbit[-1])
    # An earlier occurrence of the final key means [j..end] is the recurrent cycle.
    for j in range(len(orbit) - 1):
        if model._key(orbit[j]) == last_key:
            return min(model._key(s) for s in orbit[j:])
    return last_key                                # fixed point (no revisit): its own key


def basins_separable(model, orbits, tags, xvar, yvar, delay):
    """True iff the attractor tags name GENUINELY-SEPARATE basins worth coloring — a few
    attractors whose settled points are spread WIDE across the plotted plane (a bistable's
    walls at 0 and 6). A CHAOTIC integer map also produces several tags, but its 'attractors'
    are near-identical points clustered in one corner of the strange attractor (logistic:
    259/278/287 across a 0..930 range) — coloring those as basins would be a lie. So we
    require: 2..6 tags, each shared by ≥2 orbits, AND the min gap between distinct settled
    centroids exceeds 20% of the plotted spread. Otherwise it's one attractor; color by seed."""
    real = [t for t in tags if t is not None]
    distinct = set(real)
    if not (2 <= len(distinct) <= 6):
        return False
    # Each real basin should attract MULTIPLE inits (a saddle attracting one seed is still a
    # fixed point, but we still want ≥2 well-populated basins for the partition to read).
    counts = {t: real.count(t) for t in distinct}
    if sum(1 for c in counts.values() if c >= 2) < 2:
        return False
    # Settled centroid (x of the final state) per tag; require them spread across the plane.
    centroid = {}
    for orb, t in zip(orbits, tags):
        if t is None:
            continue
        centroid.setdefault(t, []).append(_project(model, xvar, orb[-1][xvar["name"]]))
    cxs = sorted(sum(v) / len(v) for v in centroid.values())
    if len(cxs) < 2:
        return False
    spread = cxs[-1] - cxs[0]
    if spread <= 0:
        return False
    min_gap = min(b - a for a, b in zip(cxs, cxs[1:]))
    return min_gap >= 0.2 * spread


def _ensemble_orbits(model):
    """The PRIMARY mode: forward-simulate from MANY initial conditions and scatter the
    settled (post-transient) orbit points. Returns (orbits, attractor_tags) or None.

      orbits         — list of state-dict lists, transient-trimmed, one per kept init.
      attractor_tags — parallel list: the attractor key each orbit settled into (for basin
                       coloring); distinct tags ⇒ distinct basins (bistable shows both).

    Returns None when there's no honest ensemble box (an unbounded carried var) — the caller
    then falls back to the single-orbit construction."""
    from time_series_ensemble import ensemble_inits, step_trajectory
    inits, _kind, _note = ensemble_inits(model)
    if inits is None:
        return None
    prefer_change = model.is_discrete()
    orbits, tags = [], []
    for init in inits:
        traj = step_trajectory(model, init, _STEPS, prefer_change)
        if not traj:
            continue
        tag = _attractor_key(model, traj)
        # Drop transient ticks, but keep at least the last TWO states: the delay embedding
        # needs a consecutive pair (x_t, x_{t+1}) per orbit, and a fast-halting basin (a
        # 2-state bistable orbit like 1→0) must still contribute its attractor point. A
        # 1-state orbit keeps its single state (it IS the fixed point).
        keep_min = min(2, len(traj))
        drop = min(_TRANSIENT, max(0, len(traj) - keep_min))
        kept = traj[drop:]
        orbits.append(kept)
        tags.append(tag)
    if not orbits:
        return None
    return orbits, tags


# ---- single-orbit fallback (unbounded models, no ensemble box) ---------------
def _reachable_with_depth(model, limit=400):
    """BFS the reachable set, returning (states, depths) parallel lists where
    depths[i] is the minimum number of steps from the initial state."""
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


def fallback_orbits(model):
    """Single-orbit construction for the no-ensemble (unbounded) case: the honest autonomous
    chain from the initial state, else the reachable set. Returns (orbits, point_time, mode)
    where mode is 'autonomous' | 'reachable'."""
    init = model.initial_state()
    # An unbounded OSCILLATOR (pendulum/vanderpol) whose init is the origin fixed point gives a
    # length-1 orbit — a false 'settles to a single fixed point' N/A. On this unbounded fallback
    # path there's no proven bound to violate, so excite off the origin to trace the real limit
    # cycle (guarded so bounded/honest-flat models are untouched — Marek #183).
    from time_series_walk import excited_seed
    seed = excited_seed(model) or init
    orb = model.trajectory(start=seed, steps=400) if seed is not None else []
    if len(orb) > 2:
        return [orb], [list(range(len(orb)))], "autonomous"
    states, depths = _reachable_with_depth(model)
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
    xmin = min(p["gx"] for p in pts)
    ymin = min(p["gy"] for p in pts)
    for (gx, gy), group in groups.items():
        if len(group) < 2:
            continue
        rad = 0.16
        for i, p in enumerate(group[1:], start=1):
            ang = 2 * math.pi * (i - 1) / (len(group) - 1)
            p["x"] = max(xmin, gx + rad * math.cos(ang))
            p["y"] = max(ymin, gy + rad * math.sin(ang))
