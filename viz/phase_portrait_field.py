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


# ----- numeric regime: derive the field domain from the REACHABLE orbit ------
def _orbit_extent(m, axx, axy, pin):
    """The bounding box of the actually-VISITED states for a continuous numeric
    field, derived from the program's reachable dynamics — NEVER a hardcoded box.

    Two reachable sources, unioned:
      * axis_bounds(name): the padded extent of each axis over the reachable
        BFS sample (the real domain helper). For a limit cycle whose initial
        state is the unstable fixed point this collapses to ~a point, so we also
        probe the orbit directly.
      * perturbation trajectories: seed a spread of off-origin starts and follow
        the successor chain; the bbox of the visited orbit IS the limit-cycle
        extent (vanderpol: ~±2700, NOT ±3200). The seeds are scaled to whatever
        axis_bounds suggests, then grown geometrically until the orbit stops
        expanding, so we discover the cycle's real reach rather than guessing it.

    `pin` fixes the non-axis carried vars while the two axes sweep.
    Returns (xlo, xhi, ylo, yhi)."""
    nx_, ny_ = axx["name"], axy["name"]
    xs, ys = [], []

    bx = m.axis_bounds(nx_)
    by = m.axis_bounds(ny_)
    if bx is not None:
        xs += [bx[0], bx[1]]
    if by is not None:
        ys += [by[0], by[1]]

    # scale of the perturbation seeds: start from whatever the reachable sample
    # spans (so we don't impose a magnitude), grow until the orbit stops growing.
    base = max([abs(v) for v in xs + ys] + [1.0])
    prev_reach = -1.0
    for mult in (1, 4, 16, 64, 256):
        scale = base * mult
        seeds = [(scale, 0), (0, scale), (-scale, 0), (0, -scale),
                 (scale * 0.5, scale * 0.5)]
        ox, oy = [], []
        for sx0, sy0 in seeds:
            st = dict(pin)
            st[nx_] = int(round(sx0)) if axx["kind"] == "int" else sx0
            st[ny_] = int(round(sy0)) if axy["kind"] == "int" else sy0
            tr = m.trajectory(start=st, steps=400)
            for s in tr:
                ox.append(_numeric(m, axx, s[nx_]))
                oy.append(_numeric(m, axy, s[ny_]))
        if ox:
            xs += [min(ox), max(ox)]
            ys += [min(oy), max(oy)]
            reach = max(abs(min(ox)), abs(max(ox)), abs(min(oy)), abs(max(oy)))
            # the orbit has stopped expanding with bigger seeds -> found its reach
            if reach <= prev_reach * 1.05:
                break
            prev_reach = reach

    if not xs or not ys:
        return -10.0, 10.0, -10.0, 10.0
    xlo, xhi = min(xs), max(xs)
    ylo, yhi = min(ys), max(ys)
    # square + pad so the field reads symmetrically around the cycle
    span = max(abs(xlo), abs(xhi), abs(ylo), abs(yhi), 1.0) * 1.15
    return -span, span, -span, span


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
            nxt = m.successor(state)
            if nxt is None:
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
        traj = m.trajectory(start=state, steps=400)
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


# A reachable set this small is a drawable transition GRAPH (points + arrows);
# above it the graph is an unreadable hairball and a magnitude-colored vector
# field over the bounded reachable domain is the honest picture instead.
_FINITE_STATE_CAP = 60


