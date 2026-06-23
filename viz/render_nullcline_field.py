#!/usr/bin/env python3
"""render_nullcline_field — qualitative-flow / nullcline visualization of an
Evident program's transition relation.

The sign-field IS the qualitative phase-plane analysis read straight off the
transition. Over a grid of two numeric axes we shade the plane by the SIGN of
each component's change and overlay the approximate NULLCLINES (the curves where
a component's change crosses zero); their intersections are the fixed points.

CHANNEL MAPPING (Cleveland-McGill / Mackinlay). This viz STRUCTURALLY needs two
numeric POSITION axes for the sign-field, so the axes come from `numeric_vars`,
not the generic channel assignment. The derived sign-of-change is a genuinely
informative quantitative coloring, so we KEEP it as the panel color rather than
clobbering it with a variable hue.

Three shapes, by how many numeric vars exist:

  * >= 2 numeric (e.g. vanderpol): a single full sign-region plane on the top
    two numeric vars, with both nullclines, a flow quiver, and fixed points.

  * 1 numeric + a low-cardinality categorical (e.g. vending): the honest
    dimension-add is FACET — one sign-field panel per value of the leading
    enum/bool. The plane's X axis is the numeric var; its Y axis is a SECOND
    categorical encoded as an ordinal (bool -> 0/1, enum -> variant index), so a
    single-numeric mixed system still reads as a 2-axis sign-field per mode. We
    color by sign(d numeric) — the one component that has a continuous axis.

  * 0 numeric (purely discrete, e.g. dungeon): a titled N/A placeholder — the
    sign-of-change analysis is undefined without a continuous axis.

The dynamics come ENTIRELY from querying m.successor(...) on pinned seed points —
nothing about the flow is hardcoded.

Usage:
    python3 viz/render_nullcline_field.py <smt2> <schema> <out_path>
"""
import sys
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import ListedColormap
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from evident_viz import load
# Recurrent-extent + sign-grid analysis (the data layer) lives in the sibling
# module; this file keeps the drawing + dispatch.
from nullcline_analysis import axis_extent, _sign_grid, MIN_FIELD_POINTS

VIZ = "nullcline_field"
GRID = 41            # samples per numeric axis


def _short(v):
    n = v if isinstance(v, str) else v["name"]
    return n.split(".")[-1]


# --------------------------------------------------------------------------- #
# discrete / undefined placeholder
# --------------------------------------------------------------------------- #
def placeholder(m, out_path, reason, detail=None):
    import textwrap

    # The variable list can be long (toposort: 12 carried vars). A single line
    # overflows both figure margins and clips. Wrap it onto multiple lines on
    # comma boundaries so every entry stays inside the frame.
    kinds_items = [f"{_short(v)}:{v['kind']}" for v in m.state_vars]
    kinds_line = ", ".join(kinds_items)
    state_lines = textwrap.wrap(kinds_line, width=64,
                                break_long_words=False, break_on_hyphens=False)
    reason_lines = textwrap.wrap(f"N/A for this state: {reason}", width=58,
                                 break_long_words=False, break_on_hyphens=False)

    if detail is None:
        detail = ("nullcline_field needs a numeric axis\n"
                  "(sign of d(var) requires a continuous coordinate).")
    detail_lines = []
    for para in detail.split("\n"):
        detail_lines += textwrap.wrap(para, width=60,
                                      break_long_words=False,
                                      break_on_hyphens=False) or [""]

    # Size the figure to the content so nothing is clipped regardless of how
    # many state vars / how long the detail text is.
    n_lines = len(reason_lines) + len(state_lines) + len(detail_lines)
    fig_h = max(6.0, 2.6 + 0.32 * n_lines)
    fig, ax = plt.subplots(figsize=(9, fig_h))
    ax.axis("off")
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)

    y = 0.88
    ax.text(0.5, y, "\n".join(reason_lines), ha="center", va="top",
            fontsize=14, transform=ax.transAxes)
    y -= 0.08 + 0.05 * len(reason_lines)

    ax.text(0.5, y, "state = [", ha="center", va="top",
            fontsize=10, color="#555", transform=ax.transAxes)
    y -= 0.06
    ax.text(0.5, y, "\n".join(state_lines), ha="center", va="top",
            fontsize=10, color="#555", transform=ax.transAxes)
    y -= 0.04 + 0.045 * len(state_lines)
    ax.text(0.5, y, "]", ha="center", va="top",
            fontsize=10, color="#555", transform=ax.transAxes)
    y -= 0.08

    ax.text(0.5, y, "\n".join(detail_lines), ha="center", va="top",
            fontsize=10, color="#777", transform=ax.transAxes)

    ax.set_title(f"{m.fsm} — {VIZ}", fontsize=13, fontweight="bold")
    fig.subplots_adjust(top=0.90, bottom=0.06, left=0.06, right=0.94)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)



