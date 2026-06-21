#!/usr/bin/env python3
"""render_occupancy_heatmap.py — occupancy density heatmap for ANY Evident IR.

Where does the system SPEND ITS TIME? We collect a large bag of visited points
(many seeds x many steps), pick two state axes, 2-D-histogram the visited
points over them, and draw the density as a heatmap. The bright region is the
attractor / occupied region of the state space.

Usage:
    python3 viz/render_occupancy_heatmap.py <smt2> <schema> <out.png>

Works for numeric, discrete, and mixed Evident programs:
  - NUMERIC: grid of seeds across the state box, follow each trajectory, bin the
    fixed-point coordinates over two numeric axes.
  - DISCRETE / MIXED: project every var to an ordinal (bool->0/1, enum->index),
    accumulate occupancy of reachable states, bin over two chosen axes.
  - DEGRADED: <2 usable axes -> a single-axis 1-D occupancy strip; no states ->
    a titled placeholder.

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


def collect_numeric(m, axes):
    """Many seeds x trajectory steps -> (xs, ys) for two numeric axes.

    Grid seeds across the full numeric box (we are NOT limited to reachable
    states — successor() accepts arbitrary pinned points), then follow each
    chain. The dwell in the limit cycle / fixed point dominates the histogram.
    """
    nvars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    # estimate a box from the initial state + the documented scale; fall back wide
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
            # leave any extra numeric vars at 0
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


def collect_discrete(m, axes):
    """Occupancy over the reachable graph: weight each reachable state by its
    out-degree-driven dwell, projected onto two ordinal axes."""
    states, edges = m.reachable()
    ax, ay = axes
    xs, ys = [], []
    # count visits along a long random-ish walk to get genuine dwell density,
    # plus every reachable state once so nothing is invisible.
    for st in states:
        xs.append(ordinal(m, ax, st[ax["name"]]))
        ys.append(ordinal(m, ay, st[ay["name"]]))
    # walk the successor fan to accumulate traffic
    if states:
        import random
        rng = random.Random(0)
        cur = states[0]
        for _ in range(4000):
            succs = m.successors(cur)
            if not succs:
                break
            cur = rng.choice(succs)
            xs.append(ordinal(m, ax, cur[ax["name"]]))
            ys.append(ordinal(m, ay, cur[ay["name"]]))
    return np.array(xs), np.array(ys)


def pick_axes(m):
    """Choose two axes: prefer numeric pairs, else the two most-varied vars."""
    numeric = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    if len(numeric) >= 2:
        return numeric[0], numeric[1]
    # mixed/discrete: prefer an enum + something, else first two of anything
    order = sorted(m.state_vars,
                   key=lambda v: {"enum": 0, "int": 1, "real": 1,
                                  "bool": 2, "string": 3}.get(v["kind"], 4))
    if len(order) >= 2:
        return order[0], order[1]
    return (order[0], None) if order else (None, None)


def placeholder(ax, m, reason):
    ax.text(0.5, 0.5, f"N/A\n{reason}", ha="center", va="center",
            fontsize=13, transform=ax.transAxes, wrap=True)
    ax.set_xticks([])
    ax.set_yticks([])


def render(smt2, schema, out):
    m = load(smt2, schema)
    fig, ax = plt.subplots(figsize=(7.5, 6.5))
    a0, a1 = pick_axes(m)

    title = f"{m.fsm} — {VIZ_TYPE}"

    if a0 is None:
        placeholder(ax, m, "no state variables")
        ax.set_title(title)
        fig.tight_layout()
        fig.savefig(out, dpi=120)
        plt.close(fig)
        return

    if a1 is None:
        # single-axis 1-D occupancy strip
        if m.is_discrete():
            xs, _ = collect_discrete(m, (a0, a0))
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

    # Numeric trajectory seeding only works when BOTH axes are numeric (we pin a
    # grid of prev-values). Any enum/bool/string axis -> walk the reachable graph.
    both_numeric = a0["kind"] in ("int", "real") and a1["kind"] in ("int", "real")
    discrete_path = m.is_discrete() or not both_numeric
    if discrete_path:
        xs, ys = collect_discrete(m, (a0, a1))
    else:
        xs, ys = collect_numeric(m, (a0, a1))

    if len(xs) == 0:
        placeholder(ax, m, "no visited states (transition unsat)")
        ax.set_title(title)
        fig.tight_layout()
        fig.savefig(out, dpi=120)
        plt.close(fig)
        return

    # bin count: discrete axes -> one bin per ordinal level; numeric -> 60
    def nbins(v, data):
        if v["kind"] == "bool":
            return [-0.5, 0.5, 1.5]
        if v["kind"] == "enum":
            n = len(m.enum_variants[v["name"]])
            return np.arange(-0.5, n + 0.5, 1.0)
        return 60

    bx = nbins(a0, xs)
    by = nbins(a1, ys)

    h, xedges, yedges = np.histogram2d(xs, ys, bins=[bx, by])
    # log-ish scaling so the attractor and its surroundings are both visible
    hp = np.log1p(h)
    im = ax.imshow(
        hp.T, origin="lower", aspect="auto",
        extent=[xedges[0], xedges[-1], yedges[0], yedges[-1]],
        cmap="inferno", interpolation="nearest",
    )
    cb = fig.colorbar(im, ax=ax)
    cb.set_label("log(1 + visits)")
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
