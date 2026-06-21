#!/usr/bin/env python3
"""render_phase_portrait.py — a phase-portrait (vector/direction field) renderer
for ANY Evident program's exported transition IR.

    python3 viz/render_phase_portrait.py <smt2> <schema> <out.png>

The picture is a difference-equation phase portrait: every state is a point in a
plane, and the field is the displacement successor(p) - p. The AXES carry the
two most expressive variables; a low-cardinality categorical, when present, is
lifted off the plane and used to FACET — one panel per value — which is the
honest way to ADD a dimension instead of cramming a 3rd variable onto a single
plot's color/jitter (Cleveland-McGill / Mackinlay: position is the strong
channel, facet is the dimension-adder for categoricals).

Channel mapping (via evident_viz):
  * AXES  = numeric_vars[:2] when the model has >=2 numerics (a true continuous
    field over value-space); otherwise the two most expressive vars of any kind,
    enums encoded as ordinals (enum_variants tick labels) and bools as 0/1.
  * COLOR = the derived STEP MAGNITUDE of the field (a coarse quantitative
    gradient — the one good quantitative use of hue). We keep this rather than
    recoloring by a variable; the variables ride the axes + facet.
  * FACET = a low-cardinality (<=~5) categorical, when one exists and is NOT
    already an axis. Each panel is the field/graph restricted to that value.

Two field regimes, both driven only by querying the transition via evident_viz:

  * NUMERIC (>=2 int/real vars): pin an arbitrary grid of points in value-space
    (we are NOT limited to reachable states), query successor() at each, draw the
    magnitude-colored field. Overlay trajectories from several seeds.

  * DISCRETE / MIXED (fewer than 2 numeric axis vars): there is no continuum, so
    we enumerate the reachable graph, project each visited state onto the two
    chosen (possibly ordinalized) axes, and draw the real transition arrows.
    Still a phase portrait — the arrows are the difference equation's image.

Degrades gracefully: <2 distinguishable axes, or an empty field, still emits a
titled figure (placeholder / projection).
"""
import sys
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load


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


# ----- channel assignment: axes (numeric-first) + a facet categorical -------
def plan_channels(m):
    """Decide axes, optional facet var, and regime from the ranked vars.

    Returns (axx, axy, facet_var, regime). The phase portrait NEEDS numeric axes
    for a true field, so when >=2 numerics exist we take numeric_vars[:2] for the
    plane directly (rather than assign_channels, which would put a categorical on
    y). The facet (when one exists) is the SUITABLE facet var from evident_viz —
    a low-cardinality categorical that stays ~constant within a run — so the
    dynamics live INSIDE a panel instead of being cut across panels. A var on the
    limit cycle (high within-run change rate) returns None: we then DON'T facet."""
    numeric = m.numeric_vars

    def facetable(exclude_names):
        """The suitable facet var (low-card, low within-run change rate), unless
        it's already consumed by an axis."""
        fv = m.facet_var()
        if fv is None or fv["name"] in exclude_names:
            return None
        return fv

    if len(numeric) >= 2:
        axx, axy = numeric[0], numeric[1]
        # a numeric field; facet by a suitable low-card categorical if one exists
        facet = facetable({axx["name"], axy["name"]})
        return axx, axy, facet, "numeric"

    # fewer than 2 numerics -> discrete/mixed projection. Facet by the suitable
    # low-cardinality categorical FIRST, then pick the two most expressive
    # remaining vars (numeric preferred) for the axes.
    facet = facetable(set())
    used = {facet["name"]} if facet is not None else set()
    axis_pool = [v for v in m.state_vars if v["name"] not in used]
    if len(axis_pool) < 2:
        # not enough left once we facet — don't facet, use the top-2 as axes
        facet = None
        axis_pool = list(m.state_vars)
    if len(axis_pool) < 2:
        return None, None, None, "degenerate"
    # axes: numerics first, then highest-cardinality categoricals
    num_axes = [v for v in axis_pool if _is_numeric(v)]
    cat_axes = sorted((v for v in axis_pool if not _is_numeric(v)),
                      key=lambda v: _cardinality(m, v), reverse=True)
    ordered = num_axes + cat_axes
    axx, axy = ordered[0], ordered[1]
    regime = "mixed" if num_axes else "discrete"
    return axx, axy, facet, regime


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


