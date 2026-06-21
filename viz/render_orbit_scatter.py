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
import math

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.lines import Line2D

from evident_viz import load


# ---- projection: any state value -> a float on an axis -----------------------
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


def _cardinality(model, var):
    """How many distinct projected values a var can take (for facet/color choice)."""
    k = var["kind"]
    if k == "bool":
        return 2
    if k == "enum":
        return len(model.enum_variants[var["name"]])
    return None  # numeric: unbounded


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
def _numeric_seeds(model, xvar, yvar):
    """For a 2D numeric system, several initial points so the attractor shows."""
    base = [
        {xvar["name"]: 2800, yvar["name"]: 0},
        {xvar["name"]: 400, yvar["name"]: 0},
        {xvar["name"]: 0, yvar["name"]: 2700},
        {xvar["name"]: -1500, yvar["name"]: 1500},
    ]
    seeds = []
    for s in base:
        full = {v["name"]: s.get(v["name"], 0) for v in model.state_vars}
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
    mode is one of 'numeric' | 'autonomous' | 'reachable'."""
    numeric_2d = (xvar["kind"] in ("int", "real")
                  and yvar["kind"] in ("int", "real")
                  and xvar["name"] != yvar["name"])
    if numeric_2d:
        orbits, times = [], []
        for seed in _numeric_seeds(model, xvar, yvar):
            orb = model.trajectory(start=seed, steps=400)
            if orb:
                orbits.append(orb)
                times.append(list(range(len(orb))))
        return orbits, times, "numeric"

    init = model.initial_state()
    orb = model.trajectory(start=init, steps=400) if init is not None else []
    if len(orb) > 2:
        return [orb], [list(range(len(orb)))], "autonomous"

    states, depths = _reachable_with_depth(model)
    if states:
        return [states], [depths], "reachable"
    return [], [], "autonomous"


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
        return

    orbits, point_time, mode = _build_orbits(model, xvar, yvar)
    if not orbits:
        fig, ax = plt.subplots(figsize=(8, 7))
        ax.text(0.5, 0.5,
                "N/A: no orbit produced\n(no initial state and no successor)",
                ha="center", va="center", fontsize=13)
        ax.set_title(title)
        ax.axis("off")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
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

    discrete = not (xvar["kind"] in ("int", "real")
                    and yvar["kind"] in ("int", "real"))
    # jitter overlapping discrete projections so coincident dots are visible
    if discrete:
        for p in pts:
            h = (hash((round(p["x"], 3), round(p["y"], 3), p["t"], p["seed"]))
                 & 0xffff) / 0xffff
            p["x"] += 0.11 * math.sin(h * 6.283 + p["t"] * 0.7)
            p["y"] += 0.11 * math.cos(h * 6.283 + p["t"] * 0.7)

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
    for pi, (ax, pv) in enumerate(zip(axes, panel_vals)):
        panel_pts = [p for p in pts
                     if pv is None or p["st"][facet_var["name"]] == pv]
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

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


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
