#!/usr/bin/env python3
"""phase_portrait_field.py — the per-panel DRAWING + numeric machinery for the
phase-portrait renderer.

This is the half of the phase portrait that lives in value-space: classifying a
>=2-numeric program by its REACHABLE dynamics, deriving the honest field domain
(reachable extent, perturbed-orbit extent, or the followed-trajectory dwell),
drawing the magnitude-colored vector field + overlaid trajectories (the NUMERIC
panel), and projecting a reachable (sub)graph onto the two axes as real
transition arrows (the DISCRETE panel).

It is path-agnostic: it knows nothing about output files or faceting layout — it
only takes a loaded model `m` (an evident_viz model) and a matplotlib axis, and
draws one panel into it. The renderer (`render_phase_portrait.py`) owns channel
planning, faceting, and orchestration; this module owns one-panel rendering and
the numbers behind it.

Also hosts the value<->plane projection primitives (`_numeric`, `_is_numeric`,
`_axis_ticks`, `_cardinality`) + the data-bbox helper (`_bounds_of`), since the
panel drawing is their primary consumer; the renderer re-imports them for axis
decoration and shared framing.
"""
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

# The vector field grids the successor over a whole plane and follows trajectories from many
# seeds; a chaotic / continuous system (predator-prey, spring) diverges at some points and the
# next z3 literal blows up. These guarded reads SKIP / TRUNCATE there instead of crashing —
# reusing the EXISTING transition, only clamping the read. (phase_portrait_guard.py)
from phase_portrait_guard import (
    diverged as _diverged,
    safe_successor as _safe_successor, safe_trajectory as _safe_trajectory,
)


# ----- value <-> plane coordinate ------------------------------------------
def _numeric(m, var, value):
    """Project a state value onto a real number for the chosen axis."""
    k = var["kind"]
    if k in ("int", "real"):
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        return float(m.enum_variants[var["name"]].index(value))
    if k == "string":
        return 0.0
    return 0.0


def _is_numeric(var):
    return var["kind"] in ("int", "real")


def _axis_ticks(m, var):
    """Categorical tick positions+labels for non-continuous axes, else None."""
    k = var["kind"]
    if k == "bool":
        return [0, 1], ["false", "true"]
    if k == "enum":
        names = m.enum_variants[var["name"]]
        return list(range(len(names))), names
    return None


def _cardinality(m, var):
    """How many distinct projected values an axis can take (its spread)."""
    k = var["kind"]
    if k == "enum":
        return len(m.enum_variants[var["name"]])
    if k == "bool":
        return 2
    return 1000  # numeric: treated as high-resolution


