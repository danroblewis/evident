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
    """The trajectory starts at the program's ACTUAL initial_state — the only faithful seed, so
    every plotted value is genuinely reachable.

    An earlier version nudged the first numeric var by +2000 to escape a flat fixed-point origin,
    but that seeded an UNREACHABLE state and could plot a variable far outside its proven bound
    (vending init has a self-loop under one nondeterministic choice, so it was misread as a fixed
    point and balance was seeded to 2000 ∉ [0,5] — Marek #183, a faithfulness violation). A
    genuinely-flat trajectory from a true fixed-point init is HONEST; the off-origin limit-cycle
    dynamics of a continuous system are shown by phase_portrait, which probes within the reachable
    set — never by fabricating an out-of-domain start here."""
    return m.initial_state()


def _advance(m, cur, prefer_change, visited):
    """One step of the walk. For DISCRETE programs (prefer_change), pick a
    successor that actually CHANGES the state — and, when possible, one not yet
    visited — so the trajectory explores the program rather than parking on a
    self-loop. This mirrors render_timing_diagram._advance: on a discrete graph
    the lone successor() can sit on a legal self-edge (dungeon's Entrance->Entrance
    is satisfiable, and z3 may pick it), which would report a genuinely-dynamic
    program as static. Falls back to the lone successor() for non-discrete
    (driven difference-equation) systems."""
    if not prefer_change:
        return m.successor(cur)
    succ = m.successors(cur, limit=32)
    if not succ:
        return None
    changed = [s for s in succ if m._key(s) != m._key(cur)]
    pool = changed or succ
    fresh = [s for s in pool if m._key(s) not in visited]
    return (fresh or pool)[0]


def walk(m, seed, steps):
    """Follow one successor chain from `seed`, stopping at a fixed point / revisit.

    For DRIVEN difference equations (numeric / mixed: brackets streams
    ⟨LParen,LBrack,…,BEnd⟩) the next state is DETERMINISTIC given the full previous
    state, so we follow the lone successor() — picking a 'fresh' state out of an
    out-of-bounds fan would fabricate a trace that never occurs on the declared run.

    For DISCRETE programs (all-categorical interface — an adjacency graph like
    dungeon) the lone successor() can park on a legal self-edge: Entrance->Entrance
    is satisfiable and z3 may pick it, which would make a genuinely-dynamic program
    look static. There we prefer a STATE-CHANGING, not-yet-visited successor — exactly
    what render_timing_diagram already does — so the trajectory walks
    Entrance->Hall->Gate instead of stalling at the seed."""
    prefer_change = m.is_discrete()
    cur = seed
    path = [cur]
    seen = {m._key(cur)}
    for _ in range(steps):
        nxt = _advance(m, cur, prefer_change, seen)
        if nxt is None:
            break
        path.append(nxt)
        k = m._key(nxt)
        if k in seen:        # fixed point / revisit -> stop
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

    # CHANNEL MAPPING for a stacked time series: tick is the shared x-axis; each
    # variable owns one row's y-axis (position — the best channel). We don't need
    # color/size here, so the channel job is purely ORDER: most-important var on
    # top, and group the two var TYPES so quantitative lines and categorical step
    # plots don't interleave. m.state_vars is already importance-ranked+deduped;
    # numeric_vars / categorical_vars are its type-split projections (same order).
    quant = m.numeric_vars
    cat = m.categorical_vars
    ordered = quant + cat                      # numerics on top, then categoricals
    if not ordered:
        ordered = list(m.state_vars)

    # Drop CONSTANT rows. A variable that holds one value for the entire declared
    # trajectory carries zero information as a time series — its row is a flat
    # line that wastes vertical space (find's state.s5 never leaves Unseen; a
    # balanced brackets run pins st.ok=true throughout). Suppress those rows but
    # report them as a one-line "held constant" note so no value is hidden — the
    # READER still sees "st.ok stayed true / s5 stayed Unseen", just not as a row.
    def held_value(var):
        vals = [s[var["name"]] for s in traj]
        first = vals[0]
        return first if all(v == first for v in vals) else None

    constants = [(v, held_value(v)) for v in ordered]
    constants = [(v, hv) for v, hv in constants if hv is not None]
    varying = [v for v in ordered if held_value(v) is None]

    if not varying:
        # Every variable is constant (a fixed point / degenerate seed): nothing to
        # plot over time. Render an honest summary card instead of N empty rows.
        fig, ax = plt.subplots(figsize=(10, max(3.0, 0.4 * len(constants) + 2)))
        ax.axis("off")
        lines = "\n".join(f"  {v['name']} = {hv}" for v, hv in constants)
        ax.text(0.5, 0.5,
                f"N/A — every state variable is constant over the trajectory\n"
                f"({len(traj)} ticks from seed {m.label(seed)}; no dynamics to plot)\n\n"
                f"{lines}",
                ha="center", va="center", fontsize=12, family="monospace")
        fig.suptitle(f"{m.fsm} — time_series", fontsize=14, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    ordered = varying
    nvars = len(ordered)
    fig, axes = plt.subplots(nvars, 1, sharex=True,
                             figsize=(11, max(2.2 * nvars, 3.0)))
    if nvars == 1:
        axes = [axes]

    for rank, (ax, var) in enumerate(zip(axes, ordered)):
        name = var["name"]
        kind = var["kind"]
        # importance badge: #1 is the most-varying / least-redundant var
        ax.set_title(f"#{rank + 1}  {m.var_class(var)}", loc="left",
                     fontsize=8, color="#888", pad=2)
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
            # full enum ladder as y-ticks (not just visited values), so the row
            # reads as the variable's whole categorical range
            if kind == "enum":
                variants = m.enum_variants.get(name, [])
                for i, vlbl in enumerate(variants):
                    labels.setdefault(i, vlbl)
            elif kind == "bool":
                labels.setdefault(0, "false")
                labels.setdefault(1, "true")
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
    fig.suptitle(
        f"{m.fsm} — time_series  (seed {m.label(seed)}, {len(traj)} ticks; "
        f"{nvars} varying of {len(quant) + len(cat)} vars, importance-ordered)",
        fontsize=13, fontweight="bold")
    if constants:
        held = ",  ".join(f"{v['name']}={hv}" for v, hv in constants)
        fig.text(0.5, 0.005,
                 f"held constant (suppressed): {held}",
                 ha="center", va="bottom", fontsize=8, color="#666")
    fig.tight_layout(rect=[0, 0.02 if constants else 0, 1, 0.97])
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_time_series.py <smt2> <schema> <out.png>", file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
    print(f"wrote {sys.argv[3]}")
