#!/usr/bin/env python3
"""render_scatter_matrix.py — scatterplot-matrix renderer for ANY Evident IR.

A scatterplot matrix (pairwise projections) over ALL state vars. We sample a
cloud of states from the transition relation (m.reachable() for discrete; a long
trajectory + a grid sweep for numeric), map every var to an ordinal axis
(int/real -> value, bool -> 0/1, enum -> variant index), then draw the NxN grid
of 2-D scatter panels with the variable name on each diagonal.

The dynamics come ONLY from querying the shared evident_viz Model — nothing is
hardcoded per-program.

    python3 viz/render_scatter_matrix.py <smt2> <schema> <out.png>
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load  # noqa: E402

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
    """A cloud of states + a parallel list of (i,j) edges (as index pairs into
    the cloud) when cheaply available. Returns (states, edges_or_None)."""
    if m.is_discrete():
        states, edges = m.reachable(limit=5000)
        if states:
            return states, edges
    # Numeric / mixed: long trajectory from a few seeds + a coarse grid sweep.
    states = []

    def add(s):
        if s is not None:
            states.append(s)

    # Trajectories from several seeds give the attractor / limit cycle.
    seeds = []
    init = m.initial_state()
    if init is not None:
        seeds.append(init)

    int_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    if len(int_vars) >= 2:
        a, b = int_vars[0]["name"], int_vars[1]["name"]
        base = init.copy() if init is not None else {v["name"]: 0 for v in m.state_vars}
        for (x, y) in [(2800, 0), (400, 0), (0, 2700), (-1500, 1500), (1200, -1200)]:
            s = base.copy()
            s[a] = x
            s[b] = y
            seeds.append(s)

    for seed in seeds:
        for st in m.trajectory(start=seed, steps=400):
            states.append(st)

    # Grid sweep: pin arbitrary lattice points and take ONE successor each, so
    # the matrix shows the vector field's image, not just the attractor.
    if len(int_vars) >= 2:
        a, b = int_vars[0]["name"], int_vars[1]["name"]
        base = init.copy() if init is not None else {v["name"]: 0 for v in m.state_vars}
        step = 800
        g = list(range(-3200, 3201, step))
        for x in g:
            for y in g:
                s = base.copy()
                s[a] = x
                s[b] = y
                nxt = m.successor(s)
                if nxt is not None:
                    states.append(s)
                    states.append(nxt)
    return states, None


def main():
    if len(sys.argv) != 4:
        print("usage: render_scatter_matrix.py <smt2> <schema> <out.png>", file=sys.stderr)
        sys.exit(2)
    smt2, schema, out = sys.argv[1], sys.argv[2], sys.argv[3]
    m = load(smt2, schema)
    vars_ = m.state_vars
    n = len(vars_)

    title = f"{m.fsm} — scatter_matrix"

    states, _ = sample_states(m)

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

    sz = max(2.0, 12.0 / n)
    fig, axes = plt.subplots(n, n, figsize=(sz * n, sz * n), squeeze=False)

    for i, vi in enumerate(vars_):       # row -> y axis
        for j, vj in enumerate(vars_):   # col -> x axis
            ax = axes[i][j]
            if i == j:
                # Diagonal: var name + a 1-D histogram backdrop.
                ax.hist(cols[vi["name"]], bins=15, color="#cccccc")
                ax.set_yticks([])
                ax.set_xticks([])
                ax.text(0.5, 0.5, vi["name"], transform=ax.transAxes,
                        ha="center", va="center", fontsize=max(8, 14 - n),
                        fontweight="bold", color="#202020")
                continue
            x = cols[vj["name"]]
            y = cols[vi["name"]]
            ax.scatter(x, y, s=8, alpha=0.35, c="#2050b0", edgecolors="none")

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
    fig.suptitle(f"{title}\n{len(states)} sampled states · {kind}",
                 fontsize=14, fontweight="bold")
    fig.tight_layout(rect=[0, 0, 1, 0.97])
    fig.savefig(out, dpi=120)


if __name__ == "__main__":
    main()