def _numeric_regime(m, axx, axy):
    """Classify a >=2-numeric program by its REACHABLE dynamics, so we pick the
    honest picture instead of always gridding a guessed field:

      * "finite"     — the reachable BFS terminates with a SMALL set of distinct
                       states, both axes moving. A real, drawable transition graph
                       (`wc`, 11 states in 0..10). Render the points + arrows.
      * "bounded"    — a large or non-terminating reachable march, but BOTH axes
                       move AND axis_bounds gives a finite real extent (lru's
                       caches in {-1..~80}, randomwalk's visit counters in
                       {0..~400}). Grid a vector field over THAT reachable extent
                       — never the perturbation box that fabricated ±20000 axes.
      * "continuous" — the reachable set is a lone fixed point (so axis_bounds
                       collapses to a point), but perturbation orbits trace a
                       non-trivial cycle. Genuine continuum (vanderpol's limit
                       cycle); the orbit extent IS the honest domain.
      * "degenerate" — a constant axis (one variable never moves), or a lone
                       fixed point with no orbit. No 2D field to draw.
    """
    nx_, ny_ = axx["name"], axy["name"]
    states, _ = m.reachable(limit=3000)

    dx = len({_numeric(m, axx, s[nx_]) for s in states}) if states else 0
    dy = len({_numeric(m, axy, s[ny_]) for s in states}) if states else 0

    # a MULTI-state reachable march where one axis never moves = no 2D portrait
    # (life: many gen values but pop ≡ 3, so a (gen, pop) field is a row of flat
    # arrows). A SINGLE-state reachable set (an unstable fixed point) is NOT this
    # case — its orbit lives off the initial point, so fall through to the probe
    # (vanderpol's reachable set is the lone fixed point; perturbing reveals the
    # limit cycle). Guard on len(states) > 1 so we don't kill that.
    if len(states) > 1 and (dx <= 1 or dy <= 1):
        return "degenerate"

    # a small terminating march that moves on both axes = a drawable graph
    if 2 <= len(states) <= _FINITE_STATE_CAP and dx > 1 and dy > 1:
        return "finite"

    # both axes move AND the reachable extent is bounded (robust axis_bounds, which
    # rejects ±1e6 / -1 sentinels) = a real but large/unbounded march. Grid the
    # field over the reachable domain, not a fabricated box. BUT the `states` above
    # come from m.reachable(), which pins only the two axis vars and lets the other
    # carried leaves float — so its fan can move both axes even when the program's
    # FOLLOWED trajectory holds one axis constant (randomwalk's v3/v4 are pinned at 0
    # along the real walk; the [0,400] spread is pure relational cloud). A bounded
    # field is only honest if the actual dwell moves on BOTH axes — else it's a flat
    # row of arrows over a fabricated box, so report N/A instead.
    if dx > 1 and dy > 1:
        bx = m.axis_bounds(nx_)
        by = m.axis_bounds(ny_)
        if bx is not None and by is not None:
            dwell = _dwell_span(m, axx, axy)
            if dwell is not None:
                (xlo, xhi), (ylo, yhi) = dwell
                if xhi - xlo < 1e-9 or yhi - ylo < 1e-9:
                    return "degenerate"
            return "bounded"

    # otherwise probe for a continuous orbit by perturbing off the initial state
    init = m.initial_state() or {v["name"]: 0 for v in m.state_vars}
    pin = {v["name"]: init[v["name"]] for v in m.state_vars}
    xlo, xhi, ylo, yhi = _orbit_extent(m, axx, axy, pin)
    orbit_span = max(abs(xlo), abs(xhi), abs(ylo), abs(yhi))
    init_mag = max(abs(_numeric(m, axx, init[nx_])),
                   abs(_numeric(m, axy, init[ny_])), 1.0)
    # the orbit reaches well beyond the initial state -> real continuous dynamics
    if orbit_span > 4.0 * init_mag and orbit_span > 4.0:
        return "continuous"
    return "degenerate"


def _dwell_span(m, axx, axy):
    """The per-axis extent of the states the program actually DWELLS in — its real
    deterministic trajectory — as ((xlo,xhi),(ylo,yhi)). This is the honest framing
    source for a bounded march: `m.reachable()` pins only the two axis vars and
    leaves the other carried leaves free, so its BFS fans into a relational cloud
    (lru: k0∈[-1,75], miss_count∈[0,51]) that has nothing to do with where the
    program goes (k0∈{-1,1}, miss_count∈{0..4}). `axis_bounds` is fed by that same
    fan, so gridding over it frames a mostly-empty box — the blow-out. The followed
    trajectory is the set of states the difference equation truly visits; framing to
    its robust span is the fix. Returns None for an axis if the trajectory is empty."""
    tr = m.trajectory(steps=400)
    if len(tr) < 2:
        return None
    out = []
    for v in (axx, axy):
        nm = v["name"]
        vals = sorted(_numeric(m, v, s[nm]) for s in tr)
        # IQR-fence a lone off-domain visit, but keep the RAW span (no zero-width
        # widening): a genuinely-constant axis must report lo==hi so the caller can
        # detect the collapse, not be hidden behind an artificial ±1 pad.
        if len(vals) >= 4:
            q1, q3 = vals[len(vals) // 4], vals[(3 * len(vals)) // 4]
            iqr = q3 - q1
            if iqr > 0:
                lof, hif = q1 - 3 * iqr, q3 + 3 * iqr
                vals = [x for x in vals if lof <= x <= hif] or vals
        out.append((float(min(vals)), float(max(vals))))
    return tuple(out)


def _reachable_extent(m, axx, axy):
    """The field domain for a BOUNDED numeric march: the robust extent of the states
    the program actually VISITS (its followed trajectory), lightly padded so the
    field reads. NOT axis_bounds — that helper is fed by the relational reachable
    fan and blows the frame out to a mostly-empty box (lru's [0,81]×[0,30] when the
    dwell is [-1,1]×[0,4]). NEVER the perturbation-grown box either. Falls back to
    axis_bounds only when no trajectory is available. Returns (xlo,xhi,ylo,yhi)."""
    dwell = _dwell_span(m, axx, axy)
    if dwell is not None:
        (xlo, xhi), (ylo, yhi) = dwell
    else:
        bx = m.axis_bounds(axx["name"]) or (0.0, 1.0)
        by = m.axis_bounds(axy["name"]) or (0.0, 1.0)
        xlo, xhi, ylo, yhi = bx[0], bx[1], by[0], by[1]
    # light pad so boundary states aren't pinned to the frame edge; never invert
    px = max((xhi - xlo) * 0.08, 0.5)
    py = max((yhi - ylo) * 0.08, 0.5)
    return xlo - px, xhi + px, ylo - py, yhi + py


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
