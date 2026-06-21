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


def collect_numeric(m, axes):
    """Many seeds x trajectory steps -> (xs, ys) for two numeric axes.

    Grid seeds across the full numeric box (we are NOT limited to reachable
    states — successor() accepts arbitrary pinned points), then follow each
    chain. The dwell in the limit cycle / fixed point dominates the histogram.
    """
    init = m.initial_state()
    span = 3200.0  # generous default for fixed-point numeric systems
    seeds = []
    grid = np.linspace(-span, span, 9)
    ax, ay = axes
    other = {v["name"]: 0 for v in m.state_vars}
    for gx in grid:
        for gy in grid:
            s = dict(other)
            s[ax["name"]] = int(gx) if ax["kind"] == "int" else gx
            s[ay["name"]] = int(gy) if ay["kind"] == "int" else gy
            seeds.append(s)
    # explicit seeds away from the origin fixed point (per sample notes)
    for sx, sy in ((2800, 0), (400, 0), (0, 2700)):
        s = dict(other)
        s[ax["name"]] = sx
        s[ay["name"]] = sy
        seeds.append(s)
    if init is not None:
        seeds.append(init)

    xs, ys = [], []
    for seed in seeds:
        traj = m.trajectory(start=seed, steps=120)
        # skip an initial transient so the heatmap reflects the attractor
        for st in traj[10:]:
            xs.append(ordinal(m, ax, st[ax["name"]]))
            ys.append(ordinal(m, ay, st[ay["name"]]))
    return np.array(xs), np.array(ys)


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


def nbins(m, v):
    if v["kind"] == "bool":
        return np.array([-0.5, 0.5, 1.5])
    if v["kind"] == "enum":
        n = len(m.enum_variants[v["name"]])
        return np.arange(-0.5, n + 0.5, 1.0)
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
    bx, by = nbins(m, a0), nbins(m, a1)
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
                bx, by = nbins(m, a0), nbins(m, a1)
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
