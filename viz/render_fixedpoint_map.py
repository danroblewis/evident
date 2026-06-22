#!/usr/bin/env python3
"""render_fixedpoint_map.py — the "where does it come to rest" view.

Scans/samples the state space of ANY Evident IR, asks the transition where each
sampled state goes, and surfaces the attractors:

  * FIXED POINTS  — states s with s ∈ successors(s)  (the system rests there).
  * SHORT CYCLES  — successor chains s → s1 → … → s that return to s within a
                    few steps (periodic orbits / limit cycles).

It plots a 2-axis projection of the state space:
  * fixed points as large filled markers,
  * cycle members as smaller markers linked by arrows around their loop,
  * the rest of the sampled states as faint dots, so the attractors stand out
    against the basin.

Numeric systems (int/real vars) are GRID-scanned over an auto-ranged box.
Discrete systems (bool/enum/string) are scanned over their exact reachable set.
Mixed systems grid-scan their numeric axes and pin discrete axes per slice.

CLI:  python3 viz/render_fixedpoint_map.py <smt2> <schema> <out.png>
Works for any exported Evident program, not just the bundled samples.
"""
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import FancyArrowPatch

from evident_viz import load

# Attractor detection (fixed points + limit cycles) and the value->float
# projection live in the sibling analysis module; plotting stays here.
from fixedpoint_attractors import ordinal, find_attractors, near

# Channel assignment + state sampling (the data layer) live in their own
# sibling module; plotting stays here.
from fixedpoint_states import (
    assign_channels, sample_states, _domain, axis_label, _short, _fmt,
)

VIZ_TYPE = "fixedpoint_map"


# --------------------------------------------------------------------------
# plotting
# --------------------------------------------------------------------------
CAT_PALETTE = ["#7b9acc", "#cc8a5b", "#6fb38a", "#b07cc6", "#c9c05a",
               "#5fb0c0", "#c47ba0"]
CAT_SHAPES = ["o", "s", "^", "D", "v", "P", "X"]


def render(smt2, schema, out_path):
    model = load(smt2, schema)
    ch = assign_channels(model)
    xvar, yvar = ch["x"], ch["y"]

    title = f"{model.fsm} — {VIZ_TYPE}"
    states, mode, edges = sample_states(model)

    if not states or xvar is None:
        fig, ax = plt.subplots(figsize=(9, 8))
        placeholder(ax, title, "no states could be sampled from the transition")
        finish(fig, out_path)
        return out_path

    # attractors are a global property of the dynamics — find them ONCE, then
    # render them into whichever facet panel each member lands in.
    fixed, cycles = find_attractors(model, states, mode)

    facet = ch["facet"]
    if facet is not None:
        # Suppress empty facet panels: only render facet values that actually
        # occur in the sampled/reachable set. A facet var that is constant
        # across the run (e.g. find's state.s5 == Unseen everywhere) would
        # otherwise draw permanently-empty subplots for its unreached variants.
        present = {s[facet["name"]] for s in states}
        panels = [v for v in _domain(model, facet) if v in present]
        if len(panels) <= 1:
            # facet carries no information (one occupied value) -> single panel.
            facet = None
            panels = [None]
    else:
        panels = [None]

    ncol = min(len(panels), 3)
    nrow = (len(panels) + ncol - 1) // ncol
    fig, axes = plt.subplots(nrow, ncol, figsize=(9 * ncol, 8 * nrow),
                             squeeze=False)
    flat_axes = [axes[r][c] for r in range(nrow) for c in range(ncol)]

    for ax in flat_axes[len(panels):]:
        ax.axis("off")

    for ax, pval in zip(flat_axes, panels):
        sub = ([s for s in states if s[facet["name"]] == pval]
               if facet is not None else states)
        sub_fixed = ([s for s in fixed if s[facet["name"]] == pval]
                     if facet is not None else fixed)
        sub_cycles = (_filter_cycles(cycles, facet, pval)
                      if facet is not None else cycles)
        sub_edges = ([(a, b) for (a, b) in edges
                      if a[facet["name"]] == pval and b[facet["name"]] == pval]
                     if facet is not None else edges)
        draw_panel(ax, model, ch, sub, sub_fixed, sub_cycles, len(cycles),
                   sub_edges)
        if facet is not None:
            ax.set_title(f"{_short(facet['name'])} = {_fmt(facet, pval)}",
                         fontsize=12, fontweight="bold")

    fig.suptitle(_super_title(model, ch, mode, fixed, cycles),
                 fontsize=14, fontweight="bold", y=0.99)
    finish(fig, out_path)
    return out_path


