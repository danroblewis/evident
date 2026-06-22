#!/usr/bin/env python3
"""occupancy_collect.py — the DATA layer for render_occupancy_heatmap.py.

Everything that answers "which points does the program actually visit, and over
what extent / binning?" — with no plotting policy. The renderer imports the
collection entry points (collect_numeric / collect_discrete), the axis pickers
(pick_axes / ordinal / axis_ticklabels), the degeneracy + extent guards, and the
binning helper (nbins). Dynamics come ONLY from evident_viz queries.
"""
import numpy as np


def ordinal(m, var, value):
    """Project a state value to a real number for binning."""
    k = var["kind"]
    if k == "int" or k == "real":
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        return float(m.enum_variants[var["name"]].index(value))
    if k == "string":
        return float(abs(hash(value)) % 997)
    return 0.0


def axis_ticklabels(m, var, lo, hi):
    """For discrete axes, return integer tick positions + variant/bool labels."""
    k = var["kind"]
    if k == "bool":
        return [0, 1], ["false", "true"]
    if k == "enum":
        names = m.enum_variants[var["name"]]
        return list(range(len(names))), names
    return None, None


# A heatmap over < this many distinct (x, y) cells is not a density — it's a
# scatter of a few points. Route those to the honest N/A card.
MIN_DISTINCT = 4
# Exploratory grid resolution used ONLY in the continuous fallback below.
_SEED_GRID_N = 7
_DEFAULT_SPAN = 3000.0  # last-resort box, only for genuinely unbounded continuous dynamics


def _explore(m, ax, ay):
    """Continuous fallback: the program's pinned init is an unstable fixed point
    that never reaches the limit cycle (van der Pol's origin), so the reachable
    set is degenerate. Seed an exploratory grid, follow each chain, drop the
    transient, and keep the VISITED points — the attractor the chains converge
    onto. The returned extent is the orbit's, derived from visited states."""
    def seed_span(var, default):
        # Seed span: the reachable extent (axis_bounds) when wide enough to
        # matter, else a default wide box. We seed within this, but the HISTOGRAM
        # extent is always clipped to the points actually visited — so the picture
        # is scaled to the orbit, never to the seed box.
        if var["kind"] not in ("int", "real"):
            return default
        b = m.axis_bounds(var["name"])
        if b is None:
            return default
        half = max(abs(b[0]), abs(b[1]))
        return half if half > 1.0 else default

    sx = seed_span(ax, _DEFAULT_SPAN)
    sy = seed_span(ay, _DEFAULT_SPAN)
    other = {v["name"]: 0 for v in m.state_vars}
    gx = np.linspace(-sx, sx, _SEED_GRID_N)
    gy = np.linspace(-sy, sy, _SEED_GRID_N)
    xs, ys = [], []
    for vx in gx:
        for vy in gy:
            s = dict(other)
            s[ax["name"]] = int(vx) if ax["kind"] == "int" else vx
            s[ay["name"]] = int(vy) if ay["kind"] == "int" else vy
            traj = m.trajectory(start=s, steps=160)
            for st in traj[20:]:           # skip the transient: keep the attractor
                xs.append(ordinal(m, ax, st[ax["name"]]))
                ys.append(ordinal(m, ay, st[ay["name"]]))
    return np.array(xs, float), np.array(ys, float)


def collect_numeric(m, axes):
    """(xs, ys) of where the system dwells, for two numeric axes — derived from
    the REACHABLE states, never a hardcoded box.

    Primary: plot the reachable set + the trajectory from the initial state
    directly. For a bounded/terminating program (a counter to 10) this is the
    real, tight occupied region — no fabricated structure outside it.

    Fallback (genuinely continuous dynamics whose limit cycle isn't reached from
    the pinned init): seed an exploratory grid and keep the VISITED attractor;
    the binning extent is the orbit's, not the seed box's."""
    ax, ay = axes
    # The points the program ACTUALLY visits: the reachable state set plus one
    # trajectory from the defined initial state — the honest occupancy, not a
    # guessed box. Empty if there's no init.
    states, _ = m.reachable(limit=2000)
    pts = list(states) + m.trajectory(steps=400)
    xs = np.array([ordinal(m, ax, s[ax["name"]]) for s in pts], float)
    ys = np.array([ordinal(m, ay, s[ay["name"]]) for s in pts], float)
    distinct = len({p for p in zip(xs.tolist(), ys.tolist())})
    if distinct >= MIN_DISTINCT:
        return xs, ys
    # degenerate reachable set: either a genuinely continuous attractor we must
    # explore for, or a true single-point system. _explore returns the visited
    # attractor (or stays degenerate, which the caller routes to N/A).
    return _explore(m, ax, ay)


