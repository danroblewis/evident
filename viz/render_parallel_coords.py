#!/usr/bin/env python3
"""render_parallel_coords.py — parallel-coordinates (Inselberg) view of an
Evident program's reachable state set.

Axes are ordered by importance (m.state_vars). Each sampled state is a polyline
that crosses every axis at the height of its value on that axis. Numeric axes
are scaled to the observed [min,max]; enum/bool axes map each value to an ordinal
with the category name printed as a tick.

COLOR encodes the top CATEGORICAL variable (categorical_vars[0]) — the classic
parallel-coordinates coloring that makes class structure pop: every polyline of
the same category shares a hue, so you read which axis-values cluster per class.
When the model has no categorical variable (pure-numeric, e.g. vanderpol), color
falls back to sample order (a perceptual sense of time / trajectory).

Reusable CLI for ANY Evident IR:
    python3 viz/render_parallel_coords.py <smt2> <schema> <out.png>

Sampling strategy (dynamics always come from querying the transition):
  - discrete / mixed programs  -> m.reachable() (exact reachable state set)
  - numeric programs           -> long trajectories from several seeds
    (reachable() is BFS-capped and meaningless on a continuous grid)
"""
import sys
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.collections import LineCollection
from matplotlib.cm import ScalarMappable
from matplotlib.colors import Normalize
from matplotlib.lines import Line2D

sys.path.insert(0, os.path.join(os.path.dirname(__file__)))
from evident_viz import load


# Numeric seeds for continuous systems (vanderpol-style limit cycles): a few
# off-origin points so trajectories sweep the cycle rather than sitting at a
# fixed point. Pinned arbitrary grid points — successor accepts any state.
NUMERIC_SEEDS = [
    {"x": 2800, "v": 0}, {"x": 400, "v": 0},
    {"x": 0, "v": 2700}, {"x": -1500, "v": 1500},
    {"x": -2800, "v": 0}, {"x": 1500, "v": -1500},
]


def _axis_meta(m, samples):
    """For each state var, build (kind, value->position fn, ticks).

    Numeric axes: position = the value itself, ticks = min/mid/max.
    Categorical axes (bool/enum/string): map each distinct value to an integer
    ordinal; ticks label every category.
    """
    metas = []
    for v in m.state_vars:
        name = v["name"]
        kind = v["kind"]
        vals = [s[name] for s in samples]
        if kind in ("int", "real"):
            lo, hi = min(vals), max(vals)
            if lo == hi:
                lo, hi = lo - 1, hi + 1
            mid = (lo + hi) / 2.0
            ticks = [(lo, _fmt(lo)), (mid, _fmt(mid)), (hi, _fmt(hi))]
            metas.append({"name": name, "kind": kind, "lo": lo, "hi": hi,
                          "pos": (lambda x: float(x)), "ticks": ticks})
        else:
            if kind == "enum":
                order = m.enum_variants[name]
                # keep only categories that actually occur, preserving order
                cats = [c for c in order if c in set(vals)]
                if not cats:
                    cats = order
            elif kind == "bool":
                cats = [False, True]
            else:  # string
                cats = sorted(set(vals), key=str)
            index = {c: i for i, c in enumerate(cats)}
            lo, hi = 0, max(1, len(cats) - 1)
            ticks = [(i, _cat_label(c)) for c, i in index.items()]
            metas.append({"name": name, "kind": kind, "lo": lo, "hi": hi,
                          "pos": (lambda x, idx=index: float(idx[x])),
                          "ticks": ticks})
    return metas


def _line_colors(m, samples, color_var, n):
    """Per-polyline color + legend handles + a title note.

    With a categorical color var: one qualitative hue per category (the classic
    class-revealing parallel-coords coloring) and a discrete legend. For enums
    the category ORDER follows enum_variants so the legend reads in declared
    order. Without one (pure-numeric): a viridis sample-order gradient, no
    legend (a colorbar is drawn by the caller)."""
    if color_var is None:
        cmap = plt.get_cmap("viridis")
        colors = [cmap(i / max(1, n - 1)) for i in range(n)]
        return colors, None, "color = sample order (no categorical var)"

    name = color_var["name"]
    vals = [s[name] for s in samples]
    if color_var["kind"] == "enum":
        cats = [c for c in m.enum_variants[name] if c in set(vals)] or list(set(vals))
    elif color_var["kind"] == "bool":
        cats = [c for c in (False, True) if c in set(vals)]
    else:
        cats = sorted(set(vals), key=str)

    # tab10 for <=10 classes (high-contrast qualitative), else tab20.
    qual = plt.get_cmap("tab10" if len(cats) <= 10 else "tab20")
    cat_color = {c: qual(i % qual.N) for i, c in enumerate(cats)}
    colors = [cat_color[v] for v in vals]
    handles = [Line2D([0], [0], color=cat_color[c], lw=2.4, label=_cat_label(c))
               for c in cats]
    return colors, handles, f"color = {_short(name)} ({len(cats)} classes)"


def _fmt(x):
    if abs(x - round(x)) < 1e-9:
        return str(int(round(x)))
    return f"{x:.2f}"


def _cat_label(c):
    if isinstance(c, bool):
        return "true" if c else "false"
    return str(c)


