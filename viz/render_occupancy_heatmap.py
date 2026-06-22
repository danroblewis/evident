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


# The DATA layer (point collection, axis picking, extent/degeneracy guards,
# binning) lives in its own module. This file keeps the draw layer + dispatch.
from occupancy_collect import (  # noqa: E402
    ordinal, axis_ticklabels, collect_numeric, collect_discrete,
    numeric_degeneracy, numeric_extent, _clip_to_extent, occupancy_smear,
    pick_axes, nbins, MIN_DISTINCT,
)


def draw_heatmap(fig, ax, m, a0, a1, xs, ys, vmax=None, title=None,
                 ex=None, ey=None):
    """One heatmap panel. Returns the image (for a shared colorbar). `ex`/`ey` are
    the robust per-axis extents the histogram is framed to (sentinels/outliers
    already clipped out by the caller)."""
    if len(xs) == 0:
        ax.text(0.5, 0.5, "(empty)", ha="center", va="center",
                transform=ax.transAxes, fontsize=11)
        ax.set_xticks([])
        ax.set_yticks([])
        if title:
            ax.set_title(title, fontsize=10)
        return None
    bx, by = nbins(m, a0, xs, extent=ex), nbins(m, a1, ys, extent=ey)
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
        _render_strip(m, a0, out, title)
        return

    both_numeric = a0["kind"] in ("int", "real") and a1["kind"] in ("int", "real")
    discrete_path = m.is_discrete() or not both_numeric

    # --- FACETED small multiples (one heatmap per low-card categorical value) ---
    if facet is not None and discrete_path:
        _render_faceted(m, a0, a1, facet, out, title)
        return

    # --- single-panel heatmap ---
    _render_single(m, a0, a1, discrete_path, out, title)


def _render_strip(m, a0, out, title):
    """One usable axis: a 1-D occupancy histogram strip."""
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


def _render_faceted(m, a0, a1, facet, out, title):
    """Small multiples: one heatmap panel per low-cardinality facet value, all
    sharing one density (vmax) scale."""
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


def _save_na(fig, ax, m, out, title, reason):
    """Emit the honest N/A card on the single-panel axis and save."""
    placeholder(ax, m, reason)
    ax.set_title(title)
    fig.tight_layout()
    fig.savefig(out, dpi=120)
    plt.close(fig)


def _numeric_guard(m, a0, a1, xs, ys, fig, ax, out, title):
    """Frame + degeneracy/smear guards for the numeric single-panel path. Returns
    (xs, ys, ex, ey) to draw, or None if it already emitted an N/A card."""
    # An axis that is a constant or a free-running clock (life's gen) has no
    # dwell structure — its extent is the sampling cap, not a region. Honest N/A.
    reason = numeric_degeneracy(m, a0, a1, xs, ys)
    if reason is not None:
        _save_na(fig, ax, m, out, title, reason)
        return None
    # Frame to the robust reachable extent (drops -1 'empty slot' sentinels and
    # far-out initializers) and clip the data to it, so the grid covers only the
    # occupied region instead of a sentinel-blown-out plane.
    ex, ey = numeric_extent(m, a0, xs), numeric_extent(m, a1, ys)
    xs, ys = _clip_to_extent(xs, ys, ex, ey)
    # A density over a handful of cells is a fabrication waiting to happen — the
    # reachable set is too small/degenerate for an occupancy heatmap to mean
    # anything. Render the honest N/A card instead of painting a guessed plane.
    ndistinct = len({p for p in zip(xs.tolist(), ys.tolist())})
    if ndistinct < MIN_DISTINCT:
        _save_na(fig, ax, m, out, title,
                 f"reachable set is {ndistinct} point(s), finite —\n"
                 "occupancy heatmap not meaningful")
        return None
    # Large grid + ~no repeat visits = a sparse trajectory smear (lru's monotone
    # counter swept across a spread of key values), not a dwell density. N/A.
    bx, by = nbins(m, a0, xs, extent=ex), nbins(m, a1, ys, extent=ey)
    h, _, _ = np.histogram2d(xs, ys, bins=[bx, by])
    if occupancy_smear(h):
        _save_na(fig, ax, m, out, title,
                 "no dwell concentration — the trajectory sweeps the\n"
                 "plane without returning (a smear, not occupancy)")
        return None
    return xs, ys, ex, ey


def _render_single(m, a0, a1, discrete_path, out, title):
    """The single-panel heatmap, with the numeric framing/degeneracy guards."""
    if discrete_path:
        xs, ys, _ = collect_discrete(m, (a0, a1))
    else:
        xs, ys = collect_numeric(m, (a0, a1))

    fig, ax = plt.subplots(figsize=(7.5, 6.5))
    if len(xs) == 0:
        _save_na(fig, ax, m, out, title, "no visited states (transition unsat)")
        return
    ex = ey = None
    if not discrete_path:
        guarded = _numeric_guard(m, a0, a1, xs, ys, fig, ax, out, title)
        if guarded is None:
            return
        xs, ys, ex, ey = guarded

    im = draw_heatmap(fig, ax, m, a0, a1, xs, ys, ex=ex, ey=ey)
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
