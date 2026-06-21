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

VIZ_TYPE = "fixedpoint_map"


# --------------------------------------------------------------------------
# projection: a state dict -> (x, y) float pair, choosing two axes to plot.
# --------------------------------------------------------------------------
def ordinal(model, var, value):
    """Map any value to a float coordinate for plotting."""
    k = var["kind"]
    if k in ("int", "real"):
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        return float(model.enum_variants[var["name"]].index(value))
    if k == "string":
        return float(hash(value) % 997)
    return 0.0


def pick_axes(model):
    """Choose two state vars to use as the X and Y axes of the projection.

    Prefer numeric vars (they carry the interesting continuous geometry);
    fall back to enum, then bool. Returns (xvar, yvar) — yvar may be None if
    the system has only a single var (1-D projection on a baseline)."""
    order = {"int": 0, "real": 0, "enum": 1, "bool": 2, "string": 3}
    ranked = sorted(model.state_vars, key=lambda v: order.get(v["kind"], 9))
    xvar = ranked[0]
    yvar = ranked[1] if len(ranked) > 1 else None
    return xvar, yvar


def axis_label(var):
    return f"{var['name']}  [{var['kind']}]"


# --------------------------------------------------------------------------
# sampling: produce a list of state dicts to probe.
# --------------------------------------------------------------------------
def numeric_range(model, var, samples_estimate):
    """Heuristic grid range for a numeric var. Van der Pol fixed-point IR uses
    fixed-point ints scaled to ~±3000 with a limit cycle near r~2000, so a box
    of [-3200, 3200] with a coarse-ish grid is the right default."""
    init = model.initial_state()
    base = 3200.0
    if init is not None:
        v = abs(float(init[var["name"]]))
        if v > base:
            base = v * 1.2
    lo, hi = -base, base
    n = max(2, int(samples_estimate))
    step = (hi - lo) / (n - 1)
    return [lo + i * step for i in range(n)]


def grid_states(model, max_points=900):
    """Grid-scan numeric axes, enumerating discrete axes. Returns list of state
    dicts spanning a representative box of the state space."""
    numeric = [v for v in model.state_vars if v["kind"] in ("int", "real")]
    discrete = [v for v in model.state_vars if v["kind"] not in ("int", "real")]

    # Discrete combinations (bounded; bail to a single slice if it explodes).
    def disc_domain(v):
        if v["kind"] == "bool":
            return [False, True]
        if v["kind"] == "enum":
            return list(model.enum_variants[v["name"]])
        return [None]

    disc_combos = [{}]
    for v in discrete:
        dom = disc_domain(v)
        new = []
        for combo in disc_combos:
            for val in dom:
                c = dict(combo)
                c[v["name"]] = val
                new.append(c)
        disc_combos = new
        if len(disc_combos) > 64:
            disc_combos = disc_combos[:64]
            break

    if not numeric:
        # purely discrete -> the discrete combos ARE the sample (but we prefer
        # the exact reachable set; handled by caller). Return combos as states.
        return [dict(c) for c in disc_combos]

    # Budget grid resolution so total points stay under max_points.
    per_axis = max(2, int((max_points / max(1, len(disc_combos))) ** (1.0 / len(numeric))))
    per_axis = min(per_axis, 40)
    axis_vals = {v["name"]: numeric_range(model, v, per_axis) for v in numeric}

    states = []
    for combo in disc_combos:
        # cartesian product over numeric axes
        idxs = [0] * len(numeric)
        total = 1
        for v in numeric:
            total *= len(axis_vals[v["name"]])
        for flat in range(total):
            st = dict(combo)
            rem = flat
            for v in numeric:
                vals = axis_vals[v["name"]]
                st[v["name"]] = int(vals[rem % len(vals)]) if v["kind"] == "int" else vals[rem % len(vals)]
                rem //= len(vals)
            states.append(st)
            if len(states) >= max_points:
                return states
    return states


def sample_states(model):
    """Return (states, mode).

    The reachable set from the initial state IS the real dynamics, so prefer it
    whenever it's non-trivial (vending's limit cycle, dungeon's graph). Only
    when it collapses to a point/pair AND the system carries numeric axes
    (vanderpol: reachable = the origin fixed point alone) do we fall back to a
    phase-space GRID scan to expose the surrounding orbits."""
    reach, _edges = model.reachable(limit=5000)
    has_numeric = any(v["kind"] in ("int", "real") for v in model.state_vars)
    if len(reach) > 2 or (reach and not has_numeric):
        return reach, "reachable"
    if has_numeric:
        grid = grid_states(model)
        # keep the reachable point(s) too (the true fixed point), unioned in.
        keys = {model._key(s) for s in grid}
        for s in reach:
            if model._key(s) not in keys:
                grid.append(s)
        return grid, "grid"
    return reach, "reachable"