def _collect_samples(m):
    """Return (samples, note) — a list of state dicts to draw as polylines."""
    if m.is_discrete():
        states, _ = m.reachable()
        return states, "reachable set"
    # mixed (has at least one numeric axis): if there's an enum/bool too, the
    # reachable BFS may still be a sensible finite cycle (e.g. vending). Try it
    # first; fall back to numeric seeding if it's tiny/degenerate.
    has_cat = any(v["kind"] in ("bool", "enum", "string") for v in m.state_vars)
    if has_cat:
        states, _ = m.reachable(limit=400)
        if len(states) >= 2:
            return states, "reachable set"
    # pure-numeric (or degenerate): sweep trajectories from several seeds.
    xname = next((v["name"] for v in m.state_vars if v["name"].endswith(".x")), None)
    vname = next((v["name"] for v in m.state_vars if v["name"].endswith(".v")), None)
    numeric = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    samples = []
    if xname and vname:
        seeds = [{xname: s["x"], vname: s["v"]} for s in NUMERIC_SEEDS]
    else:
        # generic numeric seed grid on the first two numeric axes
        seeds = []
        if len(numeric) >= 1:
            base = {v["name"]: 0 for v in m.state_vars}
            for d in (-1500, -500, 500, 1500):
                s = dict(base)
                s[numeric[0]["name"]] = d
                seeds.append(s)
    for seed in seeds:
        traj = m.trajectory(start=seed, steps=120)
        samples.extend(traj)
    if not samples:
        init = m.initial_state()
        if init is not None:
            samples = m.trajectory(start=init, steps=200)
    return samples, "trajectory sweep"


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    samples, note = _collect_samples(m)

    fig, ax = plt.subplots(figsize=(max(6, 2.2 * len(m.state_vars)), 6))
    title = f"{m.fsm}  ·  parallel_coords"

    if not samples or len(m.state_vars) < 2:
        reason = ("no samples from transition" if not samples
                  else f"only {len(m.state_vars)} axis — need ≥2 for parallel coords")
        ax.text(0.5, 0.5, f"N/A for this state:\n{reason}",
                ha="center", va="center", fontsize=14, color="#444",
                transform=ax.transAxes)
        ax.set_axis_off()
        ax.set_title(title, fontsize=13, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return out_path, len(samples)

    metas = _axis_meta(m, samples)
    naxes = len(metas)
    xs = list(range(naxes))

    # Normalize every axis to [0,1] so polylines share one drawing frame.
    def norm(meta, value):
        lo, hi = meta["lo"], meta["hi"]
        if isinstance(value, _OrdinalValue):
            p = value.ordinal           # already in position-space
        else:
            p = meta["pos"](value)
        if hi == lo:
            return 0.5
        return (p - lo) / (hi - lo)

    segments = []
    for s in samples:
        ys = [norm(meta, s[meta["name"]]) for meta in metas]
        segments.append(list(zip(xs, ys)))

    n = len(segments)
    color_var = m.categorical_vars[0] if m.categorical_vars else None
    colors, legend_handles, color_note = _line_colors(m, samples, color_var, n)
    lc = LineCollection(segments, colors=colors, linewidths=1.3, alpha=0.6)
    ax.add_collection(lc)

    # Draw each axis as a vertical line + its category/range ticks.
    for i, meta in enumerate(metas):
        ax.axvline(i, color="#333", linewidth=1.2, zorder=1)
        for raw, lbl in meta["ticks"]:
            y = norm(meta, _inv_for_tick(meta, raw))
            ax.plot([i - 0.04, i + 0.04], [y, y], color="#333", lw=1.0)
            ax.text(i - 0.08, y, lbl, ha="right", va="center",
                    fontsize=8, color="#222")

    ax.set_xlim(-0.6, naxes - 0.4)
    ax.set_ylim(-0.05, 1.08)
    ax.set_xticks(xs)
    ax.set_xticklabels([_short(meta["name"]) for meta in metas],
                       fontsize=9, fontweight="bold")
    ax.set_yticks([])
    for spine in ("top", "right", "left"):
        ax.spines[spine].set_visible(False)
    ax.spines["bottom"].set_visible(False)
    ax.tick_params(length=0)

    fig.suptitle(title, fontsize=13, fontweight="bold", y=0.98)
    ax.set_title(f"{n} states · {note} · {color_note}",
                 fontsize=9, color="#555", pad=10)

    if legend_handles is not None:
        # Categorical color → a discrete legend (one swatch per class).
        ax.legend(handles=legend_handles, title=_short(color_var["name"]),
                  fontsize=7.5, title_fontsize=8, loc="upper left",
                  bbox_to_anchor=(1.01, 1.0), frameon=False)
    else:
        # Numeric/time fallback → a continuous colorbar.
        sm = ScalarMappable(norm=Normalize(0, max(1, n - 1)),
                            cmap=plt.get_cmap("viridis"))
        cb = fig.colorbar(sm, ax=ax, fraction=0.025, pad=0.02)
        cb.set_label("sample order", fontsize=8)
        cb.ax.tick_params(labelsize=7)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    return out_path, n


class _OrdinalValue:
    """Sentinel so norm() places a tick at a raw position-space coordinate,
    bypassing the value->position map (used for both numeric and categorical
    ticks, whose stored `raw` is already in position space)."""
    def __init__(self, ordinal):
        self.ordinal = ordinal


def _inv_for_tick(meta, raw):
    """Tick `raw` is already a position-space coordinate (a numeric value, or a
    categorical ordinal). Wrap it so norm() uses it directly."""
    return _OrdinalValue(raw)


def _short(name):
    # drop a leading "state." / "d." prefix for readability
    return name.split(".", 1)[-1] if "." in name else name


def main():
    if len(sys.argv) != 4:
        print(__doc__)
        sys.exit(2)
    out, n = render(sys.argv[1], sys.argv[2], sys.argv[3])
    print(f"wrote {out} ({n} polylines)")


if __name__ == "__main__":
    main()
