"""diagram_zoo — demo four more state-space diagram types beyond the phase
portrait (see docs/plans/state-space-diagrams.md):

  1. State-transition graph  — the full reachable-state-vector graph (the "swap"
     view): nodes = whole state vectors, edges = transitions. Finite/low-card only.
  2. Timing diagram          — EE digital waveforms: a boolean signal (heater) and
     an analog one (temp) on a shared time axis.
  3. Basin of attraction     — colour each start state by which attractor it reaches.
  4. Bifurcation diagram     — sweep a parameter, plot the long-run state(s).

Run from prototype/:  python3 diagram_zoo.py  -> results/diagram_zoo.png
"""
import os
from collections import deque

import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Circle
from matplotlib.colors import ListedColormap

import z3
import phaseportrait as pp

INK = "#2a2c34"; MUTED = "#6b7080"; GREEN = "#2eb55f"
BLUE = "#2868d2"; ORANGE = "#eb9628"; PURPLE = "#8a4fbf"; TEAL = "#00a0a0"
FAN = "#5a6478"


def _frame(ax, title, xl="", yl=""):
    ax.set_title(title, fontsize=12, color=INK, loc="left", pad=8)
    ax.set_xlabel(xl, fontsize=9, color=MUTED)
    ax.set_ylabel(yl, fontsize=9, color=MUTED)
    ax.tick_params(colors=MUTED, labelsize=8)
    for s in ax.spines.values():
        s.set_color("#d2d6de")


# ── 1. state-transition graph (the swap view) ───────────────────────────────
def panel_state_graph(ax, CAP=2):
    def step(c, n):
        q0, q1, m0, m1 = c["q0"], c["q1"], n["q0"], n["q1"]
        return z3.Or(z3.And(q0 < CAP, m0 == q0 + 1, m1 == q1),
                     z3.And(q0 > 0, q1 < CAP, m0 == q0 - 1, m1 == q1 + 1),
                     z3.And(q1 > 0, m0 == q0, m1 == q1 - 1),
                     z3.And(m0 == q0, m1 == q1))
    M = pp.Model("queue", ["q0", "q1"], step, "Int")

    seen, edges, dq = {(0, 0)}, set(), deque([(0, 0)])
    while dq:                                          # BFS the reachable states
        s = dq.popleft()
        for succ in pp.successors(M, {"q0": s[0], "q1": s[1]}, 6):
            t = (succ["q0"], succ["q1"])
            if t != s:
                edges.add((s, t))
            if t not in seen:
                seen.add(t); dq.append(t)
    for (s, t) in edges:                               # directed edges (curved)
        ax.annotate("", xy=t, xytext=s, zorder=2,
                    arrowprops=dict(arrowstyle="-|>", color=FAN, lw=1.3,
                                    shrinkA=12, shrinkB=12,
                                    connectionstyle="arc3,rad=0.13"))
    for (x, y) in seen:                                # nodes = state vectors
        ax.add_patch(Circle((x, y), 0.17, fc="#eaf2fb", ec=BLUE, lw=1.6, zorder=5))
        ax.text(x, y, f"{x},{y}", ha="center", va="center", fontsize=8,
                color=INK, zorder=6)
    ax.set_xlim(-0.6, CAP + 0.6); ax.set_ylim(-0.6, CAP + 0.6)
    ax.set_aspect("equal")
    _frame(ax, f"1 · State-transition graph (swap view) — queue, CAP={CAP}",
           "q0", "q1")
    ax.text(0.5, -0.16, f"{len(seen)} reachable state vectors, "
            f"{len(edges)} transitions — every state drawn (finite only)",
            transform=ax.transAxes, ha="center", fontsize=8.5, color=MUTED)


