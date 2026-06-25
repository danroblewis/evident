#!/usr/bin/env python3
"""render_space_time — the evolution of a Seq-carried-state FSM as a 2D raster.

For an FSM whose carried state is a Seq (a vector that updates as a whole each tick —
a cellular automaton row, a diffusion buffer, a shift register), the natural picture is
SPACE × TIME: one row per tick (time flowing downward), one column per Seq position
(space across), each cell colored by that position's value at that tick. Stacking the
ticks turns a per-tick vector orbit into a single image of the whole evolution.

For Rule 90 seeded with a single 1 this draws the Sierpiński triangle — the XOR rule's
self-similar fractal falls straight out of the raster. The view is GENERIC, not CA-specific:
any Seq(Int)/Seq(Bool)/Seq(enum)-carried FSM works (a diffusion buffer shows a spreading
band; a shift register shows a diagonal). 0/1 sequences get a crisp binary colormap; wider
integer ranges and enums get a graded colormap so the magnitude/variant reads off the hue.

The trajectory is the literal forward simulation from `m.initial_state()` via
`successors(state)[0]`. A DETERMINISTIC FSM (one successor) yields THE trajectory; a
nondeterministic one (a free input each tick) has many — we follow one sampled run and
say so honestly in the subtitle. The view degrades to an N/A card if the model carries no
Seq (this raster only makes sense for a sequence-valued state).

CLI:  python3 viz/render_space_time.py <smt2> <schema> <out.png>
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import ListedColormap, BoundaryNorm

VIZ_TYPE = "space_time"

# How many ticks to simulate downward. Past this the raster gets too tall to read; a
# self-similar fractal (Rule 90) is fully legible well within it.
MAX_TICKS = 48


def _seq_var(m):
    """The Seq-carried var this raster is OF — the first carried leaf with kind=='seq'.
    None if the model has no sequence-valued state (then the view is N/A)."""
    for v in m.carried:
        if v.get("kind") == "seq":
            return v
    return None


def _simulate(m, seq_name, steps=MAX_TICKS):
    """Follow ONE forward run from the initial state, collecting the Seq var per tick.

    Returns (rows, nondeterministic, halted): `rows` is a list of per-tick value lists
    (the raster, top = tick 0); `nondeterministic` is True iff any visited state had
    more than one successor (so this is one sampled run, not the trajectory); `halted`
    is True iff the run stopped early (fixed point / dead transition) rather than at the
    tick cap."""
    cur = m.initial_state()
    if cur is None or seq_name not in cur:
        return [], False, False
    rows = [list(cur[seq_name])]
    nondeterministic = False
    halted = False
    for _ in range(steps - 1):
        succ = m.successors(cur, limit=8)
        if len(succ) > 1:
            nondeterministic = True
        if not succ:
            halted = True
            break
        nxt = succ[0]
        if m._key(nxt) == m._key(cur):       # fixed point — the row would just repeat
            halted = True
            break
        rows.append(list(nxt[seq_name]))
        cur = nxt
    return rows, nondeterministic, halted


def _numify(rows, elem, m, seq_name):
    """Map the raster's cell values to floats for imshow, plus the (tick-marks, labels)
    for the colorbar. Bool → 0/1; int → the int itself; enum → an ordinal over the
    declared variant order (so the colorbar reads the variant NAMES, not indices)."""
    if elem == "enum":
        variants = list(m.enum_variants.get(seq_name, []))
        # fold in any value observed outside the declared set (defensive)
        for r in rows:
            for c in r:
                if c not in variants:
                    variants.append(c)
        order = {val: i for i, val in enumerate(variants)}
        grid = [[order.get(c, 0) for c in r] for r in rows]
        return grid, list(range(len(variants))), [str(v) for v in variants]
    if elem == "bool":
        grid = [[1 if c else 0 for c in r] for r in rows]
        return grid, [0, 1], ["false", "true"]
    grid = [[int(c) for c in r] for r in rows]
    return grid, None, None


def _is_binary(grid):
    """True iff every cell is 0 or 1 — the crisp two-color (CA / single-bit) case."""
    return all(c in (0, 1) for r in grid for c in r)


def _na(m, out_path, msg):
    fig, ax = plt.subplots(figsize=(10, 4))
    ax.axis("off")
    ax.set_title(f"{m.fsm} — {VIZ_TYPE}", fontsize=14, fontweight="bold")
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=12, transform=ax.transAxes)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def render(m, out_path):
    seq = _seq_var(m)
    if seq is None:
        return _na(m, out_path,
                   "N/A: no Seq-carried state — space_time rasters a sequence-valued FSM")

    seq_name = seq["name"]
    elem = seq.get("elem", "int")
    rows, nondeterministic, halted = _simulate(m, seq_name)
    if not rows:
        return _na(m, out_path, "N/A: no initial state / no forward trajectory")

    width = max(len(r) for r in rows)
    # pad ragged rows (shouldn't happen for a pinned-length Seq, but stay robust)
    rows = [r + [0] * (width - len(r)) for r in rows]
    grid, cbar_ticks, cbar_labels = _numify(rows, elem, m, seq_name)
    nticks = len(grid)

    fig_w = max(5.0, min(14.0, 0.45 * width + 2.0))
    fig_h = max(3.5, min(13.0, 0.32 * nticks + 1.6))
    fig, ax = plt.subplots(figsize=(fig_w, fig_h))

    binary = elem != "enum" and _is_binary(grid)
    if binary:
        # #469 dark page: 0 = a dark slate near the page bg, 1 = the light accent — the CA pattern
        # reads as bright-on-dark (a white-cell raster was a glaring slab over the dark IDE).
        cmap = ListedColormap(["#1b2129", "#58a6ff"])
        norm = BoundaryNorm([-0.5, 0.5, 1.5], cmap.N)
        im = ax.imshow(grid, cmap=cmap, norm=norm, aspect="auto",
                       interpolation="nearest", origin="upper")
    else:
        cmap = plt.cm.viridis
        im = ax.imshow(grid, cmap=cmap, aspect="auto",
                       interpolation="nearest", origin="upper")

    ax.set_xlabel(f"{seq_name} position  (space →)", fontsize=10)
    ax.set_ylabel("tick  (time ↓)", fontsize=10)
    # grid lines between cells when the raster is small enough to read them
    if width <= 32 and nticks <= 40:
        ax.set_xticks([x - 0.5 for x in range(width + 1)], minor=True)
        ax.set_yticks([y - 0.5 for y in range(nticks + 1)], minor=True)
        ax.grid(which="minor", color="#cccccc", linewidth=0.4)
        ax.tick_params(which="minor", length=0)
    ax.set_xticks(range(0, width, max(1, width // 12)))
    ax.set_yticks(range(0, nticks, max(1, nticks // 12)))

    if binary:
        from matplotlib.patches import Patch
        ax.legend(handles=[Patch(facecolor="#1b2129", edgecolor="#888", label="0"),
                           Patch(facecolor="#58a6ff", label="1")],
                  loc="upper left", bbox_to_anchor=(1.01, 1.0), fontsize=9,
                  title=f"{elem} cell", framealpha=0.95)
    else:
        cbar = fig.colorbar(im, ax=ax, fraction=0.04, pad=0.02)
        cbar.set_label(f"{seq_name} cell value", fontsize=9)
        if cbar_ticks is not None:
            cbar.set_ticks(cbar_ticks)
            cbar.set_ticklabels(cbar_labels)

    run = ("one sampled run (nondeterministic — many trajectories exist)"
           if nondeterministic else "the trajectory (deterministic)")
    stop = "halted (fixed point)" if halted else f"{nticks} ticks"
    ax.set_title(
        f"{m.fsm} — {VIZ_TYPE}\n"
        f"rows = ticks, columns = {seq_name} positions, color = cell value  ·  "
        f"{width} cells × {stop}  ·  {run}",
        fontsize=11, fontweight="bold")

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def main(argv):
    if len(argv) != 4:
        print("usage: render_space_time.py <smt2> <schema> <out.png>", file=sys.stderr)
        return 2
    render(load(argv[1], argv[2]), argv[3])
    print(f"wrote {argv[3]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
