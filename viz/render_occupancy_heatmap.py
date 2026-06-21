#!/usr/bin/env python3
"""render_occupancy_heatmap.py — occupancy density heatmap for ANY Evident IR.

Where does the system SPEND ITS TIME? We collect a large bag of visited points
(many seeds x many steps), pick two state axes, 2-D-histogram the visited
points over them, and draw the density as a heatmap. The bright region is the
attractor / occupied region of the state space.

Channel mapping (Cleveland-McGill / Mackinlay):
  - POSITION (x, y): the two top-ranked axes. We prefer NUMERIC vars (the
    histogram is meaningful on a metric axis); enum -> ordinal index with
    variant tick labels, bool -> 0/1.
  - COLOR: KEPT for the derived OCCUPANCY DENSITY (log visits) — the meaningful
    quantity this viz exists to show. We do NOT overwrite it with a variable.
  - FACET (small multiples): a LOW-cardinality categorical (<= 5 values) that
    is NOT one of the axes becomes one heatmap panel per value — the honest way
    to ADD a third dimension to a high-D model.

Usage:
    python3 viz/render_occupancy_heatmap.py <smt2> <schema> <out.png>

Dynamics come ONLY from evident_viz queries — nothing about any specific program
is hardcoded here.
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load  # noqa: E402

import matplotlib  # noqa: E402
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

VIZ_TYPE = "occupancy_heatmap"
MAX_FACETS = 5  # low-cardinality threshold for small multiples


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


def cardinality(m, var):
    if var["kind"] == "bool":
        return 2
    if var["kind"] == "enum":
        return len(m.enum_variants[var["name"]])
    return None  # unbounded / numeric


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


def _reachable_points(m, ax, ay):
    """The points the program ACTUALLY visits: the reachable state set plus one
    trajectory from the defined initial state. This is the honest occupancy —
    where the program lives, not a guessed box. Empty if there's no init."""
    states, _ = m.reachable(limit=2000)
    pts = list(states) + m.trajectory(steps=400)
    xs = [ordinal(m, ax, s[ax["name"]]) for s in pts]
    ys = [ordinal(m, ay, s[ay["name"]]) for s in pts]
    return np.array(xs, float), np.array(ys, float)


def _seed_span(m, var, default):
    """Seed span for the continuous fallback: the reachable extent (axis_bounds)
    when it's wide enough to matter, else a default wide box. We seed within
    this, but the HISTOGRAM extent is always clipped to the points actually
    visited — so the picture is scaled to the orbit, never to the seed box."""
    if var["kind"] not in ("int", "real"):
        return default
    b = m.axis_bounds(var["name"])
    if b is None:
        return default
    half = max(abs(b[0]), abs(b[1]))
    return half if half > 1.0 else default


def _explore(m, ax, ay):
    """Continuous fallback: the program's pinned init is an unstable fixed point
    that never reaches the limit cycle (van der Pol's origin), so the reachable
    set is degenerate. Seed an exploratory grid, follow each chain, drop the
    transient, and keep the VISITED points — the attractor the chains converge
    onto. The returned extent is the orbit's, derived from visited states."""
    sx = _seed_span(m, ax, _DEFAULT_SPAN)
    sy = _seed_span(m, ay, _DEFAULT_SPAN)
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
    xs, ys = _reachable_points(m, ax, ay)
    distinct = len({p for p in zip(xs.tolist(), ys.tolist())})
    if distinct >= MIN_DISTINCT:
        return xs, ys
    # degenerate reachable set: either a genuinely continuous attractor we must
    # explore for, or a true single-point system. _explore returns the visited
    # attractor (or stays degenerate, which the caller routes to N/A).
    return _explore(m, ax, ay)


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


