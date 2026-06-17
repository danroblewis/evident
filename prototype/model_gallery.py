"""model_gallery — render the codebase's example TRANSITION models as phase
portraits, each beside its pretty form (the project's own Z3-AST pretty-printer).

These are the "model-like" examples (state → state′ transitions), in contrast to
the "functional-like" math in phaseportrait.py:

  sum_to    Σ 1..n as a tail-recursive accumulator   (a real algorithm)
  list_max  iterative max over a fixed list           (a real algorithm)
  cache     a session-count daemon (1-D)
  queue     a 2-stage bounded queue daemon
  pipeline  a 3-stage pipeline daemon (shown as the q2=0 slice)

Every portrait is the generic phaseportrait.render() applied to the model's Z3
transition relation; every card on the right is the model's faithful pretty form
via benchsuite.pretty. sum_to/list_max come straight from models/examples.py; the
daemons are core.Transition objects so they share the same .doc() machinery.

Note the contrast the codebase itself draws: sum_to also exists as `SumToRec`, the
SAME computation as a recursive *function* (models/examples.py). A function has no
state-space to flow through — so it has no phase portrait. That absence *is* the
"functional-like vs model-like" distinction.

Run from prototype/:  python3 model_gallery.py  -> results/model_gallery.png
"""
import os
import random
from itertools import combinations
from math import ceil

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import z3

import phaseportrait as pp
from benchsuite import pretty
from models.core import Transition
from models.examples import (SumTo, ListSum, ListMax, Gcd, RunningMean,
                             Fibonacci, TokenBucket, EXAMPLE_SEQ, ELEMMAX, LMAX)


def _sum_path(seq):
    idx, acc, p = 0, 0, [(0, 0)]
    for v in seq:
        idx, acc = idx + 1, acc + v; p.append((idx, acc))
    return p


def _max_path(seq):
    idx, best, p = 0, 0, [(0, 0)]
    for v in seq:
        idx, best = idx + 1, max(best, v); p.append((idx, best))
    return p


def _mean_path(seq):
    n, avg, p = 0, 0.0, [(0, 0.0)]
    for v in seq:
        avg = avg + (v - avg) / (n + 1); n += 1; p.append((n, avg))
    return p


def _draw_ensemble(ax, trajfn, yname, yr, examples, n_sample=120):
    """Variability over the Seq INPUT: one trajectory per sequence, all from the
    fixed start (0,0). Sample many (faint) + label a few examples with their value."""
    random.seed(7)
    for _ in range(n_sample):
        seq = [random.randint(0, ELEMMAX) for _ in range(LMAX)]
        px, py = zip(*trajfn(seq))
        ax.plot(px, py, "-", color="#aab0be", lw=0.8, alpha=0.4, zorder=1)
    for seq, c in examples:
        px, py = zip(*trajfn(seq))
        ax.plot(px, py, "-o", color=c, lw=2.2, ms=4, zorder=4,
                label=f"{seq} → {py[-1]:g}")
    ax.plot(0, 0, "o", color=INK, ms=8, mfc="white", mew=2, zorder=5)
    ax.set_xlim(-0.3, LMAX + 0.3); ax.set_ylim(*yr)
    ax.set_xlabel("idx", fontsize=9, color=MUTED)
    ax.set_ylabel(yname, fontsize=9, color=MUTED, rotation=0, labelpad=12)
    ax.tick_params(colors=MUTED, labelsize=8)
    for s in ax.spines.values():
        s.set_color("#d2d6de")
    ax.legend(loc="upper left", fontsize=7.4, framealpha=0.93, title="example Seq")


EX_SEQS = [([3, 3, 3, 3, 3, 3], "#de3c3c"), ([0, 1, 0, 2, 0, 1], "#2eb55f"),
           ([2, 1, 3, 0, 2], "#2868d2")]

INK = "#2a2c34"; MUTED = "#6b7080"
ACCENTS = ["#2868d2", "#0e8a8a", "#1f9e6d", "#8a4fbf", "#c0612a", "#b8398a"]


