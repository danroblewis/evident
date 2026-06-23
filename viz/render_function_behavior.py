#!/usr/bin/env python3
"""render_function_behavior.py — the BEHAVIOR of the extracted functions (transfer maps).

Diagram 4 of the functionizer family, connecting STRUCTURE → BEHAVIOR. The other views show how the
solver DECOMPOSED the program; this one EVALUATES each per-variable function over its input space to
show what it actually computes: V's next value as a function of the variables it reads (their previous
values). For a numeric output that's a surface/heatmap; for an enum output, a coloured region map —
which directly visualizes the guard PARTITION (which branch wins where).

It samples by pinning the input variables and solving the ongoing (¬first-tick) transition via the
model's own `successor`, so the behaviour shown is exactly what the runtime would compute — never a
re-derivation.
"""
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

sys.path.insert(0, "viz")
from evident_viz import load
from functionize import extract_functions

GRID = 13


def _domain(m, var, ranges):
    """(kind, points) for a carried input variable: enum → its variants; numeric → the REACHABLE
    excursion (the range the system actually visits), not axis_bounds — which is the SOLVED fixed
    point and collapses to ~0 on a converging system, hiding the function's behaviour."""
    if var in getattr(m, "enum_variants", {}):
        return "enum", list(m.enum_variants[var])
    lo, hi = ranges.get(var, (0.0, 5.0))
    if hi - lo < 1e-6:                                   # degenerate → a margin around the point
        c = lo or 0.0
        lo, hi = c - max(1.0, abs(c)), c + max(1.0, abs(c))
    pad = (hi - lo) * 0.05
    return "num", list(np.linspace(lo - pad, hi + pad, GRID))


def _panel(ax, m, V, inputs, base, ranges):
    """Draw V's transfer map over (up to) its two primary input variables."""
    inputs = inputs[:2]
    if not inputs:
        ax.text(0.5, 0.5, f"{V} = constant", ha="center", va="center", transform=ax.transAxes,
                color="#e6edf3"); ax.set_axis_off(); return
    doms = [(iv, *_domain(m, iv, ranges)) for iv in inputs]
    out_enum = V in getattr(m, "enum_variants", {})
    variants = list(m.enum_variants[V]) if out_enum else None

    def sample(assign):
        st = dict(base); st.update(assign)
        nxt = m.successor(st)
        return None if nxt is None else nxt.get(V)

    if len(doms) == 1:                                  # 1-D: input → next V
        iv, kind, pts = doms[0]
        ys = [sample({iv: p}) for p in pts]
        if out_enum:
            idx = [variants.index(y) if y in variants else -1 for y in ys]
            ax.step(range(len(pts)), idx, where="mid", color="#1f77b4", lw=2)
            ax.set_yticks(range(len(variants))); ax.set_yticklabels(variants, fontsize=8)
        else:
            xs = range(len(pts)) if kind == "enum" else pts
            ax.plot(xs, [y if y is not None else np.nan for y in ys], "o-", color="#1f77b4")
        ax.set_xlabel(iv + "  (prev)"); ax.set_ylabel(V + "  (next)")
        if kind == "enum":
            ax.set_xticks(range(len(pts))); ax.set_xticklabels([str(p) for p in pts], fontsize=8)
        return

    (ax_, ka, pa), (ay_, kb, pb) = doms                # 2-D grid
    Z = np.full((len(pb), len(pa)), np.nan)
    for i, va in enumerate(pa):
        for j, vb in enumerate(pb):
            y = sample({ax_: va, ay_: vb})
            if y is None:
                continue
            Z[j, i] = (variants.index(y) if (out_enum and y in variants) else (y if not out_enum else np.nan))
    im = ax.imshow(Z, origin="lower", aspect="auto", cmap="viridis",
                   extent=[0, len(pa), 0, len(pb)])
    cbar = ax.figure.colorbar(im, ax=ax, fraction=0.046, pad=0.04)
    if out_enum:
        cbar.set_ticks(range(len(variants))); cbar.set_ticklabels(variants)
    ax.set_xticks(np.arange(len(pa)) + 0.5)
    ax.set_xticklabels([f"{p:.1f}" if ka == "num" else str(p) for p in pa], fontsize=7, rotation=45)
    ax.set_yticks(np.arange(len(pb)) + 0.5)
    ax.set_yticklabels([f"{p:.1f}" if kb == "num" else str(p) for p in pb], fontsize=7)
    ax.set_xlabel(ax_ + "  (prev)"); ax.set_ylabel(ay_ + "  (prev)")


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    f = extract_functions(m)
    prev_to_var = {v["prev"]: v["name"] for v in m.carried if v.get("prev")}
    base = m.initial_state() or {v["name"]: None for v in m.carried}
    steps = f["steps"]
    if not steps:
        _placeholder(out_path, m.fsm, "no functionized variables to sample"); return
    # Reachable excursion per variable — the real input range to sample over.
    states, _ = m.reachable(limit=400)
    ranges = {}
    for v in m.carried:
        nm = v["name"]
        vals = [s[nm] for s in states if isinstance(s.get(nm), (int, float)) and not isinstance(s.get(nm), bool)]
        if vals:
            ranges[nm] = (min(vals), max(vals))
    n = len(steps)
    fig, axes = plt.subplots(1, n, figsize=(5.6 * n, 4.8), squeeze=False)
    for ax, s in zip(axes[0], steps):
        deps = sorted({d for b in s.get("branches", []) for d in b["deps"]} | set(s.get("deps", [])))
        inputs = [prev_to_var[d] for d in deps if d in prev_to_var]
        _panel(ax, m, s["var"], inputs, base, ranges)
        ax.set_title(f"{s['var']} = f({', '.join(inputs) or '·'})", fontsize=11)
    fig.suptitle(f"{m.fsm}  —  function behaviour (each variable's next value over its inputs)",
                 fontsize=12)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def _placeholder(out_path, fsm, msg):
    fig, ax = plt.subplots(figsize=(8, 6))
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13)
    ax.set_axis_off(); ax.set_title(f"{fsm}  —  function behaviour")
    fig.savefig(out_path, dpi=120, bbox_inches="tight"); plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_function_behavior.py <smt2> <schema> <out>", file=sys.stderr); sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
