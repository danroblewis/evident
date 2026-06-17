"""phase_portrait — render phase portraits of transition models with matplotlib.

Four panels, each a transition model in the "Delta" difference-equation style:

  A  SINK   — damped oscillator   Dx = v ; Dv = -w^2 x - 2 z w v
             flow spirals into a fixed point; the dashed energy contours are the
             nested Lyapunov trapping regions the flow descends (proof = picture).
  B  CYCLE  — Van der Pol          Dx = v ; Dv = mu(1-x^2) v - x
             flow converges to a limit cycle => livelock made visible.
  C  SAFE   — bounded 2-queue daemon (discrete, nondeterministic). Spacer PROVES
             0 <= q0,q1 <= CAP; the green box IS that proven invariant; every
             transition arrow (the fan) stays inside it.
  D  LEAK   — same daemon, capacity guard dropped. Spacer finds the overflow
             reachable; a red counterexample trajectory escapes the box.

The green boxes and the SAT/UNSAT verdicts are produced by Z3 Spacer at run time,
not drawn by hand. Run from prototype/:  python3 phase_portrait.py
"""
import os

import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle

import z3

GREEN = "#2eb55f"; GREENFILL = "#bfeccb"; RED = "#de3c3c"
INK = "#2a2c34"; MUTED = "#6b7080"


# ─────────────────────── continuous flows (panels A, B) ─────────────────────
def damped(x, v, w2=1.0, zeta=0.11, w=1.0):
    return v, -w2 * x - 2 * zeta * w * v


def vanderpol(x, v, mu=1.0):
    return v, mu * (1 - x * x) * v - x


def trajectory(f, x0, v0, dt, n):
    xs = np.empty(n + 1); vs = np.empty(n + 1)
    x, v = x0, v0
    for i in range(n + 1):
        xs[i], vs[i] = x, v
        dx, dv = f(x, v)
        x += dx * dt; v += dv * dt
    return xs, vs


def stream(ax, f, dom, color):
    x = np.linspace(dom[0], dom[1], 32)
    y = np.linspace(dom[2], dom[3], 32)
    X, Y = np.meshgrid(x, y)
    U, V = f(X, Y)
    speed = np.hypot(U, V)
    ax.streamplot(X, Y, U, V, color=color, density=1.1, linewidth=0.7,
                  arrowsize=0.8, broken_streamlines=True)
    return speed


def panel_sink(ax):
    dom = (-3, 3, -3, 3)
    stream(ax, lambda X, Y: damped(X, Y), dom, "#c3c8d6")
    # Lyapunov energy contours V = 1/2 (w^2 x^2 + v^2): the nested trapping regions
    x = np.linspace(-3, 3, 300); y = np.linspace(-3, 3, 300)
    X, Y = np.meshgrid(x, y)
    Vlyap = 0.5 * (X ** 2 + Y ** 2)
    cs = ax.contour(X, Y, Vlyap, levels=[0.35, 0.9, 1.8, 3.0],
                    colors="#8fb98f", linestyles="dashed", linewidths=1.0)
    for seed, col in [((2.7, 0), "#2868d2"), ((-2.7, 0.4), "#9646c8"),
                      ((0.4, 2.7), "#00a0a0"), ((-1.9, -2.2), "#eb9628")]:
        xs, vs = trajectory(damped, seed[0], seed[1], 0.03, 520)
        ax.plot(xs, vs, color=col, lw=2.0)
    ax.plot(0, 0, "o", color=INK, ms=9, mfc="white", mew=2.2, zorder=5)
    ax.annotate("fixed point\n(attractor)", (0, 0), (0.55, -1.15),
                color=INK, fontsize=8.5, ha="center",
                arrowprops=dict(arrowstyle="->", color=INK, lw=0.9))
    ax.annotate("Lyapunov energy\n= trapping regions", (1.55, 1.55), (1.0, 2.45),
                color="#5f865f", fontsize=8.5, ha="center")
    _frame(ax, dom, "A.  SINK — flow descends a Lyapunov function", "x", "v")


def panel_cycle(ax):
    dom = (-3.2, 3.2, -3.8, 3.8)
    stream(ax, lambda X, Y: vanderpol(X, Y), dom, "#c3c8d6")
    xs, vs = trajectory(vanderpol, 0.05, 0.05, 0.02, 1700)   # inside -> out to cycle
    ax.plot(xs[300:], vs[300:], color="#00a0a0", lw=2.0, label="from inside")
    xs, vs = trajectory(vanderpol, 3.0, 0.0, 0.02, 1700)     # outside -> in to cycle
    ax.plot(xs[200:], vs[200:], color="#eb9628", lw=2.0, label="from outside")
    ax.plot(0, 0, "o", color=RED, ms=8, mfc="white", mew=2.0, zorder=5)
    ax.annotate("limit cycle\n= livelock", (0, 2.65), (0, 3.15),
                color=INK, fontsize=8.5, ha="center")
    ax.legend(loc="lower right", fontsize=8, framealpha=0.9)
    _frame(ax, dom, "B.  LIMIT CYCLE — flow traps into a loop", "x", "v")


# ─────────────────────── discrete daemon (panels C, D) ──────────────────────
def queue_moves(q0, q1, CAP, guarded):
    mv = []
    if (q0 < CAP) if guarded else True:
        mv.append((q0 + 1, q1))            # arrive
    if q0 > 0 and q1 < CAP:
        mv.append((q0 - 1, q1 + 1))        # transfer q0 -> q1
    if q1 > 0:
        mv.append((q0, q1 - 1))            # depart
    return mv


