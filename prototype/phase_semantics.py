"""phase_semantics — what a phase portrait actually shows, on two questions:

A. Summing a REAL sequence (z3 Seq), generically. With a fixed list baked in as an
   if/else lookup, every step is determined → one path. With a genuine symbolic
   Seq (elements unknown, bounded 0..M), each state fans out to (idx+1, acc+v) for
   every possible element v — THAT fan is "sum any sequence." A concrete Seq is
   then just one path threading the fan.

B. Transition-relation-everywhere vs the reachable orbit. The portrait draws the
   transition at EVERY sampled state, but only the orbit from the real initial
   state is "the Fibonacci series"; other grid points are valid relation states
   that the series never reaches (they're the sequences from OTHER seeds, e.g.
   Lucas from (2,1)).

Run from prototype/:  python3 phase_semantics.py  -> results/phase_semantics.png
"""
import os

import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

import z3

INK = "#2a2c34"; MUTED = "#6b7080"; FAN = "#9aa0ad"; BLUE = "#2868d2"
ORANGE = "#eb9628"; GREEN = "#2eb55f"; TEAL = "#0e8a8a"


def _frame(ax, t, xl, yl):
    ax.set_title(t, fontsize=11.5, color=INK, loc="left", pad=8)
    ax.set_xlabel(xl, fontsize=9, color=MUTED); ax.set_ylabel(yl, fontsize=9, color=MUTED)
    ax.tick_params(colors=MUTED, labelsize=8)
    for s in ax.spines.values():
        s.set_color("#d2d6de")


# ── A. generic sum over a real z3 Seq → a fan ────────────────────────────────
ISeq = z3.SeqSort(z3.IntSort())
ELEMMAX, LMAX = 3, 8


def seq_successors(idx, acc, kmax=8):
    """Successors of (idx, acc) under  acc' = acc + s[idx], idx' = idx+1  for a
    SYMBOLIC sequence s (elements 0..ELEMMAX, length 0..LMAX). The sequence might
    have ended here (halt) or continue with any element → a fan."""
    s = z3.Const("s", ISeq)
    i2, a2 = z3.Ints("i2 a2")
    L = z3.Length(s)
    sol = z3.Solver(); sol.add(L >= 0, L <= LMAX)
    sol.add(z3.Or(
        z3.And(idx == L, i2 == idx, a2 == acc),                       # seq ended: halt
        z3.And(idx < L, s[idx] >= 0, s[idx] <= ELEMMAX,               # continues:
               i2 == idx + 1, a2 == acc + s[idx])))                   #   acc += s[idx]
    out = []
    while len(out) < kmax and sol.check() == z3.sat:
        m = sol.model(); t = (m[i2].as_long(), m[a2].as_long()); out.append(t)
        sol.add(z3.Or(i2 != t[0], a2 != t[1]))
    return out


def panel_seq(ax):
    qx, qy, qu, qv = [], [], [], []
    for idx in range(0, LMAX):
        for acc in range(0, 13):
            for (i2, a2) in seq_successors(idx, acc):
                if (i2, a2) == (idx, acc):
                    continue                                  # halt self-loop: not drawn
                qx.append(idx); qy.append(acc)
                qu.append(0.6 * (i2 - idx)); qv.append(0.6 * (a2 - acc))
    ax.quiver(qx, qy, qu, qv, color=FAN, angles="xy", scale_units="xy", scale=1,
              width=0.004, headwidth=4, headlength=5, zorder=2)
    # one concrete sequence threading the fan
    seq = [2, 1, 3, 1, 2]
    idx, acc, path = 0, 0, [(0, 0)]
    for v in seq:
        idx, acc = idx + 1, acc + v; path.append((idx, acc))
    px, py = zip(*path)
    ax.plot(px, py, "-o", color=BLUE, lw=2.4, ms=5, zorder=4,
            label="one concrete seq [2,1,3,1,2]")
    ax.set_xlim(-0.5, LMAX - 0.4); ax.set_ylim(-0.5, 13)
    ax.legend(loc="upper left", fontsize=8, framealpha=0.9)
    _frame(ax, "A · sum a real Seq — generic = FAN over all elements (0..3); "
           "concrete = one path", "idx", "acc")
    ax.text(0.5, -0.155, "each state fans to acc+0..3 because the next element is "
            "UNKNOWN — that fan IS “sum any sequence”", transform=ax.transAxes,
            ha="center", fontsize=8.3, color=MUTED)


# ── B. transition-everywhere vs the reachable orbit (Fibonacci) ──────────────
def panel_fib(ax):
    nxt = lambda a, b: (b, a + b)
    fx2, fy2, fu2, fv2 = [], [], [], []                       # the transition field
    for a in range(0, 9):
        for b in range(0, 9):
            na, nb = nxt(a, b)
            du, dv = na - a, nb - b
            n = np.hypot(du, dv) or 1
            fx2.append(a); fy2.append(b); fu2.append(du / n * 0.62); fv2.append(dv / n * 0.62)
    ax.quiver(fx2, fy2, fu2, fv2, color="#d2d6de", angles="xy", scale_units="xy",
              scale=1, width=0.004, headwidth=4, headlength=5, zorder=1)

    def orbit(a, b, n=7):
        pts = [(a, b)]
        for _ in range(n):
            a, b = nxt(a, b); pts.append((a, b))
        return [(x, y) for (x, y) in pts if x <= 8.4 and y <= 8.4]

    fib = orbit(0, 1)
    fx, fy = zip(*fib)
    ax.plot(fx, fy, "-o", color=BLUE, lw=2.6, ms=6, zorder=5,
            label="orbit from (0,1) = Fibonacci")
    luc = orbit(2, 1)
    lx, ly = zip(*luc)
    ax.plot(lx, ly, "-o", color=ORANGE, lw=2.2, ms=5, zorder=4,
            label="orbit from (2,1) = Lucas")
    ax.plot(0, 1, "o", color=GREEN, ms=10, mfc="none", mew=2.2, zorder=6)
    ax.set_xlim(-0.5, 8.5); ax.set_ylim(-0.5, 8.5); ax.set_aspect("equal")
    ax.legend(loc="upper right", fontsize=8, framealpha=0.9)
    _frame(ax, "B · transition is defined EVERYWHERE; only the orbit from the real "
           "start is the series", "a", "b")
    ax.text(0.5, -0.155, "grey arrows = the transition at states the series never "
            "reaches (other seeds give other sequences)", transform=ax.transAxes,
            ha="center", fontsize=8.3, color=MUTED)


def main():
    fig, axes = plt.subplots(1, 2, figsize=(16, 7.5))
    fig.suptitle("What a phase portrait actually shows", fontsize=17, color=INK,
                 weight="bold", y=0.99)
    panel_seq(axes[0])
    panel_fib(axes[1])
    fig.tight_layout(rect=(0, 0, 1, 0.95))
    out = os.path.join(os.path.dirname(__file__), "results", "phase_semantics.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