def nbins(m, v, data=None):
    if v["kind"] == "bool":
        return np.array([-0.5, 0.5, 1.5])
    if v["kind"] == "enum":
        n = len(m.enum_variants[v["name"]])
        return np.arange(-0.5, n + 0.5, 1.0)
    if v["kind"] == "int" and data is not None and len(data):
        d = np.asarray(data, float)
        d = d[np.isfinite(d)]
        if len(d):
            lo, hi = int(np.floor(d.min())), int(np.ceil(d.max()))
            span = hi - lo
            # Few distinct integer columns -> integer-centered full-width cells.
            if 0 <= span <= _MAX_INT_DISCRETE:
                return np.arange(lo - 0.5, hi + 1.5, 1.0)
    return 60


def draw_heatmap(fig, ax, m, a0, a1, xs, ys, vmax=None, title=None):
    """One heatmap panel. Returns the image (for a shared colorbar)."""
    if len(xs) == 0:
        ax.text(0.5, 0.5, "(empty)", ha="center", va="center",
                transform=ax.transAxes, fontsize=11)
        ax.set_xticks([])
        ax.set_yticks([])
        if title:
            ax.set_title(title, fontsize=10)
        return None
    bx, by = nbins(m, a0, xs), nbins(m, a1, ys)
    h, xedges, yedges = np.histogram2d(xs, ys, bins=[bx, by])
    hp = np.log1p(h)
    im = ax.imshow(
        hp.T, origin="lower", aspect="auto",
        extent=[xedges[0], xedges[-1], yedges[0], yedges[-1]],
        cmap="inferno", interpolation="nearest", vmin=0, vmax=vmax,
    )
    ax.set_xlabel(a0["name"])
    ax.set_ylabel(a1["name"])
    tk0, tl0 = axis_ticklabels(m, a0, xedges[0], xedges[-1])
    if tk0 is not None:
        ax.set_xticks(tk0)
        ax.set_xticklabels(tl0, rotation=30, ha="right")
    tk1, tl1 = axis_ticklabels(m, a1, yedges[0], yedges[-1])
    if tk1 is not None:
        ax.set_yticks(tk1)
        ax.set_yticklabels(tl1)
    if title:
        ax.set_title(title, fontsize=10)
    return im


def placeholder(ax, m, reason):
    ax.text(0.5, 0.5, f"N/A\n{reason}", ha="center", va="center",
            fontsize=13, transform=ax.transAxes, wrap=True)
    ax.set_xticks([])
    ax.set_yticks([])


