#!/usr/bin/env python3
"""render_phase_portrait.py — a phase-portrait (vector/direction field) renderer
for ANY Evident program's exported transition IR.

    python3 viz/render_phase_portrait.py <smt2> <schema> <out.png>

The picture: pick two axes from the state variables (the first two numeric vars
when available; otherwise the first two of any kind). Map every state to a point
in that plane (int/real as-is, bool -> 0/1, enum -> ordinal index). The vector
field is the *displacement* successor(p) - p, sampled over a grid of pinned
points and drawn as normalized arrows. A few full trajectories are overlaid, and
fixed points (successor == state) are marked.

Two regimes, both driven only by querying the transition via evident_viz:

  * NUMERIC (>=2 int/real vars): pin an arbitrary grid of points in value-space
    (we are NOT limited to reachable states), query successor() at each, draw the
    field. Overlay trajectories from several seeds.

  * DISCRETE / MIXED (fewer than 2 numeric vars, e.g. enum/bool state): there is
    no continuum to sample, so we enumerate the reachable graph, project each
    visited state onto the two chosen (possibly ordinalized) axes, and draw the
    real transition arrows between visited states. Still a phase portrait — the
    arrows are the difference equation's image — just over the discrete state set.

Degrades gracefully: if a sample has <2 distinguishable axes, or the field comes
back empty, it still emits a titled figure (placeholder / projection).
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


# ----- choose the two axes --------------------------------------------------
def _cardinality(m, var):
    """How many distinct projected values an axis can take (its spread)."""
    k = var["kind"]
    if k == "enum":
        return len(m.enum_variants[var["name"]])
    if k == "bool":
        return 2
    return 1000  # numeric: treated as high-resolution


def choose_axes(m):
    numeric = [v for v in m.state_vars if _is_numeric(v)]
    if len(numeric) >= 2:
        return numeric[0], numeric[1], "numeric"
    # mixed: prefer one numeric + one categorical, else first two of any kind
    if len(m.state_vars) >= 2:
        if len(numeric) == 1:
            # pick the most-separating categorical for the other axis
            cands = [v for v in m.state_vars if v is not numeric[0]]
            other = max(cands, key=lambda v: _cardinality(m, v))
            return numeric[0], other, "mixed"
        # fully discrete: pick the two highest-cardinality axes so the
        # projection separates states instead of collapsing them
        ranked = sorted(m.state_vars, key=lambda v: _cardinality(m, v),
                        reverse=True)
        return ranked[0], ranked[1], "discrete"
    return None, None, "degenerate"


# ----- numeric regime -------------------------------------------------------
def _value_range(m, ax_var):
    """Heuristic sampling range for a numeric axis. Probe the initial state and a
    few successors to scale; fall back to a symmetric default."""
    name = ax_var["name"]
    vals = []
    init = m.initial_state()
    if init is not None:
        vals.append(init[name])
    # probe a spread of seeds to learn the operating magnitude
    for seed_scale in (100, 1000, 3000):
        st = {v["name"]: (seed_scale if v["name"] == name else 0)
              for v in m.state_vars}
        nxt = m.successor(st)
        if nxt is not None:
            vals.append(nxt[name])
    if not vals:
        return -10.0, 10.0
    mag = max(abs(float(v)) for v in vals)
    if mag < 1:
        mag = 10.0
    span = mag * 1.4
    return -span, span


def render_numeric(m, ax, axx, axy):
    nx_, ny_ = axx["name"], axy["name"]
    other = [v for v in m.state_vars if v["name"] not in (nx_, ny_)]

    xlo, xhi = _value_range(m, axx)
    ylo, yhi = _value_range(m, axy)

    n = 21
    xs = np.linspace(xlo, xhi, n)
    ys = np.linspace(ylo, yhi, n)

    GX, GY, U, V, MAG = [], [], [], [], []
    fixed_x, fixed_y = [], []

    init = m.initial_state() or {v["name"]: 0 for v in m.state_vars}

    for xv in xs:
        for yv in ys:
            state = dict(init)
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
            # A genuine fixed point sits in the interior; zero displacement at
            # the sampling boundary is usually integer-rounding saturation, not
            # an equilibrium — exclude the outer ring.
            interior = (abs(xv) < 0.92 * max(abs(xlo), abs(xhi)) and
                        abs(yv) < 0.92 * max(abs(ylo), abs(yhi)))
            if abs(dx) < 1e-9 and abs(dy) < 1e-9 and interior:
                fixed_x.append(xv); fixed_y.append(yv)

    if GX:
        GX = np.array(GX); GY = np.array(GY)
        U = np.array(U); V = np.array(V); MAG = np.array(MAG)
        # normalize arrow length, color by raw displacement magnitude
        norm = np.where(MAG > 1e-12, MAG, 1.0)
        Un = U / norm
        Vn = V / norm
        q = ax.quiver(GX, GY, Un, Vn, MAG, cmap="viridis",
                      angles="xy", scale=30, width=0.0035,
                      pivot="mid", alpha=0.85)
        cb = plt.colorbar(q, ax=ax, fraction=0.046, pad=0.04)
        cb.set_label("step magnitude")

    # overlaid trajectories from a spread of seeds
    sx = xhi * 0.85
    sy = yhi * 0.85
    seeds = [(sx, 0), (xhi * 0.12, 0), (0, sy), (-xhi * 0.45, sy * 0.55),
             (-sx, 0), (0, -sy)]
    cmap = plt.get_cmap("autumn")
    for i, (sx0, sy0) in enumerate(seeds):
        state = dict(init)
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
    if other:
        sub = ", ".join(f"{v['name']}={init[v['name']]}" for v in other)
        ax.text(0.02, 0.02, f"slice: {sub}", transform=ax.transAxes,
                fontsize=7, color="gray", va="bottom")


# ----- discrete / mixed regime ---------------------------------------------
def render_discrete(m, ax, axx, axy):
    nx_, ny_ = axx["name"], axy["name"]
    states, edges = m.reachable(limit=3000)

    if not states:
        ax.text(0.5, 0.5, "N/A: no reachable states\n(initial_state is None)",
                ha="center", va="center", transform=ax.transAxes, fontsize=12)
        return

    # project every state to the plane; jitter coincident points a hair so
    # multiplicity is visible
    pts = []
    bucket = {}
    for s in states:
        x = _numeric(m, axx, s[nx_])
        y = _numeric(m, axy, s[ny_])
        key = (x, y)
        k = bucket.get(key, 0)
        bucket[key] = k + 1
        pts.append((x, y, k))

    def place(i):
        x, y, k = pts[i]
        if k == 0:
            return x, y
        # spiral jitter for stacked states at the same projected cell
        ang = k * 2.399963
        r = 0.08 + 0.05 * k
        return x + r * np.cos(ang), y + r * np.sin(ang)

    P = [place(i) for i in range(len(states))]

    # fixed points: an ABSORBING state — its only successor is itself. (Many
    # FSMs offer a "stay" no-op from every state, so a mere self-loop among
    # several transitions is not a fixed point; absorption is.)
    succ_keys = {}
    for (a, b) in edges:
        succ_keys.setdefault(a, set()).add(b)
    fixed = {a for a in range(len(states))
             if succ_keys.get(a) == {a}}

    # draw transition arrows (the difference equation's image)
    for (a, b) in edges:
        if a == b:
            continue
        x0, y0 = P[a]
        x1, y1 = P[b]
        ax.annotate("", xy=(x1, y1), xytext=(x0, y0),
                    arrowprops=dict(arrowstyle="-|>", color="#5a6b8c",
                                    lw=0.9, alpha=0.55,
                                    shrinkA=6, shrinkB=6),
                    zorder=2)

    xs = [p[0] for p in P]
    ys = [p[1] for p in P]
    normal = [i for i in range(len(states)) if i not in fixed]
    ax.scatter([xs[i] for i in normal], [ys[i] for i in normal],
               s=70, c="#1f77b4", edgecolors="black", zorder=4, label="state")
    if fixed:
        ax.scatter([xs[i] for i in fixed], [ys[i] for i in fixed],
                   marker="*", s=320, c="red", edgecolors="black",
                   zorder=5, label="fixed point (self-loop)")

    # mark the initial state
    init = states[0]
    ix, iy = P[0]
    ax.scatter([ix], [iy], s=160, facecolors="none", edgecolors="lime",
               linewidths=2.2, zorder=6, label="initial")

    ax.legend(loc="upper right", fontsize=8)

    ax.text(0.02, 0.98,
            f"{len(states)} reachable states, {len(edges)} transitions",
            transform=ax.transAxes, fontsize=8, color="gray", va="top")


# ----- categorical axis decoration -----------------------------------------
def _decorate_axes(m, ax, axx, axy):
    tx = _axis_ticks(m, axx)
    if tx is not None:
        ax.set_xticks(tx[0]); ax.set_xticklabels(tx[1], rotation=30, ha="right",
                                                 fontsize=8)
    ty = _axis_ticks(m, axy)
    if ty is not None:
        ax.set_yticks(ty[0]); ax.set_yticklabels(ty[1], fontsize=8)


def _axis_label(var):
    suffix = {"bool": " (0/1)", "enum": " (ordinal)"}.get(var["kind"], "")
    return f"{var['name']}{suffix}"


def render(smt2_path, schema_path, out_path):
    m = load(smt2_path, schema_path)
    axx, axy, regime = choose_axes(m)

    fig, ax = plt.subplots(figsize=(8.5, 7.5))

    if regime == "degenerate":
        ax.text(0.5, 0.5,
                f"N/A for {len(m.state_vars)}-var state:\n"
                "phase portrait needs 2 axes",
                ha="center", va="center", transform=ax.transAxes, fontsize=13)
        ax.set_xticks([]); ax.set_yticks([])
    else:
        if regime == "numeric":
            render_numeric(m, ax, axx, axy)
        else:
            render_discrete(m, ax, axx, axy)
        ax.set_xlabel(_axis_label(axx))
        ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)

    regime_note = {"numeric": "numeric vector field",
                   "mixed": "mixed projection",
                   "discrete": "discrete transition graph",
                   "degenerate": ""}[regime]
    ax.set_title(f"{m.fsm} — phase portrait\n({regime_note})", fontsize=13)

    fig.tight_layout()
    fig.savefig(out_path, dpi=120)
    plt.close(fig)
    return out_path


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