# ── the daemon transitions, as core.Transition (so they get .doc() too) ──────
def _cache_step(cur, nxt, CAP=5):
    n, m = cur["n"], nxt["n"]
    return z3.Or(z3.And(n < CAP, m == n + 1),          # open a session
                 z3.And(n > 0, m == n - 1),            # close a session
                 m == n)                                # idle
Cache = Transition("cache", [("n", "Int")], _cache_step)


def _queue_step(cur, nxt, CAP=6):
    q0, q1, m0, m1 = cur["q0"], cur["q1"], nxt["q0"], nxt["q1"]
    return z3.Or(z3.And(q0 < CAP, m0 == q0 + 1, m1 == q1),
                 z3.And(q0 > 0, q1 < CAP, m0 == q0 - 1, m1 == q1 + 1),
                 z3.And(q1 > 0, m0 == q0, m1 == q1 - 1),
                 z3.And(m0 == q0, m1 == q1))
Queue = Transition("queue", [("q0", "Int"), ("q1", "Int")], _queue_step)


def _pipe_step(cur, nxt, CAP=5):
    q0, q1, q2 = cur["q0"], cur["q1"], cur["q2"]
    m0, m1, m2 = nxt["q0"], nxt["q1"], nxt["q2"]
    return z3.Or(z3.And(q0 < CAP, m0 == q0 + 1, m1 == q1, m2 == q2),
                 z3.And(q0 > 0, q1 < CAP, m0 == q0 - 1, m1 == q1 + 1, m2 == q2),
                 z3.And(q1 > 0, q2 < CAP, m0 == q0, m1 == q1 - 1, m2 == q2 + 1),
                 z3.And(q2 > 0, m0 == q0, m1 == q1, m2 == q2 - 1),
                 z3.And(m0 == q0, m1 == q1, m2 == q2))
Pipeline = Transition("pipeline", [("q0", "Int"), ("q1", "Int"), ("q2", "Int")],
                      _pipe_step)


_SORT = {"Int": z3.IntSort, "Real": z3.RealSort, "Bool": z3.BoolSort}


def to_model(tr):
    return pp.Model(tr.name, [n for n, _ in tr.fields], tr.step,
                    sorts={n: t for n, t in tr.fields})


def pretty_step(tr, width=40):
    cur = {n: z3.Const(n, _SORT[t]()) for n, t in tr.fields}
    nxt = {n: z3.Const(n + "′", _SORT[t]()) for n, t in tr.fields}
    return pretty.expr(tr.step(cur, nxt), width=width)