def _numeric_na(m, out_path, xv, vv):
    """Emit the honest N/A card for the numeric sign-field, naming WHY no field is
    meaningful (a constant axis, a monotone counter/clock, or a finite set)."""
    states, _ = m.reachable(limit=3000)
    xn, vn = xv["name"], vv["name"]
    n = len({(s.get(xn), s.get(vn)) for s in states})
    nx = len({s[xn] for s in states if xn in s})
    nv = len({s[vn] for s in states if vn in s})
    if nx <= 1 or nv <= 1:
        const_axis = _short(xv) if nx <= 1 else _short(vv)
        reason = f"{const_axis} is constant on the reachable set"
        detail = ("a sign-field needs two varying coordinates; one axis here\n"
                  "never changes, so there is no phase plane to shade.")
    elif n >= MIN_FIELD_POINTS:
        reason = (f"{_short(xv)} × {_short(vv)} is a monotone counter/clock, "
                  "not a flow")
        detail = ("these axes only ever increase and never revisit a state —\n"
                  "gridding them would fabricate a sign-field the program\n"
                  "never enters (no recurrent / 2D structure).")
    else:
        reason = f"reachable set is {n} point(s) / finite"
        detail = ("a sign-field over a finite reachable set would\n"
                  "fabricate flow-regions the program never enters.")
    placeholder(m, out_path, reason, detail=detail)