# --------------------------------------------------------------------------
# attractor detection
# --------------------------------------------------------------------------
def state_key(model, st):
    return tuple(st[v["name"]] for v in model.state_vars)


def near(model, a, b, tol):
    """Approximate equality: exact on discrete axes, within tol on numeric."""
    for v in model.state_vars:
        av, bv = a[v["name"]], b[v["name"]]
        if v["kind"] in ("int", "real"):
            if abs(float(av) - float(bv)) > tol:
                return False
        else:
            if av != bv:
                return False
    return True


def is_absorbing(model, s, tol):
    """A genuine resting state: EVERY successor is (approximately) s itself.
    A self-loop that ALSO has other exits is not 'at rest' — it can leave."""
    try:
        succs = model.successors(s, limit=8)
    except Exception:
        one = model.successor(s)
        succs = [one] if one is not None else []
    if not succs:
        return False
    return all(near(model, s, t, tol) for t in succs)


def find_cycle_from(model, s, tol, max_len):
    """Follow ONE deterministic successor chain from s. If it returns near an
    earlier chain node, return that loop [a, b, ..., a] (period >= 2). Else None.
    `max_len` bounds the chain so long numeric limit cycles still close."""
    chain = [s]
    cur = s
    for _ in range(max_len):
        nxt = model.successor(cur)
        if nxt is None:
            return None
        for j, c in enumerate(chain):
            if near(model, nxt, c, tol):
                loop = chain[j:] + [chain[j]]
                return loop if len(loop) >= 3 else None  # period >= 2
        chain.append(nxt)
        cur = nxt
    return None


def find_attractors(model, states, mode):
    """Returns (fixed_points, cycles).

    fixed_points: absorbing states (every successor maps back to the state).
    cycles: distinct short/limit cycles [s0, ..., s0] (period >= 2).

    Discrete: scan reachable states with exact equality. Numeric: scan grid
    seeds with a coarse tolerance, and allow long chains so the limit cycle —
    whose per-tick step is small — has room to close."""
    # Cycle-closing needs slack on a coarse grid; a FIXED point must truly not
    # move (step ~ 0), so it gets a tight tolerance regardless of mode.
    cyc_tol = 30.0 if mode == "grid" else 0.0
    fix_tol = 1.0 if mode == "grid" else 0.0
    max_len = 360 if mode == "grid" else 40

    fixed = []
    cycles = []
    seen_cycle_keys = set()

    # Deep-probing every grid point for a long chain is expensive; for numeric
    # systems a handful of well-placed seeds reveal the same limit cycle.
    if mode == "grid":
        seeds = pick_numeric_seeds(model, states)
    else:
        seeds = states

    for s in states:
        if is_absorbing(model, s, fix_tol):
            fixed.append(s)

    for s in seeds:
        if any(near(model, s, f, fix_tol) for f in fixed):
            continue
        loop = find_cycle_from(model, s, cyc_tol, max_len)
        if loop is None:
            continue
        # dedupe cycles by their member set (coarsened on numeric axes)
        key = frozenset(coarse_key(model, c, cyc_tol) for c in loop[:-1])
        if key in seen_cycle_keys:
            continue
        seen_cycle_keys.add(key)
        cycles.append(loop)

    # Numeric systems whose orbits spiral onto an attractor (van der Pol) only
    # close after a long transient + full period — too long for the per-seed
    # chain above. Extract the limit cycle directly: run one long trajectory,
    # drop the transient, and take the SETTLED tail as the orbit.
    if mode == "grid" and not cycles:
        orbit = extract_limit_cycle(model, seeds, fixed, fix_tol)
        if orbit is not None:
            cycles.append(orbit)

    return fixed, cycles


def extract_limit_cycle(model, seeds, fixed, fix_tol):
    """Run a long trajectory from a mid-radius seed; if it settles onto a
    recurring orbit (tail returns near an earlier tail point), return that
    closed orbit. Returns a loop [p0, ..., p0] or None."""
    import math
    candidates = [s for s in seeds
                  if not any(near(model, s, f, fix_tol) for f in fixed)]
    for seed in candidates:
        cur = seed
        chain = [cur]
        for _ in range(700):
            nxt = model.successor(cur)
            if nxt is None:
                break
            chain.append(nxt)
            cur = nxt
        if len(chain) < 200:
            continue
        # search the settled tail for a near-recurrence (a closed loop)
        tail_start = int(len(chain) * 0.45)
        best = None
        for i in range(len(chain) - 1, tail_start + 30, -1):
            for j in range(tail_start, i - 30):
                d = _numeric_dist(model, chain[i], chain[j])
                if d <= 40.0:
                    best = (j, i)
                    break
            if best:
                break
        if best:
            j, i = best
            loop = chain[j:i] + [chain[j]]
            if len(loop) >= 4:
                return loop
    return None