def render_numeric_panel(m, ax, axx, axy, pin, draw_colorbar, extent):
    """A magnitude-colored vector field over a grid of pinned numeric points.

    `pin` carries the values of every NON-axis var (facet value, off-axis vars);
    those are fixed while we sweep the two axis vars over a grid. `extent` =
    (xlo, xhi, ylo, yhi) is the REACHABLE-orbit domain (from `_orbit_extent`) —
    we grid within it, never a guessed ±3000 box. Seeds for the overlaid
    trajectories are placed off-origin (the origin is often the fixed point of an
    oscillator, which a centered seed would never leave)."""
    nx_, ny_ = axx["name"], axy["name"]

    xlo, xhi, ylo, yhi = extent
    span = max(abs(xlo), abs(xhi), abs(ylo), abs(yhi), 1.0)

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
            interior = (abs(xv) < 0.92 * span and abs(yv) < 0.92 * span)
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

    # overlaid trajectories from a spread of off-origin seeds
    seeds = [(xhi * 0.85, 0), (xhi * 0.12, 0), (0, yhi * 0.85),
             (-xhi * 0.45, yhi * 0.55), (-xhi * 0.85, 0), (0, -yhi * 0.85)]
    cmap = plt.get_cmap("autumn")
    for i, (sx0, sy0) in enumerate(seeds):
        state = dict(pin)
        state[nx_] = int(round(sx0)) if axx["kind"] == "int" else sx0
        state[ny_] = int(round(sy0)) if axy["kind"] == "int" else sy0
        traj = m.trajectory(start=state, steps=400)
        if len(traj) < 2:
            continue
        px = [_numeric(m, axx, s[nx_]) for s in traj]
        py = [_numeric(m, axy, s[ny_]) for s in traj]
        ax.plot(px, py, "-", lw=1.6, color=cmap(i / max(1, len(seeds) - 1)),
                alpha=0.95, zorder=5)
        ax.plot(px[0], py[0], "o", color="white", mec="black", ms=6, zorder=6)

    if fixed_x:
        ax.plot(fixed_x, fixed_y, "*", color="red", ms=18, mec="black",
                label="fixed point", zorder=7)
        ax.legend(loc="upper right", fontsize=8)

    ax.set_xlim(xlo, xhi)
    ax.set_ylim(ylo, yhi)
    return q


# ----- discrete / mixed regime (projected transition graph) -----------------
def render_discrete_panel(m, ax, axx, axy, states, edges, init_key,
                          all_xy_bounds=None):
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

    def place(i):
        x, y, k = base[i]
        if k == 0:
            return x, y
        ang = k * 2.399963
        r = 0.10 + 0.06 * k
        return x + r * np.cos(ang), y + r * np.sin(ang)

    P = [place(i) for i in range(len(states))]

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


# ----- axis decoration ------------------------------------------------------
def _decorate_axes(m, ax, axx, axy):
    tx = _axis_ticks(m, axx)
    if tx is not None:
        ax.set_xticks(tx[0])
        ax.set_xticklabels(tx[1], rotation=30, ha="right", fontsize=8)
    ty = _axis_ticks(m, axy)
    if ty is not None:
        ax.set_yticks(ty[0])
        ax.set_yticklabels(ty[1], fontsize=8)


def _axis_label(var):
    suffix = {"bool": " (0/1)", "enum": " (ordinal)"}.get(var["kind"], "")
    return f"{var['name']}{suffix}"


# ----- facet helpers --------------------------------------------------------
def _facet_values(m, facet_var):
    if facet_var["kind"] == "enum":
        return list(m.enum_variants[facet_var["name"]])
    if facet_var["kind"] == "bool":
        return [False, True]
    return None


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


