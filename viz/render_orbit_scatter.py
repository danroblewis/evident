#!/usr/bin/env python3
"""render_orbit_scatter — the honest discrete-time orbit view.

Plot a trajectory as DISCRETE DOTS (not connected) in two chosen state axes,
colored by tick (a time gradient). Each dot is one sampled state of the FSM at
one tick; the gap between dots is the actual jump the difference equation makes.
For limit cycles the dots trace a closed loop; for fixed points they pile up.

Usage:
    python3 viz/render_orbit_scatter.py <smt2> <schema> <out_path>

Works for ANY Evident IR:
  - NUMERIC systems  -> seed several initial points, scatter each orbit so the
                        attractor (limit cycle / fixed point) shows up.
  - MIXED systems    -> project enum->ordinal, bool->0/1, follow the autonomous
                        orbit from the initial state.
  - DISCRETE systems -> pick two vars, project to ordinals, scatter the orbit.
                        (Degrades gracefully: discrete orbits are short / cyclic.)
"""
import sys
import os

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
    if k == "string":
        return 0.0
    return 0.0


def _axis_label(var):
    return f'{var["name"]}  [{var["kind"]}]'


def _pick_axes(model):
    """Choose two state vars for the X/Y axes. Prefer numeric pairs; otherwise
    take the two with the most distinct projected values (enum > bool)."""
    vs = model.state_vars
    numeric = [v for v in vs if v["kind"] in ("int", "real")]
    if len(numeric) >= 2:
        return numeric[0], numeric[1]
    # rank by "interestingness": enum (many ordinals) > bool, numeric first
    def rank(v):
        return {"int": 3, "real": 3, "enum": 2, "bool": 1, "string": 0}.get(v["kind"], 0)
    ordered = sorted(vs, key=rank, reverse=True)
    if len(ordered) >= 2:
        return ordered[0], ordered[1]
    if len(ordered) == 1:
        return ordered[0], ordered[0]
    return None, None


# ---- seed selection ----------------------------------------------------------
def _numeric_seeds(model, xvar, yvar):
    """For a 2D numeric system, several initial points so the attractor shows.
    These are arbitrary grid pins (successor accepts any state)."""
    # Generic seeds scaled to the system; vanderpol lives at ~±3000.
    base = [
        {xvar["name"]: 2800, yvar["name"]: 0},
        {xvar["name"]: 400, yvar["name"]: 0},
        {xvar["name"]: 0, yvar["name"]: 2700},
        {xvar["name"]: -1500, yvar["name"]: 1500},
    ]
    # Fill in any *other* state vars (shouldn't exist for a pure 2D system) with 0.
    seeds = []
    for s in base:
        full = {v["name"]: s.get(v["name"], 0) for v in model.state_vars}
        seeds.append(full)
    return seeds


def _orbit(model, start, steps):
    """A list of states following the successor chain (may revisit -> cycle)."""
    return model.trajectory(start=start, steps=steps)


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