# ── the panels (uniform schema: ranges per var, all rendered the same way) ───
#   ranges: {var: (lo, hi)}   base: held value per var (for >2-D projections)
#   seeds:  full-state start points for trajectories   equal: square aspect
PANELS = [
    (SumTo, dict(ranges={"i": (-0.5, 6.5), "acc": (-1, 18)}, style="fan",
                 max_succ=1,
                 seeds=[{"i": 5, "acc": 0}, {"i": 4, "acc": 0}, {"i": 3, "acc": 0}],
                 reachable=[{"i": 5, "acc": 0}, {"i": 4, "acc": 0}, {"i": 3, "acc": 0}],
                 title="sum_to · Σ1..n   (algorithm)"),
     "sum_to  (i, acc)",
     "Tail-recursive accumulator. The flow funnels into the i=0 halt line; the\n"
     "trajectory from (5,0) lands at (0,15) — and 15 IS the sum 1..5."),
    (ListSum, dict(ensemble=_sum_path, yname="acc", yr=(-0.5, ELEMMAX * LMAX + 1),
                   examples=EX_SEQS,
                   title="list_sum · sum a sequence   (real z3 Seq)"),
     "list_sum  (idx, acc)",
     "The only input is the SEQUENCE (idx and acc start fixed at 0), so vary the Seq,\n"
     "not the state: ONE line per sequence, all from (0,0). 120 sampled (faint) + 3\n"
     "labelled; acc fans into the 0 ≤ acc ≤ 3·idx region."),
    (ListMax, dict(ensemble=_max_path, yname="best", yr=(-0.3, ELEMMAX + 0.3),
                   examples=EX_SEQS,
                   title="list_max · max over a sequence   (real z3 Seq)"),
     "list_max  (idx, best)",
     "Variability is over the Seq, not the state (idx/best start fixed at 0). One\n"
     "trajectory per sequence from (0,0); best rises to the sequence's max element\n"
     "(≤3). 120 sampled + 3 labelled with their values."),
    (Gcd, dict(ranges={"a": (-0.5, 13), "b": (-0.5, 13)}, style="fan", max_succ=1,
               equal=True, seeds=[{"a": 12, "b": 8}, {"a": 13, "b": 5},
                                  {"a": 9, "b": 12}],
               reachable=[{"a": 12, "b": 8}, {"a": 13, "b": 5}, {"a": 9, "b": 12}],
               title="gcd · Euclid's algorithm   (algorithm)"),
     "gcd  (a, b)",
     "Euclid: (a,b) → (b, a mod b) until b=0, then gcd is in `a`. TWO interacting\n"
     "variables — every trajectory flows onto the b=0 axis, where a is the answer\n"
     "(e.g. (12,8) → (8,4) → (4,0): gcd = 4)."),
    (RunningMean, dict(ensemble=_mean_path, yname="avg", yr=(-0.3, ELEMMAX + 0.3),
                       examples=EX_SEQS,
                       title="running_mean · online average   (real z3 Seq)"),
     "running_mean  (n, avg)",
     "One line per sequence from (0,0); avg (a Real) relaxes toward each sequence's\n"
     "mean. The Seq is the input — n and avg start fixed. 120 sampled + 3 labelled."),
    (Fibonacci, dict(ranges={"a": (-0.5, 9), "b": (-0.5, 9)}, style="fan",
                     max_succ=1, equal=True, seeds=[{"a": 0, "b": 1}],
                     reachable=[{"a": 0, "b": 1}],
                     title="fibonacci · (a,b)→(b,a+b)   (never halts)"),
     "fibonacci  (a, b)",
     "Deterministic, so drawn as ORBITS (a field would falsely imply they merge —\n"
     "the map is a bijection). Bold = the series from (0,1); the grey orbits are\n"
     "OTHER seeds (e.g. Lucas), disjoint hyperbolas that never touch it."),
    (Cache, dict(ranges={"n": (-0.6, 5.6)}, style="fan", max_succ=3,
                 seeds=[{"n": 0}], title="cache · sessions   (daemon, 1-D)"),
     "cache  (n)",
     "A session-count daemon. 1-D state, so its one projection is a number line:\n"
     "open → (n+1, when n<CAP), close ← (n-1, when n>0), idle."),
    (Queue, dict(ranges={"q0": (-0.6, 6.6), "q1": (-0.6, 6.6)}, style="fan",
                 max_succ=6, equal=True, title="queue · 2-stage   (daemon)"),
     "queue  (q0, q1)",
     "Two bounded stages. Nondeterministic: each state has a FAN of moves\n"
     "(arrive, q0→q1, depart, idle). The flow stays inside [0,CAP]²."),
    (TokenBucket, dict(ranges={"tokens": (-0.6, 6.6), "pending": (-0.6, 7.6)},
                       style="fan", max_succ=6, equal=True,
                       title="token_bucket · rate limiter   (daemon)"),
     "token_bucket  (tokens, pending)",
     "A rate limiter: tokens refill up to CAP, requests queue as `pending`, a serve\n"
     "spends one token per request. Nondeterministic fan; tokens stay in [0,CAP]\n"
     "(Spacer-provable: never overspend)."),
    (Pipeline, dict(ranges={"q0": (-0.6, 5.6), "q1": (-0.6, 5.6),
                            "q2": (-0.6, 5.6)}, base={"q0": 2, "q1": 2, "q2": 2},
                    style="fan", max_succ=6, equal=True,
                    title="pipeline · 3-stage   (daemon, 3-D → all projections)"),
     "pipeline  (q0, q1, q2)",
     "Three stages = 3-D state — too many to draw directly, so render ALL\n"
     "pairwise projections (the third var held at 2). Only adjacent stages couple,\n"
     "so q0×q2 is nearly inert; q0×q1 and q1×q2 carry the transfer flow."),
]


