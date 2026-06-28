#!/usr/bin/env python3
"""render_value_heatmap — every carried variable's value over time, as a raster.

The TRANSPOSE of time_series: one ROW per carried scalar/enum/bool leaf, one COLUMN
per tick, each cell colored by that variable's value at that tick. Where time_series
stacks N small line-charts, this packs the whole trajectory into a single dense image —
the spreadsheet "every consequence in a column" + TLA+ trace, read as a heatmap.

This is the GENERALIZATION of space_time. space_time rasters a single Seq-carried var
(one row per tick, one column per Seq position); but the truest space×time models in the
corpus carry their spatial dimension as N PARALLEL scalar fields, not a Seq —  `life.ev`
is a 4×4 grid as 16 Bool cells `c00..c33`, `brackets.ev` is a stack as enum slots
`s0..s3`. Those are N/A under space_time's `kind=='seq'` gate, but they're exactly a
value-over-time raster: each cell is its own row. So this view lights up EVERY FSM with
≥2 carried leaves — counter, vending, thermostat, SIR, oscillator, life, brackets — with
the Seq/CA case falling out as the instance where a Seq's elements are the rows
(`_flatten_seqs` explodes them).

Each row gets its OWN colormap normalized to that variable's reachable domain (a Bool and
an Int aren't comparable on one scale): binary 0/1 → crisp black/white (the CA classic),
enum → a categorical palette over the declared variant order, int/real → viridis over the
row's min–max. Under-constraint shows as a row that drifts/fans rather than a clean band.

The trajectory is the literal forward simulation from `m.initial_state()` via the EXISTING
successor relation (`walk`, shared with time_series) — never reimplemented. A DETERMINISTIC
FSM yields THE trajectory; a nondeterministic one (a free input each tick) has many — we
follow one sampled run and say so honestly in the subtitle.

CLI:  python3 viz/render_value_heatmap.py <smt2> <schema> <out.png>
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.colors import ListedColormap, BoundaryNorm, Normalize

from time_series_walk import pick_seed, walk, to_ordinal, _flatten_seqs
import valueheat_data

VIZ_TYPE = "value_heatmap"

# How many ticks to walk rightward. Past this the raster gets too wide to read; a
# repeating/halting trajectory is fully legible well within it.
MAX_TICKS = 60


def _na(m, out_path, msg):
    fig, ax = plt.subplots(figsize=(10, 4))
    ax.axis("off")
    ax.set_title(f"{m.fsm} — {VIZ_TYPE}", fontsize=14, fontweight="bold")
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=12, transform=ax.transAxes)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def _walk_with_flags(m, seed, steps):
    """Walk one chain from `seed`, reporting (path, nondeterministic, halted): `path` is the
    trajectory, `nondeterministic` is True iff any visited state had >1 successor (so this is
    one sampled run), `halted` True iff it stopped on a fixed point / dead transition before
    the tick cap. Mirrors `walk` but watches the fan width along the way."""
    prefer_change = m.is_discrete()
    cur = seed
    path = [cur]
    seen = {m._key(cur)}
    nondeterministic = False
    halted = False
    for _ in range(steps):
        succ = m.successors(cur, limit=8)
        if len(succ) > 1:
            nondeterministic = True
        if not succ:
            halted = True
            break
        changed = [s for s in succ if m._key(s) != m._key(cur)]
        pool = changed or succ
        fresh = [s for s in pool if m._key(s) not in seen] if prefer_change else pool
        nxt = (fresh or pool)[0]
        k = m._key(nxt)
        if k in seen:                  # fixed point / revisit
            halted = True
            path.append(nxt)
            break
        path.append(nxt)
        seen.add(k)
        cur = nxt
    return path, nondeterministic, halted


def _row_values(m, var, traj):
    """The variable's per-tick value list along the trajectory (None where absent)."""
    return [s.get(var["name"]) for s in traj]


def _draw_row(ax, m, var, values, ncols, y):
    """Draw `var`'s value series as one horizontal heat-strip on row `y`. Each row owns its
    colormap, normalized to that variable's own domain — a Bool and an Int aren't comparable
    on one scale. Returns a (label, cell-text-list) sidecar for annotation."""
    kind = var["kind"]
    extent = (-0.5, ncols - 0.5, y + 0.5, y - 0.5)   # one cell tall, ncols wide
    if kind in ("bool",):
        grid = [[1 if v else 0 for v in values]]
        # #469 dark page: 0 = a dark slate (sits near the page bg, not a glaring white slab),
        # 1 = the light accent — so the CA pattern reads as bright-on-dark instead of a white block.
        cmap = ListedColormap(["#1b2129", "#58a6ff"])
        norm = BoundaryNorm([-0.5, 0.5, 1.5], cmap.N)
        ax.imshow(grid, cmap=cmap, norm=norm, aspect="auto", extent=extent,
                  interpolation="nearest", zorder=1)
    elif kind == "enum":
        variants = m.enum_variants.get(var["name"], [])
        order = {val: i for i, val in enumerate(variants)}
        grid = [[order.get(v, 0) for v in values]]
        n = max(1, len(variants))
        cmap = plt.get_cmap("tab10" if n <= 10 else "tab20")
        norm = BoundaryNorm([i - 0.5 for i in range(n + 1)], cmap.N)
        ax.imshow(grid, cmap=cmap, norm=norm, aspect="auto", extent=extent,
                  interpolation="nearest", zorder=1)
    else:                                  # int / real — viridis over the row's own min–max
        nums = [float(v) for v in values if isinstance(v, (int, float))]
        lo, hi = (min(nums), max(nums)) if nums else (0.0, 1.0)
        if hi <= lo:
            hi = lo + 1.0
        grid = [[float(v) if isinstance(v, (int, float)) else lo for v in values]]
        ax.imshow(grid, cmap=plt.cm.viridis, norm=Normalize(lo, hi), aspect="auto",
                  extent=extent, interpolation="nearest", zorder=1)


