"""highdim_demo — how to handle a model with too many dimensions to draw.

A phase portrait is inherently 2-D (3-D at a push). Real models have more state.
This demos the three generic escapes on a 4-stage pipeline (4-D state q0..q3):

  1. PROJECT  — a matrix of all pairwise 2-D slices (hold the other vars fixed).
                C(4,2) = 6 slices. The constraint graph says which pairs are even
                coupled; here only ADJACENT stages are, so the q0×q2 / q0×q3 /
                q1×q3 slices are nearly inert — visible, and informative.
  2. REDUCE   — collapse the 4-D state to a scalar (here total occupancy and each
                stage) and plot it over TIME. This is how daemons are actually
                monitored: a metrics-over-time dashboard is a projection onto
                (tick, derived-quantity).
  3. PROVE    — Spacer proves 0 ≤ qi ≤ CAP for ALL FOUR dimensions at once, in one
                query. Verification is dimension-agnostic; only the PICTURE has the
                2-D limit. So in high-D you lean on the proof and use the picture
                as a witness, not the guarantee.

(The fourth, design-level answer — DECOMPOSE the 4-D claim into small composable
claims each with ≤2 carried vars — lives in docs/plans/phase-portraits.md.)

Run from prototype/:  python3 highdim_demo.py  -> results/highdim_pipeline.png
"""
import os
from itertools import combinations

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import z3

import phaseportrait as pp

INK = "#2a2c34"; MUTED = "#6b7080"; GREEN = "#2eb55f"
N, CAP = 4, 4
STATE = [f"q{i}" for i in range(N)]


def pipeline_model():
    def T(c, n):
        hold = lambda ex: [n[f"q{j}"] == c[f"q{j}"] for j in range(N) if j not in ex]
        moves = [z3.And(c["q0"] < CAP, n["q0"] == c["q0"] + 1, *hold({0}))]   # arrive
        for i in range(N - 1):                                                # q_i→q_{i+1}
            moves.append(z3.And(c[f"q{i}"] > 0, c[f"q{i+1}"] < CAP,
                                n[f"q{i}"] == c[f"q{i}"] - 1,
                                n[f"q{i+1}"] == c[f"q{i+1}"] + 1, *hold({i, i + 1})))
        moves.append(z3.And(c[f"q{N-1}"] > 0, n[f"q{N-1}"] == c[f"q{N-1}"] - 1,
                            *hold({N - 1})))                                  # depart
        moves.append(z3.And(*[n[v] == c[v] for v in STATE]))                 # idle
        return z3.Or(*moves)
    bad = lambda c: z3.Or(*[z3.Or(c[v] < 0, c[v] > CAP) for v in STATE])
    return pp.Model("pipeline4", STATE, T, "Int",
                    init={v: 0 for v in STATE}, bad=bad)


def run_trace(model, T=80):
    """A concrete run: greedily fill for the first half, drain the second — so the
    total sweeps up then down, staying inside [0, N*CAP]."""
    pt = dict(model.init); rows = [dict(pt)]
    for t in range(T):
        sc = pp.successors(model, pt, 8)
        if not sc:
            break
        up = t < T / 2
        pt = max(sc, key=lambda s: sum(s.values()) if up else -sum(s.values()))
        rows.append(dict(pt))
    return rows


def main():
    M = pipeline_model()
    verdict = pp.spacer_verdict(M)
    print("Spacer (all 4 dims at once):", verdict,
          "(unsat = proved 0<=qi<=CAP forever)", flush=True)

    fig = plt.figure(figsize=(15, 13.5))
    fig.suptitle("Too many dimensions? Project · Reduce · Prove  "
                 "(a 4-stage pipeline, q0..q3)", fontsize=17, color=INK,
                 weight="bold", y=0.975)
    gs = fig.add_gridspec(3, 3, hspace=0.42, wspace=0.3,
                          left=0.06, right=0.97, top=0.9, bottom=0.07,
                          height_ratios=[1, 1, 0.9])

    # 1. PROJECT — the 6 pairwise slices (others held at CAP//2)
    base = {v: CAP // 2 for v in STATE}
    for k, (a, b) in enumerate(combinations(STATE, 2)):
        ax = fig.add_subplot(gs[k // 3, k % 3])
        fixed = {v: base[v] for v in STATE if v not in (a, b)}
        held = ", ".join(f"{v}={base[v]}" for v in fixed)
        pp.render(ax, M, a, b, (-0.5, CAP + 0.5), (-0.5, CAP + 0.5),
                  fixed=fixed, style="fan", max_succ=6, equal=True,
                  title=f"{a} × {b}   (hold {held})")

    # 2. REDUCE — the 4-D state as scalars over time
    ax = fig.add_subplot(gs[2, :])
    trace = run_trace(M)
    ts = range(len(trace))
    for i, c in enumerate(["#2868d2", "#1f9e6d", "#8a4fbf", "#c0612a"]):
        ax.plot(ts, [r[f"q{i}"] for r in trace], color=c, lw=1.6, label=f"q{i}")
    ax.plot(ts, [sum(r.values()) for r in trace], color=INK, lw=2.4,
            label="total")
    ax.axhline(N * CAP, color=GREEN, ls="--", lw=1.4)
    ax.text(1, N * CAP + 0.3, f"total bound = N·CAP = {N * CAP}  "
            "(Spacer-proved envelope)", color=GREEN, fontsize=9)
    ax.set_xlim(0, len(trace) - 1); ax.set_ylim(0, N * CAP + 2)
    ax.set_xlabel("tick", fontsize=9, color=MUTED)
    ax.set_ylabel("occupancy", fontsize=9, color=MUTED)
    ax.tick_params(colors=MUTED, labelsize=8)
    for sp in ax.spines.values():
        sp.set_color("#d2d6de")
    ax.legend(loc="upper right", fontsize=8, ncol=5, framealpha=0.9)
    ax.set_title("REDUCE — collapse 4-D state to scalars over time  "
                 f"(PROVE — Spacer: {verdict} for all 4 dims at once)",
                 fontsize=11.5, color=INK, loc="left", pad=7)

    out = os.path.join(os.path.dirname(__file__), "results", "highdim_pipeline.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