def prove_queue(CAP, guarded):
    """Spacer: prove 0<=q0,q1<=CAP, or find the overflow when the guard is gone."""
    fp = z3.Fixedpoint(); fp.set(engine="spacer")
    Inv = z3.Function("Inv", z3.IntSort(), z3.IntSort(), z3.BoolSort())
    fp.register_relation(Inv)
    q0, q1, a0, a1 = z3.Ints("q0 q1 a0 a1")
    for v in (q0, q1, a0, a1):
        fp.declare_var(v)
    arrive_ok = (q0 < CAP) if guarded else True
    trans = z3.Or(
        z3.And(arrive_ok, a0 == q0 + 1, a1 == q1),
        z3.And(q0 > 0, q1 < CAP, a0 == q0 - 1, a1 == q1 + 1),
        z3.And(q1 > 0, a0 == q0, a1 == q1 - 1),
        z3.And(a0 == q0, a1 == q1),
    )
    fp.rule(Inv(z3.IntVal(0), z3.IntVal(0)))
    fp.rule(Inv(a0, a1), [Inv(q0, q1), trans])
    return fp.query(z3.And(Inv(q0, q1), z3.Or(q0 < 0, q0 > CAP, q1 < 0, q1 > CAP)))


def panel_daemon(ax, CAP, guarded, verdict):
    # the Spacer-proven trapping region [0,CAP]^2
    ax.add_patch(Rectangle((0, 0), CAP, CAP, facecolor=GREENFILL,
                           edgecolor=GREEN, lw=2.4, zorder=0))
    # transition fan: one arrow per enabled move at each lattice point
    xs, ys, us, vs, cols = [], [], [], [], []
    for q0 in range(CAP + 1):
        for q1 in range(CAP + 1):
            for (n0, n1) in queue_moves(q0, q1, CAP, guarded):
                xs.append(q0); ys.append(q1)
                us.append(0.6 * (n0 - q0)); vs.append(0.6 * (n1 - q1))
                escapes = not (0 <= n0 <= CAP and 0 <= n1 <= CAP)
                cols.append(RED if escapes else "#5a6478")
    ax.quiver(xs, ys, us, vs, color=cols, angles="xy", scale_units="xy",
              scale=1, width=0.005, headwidth=4, headlength=5, zorder=2)
    ax.plot(range(CAP + 1), [0] * (CAP + 1), "o", color="#5a6478", ms=2.5, zorder=1)
    for q0 in range(CAP + 1):                       # lattice dots
        ax.plot([q0] * (CAP + 1), range(CAP + 1), "o", color="#5a6478", ms=2.5, zorder=1)

    if not guarded:                                 # red counterexample trajectory
        path = [(k, 0) for k in range(CAP + 2)]
        px, py = zip(*path)
        ax.plot(px, py, "-", color=RED, lw=2.6, zorder=4)
        ax.plot(px[-1], py[-1], "o", color=RED, ms=8, zorder=5)
        ax.annotate("counterexample\nq0 -> CAP+1", (CAP + 1, 0), (CAP - 1.4, 1.9),
                    color=RED, fontsize=8.5, ha="center",
                    arrowprops=dict(arrowstyle="->", color=RED, lw=1.0))

    ok = verdict == z3.unsat
    tag = "Spacer: UNSAT — PROVED SAFE" if ok else "Spacer: SAT — COUNTEREXAMPLE"
    title = ("C.  SAFE DAEMON — flow trapped in the proven box"
             if ok else "D.  LEAK — guard dropped, flow escapes the box")
    ax.text(0.5, -0.155, tag, transform=ax.transAxes, ha="center",
            fontsize=10, color=GREEN if ok else RED, weight="bold")
    _frame(ax, (-0.7, CAP + 1.7, -0.7, CAP + 1.7), title, "q0", "q1")
    ax.set_aspect("equal")


# ─────────────────────── shared framing ─────────────────────────────────────
def _frame(ax, dom, title, xl, yl):
    ax.set_xlim(dom[0], dom[1]); ax.set_ylim(dom[2], dom[3])
    ax.set_title(title, fontsize=11.5, color=INK, loc="left", pad=8)
    ax.set_xlabel(xl, fontsize=9, color=MUTED)
    ax.set_ylabel(yl, fontsize=9, color=MUTED, rotation=0, labelpad=10)
    ax.tick_params(colors=MUTED, labelsize=7.5)
    for s in ax.spines.values():
        s.set_color("#d2d6de")
    ax.axhline(0, color="#aeb4c2", lw=0.6, zorder=0)
    ax.axvline(0, color="#aeb4c2", lw=0.6, zorder=0)


def main():
    CAP = 6
    safe = prove_queue(CAP, guarded=True)
    leak = prove_queue(CAP, guarded=False)
    print("Spacer  safe(guarded):", safe, " leak(unguarded):", leak, flush=True)

    fig, axes = plt.subplots(2, 2, figsize=(13, 13))
    fig.suptitle("Phase portraits of a transition model",
                 fontsize=19, color=INK, weight="bold", x=0.5, y=0.975)
    fig.text(0.5, 0.943,
             "flow over state space  ·  fixed points  ·  limit cycles  ·  "
             "Spacer-proven trapping regions  ·  counterexample",
             ha="center", fontsize=10.5, color=MUTED)
    panel_sink(axes[0, 0])
    panel_cycle(axes[0, 1])
    panel_daemon(axes[1, 0], CAP, True, safe)
    panel_daemon(axes[1, 1], CAP, False, leak)
    fig.tight_layout(rect=(0, 0, 1, 0.93))

    out = os.path.join(os.path.dirname(__file__), "results", "phase_portrait.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