def render(smt2, schema, out):
    m = load(smt2, schema)
    title = f"{m.fsm} — {VIZ_TYPE}"

    # --- choose facet first, then axes that avoid the facet var ---
    a0_pre, a1_pre = pick_axes(m)
    facet = None
    if a0_pre is not None and a1_pre is not None and m.is_discrete():
        # faceting only makes sense for the reachable-graph path, AND only on a
        # var that stays ~constant within a run (a config/regime set once). A var
        # that changes as the system runs would split the trajectory across
        # panels and destroy the dynamics — facet_var() returns None for those.
        facet = m.facet_var()
        if facet is not None and facet["name"] in {a0_pre["name"], a1_pre["name"]}:
            facet = None
    a0, a1 = pick_axes(m, exclude=({facet["name"]} if facet else ()))

    # --- no axes at all ---
    if a0 is None:
        fig, ax = plt.subplots(figsize=(7.5, 6.5))
        placeholder(ax, m, "no state variables")
        ax.set_title(title)
        fig.tight_layout()
        fig.savefig(out, dpi=120)
        plt.close(fig)
        return

    # --- single axis -> 1-D strip ---
    if a1 is None:
        fig, ax = plt.subplots(figsize=(7.5, 6.5))
        if m.is_discrete():
            xs, _, _ = collect_discrete(m, (a0, a0))
        else:
            xs, _ = collect_numeric(m, (a0, a0))
        if len(xs) == 0:
            placeholder(ax, m, "no reachable states")
        else:
            ax.hist(xs, bins=40, color="#3b6fb0")
            ax.set_xlabel(a0["name"])
            ax.set_ylabel("occupancy (visits)")
            tk, tl = axis_ticklabels(m, a0, xs.min(), xs.max())
            if tk is not None:
                ax.set_xticks(tk)
                ax.set_xticklabels(tl, rotation=30, ha="right")
        ax.set_title(title + "\n(1-D strip: only one usable axis)")
        fig.tight_layout()
        fig.savefig(out, dpi=120)
        plt.close(fig)
        return

    both_numeric = a0["kind"] in ("int", "real") and a1["kind"] in ("int", "real")
    discrete_path = m.is_discrete() or not both_numeric

    # --- FACETED small multiples (one heatmap per low-card categorical value) ---
    if facet is not None and discrete_path:
        xs, ys, fs = collect_discrete(m, (a0, a1), facet_var=facet)
        if facet["kind"] == "enum":
            values = m.enum_variants[facet["name"]]
        else:
            values = [False, True]
        fs = np.array([ordinal(m, facet, v) for v in fs]) if len(fs) else np.array([])
        # global vmax so panels share one density scale
        gmax = 0.0
        per = []
        for val in values:
            ordv = ordinal(m, facet, val)
            mask = fs == ordv if len(fs) else np.array([], dtype=bool)
            pxs = xs[mask] if len(xs) else xs
            pys = ys[mask] if len(ys) else ys
            per.append((val, pxs, pys))
            if len(pxs):
                bx, by = nbins(m, a0, pxs), nbins(m, a1, pys)
                h, _, _ = np.histogram2d(pxs, pys, bins=[bx, by])
                gmax = max(gmax, float(np.log1p(h).max()))
        gmax = gmax or 1.0

        n = len(values)
        fig, axes = plt.subplots(1, n, figsize=(4.2 * n + 1.2, 5.4),
                                 squeeze=False)
        axes = axes[0]
        im = None
        flabel = facet["name"].split(".")[-1]
        for axp, (val, pxs, pys) in zip(axes, per):
            i = draw_heatmap(fig, axp, m, a0, a1, pxs, pys, vmax=gmax,
                             title=f"{flabel} = {val}")
            if i is not None:
                im = i
        if im is not None:
            cb = fig.colorbar(im, ax=list(axes), fraction=0.025, pad=0.02)
            cb.set_label("log(1 + visits)")
        fig.suptitle(f"{title}\ndiscrete occupancy, faceted by {flabel} "
                     f"(small multiples)", fontsize=12)
        fig.savefig(out, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    # --- single-panel heatmap ---
    if discrete_path:
        xs, ys, _ = collect_discrete(m, (a0, a1))
    else:
        xs, ys = collect_numeric(m, (a0, a1))

    fig, ax = plt.subplots(figsize=(7.5, 6.5))
    if len(xs) == 0:
        placeholder(ax, m, "no visited states (transition unsat)")
        ax.set_title(title)
        fig.tight_layout()
        fig.savefig(out, dpi=120)
        plt.close(fig)
        return
    # A density over a handful of cells is a fabrication waiting to happen — the
    # reachable set is too small/degenerate for an occupancy heatmap to mean
    # anything. Render the honest N/A card instead of painting a guessed plane.
    if not discrete_path:
        ndistinct = len({p for p in zip(xs.tolist(), ys.tolist())})
        if ndistinct < MIN_DISTINCT:
            placeholder(ax, m, f"reachable set is {ndistinct} point(s), finite —\n"
                               "occupancy heatmap not meaningful")
            ax.set_title(title)
            fig.tight_layout()
            fig.savefig(out, dpi=120)
            plt.close(fig)
            return

    im = draw_heatmap(fig, ax, m, a0, a1, xs, ys)
    if im is not None:
        cb = fig.colorbar(im, ax=ax)
        cb.set_label("log(1 + visits)")
    kind_note = "discrete occupancy" if discrete_path else "numeric attractor"
    ax.set_title(f"{title}\n{kind_note}: where the system dwells")
    fig.tight_layout()
    fig.savefig(out, dpi=120)
    plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print(f"usage: {sys.argv[0]} <smt2> <schema> <out.png>", file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
    print(f"wrote {sys.argv[3]}")
