#!/usr/bin/env python3
"""render_time_series — trajectory-over-tick renderer for ANY Evident IR.

Follows one successor chain (~60 ticks) from a seed state and plots every
state variable against tick number on stacked subplots that share the tick
axis:

  * numeric vars (int/real)  -> line plot
  * bool/enum/string vars    -> step plot (post-step), y-ticks labelled with
                                the variant / true|false names

The dynamics come entirely from querying the transition relation via
evident_viz — nothing about the three sample programs is hardcoded here. A
small per-program seed table only chooses an interesting START point for the
numeric phase systems (whose own initial_state is a fixed point at the origin);
everything else falls back to m.initial_state().

Usage:
    python3 viz/render_time_series.py <smt2> <schema> <out_path.png>
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__)))
sys.path.insert(0, "viz")

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

from evident_viz import load

STEPS = 60


def pick_seed(m):
    """Choose a START state for the trajectory.

    initial_state() is correct for most programs, but for the numeric phase
    systems it is the origin (a fixed point) — a flat, boring trajectory. When
    the initial state is a numeric fixed point, nudge off it so the time series
    actually shows the dynamics. This is generic: detect "numeric + initial is a
    self-loop" and offset, rather than hardcoding the van der Pol name.
    """
    init = m.initial_state()
    numeric = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    if init is not None and numeric:
        nxt = m.successor(init)
        is_fixed = nxt is not None and all(
            init[v["name"]] == nxt[v["name"]] for v in m.state_vars
        )
        if is_fixed:
            seed = dict(init)
            # Offset along the first numeric axis to leave the fixed point.
            v0 = numeric[0]["name"]
            cur = init.get(v0, 0)
            seed[v0] = (cur if isinstance(cur, (int, float)) else 0) + 2000
            # Verify the offset is a live state; if not, fall back to init.
            if m.successor(seed) is not None:
                return seed
    return init


def walk(m, seed, steps):
    """Follow a trajectory, but for nondeterministic transitions prefer a
    successor we have not visited yet — so a discrete system with self-loops
    (e.g. an adjacency graph where staying put is legal) produces an exploring
    walk instead of immediately parking on a self-edge. Generic: it just asks
    successors() for the fan and picks a fresh one when available."""
    cur = seed
    path = [cur]
    seen = {m._key(cur)}
    for _ in range(steps):
        nxts = m.successors(cur)
        if not nxts:
            break
        fresh = [s for s in nxts if m._key(s) not in seen]
        nxt = fresh[0] if fresh else nxts[0]
        path.append(nxt)
        k = m._key(nxt)
        if k in seen:        # only self-loops / already-seen remain -> stop
            break
        seen.add(k)
        cur = nxt
    return path


def to_ordinal(m, var, value):
    """Map a non-numeric value to a y-coordinate + its label."""
    k = var["kind"]
    if k == "bool":
        return (1 if value else 0), str(bool(value)).lower()
    if k == "enum":
        variants = m.enum_variants.get(var["name"], [])
        idx = variants.index(value) if value in variants else 0
        return idx, str(value)
    # string
    return 0, str(value)


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    seed = pick_seed(m)

    if seed is None:
        fig, ax = plt.subplots(figsize=(10, 4))
        ax.axis("off")
        ax.text(0.5, 0.5,
                f"N/A for {m.fsm}: no initial state\n(transition has no first-tick model)",
                ha="center", va="center", fontsize=14)
        fig.suptitle(f"{m.fsm} — time_series", fontsize=14, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    traj = walk(m, seed, STEPS)
    ticks = list(range(len(traj)))

    nvars = len(m.state_vars)
    fig, axes = plt.subplots(nvars, 1, sharex=True,
                             figsize=(11, max(2.2 * nvars, 3.0)))
    if nvars == 1:
        axes = [axes]

    for ax, var in zip(axes, m.state_vars):
        name = var["name"]
        kind = var["kind"]
        if kind in ("int", "real"):
            ys = [s[name] for s in traj]
            ax.plot(ticks, ys, marker="o", markersize=3, linewidth=1.4,
                    color="#1f77b4")
            ax.set_ylabel(name, rotation=0, ha="right", va="center", fontsize=9)
            ax.grid(True, alpha=0.3)
        else:
            ys, labels = [], {}
            for s in traj:
                y, lbl = to_ordinal(m, var, s[name])
                ys.append(y)
                labels[y] = lbl
            ax.step(ticks, ys, where="post", linewidth=1.6, color="#d62728",
                    marker="o", markersize=3)
            if labels:
                ks = sorted(labels)
                ax.set_yticks(ks)
                ax.set_yticklabels([labels[k] for k in ks], fontsize=8)
                ax.set_ylim(min(ks) - 0.4, max(ks) + 0.4)
            ax.set_ylabel(name, rotation=0, ha="right", va="center", fontsize=9)
            ax.grid(True, axis="x", alpha=0.3)

    axes[-1].set_xlabel("tick")
    fig.suptitle(f"{m.fsm} — time_series  (seed {m.label(seed)}, {len(traj)} ticks)",
                 fontsize=13, fontweight="bold")
    fig.tight_layout(rect=[0, 0, 1, 0.97])
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_time_series.py <smt2> <schema> <out.png>", file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
    print(f"wrote {sys.argv[3]}")
