#!/usr/bin/env python3
"""render_timing_diagram — EE-style timing/waveform diagram for any Evident IR.

One horizontal track per state variable, plotted against tick number:

  * bool / enum / string vars  -> DIGITAL waveform. The value is held flat
    between ticks and jumps on a vertical edge at each transition (classic
    logic-analyzer look). Enums map to ordinal lanes; the active variant name
    is printed at each level.
  * int / real vars            -> ANALOG track. The numeric value is drawn as a
    line over ticks, normalized into the track's band.

The dynamics come entirely from querying the transition relation via
evident_viz (z3). We follow ONE successor chain for ~40 ticks. For purely
autonomous systems whose own initial state is a fixed point (e.g. vanderpol's
origin), we pick a non-trivial seed so the waveform actually moves; otherwise
we start from the program's initial_state.

Usage:
    python3 viz/render_timing_diagram.py <smt2> <schema> <out.png>
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__)))

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.lines import Line2D

from evident_viz import load

TICKS = 40
DIGITAL = ("bool", "enum", "string")


def pick_seed(m):
    """A starting state for the trajectory.

    Prefer the program's own initial_state. But if that initial state is a
    fixed point (successor == itself), the waveform would be a flat line — so
    for numeric systems we fall back to a non-trivial off-axis seed to excite
    the dynamics.
    """
    init = m.initial_state()
    if init is not None:
        nxt = m.successor(init)
        if nxt is not None and m._key(nxt) != m._key(init):
            return init  # initial state already moves; use it

    # Need a seed. For numeric systems, perturb off the fixed point.
    numeric = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    if numeric:
        seed = {}
        # heuristic seeds biased for the fixed-point-at-origin limit-cycle case
        for i, v in enumerate(m.state_vars):
            if v["kind"] == "int":
                seed[v["name"]] = 2800 if i == 0 else 0
            elif v["kind"] == "real":
                seed[v["name"]] = 2.8 if i == 0 else 0.0
            elif v["kind"] == "bool":
                seed[v["name"]] = False
            elif v["kind"] == "enum":
                seed[v["name"]] = m.enum_variants[v["name"]][0]
            elif v["kind"] == "string":
                seed[v["name"]] = ""
        # only use the seed if it actually has a successor
        if m.successor(seed) is not None:
            return seed

    return init  # may be a fixed point (we'll degrade to a flat trace)


def _advance(m, cur, prefer_change, visited):
    """One step of the walk. For nondeterministic systems (prefer_change), pick
    a successor that actually changes the state — and, when possible, one not
    yet visited — so the waveform explores the program rather than parking on a
    self-loop. Falls back to the lone successor()."""
    if not prefer_change:
        return m.successor(cur)
    succ = m.successors(cur, limit=32)
    if not succ:
        return None
    changed = [s for s in succ if m._key(s) != m._key(cur)]
    pool = changed or succ
    fresh = [s for s in pool if m._key(s) not in visited]
    return (fresh or pool)[0]


def build_trace(m, steps=TICKS):
    """A list of state dicts of length up to steps+1, following one successor
    chain. Holds the last state if the chain dies / hits a fixed point so the
    waveform spans the full time axis."""
    cur = pick_seed(m)
    if cur is None:
        return []
    # On nondeterministic discrete programs the lone successor() can sit on a
    # self-loop; walk via successors() preferring a state-changing edge.
    prefer_change = m.is_discrete()
    trace = [cur]
    visited = {m._key(cur)}
    for _ in range(steps):
        nxt = _advance(m, cur, prefer_change, visited)
        if nxt is None:
            break
        trace.append(nxt)
        visited.add(m._key(nxt))
        cur = nxt
    # pad to full width by holding the last value (a fixed point reads as flat)
    while len(trace) < steps + 1:
        trace.append(trace[-1])
    return trace


def render(m, out_path):
    trace = build_trace(m)
    fig_title = f"{m.fsm}  —  timing_diagram"

    if not trace:
        fig, ax = plt.subplots(figsize=(11, 3))
        ax.axis("off")
        ax.text(0.5, 0.5, "N/A: no transition (no reachable trajectory)",
                ha="center", va="center", fontsize=13)
        ax.set_title(fig_title, fontsize=13, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    n = len(trace)
    ticks = list(range(n))
    nvar = len(m.state_vars)

    # vertical layout: each var gets a unit-height lane, stacked top to bottom
    lane_h = 1.0
    gap = 0.55
    fig_h = max(2.4, 0.95 * nvar + 1.4)
    fig, ax = plt.subplots(figsize=(12, fig_h))

    digital_color = "#1f77b4"
    analog_color = "#d62728"
    enum_color = "#2ca02c"

    yticklabels = []
    yticks = []

    for idx, v in enumerate(m.state_vars):
        # lanes top-to-bottom: first var at top
        base = (nvar - 1 - idx) * (lane_h + gap)
        name = v["name"]
        kind = v["kind"]
        vals = [trace[t][name] for t in range(n)]

        yticks.append(base + lane_h / 2)
        yticklabels.append(f"{name}\n[{kind}]")

        # lane separator background band
        ax.axhspan(base - gap / 2, base + lane_h + gap / 2,
                   facecolor="#f7f7f7" if idx % 2 == 0 else "#ffffff",
                   zorder=0)

        if kind == "bool":
            ys = [base + (lane_h if vals[t] else 0.0) for t in range(n)]
            ax.step(ticks, ys, where="post", color=digital_color, lw=2, zorder=3)
            ax.fill_between(ticks, base, ys, step="post",
                            color=digital_color, alpha=0.12, zorder=2)
            ax.text(-0.6, base, "0", va="center", ha="right",
                    fontsize=7, color="#888")
            ax.text(-0.6, base + lane_h, "1", va="center", ha="right",
                    fontsize=7, color="#888")

        elif kind in ("enum", "string"):
            if kind == "enum":
                variants = m.enum_variants[name]
            else:
                # build an ordinal order from observed string values
                variants = sorted(set(vals), key=lambda s: (s != "", s))
            nv = max(1, len(variants))
            order = {variant: i for i, variant in enumerate(variants)}
            ys = [base + (order.get(vals[t], 0) / max(1, nv - 1)) * lane_h
                  for t in range(n)]
            ax.step(ticks, ys, where="post", color=enum_color, lw=2, zorder=3)
            # annotate each held segment with the variant name on a transition
            last = None
            for t in range(n):
                if vals[t] != last:
                    ax.text(t + 0.08, ys[t] + 0.06, str(vals[t]),
                            fontsize=7, color="#1a661a", va="bottom", zorder=4)
                    last = vals[t]
            # lane min/max variant labels
            ax.text(-0.6, base, str(variants[0]), va="center", ha="right",
                    fontsize=6, color="#888")
            if nv > 1:
                ax.text(-0.6, base + lane_h, str(variants[-1]),
                        va="center", ha="right", fontsize=6, color="#888")

        else:  # int / real -> analog
            vmin, vmax = min(vals), max(vals)
            span = (vmax - vmin) or 1.0
            ys = [base + (vals[t] - vmin) / span * lane_h for t in range(n)]
            ax.plot(ticks, ys, color=analog_color, lw=1.6, marker="o",
                    markersize=2.5, zorder=3)
            ax.text(-0.6, base, f"{vmin}", va="center", ha="right",
                    fontsize=7, color="#888")
            ax.text(-0.6, base + lane_h, f"{vmax}", va="center", ha="right",
                    fontsize=7, color="#888")

        # baseline grid line for the lane
        ax.axhline(base, color="#cccccc", lw=0.5, zorder=1)

    ax.set_yticks(yticks)
    ax.set_yticklabels(yticklabels, fontsize=8)
    ax.set_xlim(-0.5, n - 1 + 0.5)
    ax.set_ylim(-gap, nvar * (lane_h + gap) - gap / 2)
    ax.set_xlabel("tick", fontsize=10)
    # tick grid
    ax.set_xticks(range(0, n, max(1, n // 20)))
    ax.grid(axis="x", color="#eeeeee", lw=0.5, zorder=0)
    for spine in ("top", "right", "left"):
        ax.spines[spine].set_visible(False)

    legend = [
        Line2D([0], [0], color=digital_color, lw=2, label="bool (digital)"),
        Line2D([0], [0], color=enum_color, lw=2, label="enum/string (lanes)"),
        Line2D([0], [0], color=analog_color, lw=1.6, marker="o",
               markersize=3, label="int/real (analog)"),
    ]
    ax.legend(handles=legend, loc="upper right", fontsize=7,
              framealpha=0.9, ncol=3)

    ax.set_title(fig_title + f"   ({n - 1} ticks, seed = {m.label(trace[0])})",
                 fontsize=12, fontweight="bold")
    fig.tight_layout()
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def main():
    if len(sys.argv) != 4:
        print(__doc__)
        sys.exit(2)
    smt2, schema, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    m = load(smt2, schema)
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    render(m, out_path)
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
