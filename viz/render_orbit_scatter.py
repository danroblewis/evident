#!/usr/bin/env python3
"""render_orbit_scatter — the honest discrete-time orbit view, on visual channels.

Plot a trajectory as DISCRETE DOTS (not connected) in two chosen state axes.
Each dot is one sampled state of the FSM at one tick; the gap between dots is
the actual jump the difference equation makes. For limit cycles the dots trace
a closed loop; for fixed points they pile up.

Channels (Cleveland-McGill / Mackinlay), mapping ranked vars by type:
  - POSITION (x, y): the top-ranked vars. Numeric pair when the system is
    continuous (vanderpol); else assign_channels picks the most expressive pair,
    enum -> ordinal, bool -> 0/1.
  - COLOR: a CATEGORICAL var (mode / dispensed / has_torch) — color reads
    categories best. When NO categorical var is free (pure-numeric vanderpol),
    KEEP the derived time/depth gradient — a coarse quantitative gradient is the
    one good quantitative use of color.
  - FACET: a low-cardinality categorical (<= 5 values) not already on an axis ->
    one panel per value. The honest way to ADD a dimension for a high-D model.
  - SIZE: a secondary numeric var, when one is free and helps.

The plot reads from its AXES alone; color/size/facet only ENHANCE.

Usage:
    python3 viz/render_orbit_scatter.py <smt2> <schema> <out_path>
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.lines import Line2D

from evident_viz import load

# Channel selection + orbit construction (the data layer) live in the
# sibling build module; this file keeps the drawing + dispatch.
from orbit_scatter_build import (
    _project, _axis_label, _cat_key, _select_channels, _build_orbits,
    _offset_collisions,
)
# Interactive hover-overlay sidecar (#184 increment 3). orbit_scatter ALWAYS saves
# with bbox_inches="tight", so the per-point fractions use the tight-bbox mapping.
from overlay_points import write_points, tight_fraction


# ---- axis tick labelling -----------------------------------------------------
def _label_axis(model, ax, var, which):
    setter, getter = ((ax.set_xticks, ax.set_xticklabels) if which == "x"
                      else (ax.set_yticks, ax.set_yticklabels))
    if var["kind"] == "enum":
        variants = model.enum_variants[var["name"]]
        setter(range(len(variants)))
        getter(variants, rotation=30 if which == "x" else 0,
               ha="right" if which == "x" else "right", fontsize=8)
    elif var["kind"] == "bool":
        setter([0, 1])
        getter(["false", "true"], fontsize=9)


# ---- main render -------------------------------------------------------------
def render(smt2_path, schema_path, out_path):
    model = load(smt2_path, schema_path)
    title = f"{model.fsm} — orbit_scatter"

    xvar, yvar, color_var, facet_var = _select_channels(model)
    if xvar is None:
        fig, ax = plt.subplots(figsize=(8, 7))
        ax.text(0.5, 0.5, "N/A for state: no state variables",
                ha="center", va="center", fontsize=14)
        ax.set_title(title)
        ax.axis("off")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        write_points(out_path, [])           # no orbit / degenerate → overlay no-ops
        return

    orbits, point_time, mode = _build_orbits(model, xvar, yvar)

    # Faceting cuts the plotted points across panels. That is only honest when the
    # panels are INDEPENDENT runs (multi-seed 'numeric' mode). For a single
    # autonomous orbit or the reachable set, the points form ONE connected
    # sequence threaded through changing state — faceting by a var that flips along
    # the orbit (grep's state.done) would slice the climb into separate panels and
    # crop each to its tail. Never facet those modes; keep the whole orbit on one axis.
    if mode != "numeric":
        facet_var = None

    if not orbits:
        fig, ax = plt.subplots(figsize=(8, 7))
        ax.text(0.5, 0.5,
                "N/A: no orbit produced\n(no initial state and no successor)",
                ha="center", va="center", fontsize=13)
        ax.set_title(title)
        ax.axis("off")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        write_points(out_path, [])           # no orbit / degenerate → overlay no-ops
        return

    # HONEST degenerate guard: a scatter over a finite handful of states is not a
    # meaningful orbit view — the program halts at / sits on a fixed point. Render an
    # N/A card with the real reachable count instead of inflating it into a plane.
    n_states = sum(len(o) for o in orbits)
    if n_states <= 2:
        fig, ax = plt.subplots(figsize=(8, 7))
        ax.text(0.5, 0.5,
                f"N/A — reachable set is {n_states} "
                f"state{'s' if n_states != 1 else ''} (fixed point /\n"
                "immediate halt); an orbit scatter is not meaningful.",
                ha="center", va="center", fontsize=13)
        ax.set_title(title)
        ax.axis("off")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        write_points(out_path, [])           # no orbit / degenerate → overlay no-ops
        return

    # Flatten orbit points, carrying (x, y, time, seed-index, state) per point.
    pts = []
    for oi, orb in enumerate(orbits):
        for ti, st in enumerate(orb):
            pts.append({
                "x": _project(model, xvar, st[xvar["name"]]),
                "y": _project(model, yvar, st[yvar["name"]]),
                "t": point_time[oi][ti],
                "seed": oi,
                "first": ti == 0,
                "st": st,
            })

    # Record the true (un-jittered) grid coordinate per point — axis limits must
    # frame the WHOLE orbit on its real integer/categorical grid, not the nudged
    # display coords.
    for p in pts:
        p["gx"], p["gy"] = p["x"], p["y"]

    # HONEST degenerate-axis guard. An orbit scatter reads from its two axes; if an
    # axis is CONSTANT over every plotted point, the projection has collapsed onto a
    # line (or a single node) and the picture is misleading — it looks like a real
    # orbit, but one whole dimension never moves. This is the selector handing us a
    # constant coordinate (life's state.pop pinned at 3; randomwalk's v3/v4 both
    # pinned at 0); the renderer can't pick better axes, but it MUST NOT dress a
    # flat line up as dynamics. Render an N/A card naming the collapsed axis instead.
    x_distinct = len({p["gx"] for p in pts})
    y_distinct = len({p["gy"] for p in pts})
    if x_distinct <= 1 or y_distinct <= 1:
        flat = []
        if x_distinct <= 1:
            flat.append(f'{xvar["name"]} (x) = {pts[0]["st"][xvar["name"]]}')
        if y_distinct <= 1:
            flat.append(f'{yvar["name"]} (y) = {pts[0]["st"][yvar["name"]]}')
        both = x_distinct <= 1 and y_distinct <= 1
        detail = ("both chosen axes are constant — the orbit is a single point"
                  if both else
                  "the chosen y-axis is constant" if y_distinct <= 1 else
                  "the chosen x-axis is constant")
        fig, ax = plt.subplots(figsize=(8, 7))
        ax.text(0.5, 0.5,
                f"N/A — {detail} over all {len(pts)} plotted states.\n"
                f"({'; '.join(flat)})\n"
                "An orbit scatter on these axes would be a misleading flat line.",
                ha="center", va="center", fontsize=12)
        ax.set_title(title)
        ax.axis("off")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        write_points(out_path, [])           # no orbit / degenerate → overlay no-ops
        return

    discrete = not (xvar["kind"] in ("int", "real")
                    and yvar["kind"] in ("int", "real"))
    # On a discrete (int / enum / bool) axis the data LIVES on integer grid lines;
    # continuous jitter would smear points off-grid into fabricated fractional
    # positions (vending balance landing at -0.1, 1.07, …) and even below an
    # integer axis's true minimum. So: leave distinct points exactly on their grid
    # node, and only when several points genuinely COLLIDE on the same node fan
    # them out in a tiny ring around that node — visible, but still centered on the
    # real coordinate and never pushed below the axis floor.
    if discrete:
        _offset_collisions(pts, xvar, yvar)

    # ---- panels (facet) ------------------------------------------------------
    if facet_var is not None:
        panel_vals = (model.enum_variants[facet_var["name"]]
                      if facet_var["kind"] == "enum" else [False, True])
        # keep only values that actually occur
        present = {st_v for st_v in
                   (p["st"][facet_var["name"]] for p in pts)}
        panel_vals = [v for v in panel_vals if v in present] or list(present)
    else:
        panel_vals = [None]

    ncols = len(panel_vals)
    fig, axes = plt.subplots(1, ncols, figsize=(6.6 * ncols if ncols > 1 else 8, 7),
                             squeeze=False, sharex=True, sharey=True)
    axes = axes[0]

    # ---- color scheme: categorical var, or a time/depth gradient -------------
    cmap = plt.get_cmap("viridis")
    color_legend = []
    if color_var is not None:
        # categorical: one hue per value
        if color_var["kind"] == "enum":
            cvals = model.enum_variants[color_var["name"]]
        else:
            cvals = [False, True]
        catmap = plt.get_cmap("tab10")
        cat_color = {cv: catmap(i % 10) for i, cv in enumerate(cvals)}
        for cv in cvals:
            color_legend.append(
                Line2D([0], [0], marker="o", color="w",
                       markerfacecolor=cat_color[cv], markeredgecolor="black",
                       markersize=8, label=_cat_key(model, color_var, cv)))
        color_label = color_var["name"]
    else:
        max_t = max((p["t"] for p in pts), default=1) + 1
        color_label = "steps from start" if mode == "reachable" else "tick (time)"

    scatter_for_bar = None
    overlay = []   # (ax, data_x, data_y, full_state) per plotted point — hover sidecar
    for pi, (ax, pv) in enumerate(zip(axes, panel_vals)):
        panel_pts = [p for p in pts
                     if pv is None or p["st"][facet_var["name"]] == pv]
        overlay += [(ax, p["x"], p["y"], p["st"]) for p in panel_pts]
        if color_var is not None:
            colors = [cat_color[p["st"][color_var["name"]]] for p in panel_pts]
            ax.scatter([p["x"] for p in panel_pts], [p["y"] for p in panel_pts],
                       c=colors, s=46, edgecolors="black", linewidths=0.4,
                       alpha=0.85, zorder=3)
        else:
            sc = ax.scatter(
                [p["x"] for p in panel_pts], [p["y"] for p in panel_pts],
                c=[p["t"] for p in panel_pts], cmap=cmap, vmin=0,
                vmax=max_t - 1 if max_t > 1 else 1,
                s=46, edgecolors="black", linewidths=0.4, alpha=0.85, zorder=3)
            scatter_for_bar = sc
        # seed / start rings
        for p in panel_pts:
            if p["first"]:
                ax.scatter([p["x"]], [p["y"]], s=160, facecolors="none",
                           edgecolors="red", linewidths=1.6, zorder=4)

        ax.set_xlabel(_axis_label(xvar))
        if pi == 0:
            ax.set_ylabel(_axis_label(yvar))
        ax.grid(True, linestyle=":", alpha=0.4)
        _label_axis(model, ax, xvar, "x")
        _label_axis(model, ax, yvar, "y")
        # Frame the WHOLE orbit on its true grid. Limits come from EVERY plotted
        # point's un-jittered grid coordinate (`gx`/`gy`) across ALL panels — not the
        # surviving subset of one panel — so the autonomous walk's full visited
        # sequence (grep's (0,0)→(1,1)→(2,2) climb) is never cropped to its tail.
        for which, var, gk in (("x", xvar, "gx"), ("y", yvar, "gy")):
            if var["kind"] == "enum":
                lo, hi = 0, len(model.enum_variants[var["name"]]) - 1
                pad = 0.5
            elif var["kind"] == "bool":
                lo, hi, pad = 0, 1, 0.5
            else:
                # Numeric axis: span all real grid values. Only the multi-seed
                # 'numeric' mode (vanderpol spiral) needs the IQR fence to reject a
                # ±1e6 sentinel; an autonomous/reachable orbit is framed whole.
                vals = sorted(p[gk] for p in pts)
                if len(vals) < 2:
                    continue
                if mode == "numeric":
                    nn = len(vals)
                    q1, q3 = vals[nn // 4], vals[(3 * nn) // 4]
                    if q3 > q1:
                        fence = 3 * (q3 - q1)
                        vals = [v for v in vals if q1 - fence <= v <= q3 + fence] or vals
                lo, hi = min(vals), max(vals)
                pad = (hi - lo) * 0.08 if hi > lo else 1.0
            (ax.set_xlim if which == "x" else ax.set_ylim)(lo - pad, hi + pad)
        if pv is not None:
            ax.set_title(f'{facet_var["name"]} = {_cat_key(model, facet_var, pv)}',
                         fontsize=10)

    # ---- color key: colorbar (gradient) or legend (categorical) --------------
    if color_var is None and scatter_for_bar is not None:
        cbar = fig.colorbar(scatter_for_bar, ax=list(axes), pad=0.02)
        cbar.set_label(color_label)

    # ---- legend assembled on the last axis -----------------------------------
    legend_bits = list(color_legend)
    legend_bits.append(
        Line2D([0], [0], marker="o", color="w", markerfacecolor="none",
               markeredgecolor="red", markersize=11, label="seed / start"))
    if mode == "numeric" and len(orbits) > 1:
        legend_bits.append(
            Line2D([0], [0], marker="o", color="w", markerfacecolor="gray",
                   markeredgecolor="black", markersize=8,
                   label=f"{len(orbits)} numeric seeds"))
    title_color = (f"color = {color_var['name']} [{color_var['kind']}]"
                   if color_var is not None else f"color = {color_label}")
    axes[-1].legend(handles=legend_bits, loc="best", fontsize=8, framealpha=0.9,
                    title=title_color, title_fontsize=8)

    subtitle_bits = [{"numeric": "numeric: multiple seeds → attractor",
                      "autonomous": "autonomous orbit from initial state",
                      "reachable": f"{len(pts)} reachable states, by steps from start",
                      }[mode]]
    if facet_var is not None:
        subtitle_bits.append(f"faceted by {facet_var['name']}")
    fig.suptitle(f"{title}\n{' · '.join(subtitle_bits)}", fontsize=12)

    # Map each plotted point's data coords → tight-bbox fraction BEFORE saving
    # (tight_fraction's draw() finalizes the same layout savefig uses).
    points = tight_fraction(fig, overlay)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, points)


def main(argv):
    if len(argv) != 4:
        print("usage: render_orbit_scatter.py <smt2> <schema> <out_path>",
              file=sys.stderr)
        return 2
    render(argv[1], argv[2], argv[3])
    print(f"wrote {argv[3]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