# ── 2. timing diagram (EE waveforms) ─────────────────────────────────────────
def panel_timing(ax, LO=18, HI=22, GAIN=1.4, LOSS=0.6, T=46):
    temp, on = 19.0, False
    temps, ons = [], []
    for _ in range(T):
        if temp <= LO:
            on = True
        elif temp >= HI:
            on = False
        temps.append(temp); ons.append(1 if on else 0)
        temp += GAIN * (1 if on else 0) - LOSS
    ts = range(T)
    ax.axhspan(LO, HI, color="#eaf6ee", zorder=0)
    ax.axhline(LO, color=GREEN, ls=":", lw=1.0); ax.axhline(HI, color=GREEN, ls=":", lw=1.0)
    ax.plot(ts, temps, color=TEAL, lw=1.8, label="temp (analog)")
    base, amp = HI + 1.2, 1.6                          # heater digital track up top
    ax.step(ts, [base + amp * o for o in ons], where="post", color=PURPLE,
            lw=1.8, label="heater (on/off)")
    ax.text(0, base + amp + 0.25, "heater", color=PURPLE, fontsize=8)
    ax.text(0, base - 0.55, "off", color=MUTED, fontsize=7)
    ax.text(0, base + amp + 0.05, "on", color=MUTED, fontsize=7)
    ax.set_xlim(0, T - 1); ax.set_ylim(LO - 3, HI + 4)
    _frame(ax, "2 · Timing diagram — thermostat (digital + analog)", "tick", "")
    ax.legend(loc="lower right", fontsize=7.5, framealpha=0.9)


# ── 3. basin of attraction (double-well) ─────────────────────────────────────
def panel_basin(ax, Nb=160, steps=320, dt=0.03):
    xs = np.linspace(-2, 2, Nb); ys = np.linspace(-2, 2, Nb)
    X, Y = np.meshgrid(xs, ys)
    for _ in range(steps):                             # integrate the whole grid
        dx, dy = Y, X - X ** 3 - 0.3 * Y
        X, Y = X + dx * dt, Y + dy * dt
    img = (X > 0).astype(int)                          # which well did it fall into?
    ax.imshow(img, extent=[-2, 2, -2, 2], origin="lower", aspect="equal",
              cmap=ListedColormap(["#cfe0f5", "#fbe3cf"]), alpha=0.95)
    ax.plot([-1, 1], [0, 0], "o", color=INK, ms=7, mfc="white", mew=1.8)
    ax.plot(0, 0, "x", color=RED2, ms=8, mew=2)
    ax.text(-1, 0.18, "attractor", color=BLUE, fontsize=7.5, ha="center")
    ax.text(1, 0.18, "attractor", color=ORANGE, fontsize=7.5, ha="center")
    ax.text(0, -0.28, "saddle", color=RED2, fontsize=7.5, ha="center")
    _frame(ax, "3 · Basin of attraction — double-well", "x", "y")


# ── 4. bifurcation diagram (logistic map) ────────────────────────────────────
def panel_bifurcation(ax):
    rs = np.linspace(2.5, 4.0, 1400)
    x = np.full_like(rs, 0.5)
    R, P = [], []
    for i in range(360):
        x = rs * x * (1 - x)
        if i >= 240:
            R.append(rs.copy()); P.append(x.copy())
    ax.scatter(np.concatenate(R), np.concatenate(P), s=0.06, color=INK, alpha=0.5,
               linewidths=0)
    ax.set_xlim(2.5, 4.0); ax.set_ylim(0, 1)
    _frame(ax, "4 · Bifurcation — logistic map (parameter sweep)", "parameter r",
           "long-run x")
    ax.text(0.02, 0.06, "fixed point → 2-cycle → 4-cycle → chaos", color=MUTED,
            transform=ax.transAxes, fontsize=8.5)


RED2 = "#de3c3c"


def main():
    fig, axes = plt.subplots(2, 2, figsize=(14, 12.5))
    fig.suptitle("Beyond the phase portrait — four more state-space diagrams",
                 fontsize=17, color=INK, weight="bold", y=0.975)
    fig.text(0.5, 0.945, "each answers a different question: discrete structure · "
             "behaviour over time · which attractor · behaviour vs a parameter",
             ha="center", fontsize=10, color=MUTED)
    panel_state_graph(axes[0, 0])
    panel_timing(axes[0, 1])
    panel_basin(axes[1, 0])
    panel_bifurcation(axes[1, 1])
    fig.tight_layout(rect=(0, 0, 1, 0.93))
    out = os.path.join(os.path.dirname(__file__), "results", "diagram_zoo.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
