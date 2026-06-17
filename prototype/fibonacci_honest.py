"""fibonacci_honest — the honest phase portrait of the Fibonacci map.

The direction-field portrait MISLEADS for this model: M(a,b)=(b,a+b) is a bijection
(det −1), so orbits PARTITION the plane and never merge — no state ever flows from
one orbit onto another. The normalized arrows only point toward the golden-ratio
eigendirection; they never land on the Fibonacci orbit.

The truthful invariant: |a² + ab − b²| is conserved (it flips sign each step), so
every orbit lies on its own hyperbola. Fibonacci is the |·|=1 hyperbola, Lucas the
|·|=5 one — different curves that never touch. This draws those level sets and the
discrete points hopping along them.

Run from prototype/:  python3 fibonacci_honest.py -> results/fibonacci_honest.png
"""
import os

import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

INK = "#2a2c34"; MUTED = "#6b7080"; BLUE = "#2868d2"; ORANGE = "#eb9628"
GHOST = "#dfe2e9"


def Q(a, b):
    return a * a + a * b - b * b


def orbit(a, b, n, lo, hi):
    pts = [(a, b)]
    for _ in range(n):
        a, b = b, a + b
        pts.append((a, b))
    return [(x, y) for x, y in pts if lo <= x <= hi and lo <= y <= hi]


def main():
    lo, hi = -4, 11
    xs = np.linspace(lo, hi, 500)
    X, Y = np.meshgrid(xs, xs)
    Z = X ** 2 + X * Y - Y ** 2

    fig, ax = plt.subplots(figsize=(9.5, 9))
    # other orbits = other level sets (faint) — each its own disjoint hyperbola
    ax.contour(X, Y, Z, levels=[-19, -11, -4, 4, 11, 19], colors=GHOST,
               linewidths=1.0)
    # Lucas lives on |Q| = 5
    ax.contour(X, Y, Z, levels=[-5, 5], colors=ORANGE, linewidths=1.6)
    # Fibonacci lives on |Q| = 1
    ax.contour(X, Y, Z, levels=[-1, 1], colors=BLUE, linewidths=1.8)

    def draw(seed, color, label):
        pts = orbit(*seed, 8, lo, hi)
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            ax.annotate("", xy=(x1, y1), xytext=(x0, y0),
                        arrowprops=dict(arrowstyle="-|>", color=color, lw=1.6,
                                        shrinkA=7, shrinkB=7))
        px, py = zip(*pts)
        ax.plot(px, py, "o", color=color, ms=7, zorder=5, label=label)

    draw((0, 1), BLUE, "Fibonacci  (seed (0,1), on |a²+ab−b²|=1)")
    draw((2, 1), ORANGE, "Lucas  (seed (2,1), on |a²+ab−b²|=5)")

    ax.set_xlim(lo, hi); ax.set_ylim(lo, hi); ax.set_aspect("equal")
    ax.axhline(0, color="#c8ccd6", lw=0.7); ax.axvline(0, color="#c8ccd6", lw=0.7)
    ax.set_xlabel("a", fontsize=10, color=MUTED); ax.set_ylabel("b", fontsize=10, color=MUTED)
    ax.tick_params(colors=MUTED, labelsize=8)
    for s in ax.spines.values():
        s.set_color("#d2d6de")
    ax.legend(loc="lower right", fontsize=9, framealpha=0.95)
    ax.set_title("Fibonacci, honestly — orbits are DISJOINT hyperbolas, they never "
                 "merge", fontsize=13, color=INK, loc="left", pad=10)
    ax.text(0.02, 0.97, "|a² + ab − b²| is conserved (flips sign each step), so each "
            "seed stays on its own\ncurve. Fibonacci (|·|=1) and Lucas (|·|=5) are "
            "different hyperbolas — no state\never crosses between them. The grey "
            "curves are yet other orbits.", transform=ax.transAxes, va="top",
            fontsize=8.8, color=MUTED)

    out = os.path.join(os.path.dirname(__file__), "results", "fibonacci_honest.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