def _robust_span(vals, fallback_lo, fallback_hi, pad=0.06):
    """Robust [lo, hi] of plotted data on one axis: IQR-fence to drop a lone
    off-domain point, then pad lightly. Falls back to the grid box when there's
    too little data to fence. Never returns an inverted or zero-width span."""
    vals = [v for v in vals if v is not None]
    if len(vals) < 4:
        return fallback_lo, fallback_hi
    s = sorted(vals)
    n = len(s)
    q1, q3 = s[n // 4], s[(3 * n) // 4]
    iqr = q3 - q1
    if iqr > 0:
        lof, hif = q1 - 3 * iqr, q3 + 3 * iqr
        s = [v for v in s if lof <= v <= hif] or s
    lo, hi = float(min(s)), float(max(s))
    if hi - lo < 1e-9:
        return lo - 1.0, hi + 1.0
    mrg = (hi - lo) * pad
    return lo - mrg, hi + mrg


def render_numeric_panel(m, ax, axx, axy, pin, draw_colorbar, extent,
                         fit_to_data=False, overlay=None):
    """A magnitude-colored vector field over a grid of pinned numeric points.

    `pin` carries the values of every NON-axis var (facet value, off-axis vars);
    those are fixed while we sweep the two axis vars over a grid. `extent` =
    (xlo, xhi, ylo, yhi) is the REACHABLE-orbit domain (from `_orbit_extent`) —
    we grid within it, never a guessed ±3000 box. Seeds for the overlaid
    trajectories are placed off-origin (the origin is often the fixed point of an
    oscillator, which a centered seed would never leave).

    `fit_to_data` (bounded/reachable mode): after we have the field + trajectory
    points, snap the axis limits to the ROBUST extent of what we actually PLOTTED
    (grid cells with a valid successor, plus the trajectory points), lightly
    padded — so a small-data program never shows a frame far larger than its data
    (lru's dwell sits at k0≈1..40, not the full [0,81] grid: an empty upper grid
    is a framing lie). For the orbit mode (vanderpol) we keep the symmetric box so
    the limit cycle reads centred."""
    nx_, ny_ = axx["name"], axy["name"]

    xlo, xhi, ylo, yhi = extent
    # box-relative geometry: works for a 0-centered orbit box (vanderpol) AND a
    # one-sided reachable box (lru x∈[0,81]) without flagging boundary cells.
    xc, yc = 0.5 * (xlo + xhi), 0.5 * (ylo + yhi)
    xhw, yhw = max(0.5 * (xhi - xlo), 1e-9), max(0.5 * (yhi - ylo), 1e-9)

    n = 21
    xs = np.linspace(xlo, xhi, n)
    ys = np.linspace(ylo, yhi, n)

    GX, GY, U, V, MAG = [], [], [], [], []
    fixed_x, fixed_y = [], []

    for xv in xs:
        for yv in ys:
            state = dict(pin)
            state[nx_] = int(round(xv)) if axx["kind"] == "int" else xv
            state[ny_] = int(round(yv)) if axy["kind"] == "int" else yv
            nxt = _safe_successor(m, state)          # None if this cell's step blows up
            if nxt is None or _diverged(nxt):        # skip a runaway cell, never crash/skew
                continue
            dx = _numeric(m, axx, nxt[nx_]) - xv
            dy = _numeric(m, axy, nxt[ny_]) - yv
            GX.append(xv); GY.append(yv)
            U.append(dx); V.append(dy)
            MAG.append((dx * dx + dy * dy) ** 0.5)
            interior = (abs(xv - xc) < 0.92 * xhw and abs(yv - yc) < 0.92 * yhw)
            if abs(dx) < 1e-9 and abs(dy) < 1e-9 and interior:
                fixed_x.append(xv); fixed_y.append(yv)

    q = None
    if GX:
        GX = np.array(GX); GY = np.array(GY)
        U = np.array(U); V = np.array(V); MAG = np.array(MAG)
        norm = np.where(MAG > 1e-12, MAG, 1.0)
        q = ax.quiver(GX, GY, U / norm, V / norm, MAG, cmap="viridis",
                      angles="xy", scale=30, width=0.0035,
                      pivot="mid", alpha=0.85)
        if draw_colorbar:
            cb = plt.colorbar(q, ax=ax, fraction=0.046, pad=0.04)
            cb.set_label("step magnitude")

    # overlaid trajectories from a spread of seeds placed INSIDE the box (the box
    # may be one-sided, e.g. [0,81], so seed off the centre, not off the origin).
    def _seed(fx, fy):
        return (xc + fx * xhw, yc + fy * yhw)
    seeds = [_seed(0.7, 0.0), _seed(0.1, 0.0), _seed(0.0, 0.7),
             _seed(-0.4, 0.5), _seed(-0.7, 0.0), _seed(0.0, -0.7)]
    cmap = plt.get_cmap("autumn")
    traj_x, traj_y = [], []
    for i, (sx0, sy0) in enumerate(seeds):
        state = dict(pin)
        state[nx_] = int(round(sx0)) if axx["kind"] == "int" else sx0
        state[ny_] = int(round(sy0)) if axy["kind"] == "int" else sy0
        traj = _safe_trajectory(m, state, 400)       # diverging overlay truncates, never crashes
        if len(traj) < 2:
            continue
        px = [_numeric(m, axx, s[nx_]) for s in traj]
        py = [_numeric(m, axy, s[ny_]) for s in traj]
        traj_x += px; traj_y += py
        ax.plot(px, py, "-", lw=1.6, color=cmap(i / max(1, len(seeds) - 1)),
                alpha=0.95, zorder=5)
        ax.plot(px[0], py[0], "o", color="white", mec="black", ms=6, zorder=6)
        if overlay is not None:                # orbit states are the hoverable points (#184)
            overlay.extend((ax, px[j], py[j], traj[j]) for j in range(len(traj)))

    if fixed_x:
        ax.plot(fixed_x, fixed_y, "*", color="red", ms=18, mec="black",
                label="fixed point", zorder=7)
        ax.legend(loc="upper right", fontsize=8)

    # Frame to the robust extent of what we actually plotted, not the raw grid box
    # (bounded mode only). The grid covers axis_bounds, but the dynamics may dwell
    # in a sub-region; an empty grid quadrant is a framing lie. Use the union of
    # the live-field cells (a successor exists there) and the trajectory points,
    # IQR-fenced on each axis to shed a lone off-domain cell, lightly padded.
    if fit_to_data:
        dx_pts = list(GX) + traj_x
        dy_pts = list(GY) + traj_y
        fx = _robust_span(dx_pts, xlo, xhi)
        fy = _robust_span(dy_pts, ylo, yhi)
        ax.set_xlim(*fx)
        ax.set_ylim(*fy)
    else:
        ax.set_xlim(xlo, xhi)
        ax.set_ylim(ylo, yhi)
    return q


# ----- discrete / mixed regime (projected transition graph) -----------------
def render_discrete_panel(m, ax, axx, axy, states, edges, init_key,
                          all_xy_bounds=None, overlay=None):
    """Project a (sub)set of reachable states onto the two axes and draw the
    real transition arrows. `states` is a list of state dicts; `edges` a list of
    (i, j) into that list. Absorbing states (only successor is self) are starred.
    """
    nx_, ny_ = axx["name"], axy["name"]
    if not states:
        ax.text(0.5, 0.5, "(no states in this panel)",
                ha="center", va="center", transform=ax.transAxes,
                fontsize=10, color="gray")
        return

    bucket = {}
    base = []
    for s in states:
        x = _numeric(m, axx, s[nx_])
        y = _numeric(m, axy, s[ny_])
        k = bucket.get((x, y), 0)
        bucket[(x, y)] = k + 1
        base.append((x, y, k))

    # Coincident states share a cell; we fan them out with a small jitter so the
    # nodes don't overprint. The jitter must NOT push a point past the real
    # min/max of the axis's reachable values (e.g. an integer balance ∈ {0,1,2,3}
    # must never render at x ≈ -0.12 — that fabricates an off-domain state). We
    # clamp the jittered coordinate into [min,max] of the plotted data per axis.
    all_x = [b[0] for b in base]
    all_y = [b[1] for b in base]
    xmin, xmax = min(all_x), max(all_x)
    ymin, ymax = min(all_y), max(all_y)

    def _reflect(v, lo, hi):
        """Fold an out-of-range jitter back inside [lo,hi] (preserves separation
        better than a hard clamp, which would re-stack boundary nodes)."""
        if hi <= lo:
            return lo
        if v < lo:
            return lo + (lo - v)
        if v > hi:
            return hi - (v - hi)
        return v

    def place(i):
        x, y, k = base[i]
        if k == 0:
            return x, y
        ang = k * 2.399963
        r = 0.10 + 0.06 * k
        # never let the fan exceed the per-axis reachable extent
        jx = _reflect(x + r * np.cos(ang), xmin, xmax)
        jy = _reflect(y + r * np.sin(ang), ymin, ymax)
        return jx, jy

    P = [place(i) for i in range(len(states))]
    if overlay is not None:                    # hoverable points (#184): placed coords
        overlay.extend((ax, P[i][0], P[i][1], states[i]) for i in range(len(states)))

    succ = {}
    for (a, b) in edges:
        succ.setdefault(a, set()).add(b)
    fixed = {a for a in range(len(states)) if succ.get(a) == {a}}

    for (a, b) in edges:
        if a == b:
            continue
        x0, y0 = P[a]
        x1, y1 = P[b]
        ax.annotate("", xy=(x1, y1), xytext=(x0, y0),
                    arrowprops=dict(arrowstyle="-|>", color="#5a6b8c",
                                    lw=0.9, alpha=0.55, shrinkA=6, shrinkB=6),
                    zorder=2)

    xs = [p[0] for p in P]
    ys = [p[1] for p in P]
    normal = [i for i in range(len(states)) if i not in fixed]
    ax.scatter([xs[i] for i in normal], [ys[i] for i in normal],
               s=70, c="#1f77b4", edgecolors="black", zorder=4, label="state")
    if fixed:
        ax.scatter([xs[i] for i in fixed], [ys[i] for i in fixed],
                   marker="*", s=320, c="red", edgecolors="black",
                   zorder=5, label="absorbing")

    # mark the global initial state if it lives in this panel
    for i, s in enumerate(states):
        if m._key(s) == init_key:
            ax.scatter([P[i][0]], [P[i][1]], s=160, facecolors="none",
                       edgecolors="lime", linewidths=2.2, zorder=6,
                       label="initial")
            break

    if all_xy_bounds is not None:
        (gxlo, gxhi, gylo, gyhi) = all_xy_bounds
        ax.set_xlim(gxlo, gxhi)
        ax.set_ylim(gylo, gyhi)


def _bounds_of(m, states, axx, axy, pad=0.6):
    nx_, ny_ = axx["name"], axy["name"]
    xs = [_numeric(m, axx, s[nx_]) for s in states]
    ys = [_numeric(m, axy, s[ny_]) for s in states]
    if not xs:
        return (-1, 1, -1, 1)
    xlo, xhi = min(xs), max(xs)
    ylo, yhi = min(ys), max(ys)
    if xhi - xlo < 1e-9:
        xlo, xhi = xlo - 1, xhi + 1
    if yhi - ylo < 1e-9:
        ylo, yhi = ylo - 1, yhi + 1
    return (xlo - pad, xhi + pad, ylo - pad, yhi + pad)