def _numeric_dist(model, a, b):
    import math
    s = 0.0
    for v in model.state_vars:
        if v["kind"] in ("int", "real"):
            s += (float(a[v["name"]]) - float(b[v["name"]])) ** 2
    return math.sqrt(s)


def coarse_key(model, st, tol):
    parts = []
    q = max(tol, 1.0)
    for v in model.state_vars:
        val = st[v["name"]]
        if v["kind"] in ("int", "real"):
            parts.append(round(float(val) / q))
        else:
            parts.append(val)
    return tuple(parts)


def pick_numeric_seeds(model, states):
    """A spread of seeds across the scanned box: a ring of mid-radius points
    (likely to land in the limit-cycle basin) plus a few near-origin points
    (to catch a central fixed point's basin)."""
    numeric = [v for v in model.state_vars if v["kind"] in ("int", "real")]
    if len(numeric) < 2:
        return states[: min(len(states), 60)]
    import math
    xv, yv = numeric[0], numeric[1]
    base = max(abs(ordinal(model, xv, s[xv["name"]])) for s in states) or 3000.0
    seeds = []
    template = dict(states[0]) if states else {}
    for r in (0.15, 0.5, 0.85):
        for k in range(8):
            a = 2 * math.pi * k / 8
            st = dict(template)
            st[xv["name"]] = int(r * base * math.cos(a)) if xv["kind"] == "int" else r * base * math.cos(a)
            st[yv["name"]] = int(r * base * math.sin(a)) if yv["kind"] == "int" else r * base * math.sin(a)
            seeds.append(st)
    return seeds


# --------------------------------------------------------------------------
# plotting
# --------------------------------------------------------------------------
def render(smt2, schema, out_path):
    model = load(smt2, schema)
    xvar, yvar = pick_axes(model)

    fig, ax = plt.subplots(figsize=(9, 8))
    title = f"{model.fsm} — {VIZ_TYPE}"

    states, mode = sample_states(model)

    if not states:
        placeholder(ax, title, "no states could be sampled from the transition")
        finish(fig, out_path)
        return out_path

    def proj(st):
        x = ordinal(model, xvar, st[xvar["name"]])
        y = ordinal(model, yvar, st[yvar["name"]]) if yvar else 0.0
        return x, y

    # background: faint dots for every sampled state (the basin).
    bx = [proj(s)[0] for s in states]
    by = [proj(s)[1] for s in states]
    ax.scatter(bx, by, s=10, c="#d9d9e3", alpha=0.55, linewidths=0,
               zorder=1, label=f"sampled states ({len(states)})")

    fixed, cycles = find_attractors(model, states, mode)

    # cycle members + loop arrows. Short cycles -> arrowed polygon; long
    # (limit-cycle) orbits -> a connected line with sparse direction arrows.
    cyc_pts_x, cyc_pts_y = [], []
    labelled = False
    for loop in cycles:
        pts = [proj(s) for s in loop]
        long_orbit = len(loop) > 12
        if long_orbit:
            ax.plot([p[0] for p in pts], [p[1] for p in pts],
                    color="#1f77b4", alpha=0.85, lw=1.8, zorder=3,
                    label=None if labelled else f"limit cycle(s) ({len(cycles)})")
            labelled = True
            # a few direction arrows around the orbit
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
                   label=None if labelled else f"cycle members ({len(cycles)} cycle(s))")

    # fixed points on top
    if fixed:
        fx = [proj(s)[0] for s in fixed]
        fy = [proj(s)[1] for s in fixed]
        ax.scatter(fx, fy, s=160, c="#d62728", marker="o",
                   edgecolors="black", linewidths=1.0, zorder=5,
                   label=f"fixed points ({len(fixed)})")

    ax.set_title(title, fontsize=14, fontweight="bold")
    ax.set_xlabel(axis_label(xvar))
    ax.set_ylabel(axis_label(yvar) if yvar else "(single-axis projection)")

    # enum / bool axes: label the ticks with variant names / true-false.
    decorate_axis(ax, model, xvar, "x")
    if yvar:
        decorate_axis(ax, model, yvar, "y")

    subtitle = f"scan: {mode}   |   "
    if not fixed and not cycles:
        subtitle += "no fixed points or short cycles found in the scanned region"
    else:
        bits = []
        if fixed:
            bits.append(f"{len(fixed)} fixed point(s)")
        if cycles:
            lens = sorted({len(c) - 1 for c in cycles})
            bits.append(f"{len(cycles)} cycle(s) (period {lens})")
        subtitle += "  +  ".join(bits)
    ax.text(0.5, -0.10, subtitle, transform=ax.transAxes, ha="center",
            fontsize=9, color="#555555")

    ax.legend(loc="upper right", fontsize=8, framealpha=0.9)
    ax.grid(True, alpha=0.2)
    finish(fig, out_path)
    return out_path


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