# ---- main render -------------------------------------------------------------
def render(smt2_path, schema_path, out_path):
    model = load(smt2_path, schema_path)
    fig, ax = plt.subplots(figsize=(8, 7))
    title = f"{model.fsm} — orbit_scatter"

    xvar, yvar = _pick_axes(model)
    if xvar is None:
        ax.text(0.5, 0.5, "N/A for state: no state variables",
                ha="center", va="center", fontsize=14)
        ax.set_title(title)
        ax.axis("off")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    numeric_2d = (xvar["kind"] in ("int", "real")
                  and yvar["kind"] in ("int", "real")
                  and xvar["name"] != yvar["name"])

    # Build the list of orbits to scatter.
    orbits = []  # each: list of state dicts
    tick_index = None  # optional per-point color values (else 0..len-1 used)
    if numeric_2d:
        for seed in _numeric_seeds(model, xvar, yvar):
            orb = _orbit(model, seed, steps=400)
            if orb:
                orbits.append(orb)
        subtitle = "numeric: multiple seeds → attractor"
    else:
        init = model.initial_state()
        orb = _orbit(model, init, steps=400) if init is not None else []
        if len(orb) > 2:
            # A real autonomous orbit (e.g. vending's limit cycle).
            orbits.append(orb)
            subtitle = "autonomous orbit from initial state"
            tick_index = None
        else:
            # Nondeterministic / quickly-terminating chain: a single-successor
            # orbit is a dead end. Scatter the BFS-reachable set instead, colored
            # by distance-from-start (a time-like gradient that respects the fan).
            states, depths = _reachable_with_depth(model)
            if states:
                orbits.append(states)
                tick_index = depths  # color by BFS depth, not chain position
                subtitle = (f"discrete/nondeterministic: {len(states)} reachable "
                            f"states, colored by steps from start")
            else:
                subtitle = "autonomous orbit from initial state"
                tick_index = None

    if not orbits:
        ax.text(0.5, 0.5,
                "N/A: no orbit produced\n(no initial state and no successor)",
                ha="center", va="center", fontsize=13)
        ax.set_title(title)
        ax.axis("off")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    # Scatter each orbit as discrete dots, colored by tick.
    cmap = plt.get_cmap("viridis")
    if tick_index is not None:
        max_t = max(tick_index) + 1
    else:
        max_t = max(len(o) for o in orbits)
    markers = ["o", "s", "^", "D", "v", "P", "X", "*"]
    is_discrete = not numeric_2d  # discrete/mixed projections may collide on a grid

    scatter_for_bar = None
    for oi, orb in enumerate(orbits):
        xs = [_project(model, xvar, st[xvar["name"]]) for st in orb]
        ys = [_project(model, yvar, st[yvar["name"]]) for st in orb]
        ticks = tick_index if tick_index is not None else list(range(len(orb)))
        # jitter discrete projections slightly so overlapping dots are visible
        if is_discrete and len(orbits) == 1 and max_t > 1:
            import math
            n = len(orb)
            xs = [x + 0.11 * math.sin(i * 2.399) for x, i in zip(xs, range(n))]
            ys = [y + 0.11 * math.cos(i * 2.399) for y, i in zip(ys, range(n))]
        sc = ax.scatter(
            xs, ys, c=ticks, cmap=cmap, vmin=0, vmax=max_t - 1 if max_t > 1 else 1,
            s=46, marker=markers[oi % len(markers)],
            edgecolors="black", linewidths=0.4, alpha=0.85,
            zorder=3,
        )
        scatter_for_bar = sc
        # mark the seed/start with a hollow ring
        ax.scatter([xs[0]], [ys[0]], s=160, facecolors="none",
                   edgecolors="red", linewidths=1.6, zorder=4)

    cbar = fig.colorbar(scatter_for_bar, ax=ax, pad=0.02)
    cbar.set_label("steps from start" if tick_index is not None else "tick (time)")

    ax.set_xlabel(_axis_label(xvar))
    ax.set_ylabel(_axis_label(yvar))
    ax.set_title(f"{title}\n{subtitle}", fontsize=12)
    ax.grid(True, linestyle=":", alpha=0.4)

    # For enum axes, label ticks with variant names.
    for var, setter, getter in ((xvar, ax.set_xticks, ax.set_xticklabels),
                                 (yvar, ax.set_yticks, ax.set_yticklabels)):
        if var["kind"] == "enum":
            variants = model.enum_variants[var["name"]]
            setter(range(len(variants)))
            getter(variants, rotation=30, ha="right", fontsize=8)
        elif var["kind"] == "bool":
            setter([0, 1])
            getter(["false", "true"], fontsize=9)

    legend_bits = [
        Line2D([0], [0], marker="o", color="w", markerfacecolor="gray",
               markeredgecolor="black", markersize=8, label="state @ tick"),
        Line2D([0], [0], marker="o", color="w", markerfacecolor="none",
               markeredgecolor="red", markersize=11, label="seed / start"),
    ]
    if numeric_2d and len(orbits) > 1:
        legend_bits.append(
            Line2D([0], [0], marker="s", color="w", markerfacecolor="gray",
                   markeredgecolor="black", markersize=8,
                   label=f"{len(orbits)} seeds (marker shape)"))
    ax.legend(handles=legend_bits, loc="best", fontsize=8, framealpha=0.9)

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