# A numeric axis that takes a DISTINCT (injective — one value per visited state),
# consecutive value in every reachable state AND fills the reachable-BFS cap is a
# free-running clock (life's gen: 0,1,2,…, one per state, forever). Its extent is
# an artifact of where we stopped sampling, not a region the system "dwells" in —
# there is no occupancy to show along it. Injectivity is the discriminator: an
# ACCUMULATOR (lru's miss_count) revisits values across states, so it's NOT a clock
# even when its values happen to densely cover a range; it pairs into a real
# occupancy region against the other axis.
def _is_free_running_counter(m, var):
    if var["kind"] != "int":
        return False
    states, _ = m.reachable(limit=2000)
    if len(states) < 2000:                 # reachable terminated -> bounded, real
        return False
    vals = [s[var["name"]] for s in states]
    return m._is_unit_counter("int", vals)  # distinct+consecutive per state = a clock


def numeric_degeneracy(m, ax, ay, xs, ys):
    """Return an N/A reason string when a (numeric, numeric) occupancy heatmap is
    not meaningful — a constant axis, or a free-running counter whose extent is
    just the sampling cap — else None. The heatmap exists to show WHERE the system
    dwells; if an axis is a clock or a constant there is no dwell structure."""
    nx = len({round(float(v), 6) for v in xs if np.isfinite(v)})
    ny = len({round(float(v), 6) for v in ys if np.isfinite(v)})
    if nx <= 1 and ny <= 1:
        return "both axes are constant — a single point, no occupancy"
    if nx <= 1:
        return f"{ax['name']} is constant ({ny}×1) — no 2-D occupancy to show"
    if ny <= 1:
        return f"{ay['name']} is constant (1×{nx}) — no 2-D occupancy to show"
    cx = _is_free_running_counter(m, ax)
    cy = _is_free_running_counter(m, ay)
    if cx and cy:
        return "both axes are free-running counters — extent is the sampling cap"
    if cx:
        return (f"{ax['name']} is a free-running counter (0,1,2,…) and "
                f"{ay['name']} is constant — no occupancy region")
    if cy:
        return (f"{ay['name']} is a free-running counter (0,1,2,…) and "
                f"{ax['name']} is constant — no occupancy region")
    return None


def numeric_extent(m, var, data):
    """The robust plotted extent for a numeric axis, derived from the points
    ACTUALLY plotted (the reachable set, or the explored attractor for a continuous
    system). An IQR fence rejects sparse outliers and 'empty slot' sentinels (lru's
    lone -1) so they can't blow the axis out past the occupied region — but the bulk
    of the orbit is kept, so a wide-but-real limit cycle (van der Pol's ±3000) is
    NOT crushed to a hardcoded box. Frames from the data, never from a guessed
    span."""
    d = np.asarray(data, float)
    d = d[np.isfinite(d)]
    if len(d) == 0:
        return None
    lo, hi = float(d.min()), float(d.max())
    if len(d) >= 8:
        q1, q3 = np.percentile(d, 25), np.percentile(d, 75)
        iqr = q3 - q1
        if iqr > 0:
            fl, fh = q1 - 3 * iqr, q3 + 3 * iqr
            kept = d[(d >= fl) & (d <= fh)]
            if len(kept):
                lo, hi = float(kept.min()), float(kept.max())
    return (lo - 1.0, hi + 1.0) if lo == hi else (lo, hi)


def _clip_to_extent(xs, ys, ex, ey):
    """Drop points outside the robust per-axis extent so a stray sentinel doesn't
    survive into the histogram (and so the bin grid covers only the framed range)."""
    if ex is None or ey is None:
        return xs, ys
    keep = ((xs >= ex[0]) & (xs <= ex[1]) & (ys >= ey[0]) & (ys <= ey[1]))
    return xs[keep], ys[keep]