# ----- top-level orchestration ----------------------------------------------
def render(smt2_path, schema_path, out_path):
    m = load(smt2_path, schema_path)
    axx, axy, facet_var, regime = plan_channels(m)

    if regime == "degenerate":
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        ax.text(0.5, 0.5,
                f"N/A for {len(m.state_vars)}-var state:\n"
                "phase portrait needs 2 axes",
                ha="center", va="center", transform=ax.transAxes, fontsize=13)
        ax.set_xticks([]); ax.set_yticks([])
        ax.set_title(f"{m.fsm} — phase portrait", fontsize=13)
        fig.tight_layout()
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        return out_path

    facet_vals = _facet_values(m, facet_var) if facet_var is not None else None

    if regime == "numeric":
        # A continuous vector field is only HONEST when the reachable dynamics are
        # genuinely continuous/unbounded — a limit cycle, an open orbit. A
        # TERMINATING numeric program (a counter that marches 0..10 and halts) has
        # a small FINITE reachable set; gridding a guessed field over it fabricates
        # cycles/basins/fixed-point stars the program never enters (the bug).
        # Classify by the program's OWN reachable set, never a guessed box.
        kind = _numeric_regime(m, axx, axy)
        if kind == "finite":
            # finite reachable march -> the honest transition graph, NOT a field
            _render_discrete(m, axx, axy, facet_var, facet_vals, "mixed", out_path)
        elif kind == "continuous":
            _render_numeric(m, axx, axy, facet_var, facet_vals, out_path)
        else:  # "degenerate": a lone fixed point, no orbit -> no meaningful field
            _render_na(
                m, axx, axy,
                "N/A — reachable set is a single fixed point;\n"
                "no continuum and no orbit for a phase portrait", out_path)
    else:
        _render_discrete(m, axx, axy, facet_var, facet_vals, regime, out_path)
    return out_path


def _render_na(m, axx, axy, msg, out_path):
    fig, ax = plt.subplots(figsize=(8.5, 7.5))
    ax.text(0.5, 0.5, msg, ha="center", va="center",
            transform=ax.transAxes, fontsize=13)
    ax.set_xticks([]); ax.set_yticks([])
    ax.set_title(f"{m.fsm} — phase portrait", fontsize=13)
    fig.tight_layout()
    fig.savefig(out_path, dpi=120)
    plt.close(fig)


def _numeric_regime(m, axx, axy):
    """Classify a >=2-numeric program by its REACHABLE dynamics, so we pick the
    honest picture instead of always gridding a field:

      * "finite"     — the reachable BFS terminates with a small set of distinct
                       states spanning >1 point on the axes. A real, drawable
                       transition graph (e.g. a `wc` counter, 11 states in
                       0..10). Render those points + arrows, NOT a vector field.
      * "continuous" — the reachable set from the initial state is degenerate
                       (a fixed point) OR very large, BUT perturbation orbits
                       trace a non-trivial cycle. Genuinely continuous dynamics —
                       a vector field scaled to the ORBIT extent is the honest
                       picture (vanderpol's limit cycle).
      * "degenerate" — a lone fixed point with no orbit. Nothing to draw.
    """
    nx_, ny_ = axx["name"], axy["name"]
    states, _ = m.reachable(limit=3000)

    def axis_spread(sts):
        xs = {_numeric(m, axx, s[nx_]) for s in sts}
        ys = {_numeric(m, axy, s[ny_]) for s in sts}
        return len(xs) > 1 or len(ys) > 1

    # a small, terminating reachable set that actually moves on the axes = finite
    if 2 <= len(states) < 3000 and axis_spread(states):
        return "finite"

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


def _panel_grid(n):
    cols = min(n, 3)
    rows = (n + cols - 1) // cols
    return rows, cols


