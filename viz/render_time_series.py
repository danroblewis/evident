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
    """Follow the DETERMINISTIC successor chain from `seed` — the declared seed's
    actual trajectory, identical to evident_viz.trajectory() and what
    timing_diagram traces. Stops at a fixed point (self-loop) or a revisit.

    Why successor() and not successors()+fresh-preference: a difference equation
    with a fixed input (brackets streams ⟨LParen,LBrack,…,BEnd⟩) is DETERMINISTIC
    in the next state given the full previous state. The old fresh-preference
    explore wandered OFF that path: once the input cursor runs past the fixed
    input (done=true), the out-of-bounds token is unconstrained, so successors()
    returns a FAN of spurious states that never occur on the declared run — and
    picking a 'fresh' one from that fan fabricated a trace where st.ok flips false
    on a balanced input. The deterministic chain stays on the real trajectory.

    Only when the deterministic chain immediately parks on a self-loop at the seed
    (e.g. an adjacency graph where staying put is legal AND the solver picks the
    self-edge) do we fall back to fan-exploration to produce a moving walk."""
    cur = seed
    path = [cur]
    seen = {m._key(cur)}
    for _ in range(steps):
        nxt = m.successor(cur)
        if nxt is None:
            break
        path.append(nxt)
        k = m._key(nxt)
        if k in seen:        # fixed point / revisit -> stop
            break
        seen.add(k)
        cur = nxt

    # Genuinely-stuck-at-seed nondeterministic graph: deterministic following
    # parked on tick 0 but the fan offers somewhere fresh to go. Re-walk via the
    # fan only in that degenerate case (a discrete graph, not a driven equation).
    if len(path) <= 1:
        fan0 = m.successors(seed)
        if len(fan0) > 1 or (fan0 and m._key(fan0[0]) != m._key(seed)):
            cur, path, seen = seed, [seed], {m._key(seed)}
            for _ in range(steps):
                nxts = m.successors(cur)
                if not nxts:
                    break
                fresh = [s for s in nxts if m._key(s) not in seen]
                nxt = fresh[0] if fresh else nxts[0]
                path.append(nxt)
                k = m._key(nxt)
                if k in seen:
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