# On a LARGE numeric grid, a heatmap is only honest if the system DWELLS — returns
# to cells repeatedly, concentrating visits. lru sweeps a monotone counter against a
# spread of key values: each (k0, miss_count) cell is touched ~once, so the picture
# is a sparse trajectory smear spanning a blown-out key axis, not a density. When
# most occupied cells are singletons (visited exactly once) on a big grid, there is
# no occupancy to show -> N/A. A small integer grid (wc's staircase) is exempt: it's
# a complete, readable lattice where per-cell counts are naturally low.
_BIG_GRID = 100        # cells; below this it's a discrete lattice, not a plane
_SMEAR_FRAC = 0.6      # >this fraction of occupied cells visited once = a smear


def occupancy_smear(h):
    occ = h[h > 0]
    if h.size < _BIG_GRID or len(occ) == 0:
        return False
    return float((occ <= 1).sum()) / len(occ) > _SMEAR_FRAC


def collect_discrete(m, axes, facet_var=None):
    """Occupancy over the reachable graph. Returns (xs, ys, fs) where fs is the
    ordinal-projected facet value per visited point (or None when not faceting).

    We seed every reachable state once (nothing invisible) then walk the
    successor fan to accumulate genuine dwell traffic."""
    states, edges = m.reachable()
    ax, ay = axes
    xs, ys, fs = [], [], []

    def push(st):
        xs.append(ordinal(m, ax, st[ax["name"]]))
        ys.append(ordinal(m, ay, st[ay["name"]]))
        if facet_var is not None:
            fs.append(st[facet_var["name"]])

    for st in states:
        push(st)
    if states:
        import random
        rng = random.Random(0)
        cur = states[0]
        for _ in range(4000):
            succs = m.successors(cur)
            if not succs:
                break
            cur = rng.choice(succs)
            push(cur)
    return np.array(xs), np.array(ys), fs


def pick_axes(m, exclude=()):
    """Two axes: prefer the two top numeric vars (metric histogram); else fall
    back to assign_channels' x/y, skipping anything in `exclude`."""
    numeric = [v for v in m.numeric_vars if v["name"] not in exclude]
    if len(numeric) >= 2:
        return numeric[0], numeric[1]
    # mixed/discrete: use channel assignment, then top up from remaining vars
    ch = m.assign_channels(["x", "y"])
    chosen, seen = [], set(exclude)
    for c in ("x", "y"):
        v = ch[c]
        if v and v["name"] not in seen:
            chosen.append(v)
            seen.add(v["name"])
    for v in m.state_vars:
        if len(chosen) >= 2:
            break
        if v["name"] not in seen:
            chosen.append(v)
            seen.add(v["name"])
    if len(chosen) >= 2:
        return chosen[0], chosen[1]
    return (chosen[0], None) if chosen else (None, None)


# An int axis with at most this many distinct reachable values is treated as a
# discrete categorical axis: each value gets one full-width integer-centered
# cell, instead of being smeared into a single thin sliver of a 60-bin grid.
_MAX_INT_DISCRETE = 24


def nbins(m, v, data=None, extent=None):
    if v["kind"] == "bool":
        return np.array([-0.5, 0.5, 1.5])
    if v["kind"] == "enum":
        n = len(m.enum_variants[v["name"]])
        return np.arange(-0.5, n + 0.5, 1.0)
    if v["kind"] == "int":
        # Bin over the robust extent (frames out sentinels/outliers) when given,
        # else over the data range.
        if extent is not None:
            lo, hi = int(np.floor(extent[0])), int(np.ceil(extent[1]))
        elif data is not None and len(data):
            d = np.asarray(data, float)
            d = d[np.isfinite(d)]
            if not len(d):
                return 60
            lo, hi = int(np.floor(d.min())), int(np.ceil(d.max()))
        else:
            return 60
        span = hi - lo
        # Few distinct integer columns -> integer-centered full-width cells.
        if 0 <= span <= _MAX_INT_DISCRETE:
            return np.arange(lo - 0.5, hi + 1.5, 1.0)
    if extent is not None and v["kind"] in ("int", "real"):
        return np.linspace(extent[0], extent[1], 61)
    return 60
