#!/usr/bin/env python3
"""render_timing_diagram — EE-style timing/waveform diagram for any Evident IR.

One horizontal track per state variable, plotted against tick number. Tracks
are stacked in importance order (m.state_vars): the most informative variable
sits on top (#1), so the eye reads the dominant signal first. Encoding stays
keyed to the variable's TYPE (digital / lane / analog):

  * bool / enum / string vars  -> DIGITAL waveform. The value is held flat
    between ticks and jumps on a vertical edge at each transition (classic
    logic-analyzer look). Enums map to ordinal lanes; the active variant name
    is printed at each level.
  * int / real vars            -> ANALOG track. The numeric value is drawn as a
    line over ticks, normalized into the track's band.

ALL INITIAL CONDITIONS (the diagram-review upgrade). For a finitely-DISCRETE
program the timing diagram roots on the GLOBAL all-initial-conditions graph
(`Model.full_state_graph` — the same root state_graph / basin_map / transition_matrix
use), follows EVERY valid starting state forward via the existing successor relation,
and draws an ENSEMBLE of timelines. Per signal we show the reachable ENVELOPE at each
tick: where all timelines agree the trace is crisp; where they diverge it fans into a
filled band — so a 0/1 Seq/enum/bool signal reads as a proper digital trace bounded by
what ANY initial condition can do, not one run. (See `timing_ensemble.py`.)

The HONEST SINGLE-SEED FALLBACK is kept for real / string / seq / unbounded-int /
two-tick models (not finitely enumerable): there we follow ONE successor chain for
~40 ticks. For purely autonomous systems whose own initial state is a fixed point
(e.g. vanderpol's origin) we pick a non-trivial seed so the waveform moves; otherwise
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
from timing_ensemble import build_ensemble, track_band

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


def _expand_tracks(state_vars):
    """One lane per SCALAR track. A scalar var is one track; a Seq(elem) of length L
    becomes L analog tracks `name[0..L-1]` (a Seq is a vector — a single lane can't show
    its per-element dynamics). Each track carries a `get(state)` that pulls its scalar
    value out of a trajectory state dict, so the lane loop never touches list values."""
    tracks = []
    for v in state_vars:
        if v["kind"] == "seq":
            elem = v.get("elem", "int")
            for i in range(v.get("len", 0)):
                tracks.append({"name": f"{v['name']}[{i}]", "kind": elem,
                               "get": (lambda s, nm=v["name"], j=i: s[nm][j])})
        else:
            tracks.append({"name": v["name"], "kind": v["kind"],
                           "get": (lambda s, nm=v["name"]: s[nm])})
    return tracks


LANE_H = 1.0
GAP = 0.55
DIGITAL_COLOR = "#1f77b4"
ANALOG_COLOR = "#d62728"
ENUM_COLOR = "#2ca02c"


def _ordinal_levels(m, track, observed):
    """The y-ORDINAL map for a track: (level_of(value) -> [0,1], lo_label, hi_label).
    bool -> {False:0, True:1}; enum -> index in the declared variant list; string ->
    rank in the observed-value order; int/real -> linear over the observed [min,max]
    span. Shared by the single-seed AND ensemble paths so a value maps to the SAME lane
    height in both — `observed` is every value the lane shows (one trace, or the union
    over the ensemble)."""
    kind = track["kind"]
    if kind == "bool":
        return (lambda v: 1.0 if v else 0.0), "0", "1"
    if kind in ("enum", "string"):
        if kind == "enum":
            variants = m.enum_variants[track["name"].split("[")[0]]
        else:
            variants = sorted(set(observed), key=lambda s: (s != "", s))
        nv = max(1, len(variants))
        order = {variant: i for i, variant in enumerate(variants)}
        lo = str(variants[0]) if variants else ""
        hi = str(variants[-1]) if nv > 1 else ""
        return (lambda v: order.get(v, 0) / max(1, nv - 1)), lo, hi
    vmin, vmax = (min(observed), max(observed)) if observed else (0, 1)
    span = (vmax - vmin) or 1.0
    return (lambda v: (v - vmin) / span), f"{vmin}", f"{vmax}"


def _lane_chrome(ax, idx, base, track, lo, hi):
    """Per-lane background band, baseline, and the lo/hi axis labels — shared by both
    drawers so the lane frame is identical whether we plot a single trace or a band."""
    ax.axhspan(base - GAP / 2, base + LANE_H + GAP / 2,
               facecolor="#f7f7f7" if idx % 2 == 0 else "#ffffff", zorder=0)
    ax.axhline(base, color="#cccccc", lw=0.5, zorder=1)
    if lo:
        ax.text(-0.6, base, lo, va="center", ha="right", fontsize=7, color="#888")
    if hi:
        ax.text(-0.6, base + LANE_H, hi, va="center", ha="right", fontsize=7,
                color="#888")


def _draw_single_lane(ax, m, track, base, vals, n):
    """One trace (the from-init / fallback path): a crisp digital/analog waveform."""
    ticks = list(range(n))
    level, lo, hi = _ordinal_levels(m, track, vals)
    kind = track["kind"]
    ys = [base + level(vals[t]) * LANE_H for t in range(n)]
    if kind in ("bool", "enum", "string"):
        color = DIGITAL_COLOR if kind == "bool" else ENUM_COLOR
        ax.step(ticks, ys, where="post", color=color, lw=2, zorder=3)
        if kind == "bool":
            ax.fill_between(ticks, base, ys, step="post",
                            color=color, alpha=0.12, zorder=2)
        else:
            last = None
            for t in range(n):
                if vals[t] != last:
                    ax.text(t + 0.08, ys[t] + 0.06, str(vals[t]), fontsize=7,
                            color="#1a661a", va="bottom", zorder=4)
                    last = vals[t]
    else:
        ax.plot(ticks, ys, color=ANALOG_COLOR, lw=1.6, marker="o",
                markersize=2.5, zorder=3)
    _lane_chrome(ax, track["_idx"], base, track, lo, hi)


_ENSEMBLE_OVERLAY = 12   # how many individual timelines to draw faintly over the band


def _draw_ensemble_lane(ax, m, track, base, bands, n, ensemble):
    """The ALL-INITIAL-CONDITIONS envelope for one signal: bands[t] is the set of values
    the signal takes at tick t over every timeline. We draw, per tick, the [min,max] of
    the reachable LEVELS as a filled band (step-held) plus its min/max edges — so a tick
    where all timelines agree collapses to a crisp digital edge, and a tick where they
    diverge shows the spread the program can be in from SOME initial condition. A faint
    sample of the actual individual timelines is overlaid so the ensemble reads as real
    trajectories (their shape), not just an opaque envelope."""
    ticks = list(range(n))
    observed = [v for band in bands for v in band]
    level, lo, hi = _ordinal_levels(m, track, observed)
    kind = track["kind"]
    digital = kind in ("bool", "enum", "string")
    color = DIGITAL_COLOR if kind == "bool" else (ENUM_COLOR if digital else ANALOG_COLOR)
    los = [base + min(level(v) for v in bands[t]) * LANE_H for t in range(n)]
    his = [base + max(level(v) for v in bands[t]) * LANE_H for t in range(n)]
    ax.fill_between(ticks, los, his, step="post" if digital else None,
                    color=color, alpha=0.18, zorder=2)
    # faint individual timelines (a stable evenly-spaced sample of the ensemble), so the
    # picture shows the real per-initial-condition runs inside the reachable envelope.
    get = track["get"]
    step = max(1, len(ensemble) // _ENSEMBLE_OVERLAY)
    for trace in ensemble[::step]:
        ys = [base + level(get(trace[t])) * LANE_H for t in range(n)]
        if digital:
            ax.step(ticks, ys, where="post", color=color, lw=0.8, alpha=0.45, zorder=3)
        else:
            ax.plot(ticks, ys, color=color, lw=0.8, alpha=0.45, zorder=3)
    # envelope edges on top, so the reachable bound stays crisp over the sampled runs.
    if digital:
        ax.step(ticks, los, where="post", color=color, lw=1.6, zorder=4)
        ax.step(ticks, his, where="post", color=color, lw=1.6, zorder=4)
    else:
        ax.plot(ticks, los, color=color, lw=1.4, zorder=4)
        ax.plot(ticks, his, color=color, lw=1.4, zorder=4)
    if kind in ("enum", "string"):
        last = None
        for t in range(n):
            v = bands[t][0]
            if v != last:
                ax.text(t + 0.08, los[t] + 0.06, str(v), fontsize=6,
                        color="#1a661a", va="bottom", zorder=5)
                last = v
    _lane_chrome(ax, track["_idx"], base, track, lo, hi)


def _placeholder_fig(out_path, title, msg):
    fig, ax = plt.subplots(figsize=(11, 3))
    ax.axis("off")
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13)
    ax.set_title(title, fontsize=13, fontweight="bold")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def _finish(fig, ax, n, nvar, fig_title, subtitle):
    ax.set_xlim(-0.5, n - 1 + 0.5)
    ax.set_ylim(-GAP, nvar * (LANE_H + GAP) - GAP / 2)
    ax.set_xlabel("tick", fontsize=10)
    ax.set_xticks(range(0, n, max(1, n // 20)))
    ax.grid(axis="x", color="#eeeeee", lw=0.5, zorder=0)
    for spine in ("top", "right", "left"):
        ax.spines[spine].set_visible(False)
    ax.legend(handles=[
        Line2D([0], [0], color=DIGITAL_COLOR, lw=2, label="bool (digital)"),
        Line2D([0], [0], color=ENUM_COLOR, lw=2, label="enum/string (lanes)"),
        Line2D([0], [0], color=ANALOG_COLOR, lw=1.6, marker="o", markersize=3,
               label="int/real (analog)"),
    ], loc="upper right", fontsize=7, framealpha=0.9, ncol=3)
    ax.set_title(fig_title + subtitle, fontsize=12, fontweight="bold")
    fig.tight_layout()


def render(m, out_path):
    fig_title = f"{m.fsm}  —  timing_diagram"
    tracks = _expand_tracks(m.state_vars)
    for idx, tr in enumerate(tracks):
        tr["_idx"] = idx
    nvar = len(tracks)

    # ALL-INITIAL-CONDITIONS ensemble for finitely-discrete programs; honest single-seed
    # fallback (None) for real / string / seq / unbounded / two-tick / over-cap.
    ensemble = build_ensemble(m, TICKS)
    if ensemble is None:
        trace = build_trace(m)
        if not trace:
            _placeholder_fig(out_path, fig_title,
                             "N/A: no transition (no reachable trajectory)")
            return
        n = len(trace)
    else:
        n = len(ensemble[0])

    fig_h = max(2.4, 0.95 * nvar + 1.4)
    fig, ax = plt.subplots(figsize=(12, fig_h))
    yticks, yticklabels = [], []

    for idx, track in enumerate(tracks):
        base = (nvar - 1 - idx) * (LANE_H + GAP)   # first track on top
        yticks.append(base + LANE_H / 2)
        yticklabels.append(f"#{idx + 1}  {track['name']}\n[{track['kind']}]")
        if ensemble is None:
            vals = [track["get"](trace[t]) for t in range(n)]
            _draw_single_lane(ax, m, track, base, vals, n)
        else:
            bands = track_band(track, ensemble, n)
            _draw_ensemble_lane(ax, m, track, base, bands, n, ensemble)

    ax.set_yticks(yticks)
    ax.set_yticklabels(yticklabels, fontsize=8)
    if ensemble is None:
        subtitle = f"   ({n - 1} ticks, single seed = {m.label(trace[0])})"
    else:
        subtitle = (f"   ({n - 1} ticks, ensemble over all {len(ensemble)} "
                    f"initial conditions — band = reachable envelope)")
    _finish(fig, ax, n, nvar, fig_title, subtitle)
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