def _filter_cycles(cycles, facet, pval):
    """A cycle belongs to a facet panel iff all its members share that facet value
    (discrete facet axes don't change along a numeric limit cycle)."""
    out = []
    for loop in cycles:
        if all(s[facet["name"]] == pval for s in loop):
            out.append(loop)
    return out


def _draw_edges(ax, proj, edges):
    """Faint connecting structure: the reachable transition graph. Drawing the
    basin's edges (not just its dots) turns scattered points into a legible graph
    the fixed points sit at the SINKS of — drawn UNDER everything so it reads as
    context, never competing with the attractor markers."""
    if not edges:
        return
    seen_seg = set()
    for a, b in edges:
        (x0, y0), (x1, y1) = proj(a), proj(b)
        if (x0, y0) == (x1, y1):
            continue                       # self-loop: a dot, not a segment
        seg = (round(x0, 3), round(y0, 3), round(x1, 3), round(y1, 3))
        if seg in seen_seg:
            continue
        seen_seg.add(seg)
        ax.plot([x0, x1], [y0, y1], color="#b9bdcc", alpha=0.5,
                lw=0.8, zorder=0, solid_capstyle="round")


def _draw_cycles(ax, proj, cycles, total_cycles):
    """Limit-cycle members + their loop arrows. Long orbits draw as a polyline
    with sparse arrows; short cycles draw per-edge arrows + member dots."""
    cyc_pts_x, cyc_pts_y = [], []
    labelled = False
    for loop in cycles:
        pts = [proj(s) for s in loop]
        long_orbit = len(loop) > 12
        if long_orbit:
            ax.plot([p[0] for p in pts], [p[1] for p in pts],
                    color="#1f77b4", alpha=0.85, lw=1.8, zorder=3,
                    label=None if labelled else f"limit cycle(s) ({total_cycles})")
            labelled = True
            step = max(1, len(pts) // 8)
            for i in range(0, len(pts) - 1, step):
                (x0, y0), (x1, y1) = pts[i], pts[i + 1]
                ax.add_patch(FancyArrowPatch(
                    (x0, y0), (x1, y1), arrowstyle="-|>", mutation_scale=12,
                    color="#1f77b4", alpha=0.9, lw=0, zorder=4,
                    shrinkA=0, shrinkB=0))
        else:
            for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
                ax.add_patch(FancyArrowPatch(
                    (x0, y0), (x1, y1), arrowstyle="-|>", mutation_scale=12,
                    color="#1f77b4", alpha=0.8, lw=1.4,
                    shrinkA=3, shrinkB=3, zorder=3))
            for (x, y) in pts[:-1]:
                cyc_pts_x.append(x)
                cyc_pts_y.append(y)
    if cyc_pts_x:
        ax.scatter(cyc_pts_x, cyc_pts_y, s=55, c="#1f77b4",
                   edgecolors="white", linewidths=0.7, zorder=5,
                   label=None if labelled else f"cycle members ({total_cycles} cycle(s))")


def draw_panel(ax, model, ch, states, fixed, cycles, total_cycles, edges=None):
    xvar, yvar = ch["x"], ch["y"]
    cvar, svar = ch["color"], ch["shape"]

    def proj(st):
        x = ordinal(model, xvar, st[xvar["name"]])
        y = ordinal(model, yvar, st[yvar["name"]]) if yvar else 0.0
        return x, y

    _draw_edges(ax, proj, edges)

    # background basin: sampled states, encoded by a CATEGORICAL color and/or
    # marker SHAPE. The derived attractor coloring (red/blue) is drawn on top and
    # untouched — color/shape here only reveal the basin's categorical structure.
    if states:
        if cvar is not None or svar is not None:
            _scatter_categorical(ax, model, states, proj, cvar, svar)
        else:
            bx = [proj(s)[0] for s in states]
            by = [proj(s)[1] for s in states]
            ax.scatter(bx, by, s=34, c="#9aa0b5", alpha=0.85,
                       edgecolors="white", linewidths=0.4,
                       zorder=1, label=f"sampled states ({len(states)})")

    _draw_cycles(ax, proj, cycles, total_cycles)

    if fixed:
        fx = [proj(s)[0] for s in fixed]
        fy = [proj(s)[1] for s in fixed]
        ax.scatter(fx, fy, s=160, c="#d62728", marker="*",
                   edgecolors="black", linewidths=1.0, zorder=6,
                   label=f"fixed points ({len(fixed)})")

    ax.set_xlabel(axis_label(xvar))
    ax.set_ylabel(axis_label(yvar) if yvar else "(single-axis projection)")
    decorate_axis(ax, model, xvar, "x")
    if yvar:
        decorate_axis(ax, model, yvar, "y")
    # Place the legend OUTSIDE the axes (upper-left, anchored just past the
    # right edge) so it never overprints plotted markers — notably a fixed-point
    # star that lands in a top-right corner (wc's absorbing state at (10, 3)).
    handles, labels = ax.get_legend_handles_labels()
    if handles:
        ax.legend(loc="upper left", bbox_to_anchor=(1.01, 1.0),
                  fontsize=8, framealpha=0.95, borderaxespad=0.0)
    ax.grid(True, alpha=0.2)


def _scatter_categorical(ax, model, states, proj, cvar, svar):
    """One scatter call per (color-value, shape-value) cell of the background
    basin. Color = hue (excellent for categorical), shape = marker glyph."""
    cvals = _domain(model, cvar) if cvar is not None else [None]
    svals = _domain(model, svar) if svar is not None else [None]
    cmap = {v: CAT_PALETTE[i % len(CAT_PALETTE)] for i, v in enumerate(cvals)}
    smap = {v: CAT_SHAPES[i % len(CAT_SHAPES)] for i, v in enumerate(svals)}
    for cv in cvals:
        for sv in svals:
            pts = [proj(s) for s in states
                   if (cvar is None or s[cvar["name"]] == cv)
                   and (svar is None or s[svar["name"]] == sv)]
            if not pts:
                continue
            bits = []
            if cvar is not None:
                bits.append(f"{_short(cvar['name'])}={_fmt(cvar, cv)}")
            if svar is not None:
                bits.append(f"{_short(svar['name'])}={_fmt(svar, sv)}")
            ax.scatter([p[0] for p in pts], [p[1] for p in pts],
                       s=44, c=cmap[cv] if cvar is not None else "#9aa0b5",
                       marker=smap[sv] if svar is not None else "o",
                       alpha=0.9, edgecolors="white", linewidths=0.4,
                       zorder=1, label=", ".join(bits))


def _super_title(model, ch, mode, fixed, cycles):
    cs = []
    for chan in ("color", "shape", "facet"):
        if ch[chan] is not None:
            cs.append(f"{chan}={_short(ch[chan]['name'])}")
    chan_note = ("   |   " + ", ".join(cs)) if cs else ""
    bits = []
    if fixed:
        bits.append(f"{len(fixed)} fixed point(s)")
    if cycles:
        lens = sorted({len(c) - 1 for c in cycles})
        bits.append(f"{len(cycles)} cycle(s) (period {lens})")
    attr = "  +  ".join(bits) if bits else "no fixed points / short cycles found"
    return f"{model.fsm} — {VIZ_TYPE}   (scan: {mode}{chan_note})\n{attr}"


def decorate_axis(ax, model, var, which):
    if var["kind"] == "enum":
        names = model.enum_variants[var["name"]]
        ticks = list(range(len(names)))
        if which == "x":
            ax.set_xticks(ticks)
            ax.set_xticklabels(names, rotation=30, ha="right", fontsize=8)
        else:
            ax.set_yticks(ticks)
            ax.set_yticklabels(names, fontsize=8)
    elif var["kind"] == "bool":
        if which == "x":
            ax.set_xticks([0, 1])
            ax.set_xticklabels(["false", "true"], fontsize=8)
        else:
            ax.set_yticks([0, 1])
            ax.set_yticklabels(["false", "true"], fontsize=8)


def placeholder(ax, title, reason):
    ax.set_title(title, fontsize=14, fontweight="bold")
    ax.text(0.5, 0.5, f"N/A\n{reason}", transform=ax.transAxes,
            ha="center", va="center", fontsize=13, color="#999999")
    ax.set_xticks([])
    ax.set_yticks([])


def finish(fig, out_path):
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    fig.tight_layout()
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def main():
    if len(sys.argv) != 4:
        print("usage: render_fixedpoint_map.py <smt2> <schema> <out.png>",
              file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])


if __name__ == "__main__":
    main()
