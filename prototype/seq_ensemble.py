"""seq_ensemble — the HONEST variability for a Seq-parameterized model.

A fan-field draws the transition from every (idx, best) state — including states you
never start in. But list_max/list_sum/running_mean have a FIXED start (idx=0,
acc/best=0) and their only real input is the SEQUENCE. So the meaningful variability
is: fix the start, vary the Seq, and draw the resulting trajectory — ONE line per
sequence. You can't draw all sequences (infinitely/exponentially many), so SAMPLE
many; the spread of lines is the variability, and a few example sequences are
labeled with their values.

Run from prototype/:  python3 seq_ensemble.py -> results/seq_ensemble.png
"""
import os
import random

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

INK = "#2a2c34"; MUTED = "#6b7080"
ELEMMAX, LMAX = 3, 6
random.seed(7)


def max_traj(seq):
    best, p = 0, [(0, 0)]
    for i, v in enumerate(seq):
        best = max(best, v); p.append((i + 1, best))
    return p


def sum_traj(seq):
    acc, p = 0, [(0, 0)]
    for i, v in enumerate(seq):
        acc += v; p.append((i + 1, acc))
    return p


def mean_traj(seq):
    n, avg, p = 0, 0.0, [(0, 0.0)]
    for v in seq:
        avg = avg + (v - avg) / (n + 1); n += 1; p.append((n, avg))
    return p


PANELS = [
    ("list_max  ·  best = max so far", max_traj, "best", (-0.3, ELEMMAX + 0.3)),
    ("list_sum  ·  acc = running total", sum_traj, "acc", (-0.5, ELEMMAX * LMAX + 1)),
    ("running_mean  ·  avg (a Real)", mean_traj, "avg", (-0.3, ELEMMAX + 0.3)),
]
EXAMPLES = [([3, 3, 3, 3, 3, 3], "#de3c3c"), ([0, 1, 0, 2, 0, 1], "#2eb55f"),
            ([2, 1, 3, 0, 2], "#2868d2")]


def main():
    sample = [[random.randint(0, ELEMMAX) for _ in range(LMAX)] for _ in range(120)]
    fig, axes = plt.subplots(1, 3, figsize=(17, 5.8))
    fig.suptitle("Variability over the Seq parameter — one line per sequence, all "
                 "from the fixed start (0,0)", fontsize=15, color=INK,
                 weight="bold", y=0.99)
    for ax, (title, traj, yl, yr) in zip(axes, PANELS):
        for seq in sample:                            # the ensemble (sampled Seqs)
            px, py = zip(*traj(seq))
            ax.plot(px, py, "-", color="#aab0be", lw=0.8, alpha=0.45, zorder=1)
        for seq, c in EXAMPLES:                       # a few labelled example Seqs
            px, py = zip(*traj(seq))
            ax.plot(px, py, "-o", color=c, lw=2.2, ms=4, zorder=4,
                    label=f"{seq} → {py[-1]:g}")
        ax.plot(0, 0, "o", color=INK, ms=8, mfc="white", mew=2, zorder=5)
        ax.set_xlim(-0.3, LMAX + 0.3); ax.set_ylim(*yr)
        ax.set_xlabel("idx", fontsize=9, color=MUTED)
        ax.set_ylabel(yl, fontsize=9, color=MUTED, rotation=0, labelpad=12)
        ax.tick_params(colors=MUTED, labelsize=8)
        for s in ax.spines.values():
            s.set_color("#d2d6de")
        ax.set_title(title, fontsize=11.5, color=INK, loc="left", pad=7)
        ax.legend(loc="upper left", fontsize=7.6, framealpha=0.93, title="example Seq")
    fig.text(0.5, 0.015, "the start (idx=0, value=0) is NOT a variable — only the "
             "Seq is; 120 sampled sequences (elements 0..3, length 6) shown faint, "
             "3 labelled", ha="center", fontsize=9, color=MUTED)
    fig.tight_layout(rect=(0, 0.03, 1, 0.95))
    out = os.path.join(os.path.dirname(__file__), "results", "seq_ensemble.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