def _render_numeric(m, axx, axy, facet_var, facet_vals, out_path):
    init = m.initial_state() or {v["name"]: 0 for v in m.state_vars}
    other = [v for v in m.state_vars
             if v["name"] not in (axx["name"], axy["name"])
             and (facet_var is None or v["name"] != facet_var["name"])]

    if facet_var is None:
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        pin = {v["name"]: init[v["name"]] for v in m.state_vars}
        extent = _orbit_extent(m, axx, axy, pin)
        render_numeric_panel(m, ax, axx, axy, pin, draw_colorbar=True,
                             extent=extent)
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        ax.set_title(f"{m.fsm} — phase portrait\n"
                     "(numeric vector field, reachable-orbit extent)",
                     fontsize=13)
        fig.tight_layout()
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        return

    rows, cols = _panel_grid(len(facet_vals))
    fig, axes = plt.subplots(rows, cols, figsize=(5.2 * cols, 4.8 * rows),
                             squeeze=False)
    flat = [axes[r][c] for r in range(rows) for c in range(cols)]
    last_q = None
    for idx, fval in enumerate(facet_vals):
        ax = flat[idx]
        pin = {v["name"]: init[v["name"]] for v in m.state_vars}
        pin[facet_var["name"]] = fval
        extent = _orbit_extent(m, axx, axy, pin)
        q = render_numeric_panel(m, ax, axx, axy, pin, draw_colorbar=False,
                                 extent=extent)
        if q is not None:
            last_q = q
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        ax.set_title(f"{facet_var['name']} = {fval}", fontsize=11)
    for j in range(len(facet_vals), len(flat)):
        flat[j].axis("off")
    if last_q is not None:
        fig.colorbar(last_q, ax=axes.ravel().tolist(), fraction=0.025,
                     pad=0.02, label="step magnitude")
    fig.suptitle(f"{m.fsm} — phase portrait  (faceted by {facet_var['name']})",
                 fontsize=14)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def _render_discrete(m, axx, axy, facet_var, facet_vals, regime, out_path):
    states, edges = m.reachable(limit=3000)
    if not states:
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        ax.text(0.5, 0.5, "N/A: no reachable states\n(initial_state is None)",
                ha="center", va="center", transform=ax.transAxes, fontsize=12)
        ax.set_title(f"{m.fsm} — phase portrait", fontsize=13)
        fig.tight_layout()
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        return

    init_key = m._key(states[0])
    bounds = _bounds_of(m, states, axx, axy)

    if facet_var is None:
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        render_discrete_panel(m, ax, axx, axy, states, edges, init_key, bounds)
        ax.legend(loc="upper right", fontsize=8)
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        ax.text(0.02, 0.98,
                f"{len(states)} reachable states, {len(edges)} transitions",
                transform=ax.transAxes, fontsize=8, color="gray", va="top")
        ax.set_title(f"{m.fsm} — phase portrait\n(discrete transition graph)",
                     fontsize=13)
        fig.tight_layout()
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        return

    # FACET: one panel per facet value. A state belongs to a panel by its facet
    # value; an edge stays IN the panel only if both endpoints share it (a
    # cross-facet edge would need a 3rd axis to draw honestly, so we annotate
    # the count instead of drawing a misleading in-plane arrow).
    fname = facet_var["name"]
    rows, cols = _panel_grid(len(facet_vals))
    fig, axes = plt.subplots(rows, cols, figsize=(5.4 * cols, 4.8 * rows),
                             squeeze=False)
    flat = [axes[r][c] for r in range(rows) for c in range(cols)]

    for idx, fval in enumerate(facet_vals):
        ax = flat[idx]
        keep = [i for i, s in enumerate(states) if s[fname] == fval]
        remap = {gi: li for li, gi in enumerate(keep)}
        sub_states = [states[gi] for gi in keep]
        sub_edges = [(remap[a], remap[b]) for (a, b) in edges
                     if a in remap and b in remap]
        crossing = sum(1 for (a, b) in edges
                       if (a in remap) != (b in remap)
                       and (a in remap or b in remap))
        render_discrete_panel(m, ax, axx, axy, sub_states, sub_edges,
                              init_key, bounds)
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        note = f"{len(sub_states)} states"
        if crossing:
            note += f", {crossing} cross-facet"
        ax.text(0.02, 0.98, note, transform=ax.transAxes, fontsize=7,
                color="gray", va="top")
        ax.set_title(f"{fname} = {fval}", fontsize=11)

    # one shared legend
    handles, labels = flat[0].get_legend_handles_labels()
    for j in range(len(facet_vals), len(flat)):
        flat[j].axis("off")
    if handles:
        fig.legend(handles, labels, loc="lower center", ncol=len(labels),
                   fontsize=9, frameon=True)
    fig.suptitle(
        f"{m.fsm} — phase portrait  (faceted by {fname}; "
        f"{len(states)} states, {len(edges)} transitions)", fontsize=13)
    fig.tight_layout(rect=(0, 0.05, 1, 0.96))
    fig.savefig(out_path, dpi=120)
    plt.close(fig)


def main(argv):
    if len(argv) != 4:
        print("usage: render_phase_portrait.py <smt2> <schema> <out.png>",
              file=sys.stderr)
        return 2
    out = render(argv[1], argv[2], argv[3])
    size = os.path.getsize(out)
    print(f"wrote {out} ({size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
