#!/usr/bin/env python3
"""render_orbit_scatter — the all-initial-conditions orbit view, on visual channels.

PRIMARY mode (the diagram-review fix): sample MANY initial conditions, not one seeded
orbit. We seed from the model's bounded state space (`full_state_graph` discrete /
`proven_range` grid continuous, via the shared `time_series_ensemble` seeder), forward-
simulate each with the EXISTING successor relation (clamping divergence), DROP the first
few transient ticks, and scatter the resulting attractor points. Each orbit is tagged with
the attractor it settles into, so a MULTI-ATTRACTOR system (bistable) shows BOTH basins —
the old single-from-init orbit only ever showed the seed's basin.

For a SINGLE numeric var, x-vs-x is a useless 45° diagonal, so we DELAY-EMBED: plot
(x_t, x_{t+1}). The scatter then traces the map's graph — the logistic parabola, a fixed
point as one dot on the diagonal, a 2-cycle as two mirrored off-diagonal dots.

For ≥2 numeric vars we scatter the two principal vars (the honest phase plane).

FALLBACK (only when no honest ensemble box exists — an unbounded carried var): the old
single-orbit construction (autonomous chain / reachable set), faithfully flagged.

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

from orbit_scatter_build import (
    _project, _axis_label, _cat_key, _select_channels, _ensemble_orbits,
    fallback_orbits, is_delay_embed, basins_separable, _offset_collisions,
)
from overlay_points import write_points, tight_fraction
from axis_select import resolve_axes, write_axes


def _na_card(out_path, title, msg, fontsize=13):
    fig, ax = plt.subplots(figsize=(8, 7))
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=fontsize)
    ax.set_title(title)
    ax.axis("off")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, [])


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


# ---- point construction ------------------------------------------------------
def _ensemble_points(model, orbits, tags, xvar, yvar, delay):
    """Flatten the transient-trimmed ensemble orbits into plotted points. For the
    single-numeric-var DELAY embedding each consecutive pair (x_t, x_{t+1}) is one point;
    otherwise each state is one point at (xvar, yvar). Every point carries its attractor
    tag (`attr`) so multi-attractor systems color both basins."""
    pts = []
    for oi, (orb, tag) in enumerate(zip(orbits, tags)):
        if delay:
            for ti in range(len(orb) - 1):
                pts.append({
                    "x": _project(model, xvar, orb[ti][xvar["name"]]),
                    "y": _project(model, xvar, orb[ti + 1][xvar["name"]]),
                    "seed": oi, "attr": tag, "first": ti == 0, "st": orb[ti],
                })
        else:
            for ti, st in enumerate(orb):
                pts.append({
                    "x": _project(model, xvar, st[xvar["name"]]),
                    "y": _project(model, yvar, st[yvar["name"]]),
                    "seed": oi, "attr": tag, "first": ti == 0, "st": st,
                })
    return pts


def _fallback_points(model, orbits, point_time, xvar, yvar, delay):
    """Flatten the single-orbit fallback into plotted points, carrying `t` for the
    time/depth color gradient (no attractor tag — one orbit, one basin)."""
    pts = []
    for oi, orb in enumerate(orbits):
        if delay:
            for ti in range(len(orb) - 1):
                pts.append({
                    "x": _project(model, xvar, orb[ti][xvar["name"]]),
                    "y": _project(model, xvar, orb[ti + 1][xvar["name"]]),
                    "t": point_time[oi][ti], "seed": oi, "attr": None,
                    "first": ti == 0, "st": orb[ti],
                })
        else:
            for ti, st in enumerate(orb):
                pts.append({
                    "x": _project(model, xvar, st[xvar["name"]]),
                    "y": _project(model, yvar, st[yvar["name"]]),
                    "t": point_time[oi][ti], "seed": oi, "attr": None,
                    "first": ti == 0, "st": st,
                })
    return pts


def _axis_var(yvar, xvar, delay):
    """The var whose grid/label drives the y-axis: xvar (delayed: y is x's next value)
    or yvar (the second principal var)."""
    return xvar if delay else yvar


def _set_limits(model, ax, pts, xvar, yvar, delay):
    """Frame the whole scatter on its true grid; an IQR fence rejects a divergence
    sentinel on a continuous axis."""
    for which, var, gk in (("x", xvar, "gx"), ("y", _axis_var(yvar, xvar, delay), "gy")):
        if var["kind"] == "enum":
            lo, hi, pad = 0, len(model.enum_variants[var["name"]]) - 1, 0.5
        elif var["kind"] == "bool":
            lo, hi, pad = 0, 1, 0.5
        else:
            vals = sorted(p[gk] for p in pts)
            if len(vals) < 2:
                continue
            nn = len(vals)
            q1, q3 = vals[nn // 4], vals[(3 * nn) // 4]
            if q3 > q1:
                fence = 3 * (q3 - q1)
                vals = [v for v in vals if q1 - fence <= v <= q3 + fence] or vals
            lo, hi = min(vals), max(vals)
            pad = (hi - lo) * 0.08 if hi > lo else 1.0
        (ax.set_xlim if which == "x" else ax.set_ylim)(lo - pad, hi + pad)


# ---- orbit construction phase ------------------------------------------------
def _build_orbits(model, out_path, title, xvar, yvar, delay):
    """PRIMARY: the all-initial-conditions ensemble. Falls back only when there is no honest
    bounded box to seed (an unbounded carried var) — then the single-orbit construction.
    Returns (orbits, tags, mode, pts, n_attractors, na_note); na_note is a string (with the
    N/A card already written) when no orbit/points exist, in which case the rest is None."""
    ens = _ensemble_orbits(model)
    if ens is not None:
        orbits, tags = ens
        mode = "ensemble"
        pts = _ensemble_points(model, orbits, tags, xvar, yvar, delay)
        n_attractors = len({t for t in tags if t is not None})
    else:
        orbits, point_time, mode = fallback_orbits(model)
        tags = None
        if not orbits:
            _na_card(out_path, title,
                     "N/A: no orbit produced\n(no initial state and no successor)")
            return None, None, None, None, None, "orbit_scatter: N/A (no orbit)"
        pts = _fallback_points(model, orbits, point_time, xvar, yvar, delay)
        n_attractors = 0

    if not pts:
        _na_card(out_path, title, "N/A: orbit produced no plottable points")
        return None, None, None, None, None, "orbit_scatter: N/A (no points)"

    # Record the true (un-jittered) grid coordinate per point.
    for p in pts:
        p["gx"], p["gy"] = p["x"], p["y"]
    return orbits, tags, mode, pts, n_attractors, None


# ---- degenerate-axis guard ---------------------------------------------------
def _degenerate_axis_na(out_path, title, pts, xvar, yvar, delay):
    """HONEST degenerate-axis guard. The scatter reads from its two axes; if an axis is
    CONSTANT over every plotted point the projection collapsed onto a line. For the DELAY
    embedding a constant axis means the orbit is genuinely a single fixed point (x_t ==
    x_{t+1} everywhere) — still report it, but as a fixed point, not fake dynamics. Returns
    a note string (and writes the N/A card) when degenerate, else None."""
    x_distinct = len({p["gx"] for p in pts})
    y_distinct = len({p["gy"] for p in pts})
    if x_distinct <= 1 and y_distinct <= 1:
        v0 = pts[0]["st"][xvar["name"]]
        _na_card(out_path, title,
                 f"N/A — every orbit settles to the single fixed point "
                 f"{xvar['name']} = {v0}.\nThere is no orbit to scatter.", 12)
        return "orbit_scatter: N/A (single fixed point)"
    if (x_distinct <= 1 or y_distinct <= 1) and not delay:
        flat = (f'{xvar["name"]} (x) = {pts[0]["st"][xvar["name"]]}' if x_distinct <= 1
                else f'{yvar["name"]} (y) = {pts[0]["st"][yvar["name"]]}')
        _na_card(out_path, title,
                 f"N/A — the chosen {'x' if x_distinct <= 1 else 'y'}-axis is constant "
                 f"over all {len(pts)} plotted states.\n({flat})\n"
                 "An orbit scatter on these axes would be a misleading flat line.", 12)
        return "orbit_scatter: N/A (collapsed axis)"
    return None


# ---- axes / legend / subtitle finalization -----------------------------------
def _finalize_axes_and_legend(fig, ax, model, pts, xvar, yvar, yaxis_var, delay,
                              color_legend, color_label, scatter_for_bar,
                              show_seed_rings, mode, orbits, n_attractors, color_by_attr, title):
    """Axis labels/ticks/limits, the optional colorbar, the assembled legend, and the
    mode-specific subtitle — the static framing that's identical regardless of color mode."""
    ax.set_xlabel(_axis_label(xvar) + (r"   ($x_t$)" if delay else ""))
    ax.set_ylabel((_axis_label(xvar) + r"   ($x_{t+1}$)") if delay
                  else _axis_label(yvar))
    ax.grid(True, linestyle=":", alpha=0.4)
    _label_axis(model, ax, xvar, "x")
    _label_axis(model, ax, yaxis_var, "y")
    _set_limits(model, ax, pts, xvar, yvar, delay)

    # ---- color key ----------------------------------------------------------
    if scatter_for_bar is not None:
        cbar = fig.colorbar(scatter_for_bar, ax=ax, pad=0.02)
        cbar.set_label(color_label)

    legend_bits = list(color_legend)
    if show_seed_rings:
        legend_bits.append(Line2D([0], [0], marker="o", color="w", markerfacecolor="none",
                                  markeredgecolor="red", markersize=11, label="seed / start"))
    if delay:
        legend_bits.append(Line2D([0], [0], linestyle="--", color="gray",
                                  label="x_t = x_{t+1}"))
    title_color = f"color = {color_label}"
    ax.legend(handles=legend_bits, loc="best", fontsize=8, framealpha=0.9,
              title=title_color, title_fontsize=8)

    embed = "delay-embedded (x_t vs x_{t+1})" if delay else "two principal vars"
    subtitle = {
        "ensemble": (f"all initial conditions: {len(orbits)} orbits, transients dropped"
                     + (f" · {n_attractors} basins" if color_by_attr else "")
                     + f" · {embed}"),
        "autonomous": f"single autonomous orbit (unbounded — no ensemble box) · {embed}",
        "reachable": f"{len(pts)} reachable states (unbounded fallback) · {embed}",
    }[mode]
    fig.suptitle(f"{title}\n{subtitle}", fontsize=12)


# ---- color / scatter-draw phase ---------------------------------------------
def _draw_colored_scatter(ax, model, pts, mode, orbits, tags, xvar, yvar, delay, color_var):
    """Pick the coloring strategy and draw the scatter. Returns
    (color_legend, color_label, scatter_for_bar, color_by_attr). The strategy:
    ATTRACTOR (a genuine, spatially-separate basin partition — bistable walls),
    else CATEGORICAL (a color var on the single-orbit fallback), else SEED (the
    fan of initial conditions for a single-attractor ensemble), else TIME gradient."""
    cmap = plt.get_cmap("viridis")
    color_legend = []
    # Color BY ATTRACTOR only for a genuine, SPATIALLY-SEPARATE basin partition (a bistable's
    # walls at 0 and 6) — `basins_separable` rejects a chaotic map's near-identical clustered
    # tags (one strange attractor, not N basins). Otherwise fall through to seed coloring.
    color_by_attr = (mode == "ensemble"
                     and basins_separable(model, orbits, tags, xvar, yvar, delay))
    scatter_for_bar = None

    if color_by_attr:
        # The headline: color each point by the ATTRACTOR its orbit settled into, so a
        # bistable shows both basins as distinct hues. Tags are opaque keys → stable order.
        tag_order = sorted({p["attr"] for p in pts}, key=lambda t: str(t))
        catmap = plt.get_cmap("tab10")
        attr_color = {t: catmap(i % 10) for i, t in enumerate(tag_order)}
        ax.scatter([p["x"] for p in pts], [p["y"] for p in pts],
                   c=[attr_color[p["attr"]] for p in pts], s=46,
                   edgecolors="black", linewidths=0.4, alpha=0.85, zorder=3)
        for i, t in enumerate(tag_order):
            color_legend.append(Line2D([0], [0], marker="o", color="w",
                                       markerfacecolor=attr_color[t], markeredgecolor="black",
                                       markersize=8, label=f"basin {i + 1}"))
        color_label = "attractor (basin)"
    elif color_var is not None and mode != "ensemble":
        cvals = (model.enum_variants[color_var["name"]]
                 if color_var["kind"] == "enum" else [False, True])
        catmap = plt.get_cmap("tab10")
        cat_color = {cv: catmap(i % 10) for i, cv in enumerate(cvals)}
        ax.scatter([p["x"] for p in pts], [p["y"] for p in pts],
                   c=[cat_color[p["st"][color_var["name"]]] for p in pts], s=46,
                   edgecolors="black", linewidths=0.4, alpha=0.85, zorder=3)
        for cv in cvals:
            color_legend.append(Line2D([0], [0], marker="o", color="w",
                                       markerfacecolor=cat_color[cv], markeredgecolor="black",
                                       markersize=8, label=_cat_key(model, color_var, cv)))
        color_label = color_var["name"]
    elif mode == "ensemble":
        # Single attractor (or untagged): color by SEED so the fan of initial conditions is
        # visible — every orbit converging to the one attractor reads as its own track.
        nseed = max((p["seed"] for p in pts), default=0) + 1
        sc = ax.scatter([p["x"] for p in pts], [p["y"] for p in pts],
                        c=[p["seed"] for p in pts], cmap=cmap, vmin=0,
                        vmax=nseed - 1 if nseed > 1 else 1, s=46,
                        edgecolors="black", linewidths=0.4, alpha=0.85, zorder=3)
        scatter_for_bar = sc
        color_label = "initial condition (seed #)"
    else:
        max_t = max((p["t"] for p in pts), default=1) + 1
        sc = ax.scatter([p["x"] for p in pts], [p["y"] for p in pts],
                        c=[p["t"] for p in pts], cmap=cmap, vmin=0,
                        vmax=max_t - 1 if max_t > 1 else 1, s=46,
                        edgecolors="black", linewidths=0.4, alpha=0.85, zorder=3)
        scatter_for_bar = sc
        color_label = "steps from start" if mode == "reachable" else "tick (time)"

    return color_legend, color_label, scatter_for_bar, color_by_attr


# ---- main render -------------------------------------------------------------
def render(smt2_path, schema_path, out_path, x_var=None, y_var=None):
    model = load(smt2_path, schema_path)
    title = f"{model.fsm} — orbit_scatter"

    xvar, yvar, color_var, facet_var = _select_channels(model)
    if xvar is None:
        _na_card(out_path, title, "N/A for state: no state variables", 14)
        return "orbit_scatter: N/A (no state variables)"
    # #445: honor an explicit axis request, falling back to _select_channels' auto-pick. A
    # single-numeric system delay-embeds (x_t vs x_{t+1}); overriding makes sense only when 2
    # numeric axes exist, so resolve against them and leave the delay case to the auto-pick.
    if yvar is not None:
        xvar, yvar, axinfo = resolve_axes(model, x_var, y_var, xvar, yvar)
        write_axes(out_path, axinfo)

    delay = is_delay_embed(xvar, yvar)

    orbits, tags, mode, pts, n_attractors, na = _build_orbits(
        model, out_path, title, xvar, yvar, delay)
    if na is not None:
        return na

    na = _degenerate_axis_na(out_path, title, pts, xvar, yvar, delay)
    if na is not None:
        return na

    yaxis_var = _axis_var(yvar, xvar, delay)
    discrete = not (xvar["kind"] in ("int", "real")
                    and yaxis_var["kind"] in ("int", "real"))
    if discrete:
        _offset_collisions(pts, xvar, yaxis_var)

    # ---- color: attractor (ensemble multi-basin), categorical, or time gradient ----
    fig, ax = plt.subplots(figsize=(8, 7))
    color_legend, color_label, scatter_for_bar, color_by_attr = _draw_colored_scatter(
        ax, model, pts, mode, orbits, tags, xvar, yvar, delay, color_var)

    # seed / start rings — only for a SMALL ensemble (a handful of orbits, where marking each
    # start aids reading). Across a dense fan (a chaotic map's 64 orbits) a ring per orbit
    # just carpets the attractor in red, so we drop them and let the scatter speak.
    show_seed_rings = len(orbits) <= 12
    if show_seed_rings:
        for p in pts:
            if p["first"]:
                ax.scatter([p["x"]], [p["y"]], s=160, facecolors="none",
                           edgecolors="red", linewidths=1.6, zorder=4)

    # ---- diagonal guide on the delay embedding (x_t = x_{t+1} → fixed points) -------
    if delay:
        lo = min(min(p["gx"] for p in pts), min(p["gy"] for p in pts))
        hi = max(max(p["gx"] for p in pts), max(p["gy"] for p in pts))
        ax.plot([lo, hi], [lo, hi], linestyle="--", color="gray",
                linewidth=1.0, alpha=0.6, zorder=1)

    _finalize_axes_and_legend(fig, ax, model, pts, xvar, yvar, yaxis_var, delay,
                              color_legend, color_label, scatter_for_bar,
                              show_seed_rings, mode, orbits, n_attractors, color_by_attr, title)

    overlay = [(ax, p["x"], p["y"], p["st"]) for p in pts]
    points = tight_fraction(fig, overlay)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, points)
    return (f"orbit_scatter: {mode} · {len(orbits)} orbits · "
            f"{n_attractors} attractors · {'delay-embed' if delay else '2-var'}")


def main(argv):
    if len(argv) != 4:
        print("usage: render_orbit_scatter.py <smt2> <schema> <out_path>",
              file=sys.stderr)
        return 2
    note = render(argv[1], argv[2], argv[3])
    print(f"wrote {argv[3]} ({note})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
