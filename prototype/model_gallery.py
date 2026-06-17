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

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import z3

import phaseportrait as pp
from benchsuite import pretty
from models.core import Transition
from models.examples import SumTo, ListMax

INK = "#2a2c34"; MUTED = "#6b7080"; ACCENTS = ["#2868d2", "#1f9e6d", "#8a4fbf",
                                               "#c0612a", "#b8398a"]


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


# ── the panels: (transition, render-kwargs, card-header) ─────────────────────
PANELS = [
    (SumTo, dict(xaxis="i", yaxis="acc", xr=(-0.5, 6.5), yr=(-1, 18),
                 style="fan", max_succ=1,
                 seeds=[{"i": 5, "acc": 0}, {"i": 4, "acc": 0}, {"i": 3, "acc": 0}],
                 title="sum_to · Σ1..n   (algorithm)"),
     "sum_to  (i, acc)"),
    (ListMax, dict(xaxis="idx", yaxis="best", xr=(-0.5, 8.5), yr=(-1.5, 10.5),
                   style="fan", max_succ=1, seeds=[{"idx": 0, "best": 0}],
                   title="list_max · max over a list   (algorithm)"),
     "list_max  (idx, best)"),
    (Cache, dict(xaxis="n", xr=(-0.6, 5.6), style="fan", max_succ=3,
                 seeds=[{"n": 0}], title="cache · sessions   (daemon, 1-D)"),
     "cache  (n)"),
    (Queue, dict(xaxis="q0", yaxis="q1", xr=(-0.6, 6.6), yr=(-0.6, 6.6),
                 style="fan", max_succ=6, title="queue · 2-stage   (daemon)"),
     "queue  (q0, q1)"),
    (Pipeline, dict(xaxis="q0", yaxis="q1", xr=(-0.6, 5.6), yr=(-0.6, 5.6),
                    fixed={"q2": 0}, style="fan", max_succ=6,
                    title="pipeline · 3-stage, q2=0 slice   (daemon)"),
     "pipeline  (q0, q1, q2)"),
]


def draw_cards(ax):
    ax.axis("off")
    ax.text(0.0, 1.0, "Model definitions", transform=ax.transAxes, va="top",
            ha="left", fontsize=15, weight="bold", color=INK)
    ax.text(0.0, 0.973, "each model's faithful pretty form  (state → state′)  "
            "— ′ marks the next-tick value", transform=ax.transAxes, va="top",
            ha="left", fontsize=9, color=MUTED, style="italic")
    y = 0.94
    for (tr, _, header), accent in zip(PANELS, ACCENTS):
        body = header + "\n" + pretty_step(tr)
        ax.text(0.0, y, body, transform=ax.transAxes, va="top", ha="left",
                family="monospace", fontsize=8.0, color=INK, linespacing=1.35,
                bbox=dict(boxstyle="round,pad=0.5", fc="#fbfcfe",
                          ec=accent, lw=1.4))
        y -= (body.count("\n") + 1) * 0.0130 + 0.024


def main():
    fig = plt.figure(figsize=(18, 15))
    fig.suptitle("Example transition models — program-like, not mathematical",
                 fontsize=20, color=INK, weight="bold", x=0.5, y=0.975)
    fig.text(0.5, 0.948, "every portrait is the generic render() on the model's Z3 "
             "transition;  algorithms (sum_to, list_max) flow to a fixed point whose "
             "coordinate IS the answer",
             ha="center", fontsize=11, color=MUTED)

    outer = fig.add_gridspec(1, 2, width_ratios=[1.5, 1.0], wspace=0.07,
                             left=0.045, right=0.985, top=0.925, bottom=0.045)
    grid = outer[0].subgridspec(3, 2, hspace=0.4, wspace=0.22)
    cells = [grid[0, 0], grid[0, 1], grid[1, 0], grid[1, 1], grid[2, 0]]
    for (tr, kw, _), cell in zip(PANELS, cells):
        pp.render(fig.add_subplot(cell), to_model(tr), **kw)

    note = fig.add_subplot(grid[2, 1]); note.axis("off")
    note.text(0.5, 0.5, "the SAME sum_to also exists as a\nrecursive *function* "
              "(SumToRec).\n\nA function has no state space —\nso it has no phase "
              "portrait.\n\nThat absence is the\n“functional-like vs model-like”\n"
              "distinction.", transform=note.transAxes, ha="center", va="center",
              fontsize=10.5, color=MUTED, linespacing=1.5,
              bbox=dict(boxstyle="round,pad=0.8", fc="#f5f6f9", ec="#d2d6de", lw=1.2))

    draw_cards(fig.add_subplot(outer[1]))
    out = os.path.join(os.path.dirname(__file__), "results", "model_gallery.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
