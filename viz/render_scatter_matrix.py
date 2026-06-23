#!/usr/bin/env python3
"""render_scatter_matrix.py — scatterplot-matrix renderer for ANY Evident IR.

A scatterplot matrix (pairwise projections) over ALL state vars, ordered by
importance (m.state_vars). We sample a cloud of states from the transition
relation (m.reachable() for discrete; a long trajectory + a grid sweep for
numeric), map every var to an ordinal axis (int/real -> value, bool -> 0/1,
enum -> variant index), then draw the NxN grid of 2-D scatter panels with the
variable name on each diagonal.

Channel mapping: the two PROJECTION axes of each panel are the matrix's row/col
vars (position — the strongest channel, carrying every var pairwise). The third
dimension is COLOR: every point is hued by the top categorical variable
(m.categorical_vars[0]) — the classic high-D scatter-matrix coloring — with a
legend. One legend for the whole figure; the panels read from their axes alone.

The dynamics come ONLY from querying the shared evident_viz Model — nothing is
hardcoded per-program.

    python3 viz/render_scatter_matrix.py <smt2> <schema> <out.png>
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load  # noqa: E402

import z3  # noqa: E402
import matplotlib  # noqa: E402
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402


def ordinal(m, var, value):
    """Map a state value to a numeric axis coordinate."""
    k = var["kind"]
    if k == "int":
        return float(value)
    if k == "real":
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        return float(m.enum_variants[var["name"]].index(value))
    if k == "string":
        return 0.0  # strings get a placeholder axis
    return 0.0


def tick_info(m, var):
    """Return (ticks, ticklabels) for a categorical axis, else (None, None)."""
    k = var["kind"]
    if k == "bool":
        return [0, 1], ["F", "T"]
    if k == "enum":
        vs = m.enum_variants[var["name"]]
        return list(range(len(vs))), vs
    return None, None


def sample_states(m):
    """A cloud of states drawn from the program's REACHABLE set + a parallel list
    of (i,j) edges (as index pairs into the cloud) when cheaply available.
    Returns (states, edges_or_None).

    The cloud is always anchored to states the program ACTUALLY visits — never a
    hardcoded ±3000 box. For discrete programs this is the exact reachable graph;
    for numeric ones it's the reachable cloud (capped) plus a long trajectory for
    the attractor / limit cycle. Any supplementary grid sweep is confined to the
    REACHABLE extent (m.axis_bounds), so the off-diagonal panels show the real
    vector field over the domain the program enters, not invented structure over a
    guessed plane."""
    # Reachable graph: exact for discrete, the true visited cloud for numeric. A scatter cloud reads
    # the same at ~800 points as at 5000 — overplotting adds nothing but seconds (reachable(5000) on a
    # 4-var FSM was ~6s, the dominant cost of that analyze; #217). Cap the cloud; the boundary box +
    # the analyze's own bounds still convey the full extent.
    states, edges = m.reachable(limit=800)
    if m.is_discrete():
        return states, edges

    # Numeric / mixed. The reachable-from-init cloud is the ground truth, but for a
    # continuous oscillator the init may sit at a fixed point whose basin is tiny
    # (e.g. van der Pol relaxes to (0,0) from the origin while the limit cycle lives
    # far out). So we ALSO probe the attractor with trajectories seeded off the
    # fixed point — and we scale everything to the extent those trajectories
    # actually trace (the limit-cycle extent), never a hardcoded ±3000 box.
    if not states:
        states = []
    edges = None

    init = m.initial_state()
    for st in m.trajectory(start=init, steps=400):
        states.append(st)

    int_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]

    def extent(name):
        vals = [s[name] for s in states if type(s.get(name)) in (int, float)]
        return (min(vals), max(vals)) if vals else (0.0, 0.0)

    # Does the cloud we have ALREADY span a real domain? If the reachable set has
    # genuine variation (a terminating counter visits a dozen distinct states), THAT
    # is the whole truth — plot it directly, no sweep, no invented structure.
    if len(int_vars) >= 2:
        a, b = int_vars[0]["name"], int_vars[1]["name"]
        ax_lo, ax_hi = extent(a)
        bx_lo, bx_hi = extent(b)
        degenerate = (ax_hi - ax_lo) < 1e-6 and (bx_hi - bx_lo) < 1e-6
    else:
        degenerate = False

    if not degenerate:
        # The reachable cloud is the real, fully-enumerated picture (or there's only
        # one numeric axis). Don't sweep — sweeping a finite program's lattice
        # fabricates states it never enters. Return what's actually visited.
        return states, edges

    # Degenerate: the reachable set is a single fixed point, but this is a continuous
    # system (the init sits at a fixed point whose basin is tiny — e.g. van der Pol
    # relaxes to (0,0) from the origin while the limit cycle lives far out). Probe
    # outward on a geometric ladder of off-origin seeds to capture the attractor, and
    # let the orbit's OWN extent set the axes — never a hardcoded ±3000 box.
    a, b = int_vars[0]["name"], int_vars[1]["name"]
    for scale in (1, 4, 16, 64, 256, 1024):
        for (sx, sy) in [(scale, 0), (0, scale), (-scale, scale), (scale, scale)]:
            traj = m.trajectory(start={**init, a: sx, b: sy}, steps=400)
            if len(traj) > 2:
                states.extend(traj)
    ax_lo, ax_hi = extent(a)
    bx_lo, bx_hi = extent(b)

    # Vector-field sweep, confined to the DISCOVERED attractor extent (never ±3000).
    # Only when even the attractor probe found nothing finite do we fall back to a
    # default wide box (genuinely unbounded continuous dynamics with no orbit).
    def axis_grid(lo, hi, default_name, n=9):
        if hi <= lo + 1e-9:
            bnds = m.axis_bounds(default_name)
            lo, hi = bnds if bnds is not None else (-3000.0, 3000.0)
        if hi <= lo:
            return [lo]
        step = (hi - lo) / (n - 1)
        return [lo + step * k for k in range(n)]

    gx, gy = axis_grid(ax_lo, ax_hi, a), axis_grid(bx_lo, bx_hi, b)
    base = init.copy()
    for x in gx:
        for y in gy:
            s = base.copy()
            s[a] = x
            s[b] = y
            nxt = m.successor(s)
            if nxt is not None:
                states.append(s)
                states.append(nxt)
    return states, edges


def _draw_scatter_matrix(m, vars_, states, title, subtag, out):
    """Draw the NxN pairwise scatter matrix over `vars_` for the sampled `states`.

    Shared by BOTH the FSM path (states = a reachable/trajectory cloud) and the
    claim path (states = block-and-resolve witnesses of the constraint). `m` only
    needs to answer `ordinal`/`tick_info` (via the two module helpers) plus
    `enum_variants` / `categorical_vars` / `is_discrete()` — an FSM Model and the
    lightweight ClaimModel below both qualify. `subtag` is the per-cloud caption
    ("reachable cloud + trajectory" for FSM, "z3 witnesses" for a claim)."""
    n = len(vars_)

    # Degrade gracefully: nothing to plot, or single var.
    if not states or n < 1:
        fig, ax = plt.subplots(figsize=(7, 7))
        ax.axis("off")
        ax.text(0.5, 0.5,
                f"N/A for this state: no sampled states\n({n} vars)",
                ha="center", va="center", fontsize=14, wrap=True)
        ax.set_title(title)
        fig.savefig(out, dpi=120, bbox_inches="tight")
        return

    if n == 1:
        # A single var has no pairwise plane: show its 1-D distribution.
        v = vars_[0]
        xs = [ordinal(m, v, s[v["name"]]) for s in states]
        fig, ax = plt.subplots(figsize=(7, 5))
        ax.hist(xs, bins=20, color="#4060c0", alpha=0.8)
        ax.set_xlabel(v["name"])
        t, tl = tick_info(m, v)
        if t is not None:
            ax.set_xticks(t)
            ax.set_xticklabels(tl, rotation=45, ha="right")
        ax.set_title(title + "\n(single var: 1-D distribution; scatter matrix needs >=2)")
        fig.savefig(out, dpi=120, bbox_inches="tight")
        return

    # Precompute ordinal columns once.
    cols = {v["name"]: [ordinal(m, v, s[v["name"]]) for s in states] for v in vars_}

    # Robust per-axis display limits, computed from the PLOTTED points. A numeric
    # var can carry sentinel seeds (±1e6 fold initializers, -1 'none' markers) that
    # a single state injects; left to matplotlib autoscale they blow the axis out to
    # ±1e6 and crush the real cluster to a dot (the csv_stats state.min/state.max
    # defect). We fence each numeric axis to an IQR-robust extent of its own column
    # so the sentinel point falls outside the view while every genuine state stays
    # in-frame (the limit-cycle spread of a continuous orbit is preserved — it has
    # no sentinel, so the fence is wide). Categorical (bool/enum) axes are left
    # alone (they keep their ordinal extent).
    def robust_limits(values):
        vals = sorted(v for v in values if v == v)  # drop NaN
        if not vals:
            return None
        n = len(vals)
        q1, q3 = vals[n // 4], vals[(3 * n) // 4]
        iqr = q3 - q1
        if iqr > 0:  # reject ±1e6 / far-out sentinels via a 3-IQR fence
            lof, hif = q1 - 3 * iqr, q3 + 3 * iqr
            vals = [v for v in vals if lof <= v <= hif] or vals
        lo, hi = float(min(vals)), float(max(vals))
        if lo == hi:
            return (lo - 1.0, hi + 1.0)
        pad = (hi - lo) * 0.08
        return (lo - pad, hi + pad)

    axis_limits = {}
    for v in vars_:
        if v["kind"] in ("int", "real"):
            lim = robust_limits(cols[v["name"]])
            if lim is not None:
                axis_limits[v["name"]] = lim

    # COLOR channel: hue every point by the top categorical var (enum/bool/string).
    # This is the classic high-D scatter-matrix coloring — a 3rd dimension carried
    # on top of every pairwise projection. Falls back to a flat color if the model
    # has no categorical interface var (e.g. a purely numeric system).
    cat = m.categorical_vars[0] if m.categorical_vars else None
    legend_handles = None
    if cat is not None:
        cname = cat["name"]
        # The categories, in a stable order: enum -> variant order, bool -> F/T.
        if cat["kind"] == "enum":
            categories = list(m.enum_variants[cname])
            cat_label = lambda v: v
        elif cat["kind"] == "bool":
            categories = [False, True]
            cat_label = lambda v: "T" if v else "F"
        else:  # string: discover the distinct values that actually occur
            categories = sorted({s[cname] for s in states}, key=str)
            cat_label = str
        cmap = plt.get_cmap("tab10" if len(categories) <= 10 else "tab20")
        idx_of = {c: k for k, c in enumerate(categories)}
        color_of = {c: cmap(k % cmap.N) for k, c in enumerate(categories)}
        # Only categories that actually occur in the sample get a legend entry.
        seen = []
        seen_set = set()
        point_colors = []
        for s in states:
            cv = s[cname]
            point_colors.append(color_of.get(cv, "#888888"))
            if cv not in seen_set:
                seen_set.add(cv)
                seen.append(cv)
        order = [c for c in categories if c in seen_set]
        legend_handles = [
            plt.Line2D([0], [0], marker="o", linestyle="", markersize=7,
                       markerfacecolor=color_of[c], markeredgecolor="none",
                       label=cat_label(c))
            for c in order
        ]
        legend_title = cname
    else:
        point_colors = "#2050b0"

    sz = max(2.0, 12.0 / n)
    fig, axes = plt.subplots(n, n, figsize=(sz * n, sz * n), squeeze=False)

    for i, vi in enumerate(vars_):       # row -> y axis
        for j, vj in enumerate(vars_):   # col -> x axis
            ax = axes[i][j]
            if i == j:
                # Diagonal: var name + a 1-D histogram backdrop. Confine the bins to
                # the robust range so a sentinel outlier doesn't compress every bar
                # into the first bin.
                hrange = axis_limits.get(vi["name"])
                ax.hist(cols[vi["name"]], bins=15, color="#cccccc", range=hrange)
                if hrange is not None:
                    ax.set_xlim(hrange)
                ax.set_yticks([])
                ax.set_xticks([])
                ax.text(0.5, 0.5, vi["name"], transform=ax.transAxes,
                        ha="center", va="center", fontsize=max(8, 14 - n),
                        fontweight="bold", color="#202020")
                continue
            x = cols[vj["name"]]
            y = cols[vi["name"]]
            ax.scatter(x, y, s=10, alpha=0.45, c=point_colors, edgecolors="none")

            # Clamp numeric axes to the robust extent so sentinel seeds
            # (±1e6 / -1) fall off-panel instead of blowing out the scale.
            if vj["name"] in axis_limits:
                ax.set_xlim(axis_limits[vj["name"]])
            if vi["name"] in axis_limits:
                ax.set_ylim(axis_limits[vi["name"]])

            # Categorical ticks where appropriate; keep it readable for large n.
            tx, txl = tick_info(m, vj)
            ty, tyl = tick_info(m, vi)
            if tx is not None and len(tx) <= 8:
                ax.set_xticks(tx)
                if i == n - 1:
                    ax.set_xticklabels(txl, rotation=45, ha="right", fontsize=6)
                else:
                    ax.set_xticklabels([])
            elif i != n - 1:
                ax.set_xticklabels([])
            if ty is not None and len(ty) <= 8:
                ax.set_yticks(ty)
                if j == 0:
                    ax.set_yticklabels(tyl, fontsize=6)
                else:
                    ax.set_yticklabels([])
            elif j != 0:
                ax.set_yticklabels([])

            if i == n - 1:
                ax.set_xlabel(vj["name"], fontsize=max(6, 11 - n))
            if j == 0:
                ax.set_ylabel(vi["name"], fontsize=max(6, 11 - n))
            ax.grid(True, alpha=0.15)

    kind = "discrete" if m.is_discrete() else "numeric/mixed"
    # This view draws its OWN denser cloud (reachable up to 5000 + an attractor trajectory), so its
    # point count legitimately differs from the analyze stats' reachable count (capped at 400). Label
    # it as POINTS, not "states", so the two counts don't read as a contradiction (Ana #201/#77).
    pts = "point" if len(states) == 1 else "points"
    sub = f"{len(states)} sample {pts} ({subtag}) · {kind}"
    if legend_handles is not None:
        sub += f" · color = {legend_title}"
    fig.suptitle(f"{title}\n{sub}", fontsize=14, fontweight="bold")

    if legend_handles:
        # One figure-level legend on the right — the panels read from axes alone;
        # color only ENHANCES with the top categorical var.
        fig.legend(handles=legend_handles, title=legend_title,
                   loc="center left", bbox_to_anchor=(1.0, 0.5),
                   fontsize=9, title_fontsize=10, frameon=True)
        fig.tight_layout(rect=[0, 0, 0.97, 0.97])
        fig.savefig(out, dpi=120, bbox_inches="tight")
    else:
        fig.tight_layout(rect=[0, 0, 1, 0.97])
        fig.savefig(out, dpi=120)


class ClaimModel:
    """A minimal stand-in for the FSM `Model` that the scatter-matrix DRAWER needs,
    backed by a CLAIM's solution-space witnesses rather than a reachable cloud.

    A raw claim (no fsm) has no transition, so the FSM `load()` (which reads
    `schema["fsm"]` and BFS-walks `reachable()`) doesn't apply. Instead the claim's
    SOLUTION SPACE is the cloud: we enumerate distinct satisfying assignments
    (block-and-resolve) and plot the pairwise scatter matrix over its numeric (and
    categorical, for color) variables — exactly the FSM picture, sampling solutions
    instead of states. Exposes just the surface the drawer queries."""

    def __init__(self, name, plot_vars, enum_variants, cat_vars):
        self.fsm = name                       # used only in the title
        self.state_vars = plot_vars           # numeric vars drive the matrix axes
        self.enum_variants = enum_variants    # var name -> [variant names]
        self._cat_vars = cat_vars             # enum/bool vars (the color channel)

    @property
    def categorical_vars(self):
        return self._cat_vars

    def is_discrete(self):
        # A claim's witness sample is a discrete set of solutions; label it "discrete"
        # so the caption ("N points · discrete") reads honestly.
        return True


def claim_witnesses(smt2_path, schema_path, limit=600):
    """Enumerate up to `limit` DISTINCT satisfying assignments of a claim and the vars to
    plot. Block-and-resolve over the claim's int/real/bool/enum vars — the same witness
    enumeration the solve panel / successors() use, but over a static constraint with no
    transition. Returns (states, plot_vars, cat_vars, enum_variants, feasible):
      * states        — list of dicts {full_var_name -> python value}, one per witness
      * plot_vars     — the numeric (int/real) vars to put on the matrix axes
      * cat_vars      — enum/bool vars (the color channel + categorical axes)
      * enum_variants — {var name -> [variant names]} for ordinal/tick mapping
      * feasible      — False iff the claim is UNSAT (states is then empty)

    A purely categorical claim (no numeric var) yields plot_vars=[] → the caller renders
    the honest empty state. Reuses render_claim_space._load_claim for the smt2 parse."""
    import render_claim_space as RC

    sch, body, consts = RC._load_claim(smt2_path, schema_path)
    vars_ = sch.get("vars", [])
    # Only scalars we can map to an axis or a color: int/real (numeric axes),
    # bool/enum (categorical color/axes). Seq/string/set vars aren't scatter-plottable.
    plot_vars = [v for v in vars_ if v.get("kind") in ("int", "real")
                 and v["name"] in consts]
    cat_vars = [v for v in vars_ if v.get("kind") in ("bool", "enum")
                and v["name"] in consts]
    enum_variants = {v["name"]: v.get("variants", [])
                     for v in cat_vars if v.get("kind") == "enum"}

    sample_vars = plot_vars + cat_vars
    s = z3.Solver()
    s.add(body)
    feasible = s.check() == z3.sat
    states = []
    if not feasible or not sample_vars:
        return states, plot_vars, cat_vars, enum_variants, feasible

    while len(states) < limit and s.check() == z3.sat:
        mod = s.model()
        st, block = {}, []
        for v in sample_vars:
            c = consts[v["name"]]
            mv = mod.eval(c, model_completion=True)
            st[v["name"]] = _decode(mv, v["kind"])
            block.append(c != mv)              # differ on SOME observed var → distinct
        states.append(st)
        s.add(z3.Or(*block))
    return states, plot_vars, cat_vars, enum_variants, feasible


def _decode(mv, kind):
    """One z3 model value → python, by the var's declared kind."""
    if kind == "bool":
        return z3.is_true(mv)
    if kind == "enum":
        return mv.decl().name()
    try:
        return mv.as_long()
    except Exception:
        try:
            return round(float(mv.as_fraction()), 6)
        except Exception:
            return 0.0


