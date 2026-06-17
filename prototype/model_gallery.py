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


# ── the panels: (transition, render-kwargs, card-header, blurb) ──────────────
PANELS = [
    (SumTo, dict(xaxis="i", yaxis="acc", xr=(-0.5, 6.5), yr=(-1, 18),
                 style="fan", max_succ=1,
                 seeds=[{"i": 5, "acc": 0}, {"i": 4, "acc": 0}, {"i": 3, "acc": 0}],
                 title="sum_to · Σ1..n   (algorithm)"),
     "sum_to  (i, acc)",
     "Tail-recursive accumulator. The flow funnels into the i=0 halt line; the\n"
     "trajectory from (5,0) lands at (0,15) — and 15 IS the sum 1..5."),
    (ListMax, dict(xaxis="idx", yaxis="best", xr=(-0.5, 8.5), yr=(-1.5, 10.5),
                   style="fan", max_succ=1, seeds=[{"idx": 0, "best": 0}],
                   title="list_max · max over a list   (algorithm)"),
     "list_max  (idx, best)",
     "Iterative max over [3,1,4,1,5,9,2,6] (composes the `at` lookup sub-model).\n"
     "Flows right to idx=8, settling at best=9 — the maximum."),
    (Cache, dict(xaxis="n", xr=(-0.6, 5.6), style="fan", max_succ=3,
                 seeds=[{"n": 0}], title="cache · sessions   (daemon, 1-D)"),
     "cache  (n)",
     "A session-count daemon. 1-D state, so the portrait is a number line:\n"
     "open → (n+1, when n<CAP), close ← (n-1, when n>0), idle."),
    (Queue, dict(xaxis="q0", yaxis="q1", xr=(-0.6, 6.6), yr=(-0.6, 6.6),
                 style="fan", max_succ=6, equal=True,
                 title="queue · 2-stage   (daemon)"),
     "queue  (q0, q1)",
     "Two bounded stages. Nondeterministic: each state has a FAN of moves\n"
     "(arrive, q0→q1, depart, idle). The flow stays inside [0,CAP]²."),
    (Pipeline, dict(xaxis="q0", yaxis="q1", xr=(-0.6, 5.6), yr=(-0.6, 5.6),
                    fixed={"q2": 0}, style="fan", max_succ=6, equal=True,
                    title="pipeline · 3-stage, q2=0 slice   (daemon)"),
     "pipeline  (q0, q1, q2)",
     "Three stages — 3-D state, so this is the q2=0 SLICE. Moves that change q2\n"
     "leave the slice (their q1 drop shows, the q2 rise is hidden by projection)."),
]


def render_to_file(tr, kw, header, blurb, accent, outdir):
    body = header + "\n" + pretty_step(tr, width=46)
    nlines = body.count("\n") + 1
    H = max(5.6, nlines * 0.245 + 2.0)
    fig = plt.figure(figsize=(12.5, H))
    fig.suptitle(kw["title"], fontsize=15, color=INK, weight="bold",
                 x=0.07, ha="left", y=0.965)
    fig.text(0.07, 0.915, blurb, fontsize=9.5, color=MUTED, va="top",
             linespacing=1.4)
    gs = fig.add_gridspec(1, 2, width_ratios=[1.15, 1.0], wspace=0.04,
                          left=0.07, right=0.97, top=0.80, bottom=0.1)
    kw2 = dict(kw); kw2.pop("title")
    pp.render(fig.add_subplot(gs[0]), to_model(tr), title="", **kw2)
    cax = fig.add_subplot(gs[1]); cax.axis("off")
    cax.text(0.0, 1.0, body, transform=cax.transAxes, va="top", ha="left",
             family="monospace", fontsize=10.5, color=INK, linespacing=1.45,
             bbox=dict(boxstyle="round,pad=0.7", fc="#fbfcfe", ec=accent, lw=1.6))
    path = os.path.join(outdir, f"{tr.name}.png")
    fig.savefig(path, dpi=130, facecolor="white"); plt.close(fig)
    return path


def main():
    outdir = os.path.join(os.path.dirname(__file__), "results", "models")
    os.makedirs(outdir, exist_ok=True)
    for (tr, kw, header, blurb), accent in zip(PANELS, ACCENTS):
        path = render_to_file(tr, kw, header, blurb, accent, outdir)
        print("wrote", os.path.relpath(path), flush=True)


if __name__ == "__main__":
    main()