def _code_card(fig, cell, body, accent):
    cax = fig.add_subplot(cell); cax.axis("off")
    cax.text(0.0, 1.0, body, transform=cax.transAxes, va="top", ha="left",
             family="monospace", fontsize=10.5, color=INK, linespacing=1.45,
             bbox=dict(boxstyle="round,pad=0.7", fc="#fbfcfe", ec=accent, lw=1.6))


def render_to_file(tr, kw, header, blurb, accent, outdir):
    """Each model beside its pretty form. A Seq-parameterized model shows the
    variability over its Seq input (ensemble of trajectories); a state model shows
    its 2-D projection(s) — one for 2-D, a number line for 1-D, the matrix for >2-D."""
    model = to_model(tr)
    body = header + "\n" + pretty_step(tr, width=46)
    nlines = body.count("\n") + 1

    if "ensemble" in kw:                                  # variability over the Seq input
        H = max(5.6, nlines * 0.245 + 2.0)
        fig = plt.figure(figsize=(12.5, H))
        gs = fig.add_gridspec(1, 2, width_ratios=[1.15, 1.0], wspace=0.04,
                              left=0.07, right=0.97, top=0.80, bottom=0.1)
        _draw_ensemble(fig.add_subplot(gs[0]), kw["ensemble"], kw["yname"],
                       kw["yr"], kw["examples"])
        _code_card(fig, gs[1], body, accent)
    else:                                                 # state-space projection(s)
        state = model.state
        ranges = kw["ranges"]; base = kw.get("base", {})
        seeds = kw.get("seeds", []); style = kw.get("style", "fan")
        ms = kw.get("max_succ", 6); equal = kw.get("equal", False)
        pairs = list(combinations(state, 2))
        ncells = max(1, len(pairs))                       # 1-D model: one number line
        cols = 1 if ncells == 1 else (2 if ncells <= 4 else 3)
        rows = ceil(ncells / cols)
        H = max(5.6, nlines * 0.245 + 2.2, rows * 3.9 + 2.0)
        W = 12.5 if ncells == 1 else 15.5
        wr = [1.15, 1.0] if ncells == 1 else [1.55, 1.0]
        fig = plt.figure(figsize=(W, H))
        outer = fig.add_gridspec(1, 2, width_ratios=wr, wspace=0.05,
                                 left=0.06, right=0.97, top=0.80, bottom=0.09)
        grid = outer[0].subgridspec(rows, cols, hspace=0.5, wspace=0.34)
        if not pairs:                                     # 1-D: a number line
            pp.render(fig.add_subplot(grid[0, 0]), model, state[0], None,
                      ranges[state[0]], style=style, max_succ=ms, seeds=seeds, title="")
        for k, (a, b) in enumerate(pairs):
            ax = fig.add_subplot(grid[k // cols, k % cols])
            held = [v for v in state if v not in (a, b)]
            fixed = {v: base.get(v, 0) for v in held}
            tag = f"   ({', '.join(f'{v}={fixed[v]}' for v in held)})" if held else ""
            pp.render(ax, model, a, b, ranges[a], ranges[b], fixed=fixed, style=style,
                      max_succ=ms, equal=equal, seeds=(seeds if not held else []),
                      reachable=(kw.get("reachable") if not held else None),
                      paths=(kw.get("paths") if not held else None),
                      title=(f"{a} × {b}{tag}" if len(pairs) > 1 else ""))
        _code_card(fig, outer[1], body, accent)

    fig.suptitle(kw["title"], fontsize=15, color=INK, weight="bold",
                 x=0.06, ha="left", y=0.965)
    fig.text(0.06, 0.915, blurb, fontsize=9.5, color=MUTED, va="top",
             linespacing=1.4)
    path = os.path.join(outdir, f"{tr.name}.png")
    fig.savefig(path, dpi=130, facecolor="white"); plt.close(fig)
    return path


def main():
    outdir = os.path.join(os.path.dirname(__file__), "results", "models")
    os.makedirs(outdir, exist_ok=True)
    for k, (tr, kw, header, blurb) in enumerate(PANELS):
        path = render_to_file(tr, kw, header, blurb, ACCENTS[k % len(ACCENTS)], outdir)
        print("wrote", os.path.relpath(path), flush=True)


if __name__ == "__main__":
    main()