def _cell_text(m, var, values):
    """A short per-cell label (the value), used only when the raster is small enough to read."""
    kind = var["kind"]
    out = []
    for v in values:
        if v is None:
            out.append("")
        elif kind == "bool":
            out.append("1" if v else "0")
        elif kind == "enum":
            out.append(str(v)[:3])
        else:
            out.append(str(v))
    return out


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    seed = pick_seed(m)
    if seed is None:
        msg = "N/A: no initial state (the transition has no first-tick model)"
        valueheat_data.write(out_path, valueheat_data.build(m, [], [], False, False, MAX_TICKS, msg))
        return _na(m, out_path, msg)

    traj, nondet, halted = _walk_with_flags(m, seed, MAX_TICKS)

    # Rows = EVERY carried leaf (the full interface set), NOT the ranked-and-deduped
    # `state_vars`: this is a per-variable raster, so life's 16 grid cells must each be a row —
    # dedup would collapse the equivalent cells and erase the CA. Append derived display vars
    # (a `done`/`pop` flag the transition computes) the way time_series does. Then explode any
    # Seq-carried var into per-element pseudo-rows (the space_time/CA case as one instance).
    row_src = list(m.interface_vars) + [v for v in m.derived if v["name"] in traj[0]]
    flat_vars, traj = _flatten_seqs(row_src, traj)
    rows = list(flat_vars)
    # The abstract substrate (`<out>.data.json`): the raster's MEANING, built from the SAME rows +
    # sampled trajectory the picture rasters. Emitted on EVERY path (incl. N/A) so the golden suite
    # always has data — `ticks ≪ max_ticks` or an `n_distinct==1` row is the regression signal.
    valueheat_data.write(out_path, valueheat_data.build(m, rows, traj, nondet, halted, MAX_TICKS))
    if len(rows) < 2 or len(traj) < 2:
        return _na(m, out_path,
                   "N/A: value_heatmap needs ≥2 carried variables over ≥2 ticks "
                   f"({len(rows)} var(s), {len(traj)} tick(s))")

    ncols = len(traj)
    nrows = len(rows)
    fig_w = max(6.0, min(18.0, 0.34 * ncols + 3.0))
    fig_h = max(3.0, min(16.0, 0.42 * nrows + 1.6))
    fig, ax = plt.subplots(figsize=(fig_w, fig_h))

    small = ncols <= 24 and nrows <= 28          # readable enough to annotate cells
    for y, var in enumerate(rows):
        values = _row_values(m, var, traj)
        _draw_row(ax, m, var, values, ncols, y)
        if small:
            for x, txt in enumerate(_cell_text(m, var, values)):
                if txt:
                    ax.text(x, y, txt, ha="center", va="center", fontsize=7,
                            color="#888", zorder=3)

    ax.set_xlim(-0.5, ncols - 0.5)
    ax.set_ylim(nrows - 0.5, -0.5)               # row 0 at top, time → right
    ax.set_yticks(range(nrows))
    ax.set_yticklabels([v["name"].split(".")[-1] for v in rows], fontsize=8)
    ax.set_xticks(range(0, ncols, max(1, ncols // 16)))
    ax.set_xlabel("tick  (time →)", fontsize=10)
    ax.set_ylabel("carried variable", fontsize=10)
    # cell grid lines when small enough to read
    if small:
        ax.set_xticks([x - 0.5 for x in range(ncols + 1)], minor=True)
        ax.set_yticks([y - 0.5 for y in range(nrows + 1)], minor=True)
        ax.grid(which="minor", color="#cccccc", linewidth=0.4)
        ax.tick_params(which="minor", length=0)

    run = ("one sampled run (nondeterministic — many trajectories exist)"
           if nondet else "the trajectory (deterministic)")
    stop = "halted (fixed point)" if halted else f"{ncols} ticks"
    ax.set_title(
        f"{m.fsm} — {VIZ_TYPE}\n"
        f"rows = carried variables, columns = ticks, color = value (per-row scale)  ·  "
        f"{nrows} vars × {stop}  ·  {run}",
        fontsize=11, fontweight="bold")

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def main(argv):
    if len(argv) != 4:
        print("usage: render_value_heatmap.py <smt2> <schema> <out.png>", file=sys.stderr)
        return 2
    render(argv[1], argv[2], argv[3])
    print(f"wrote {argv[3]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