def _render_claim(smt2, schema, out):
    """Render the scatter matrix for a CLAIM (no fsm): the solution space sampled as a
    cloud of witnesses. Honest empty-state when UNSAT or when the claim has no numeric
    variable to put on an axis."""
    import json as _json
    name = _json.load(open(schema)).get("claim", "claim")
    states, plot_vars, cat_vars, enum_variants, feasible = claim_witnesses(smt2, schema)
    title = f"{name} — scatter_matrix"

    if not feasible:
        return _empty(out, title, "claim is UNSATISFIABLE\n(no assignment satisfies it — nothing to scatter)")
    if len(plot_vars) < 1:
        return _empty(out, title,
                      "claim has no numeric variable to scatter\n"
                      "(its solution space is categorical — see claim_space for its feasibility grid)")

    m = ClaimModel(name, plot_vars, enum_variants, cat_vars)
    _draw_scatter_matrix(m, plot_vars, states, title, "z3 witnesses of the constraint", out)
    return out


def _empty(out, title, msg):
    """Honest empty-state card — UNSAT claim, or no numeric var to scatter."""
    fig, ax = plt.subplots(figsize=(7, 7))
    ax.axis("off")
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13, wrap=True)
    ax.set_title(title)
    fig.savefig(out, dpi=120, bbox_inches="tight")
    return out


def main():
    if len(sys.argv) != 4:
        print("usage: render_scatter_matrix.py <smt2> <schema> <out.png>", file=sys.stderr)
        sys.exit(2)
    smt2, schema, out = sys.argv[1], sys.argv[2], sys.argv[3]

    # A CLAIM schema (no "fsm" key) has no transition to BFS — sample its SOLUTION SPACE
    # instead. The FSM path is unchanged.
    import json as _json
    sch = _json.load(open(schema))
    if "fsm" not in sch and "claim" in sch:
        _render_claim(smt2, schema, out)
        return

    m = load(smt2, schema)
    vars_ = m.state_vars
    title = f"{m.fsm} — scatter_matrix"
    states, _ = sample_states(m)
    _draw_scatter_matrix(m, vars_, states, title, "reachable cloud + trajectory", out)


if __name__ == "__main__":
    main()