def render_numeric(m, out_path, xv, vv):
    # A diverging continuous map (Lotka-Volterra) can blow the reachable extent out
    # to the codec's ±1e18 clamp; guard the whole sample so a non-finite / overflowing
    # axis degrades to the honest N/A card instead of crashing the renderer.
    try:
        extent = axis_extent(m, xv, vv)
        if extent is None or not all(np.isfinite(v) for v in extent):
            _numeric_na(m, out_path, xv, vv)
            return
        xlo, xhi, vlo, vhi = extent
        xs = np.linspace(xlo, xhi, GRID)
        vs = np.linspace(vlo, vhi, GRID)
        DX, DV = _sign_grid(m, xv, vv, xs, vs)
    except (OverflowError, ValueError, FloatingPointError):
        _numeric_na(m, out_path, xv, vv)
        return

    sdx, sdv = np.sign(DX), np.sign(DV)
    region = np.full((GRID, GRID), np.nan)
    mask = ~np.isnan(DX)
    region[mask] = (sdx[mask] >= 0).astype(float) + 2 * (sdv[mask] >= 0).astype(float)

    fig, ax = plt.subplots(figsize=(9, 7.5))
    cmap = ListedColormap(["#cfe8ff", "#ffe0cc", "#d8f0d0", "#f3d6ec"])
    ax.imshow(region, origin="lower", extent=[xlo, xhi, vlo, vhi],
              aspect="auto", cmap=cmap, vmin=-0.5, vmax=3.5, alpha=0.85,
              interpolation="nearest")

    X, V = np.meshgrid(xs, vs)
    try:
        cx = ax.contour(X, V, DX, levels=[0], colors="#1f4fff", linewidths=2.4)
        ax.clabel(cx, fmt={0: f"d{_short(xv)}=0"}, fontsize=9)
    except Exception:
        pass
    try:
        cv = ax.contour(X, V, DV, levels=[0], colors="#d62728", linewidths=2.4)
        ax.clabel(cv, fmt={0: f"d{_short(vv)}=0"}, fontsize=9)
    except Exception:
        pass

    step = max(1, GRID // 18)
    Xq, Vq = X[::step, ::step], V[::step, ::step]
    U, W = DX[::step, ::step], DV[::step, ::step]
    mag = np.hypot(U, W)
    with np.errstate(invalid="ignore", divide="ignore"):
        Un = np.where(mag > 0, U / mag, 0)
        Wn = np.where(mag > 0, W / mag, 0)
    ax.quiver(Xq, Vq, Un, Wn, color="#444", alpha=0.55,
              scale=32, width=0.0026, pivot="mid")

    near0 = mask & (np.abs(DX) <= _tol(DX)) & (np.abs(DV) <= _tol(DV))
    # Only mark fixed points when they are SPARSE — isolated equilibria. If most
    # of the plane reads near-zero, the field is degenerate (a clock/constant that
    # slipped through), and carpeting it with dots both lies and overprints the
    # legend into illegible glyphs; skip the scatter in that case.
    if near0.any() and near0.sum() <= 0.25 * mask.sum():
        ax.scatter(X[near0], V[near0], s=70, facecolor="black",
                   edgecolor="white", zorder=6, label="≈ fixed point")

    ax.set_xlim(xlo, xhi); ax.set_ylim(vlo, vhi)
    ax.set_xlabel(_short(xv)); ax.set_ylabel(_short(vv))
    ax.set_title(f"{m.fsm} — {VIZ}\nsign-regions of (d{_short(xv)}, d{_short(vv)}) "
                 f"+ nullclines", fontsize=13, fontweight="bold")

    from matplotlib.patches import Patch
    leg = [
        Patch(facecolor="#f3d6ec", label="d{0}↑ d{1}↑".format(_short(xv), _short(vv))),
        Patch(facecolor="#d8f0d0", label="d{0}↓ d{1}↑".format(_short(xv), _short(vv))),
        Patch(facecolor="#ffe0cc", label="d{0}↑ d{1}↓".format(_short(xv), _short(vv))),
        Patch(facecolor="#cfe8ff", label="d{0}↓ d{1}↓".format(_short(xv), _short(vv))),
    ]
    ax.legend(handles=leg, loc="upper right", fontsize=8, framealpha=1.0).set_zorder(20)
    ax.grid(alpha=0.15, linewidth=0.5)
    fig.tight_layout()
    fig.savefig(out_path, dpi=120)
    plt.close(fig)


def _tol(arr):
    finite = arr[~np.isnan(arr)]
    if finite.size == 0:
        return 0.0
    scale = np.percentile(np.abs(finite), 60)
    return max(1.0, 0.12 * scale)


# --------------------------------------------------------------------------- #
# faceted mixed sign-field: one panel per categorical value (the dimension-add)
# --------------------------------------------------------------------------- #
def _cat_levels(m, cv):
    """Ordinal levels + tick labels for a categorical axis var. `cv` may be None
    (no second categorical axis exists — thermostat/elevator), in which case there
    are no levels and the caller falls back to a single ['·'] row."""
    if cv is None:
        return [], []
    if cv["kind"] == "bool":
        return [False, True], ["false", "true"]
    if cv["kind"] == "enum":
        variants = m.enum_variants.get(cv["name"], [])
        return list(variants), list(variants)
    return [], []


def _num_range(m, xv):
    """Honest integer range of the numeric var: the values it actually takes
    across REACHABLE states (respects the schema bounds and reachability, rather
    than probing arbitrary out-of-domain prev-states)."""
    try:
        states, _ = m.reachable(limit=2000)
    except Exception:
        states = []
    vals = [s[xv["name"]] for s in states if xv["name"] in s]
    if not vals:
        return list(range(0, 4))
    # A Real-valued numeric var (thermostat's temp) yields float min/max; range()
    # demands ints. Floor/ceil to an integer window so the faceted panel always has
    # an integer x-axis (the sign-field samples one column per integer step).
    import math
    lo, hi = int(math.floor(min(vals))), int(math.ceil(max(vals)))
    if hi - lo > 32:                         # cap the window for sanity
        hi = lo + 32
    if hi <= lo:                             # degenerate (constant axis)
        hi = lo + 1
    return list(range(lo, hi + 1))


def _succ_fill(m, partial):
    """successor() over a partial state: fill any carried var not in `partial`
    with a benign default so _pin_prev is total, then query."""
    st = dict(partial)
    for v in m.carried:
        if v["name"] in st:
            continue
        if v["kind"] == "bool":
            st[v["name"]] = False
        elif v["kind"] == "int":
            st[v["name"]] = 0
        elif v["kind"] == "real":
            st[v["name"]] = 0.0
        elif v["kind"] == "enum":
            st[v["name"]] = m.enum_variants.get(v["name"], ["?"])[0]
        elif v["kind"] == "string":
            st[v["name"]] = ""
    return m.successor(st)


def _draw_facet_panel(ax, m, xv, facet, yv, fval, ylevels, ylabels, xrange,
                      cmap, p, flabels):
    """One faceted sign-field panel: sample sign(d xv) over (xrange × ylevels),
    shade it, and overlay the d(xv)=0 nullcline dots + per-cell flow arrows."""
    grid = np.full((len(ylevels), len(xrange)), np.nan)
    dxg = np.full((len(ylevels), len(xrange)), np.nan)
    for j, yl in enumerate(ylevels):
        for i, xval in enumerate(xrange):
            st = {xv["name"]: xval}
            if fval is not None:
                st[facet["name"]] = fval
            if yv is not None:
                st[yv["name"]] = yl
            nxt = _succ_fill(m, st)
            if nxt is None:
                continue
            d = nxt[xv["name"]] - xval
            dxg[j, i] = d
            grid[j, i] = (1.0 if d > 0 else (-1.0 if d < 0 else 0.0))

    # map sign {-1,0,1} -> {0,1,2} colormap indices
    region = np.where(np.isnan(grid), np.nan, grid + 1.0)
    ax.imshow(region, origin="lower", aspect="auto",
              extent=[-0.5, len(xrange) - 0.5, -0.5, len(ylevels) - 0.5],
              cmap=cmap, vmin=-0.5, vmax=2.5, alpha=0.9,
              interpolation="nearest")

    # nullcline: the d(xv)=0 boundary, drawn as cell edges where sign flips
    # plus an arrow per cell showing the direction of d(xv) along the axis.
    for j in range(len(ylevels)):
        for i in range(len(xrange)):
            d = dxg[j, i]
            if np.isnan(d) or d == 0:
                if not np.isnan(d):
                    ax.scatter([i], [j], s=90, marker="o",
                               facecolor="black", edgecolor="white",
                               zorder=5)
                continue
            dirn = 0.30 if d > 0 else -0.30
            ax.annotate("", xy=(i + dirn, j), xytext=(i - dirn, j),
                        arrowprops=dict(arrowstyle="-|>", color="#333",
                                        lw=1.3, alpha=0.8), zorder=4)

    ax.set_xticks(range(len(xrange)))
    ax.set_xticklabels([str(x) for x in xrange], fontsize=8)
    ax.set_yticks(range(len(ylevels)))
    ax.set_yticklabels(ylabels, fontsize=9)
    ax.set_xlabel(_short(xv))
    if p == 0:
        ax.set_ylabel(_short(yv) if yv is not None else "")
    title = f"{_short(facet)} = {flabels[p]}" if fval is not None else "flow"
    ax.set_title(title, fontsize=11, fontweight="bold")
    ax.set_xlim(-0.5, len(xrange) - 0.5)
    ax.set_ylim(-0.5, len(ylevels) - 0.5)
    ax.grid(alpha=0.12, linewidth=0.5)


def render_faceted(m, out_path, xv, facet, yv):
    """One sign-field panel per value of `facet`. X = numeric `xv`; Y = ordinal
    categorical `yv` (variant/bool index). Color = sign(d xv)."""
    fvals, flabels = _cat_levels(m, facet)
    ylevels, ylabels = _cat_levels(m, yv)
    if not ylevels:                       # no second categorical axis available
        ylevels, ylabels = [0], ["·"]

    xrange = _num_range(m, xv)
    n = len(fvals) or 1
    fig, axes = plt.subplots(1, n, figsize=(4.6 * n, 5.4), squeeze=False)
    axes = axes[0]

    cmap = ListedColormap(["#cfe8ff", "#eeeeee", "#f3d6ec"])   # d↓ / 0 / d↑

    for p, fval in enumerate(fvals or [None]):
        _draw_facet_panel(axes[p], m, xv, facet, yv, fval, ylevels, ylabels,
                          xrange, cmap, p, flabels)

    from matplotlib.patches import Patch
    handles = [
        Patch(facecolor="#f3d6ec", label=f"d{_short(xv)} ↑ (increases)"),
        Patch(facecolor="#eeeeee", label=f"d{_short(xv)} = 0 (nullcline ●)"),
        Patch(facecolor="#cfe8ff", label=f"d{_short(xv)} ↓ (decreases)"),
    ]
    fig.legend(handles=handles, loc="lower center", ncol=3, fontsize=9,
               framealpha=0.9, bbox_to_anchor=(0.5, -0.02))
    ylab = _short(yv) if yv is not None else "—"
    fig.suptitle(f"{m.fsm} — {VIZ}  ·  faceted by {_short(facet)}\n"
                 f"sign of d{_short(xv)} over ({_short(xv)} × {ylab})",
                 fontsize=13, fontweight="bold")
    fig.tight_layout(rect=[0, 0.04, 1, 0.94])
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


# --------------------------------------------------------------------------- #
def main():
    if len(sys.argv) != 4:
        print("usage: render_nullcline_field.py <smt2> <schema> <out_path>",
              file=sys.stderr)
        sys.exit(2)
    smt2, schema, out = sys.argv[1], sys.argv[2], sys.argv[3]
    m = load(smt2, schema)

    nums = m.numeric_vars
    cats = m.categorical_vars

    # Only facet by a variable that stays ~constant within a run (a config/regime
    # set once). A var that CHANGES on the trajectory (e.g. a limit-cycle mode)
    # would split the dynamics across panels and destroy the cycle.
    facet = m.facet_var(max_card=6, max_change=0.25) if len(nums) == 1 else None

    if len(nums) >= 2:
        # canonical two-axis sign-field on the top two numeric vars.
        render_numeric(m, out, nums[0], nums[1])
    elif len(nums) == 1 and facet is not None:
        # MIXED: facet by a suitable ~static categorical; a second categorical
        # (if any, and distinct from the facet) becomes the ordinal Y axis.
        yv = next((c for c in cats if c["name"] != facet["name"]), None)
        render_faceted(m, out, nums[0], facet, yv)
    else:
        reason = ("purely discrete state (no numeric axis)"
                  if not nums else
                  f"{len(nums)} numeric var(s) and no suitable facet variable")
        placeholder(m, out, reason)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
