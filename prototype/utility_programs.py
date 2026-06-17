"""utility_programs — practical IO/effectful programs as transition models.

The folds in model_gallery.py are CLOSED systems: the answer ends up in the final
state. A real utility program is OPEN — it READS inputs and WRITES outputs as
*effects*, and the "answer" is the effect TRACE, not a fixed point. So the useful
view is the control graph + the effect/timing trace, not a phase portrait of the
(often trivial) internal state. See docs/plans/effectful-models.md.

Four panels:
  add2   "read 2 values, add them" — control graph with the read/write effects on
         the edges (the computation a+b is pure; the IO is effects).
  echo   "read a line, write it, repeat" — the read⇄write loop as a timing diagram
         of stdin/stdout events.
  LR     a toy LR(0) parser for  E → E + n | n :
           - its parsing automaton as a state-transition graph (shift/goto/reduce)
           - a parse of "n + n + n" as a stack-depth-over-step trace (the shift/
             reduce sawtooth). An LR parser is a pushdown automaton = a transition
             system with a stack.

Run from prototype/:  python3 utility_programs.py  -> results/utility_programs.png
"""
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch, Circle

INK = "#2a2c34"; MUTED = "#6b7080"; BLUE = "#2868d2"; GREEN = "#2eb55f"
PURPLE = "#8a4fbf"; ORANGE = "#c0612a"; TEAL = "#0e8a8a"; RED = "#de3c3c"
FAN = "#5a6478"


def _title(ax, t, xl="", yl=""):
    ax.set_title(t, fontsize=12, color=INK, loc="left", pad=8)
    ax.set_xlabel(xl, fontsize=9, color=MUTED); ax.set_ylabel(yl, fontsize=9, color=MUTED)
    ax.tick_params(colors=MUTED, labelsize=8)
    for s in ax.spines.values():
        s.set_color("#d2d6de")


def _node(ax, xy, label, fc="#eaf2fb", ec=BLUE, w=0.62, h=0.32):
    ax.add_patch(FancyBboxPatch((xy[0] - w / 2, xy[1] - h / 2), w, h,
                 boxstyle="round,pad=0.02", fc=fc, ec=ec, lw=1.6, zorder=4))
    ax.text(xy[0], xy[1], label, ha="center", va="center", fontsize=8.5,
            color=INK, zorder=5)


def _edge(ax, a, b, label="", color=FAN, rad=0.0, lcolor=None, dy=0.0):
    ax.annotate("", xy=b, xytext=a, zorder=2,
                arrowprops=dict(arrowstyle="-|>", color=color, lw=1.5,
                                shrinkA=20, shrinkB=20,
                                connectionstyle=f"arc3,rad={rad}"))
    if label:
        mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2 + dy
        ax.text(mx, my, label, ha="center", va="center", fontsize=7.8,
                color=lcolor or color, zorder=6,
                bbox=dict(boxstyle="round,pad=0.15", fc="white", ec="none"))


# ── add2: read two values, add them — control graph + effects ────────────────
def panel_add2(ax):
    for i, lbl in enumerate(["read a", "read b", "add+write", "done"]):
        fc = "#f0e9f8" if i in (0, 1, 2) else "#eaf6ee"
        _node(ax, (i, 0), lbl, fc=fc, ec=(PURPLE if i < 3 else GREEN))
    _edge(ax, (0, 0), (1, 0), "⚡ Read → a", PURPLE, dy=0.33, lcolor=PURPLE)
    _edge(ax, (1, 0), (2, 0), "⚡ Read → b", PURPLE, dy=0.33, lcolor=PURPLE)
    _edge(ax, (2, 0), (3, 0), "⚡ Write(a+b)", ORANGE, dy=0.33, lcolor=ORANGE)
    ax.text(2, -0.5, "the value a+b is PURE; the ⚡ reads/writes are EFFECTS\n"
            "(inputs = uninterpreted until the harness supplies them)",
            ha="center", fontsize=8.2, color=MUTED)
    ax.set_xlim(-0.7, 3.7); ax.set_ylim(-0.9, 0.7); ax.axis("off")
    _title(ax, "1 · add2 — read 2 values, add them  (control + effects)")


# ── echo: read⇄write loop as a stdin/stdout timing diagram ───────────────────
def panel_echo(ax):
    lines = ["hi", "world", "ok"]
    # event trace: Read(x) then Write(x) for each line, then Read(EOF)
    x = 0
    for s in lines:
        ax.add_patch(FancyBboxPatch((x - 0.32, 1 - 0.18), 0.64, 0.36,
                     boxstyle="round,pad=0.02", fc="#eaf2fb", ec=BLUE, lw=1.4))
        ax.text(x, 1, f'"{s}"', ha="center", va="center", fontsize=8, color=INK)
        ax.add_patch(FancyBboxPatch((x + 1 - 0.32, -0.18), 0.64, 0.36,
                     boxstyle="round,pad=0.02", fc="#fdeee0", ec=ORANGE, lw=1.4))
        ax.text(x + 1, 0, f'"{s}"', ha="center", va="center", fontsize=8, color=INK)
        ax.annotate("", xy=(x + 1, 0.22), xytext=(x, 0.78),
                    arrowprops=dict(arrowstyle="-|>", color=FAN, lw=1.2,
                                    connectionstyle="arc3,rad=-0.2"))
        x += 2
    ax.text(x - 0.2, 1, "EOF", ha="center", va="center", fontsize=8, color=RED)
    ax.axhline(1, color="#dfe3ea", lw=1, zorder=0)
    ax.axhline(0, color="#dfe3ea", lw=1, zorder=0)
    ax.text(-0.9, 1, "stdin\n(Read)", ha="right", va="center", fontsize=8, color=BLUE)
    ax.text(-0.9, 0, "stdout\n(Write)", ha="right", va="center", fontsize=8, color=ORANGE)
    ax.set_xlim(-2.2, x); ax.set_ylim(-0.7, 1.7); ax.axis("off")
    _title(ax, "2 · echo — read⇄write loop  (IO timing / effect trace)")


# ── a toy LR(0) parser for  E → E + n | n ────────────────────────────────────
ACTION = {0: {"n": ("s", 2)}, 1: {"+": ("s", 3), "$": ("acc",)},
          2: {"+": ("r", 2), "$": ("r", 2)}, 3: {"n": ("s", 4)},
          4: {"+": ("r", 1), "$": ("r", 1)}}
GOTO = {(0, "E"): 1}
POPN = {1: 3, 2: 1}                                   # rule 1: E→E+n, rule 2: E→n


def panel_lr_automaton(ax):
    pos = {0: (0, 1.0), 1: (1.4, 1.7), 2: (1.4, 0.3), 3: (2.8, 1.7), 4: (4.2, 1.7)}
    reduce = {2: "reduce E→n", 4: "reduce E→E+n"}
    for s, xy in pos.items():
        ec = GREEN if s in reduce else BLUE
        ax.add_patch(Circle(xy, 0.22, fc="#eaf2fb" if s not in reduce else "#eaf6ee",
                            ec=ec, lw=1.8, zorder=4))
        ax.text(xy[0], xy[1], f"I{s}", ha="center", va="center", fontsize=9, color=INK)
    _edge(ax, pos[0], pos[1], "E", BLUE, rad=0.0, dy=0.16)
    _edge(ax, pos[0], pos[2], "n", BLUE, rad=0.0, dy=0.16)
    _edge(ax, pos[1], pos[3], "+", BLUE, rad=0.0, dy=0.16)
    _edge(ax, pos[3], pos[4], "n", BLUE, rad=0.0, dy=0.16)
    for s, lbl in reduce.items():
        ax.text(pos[s][0], pos[s][1] - 0.4, lbl, ha="center", fontsize=7.5, color=GREEN)
    ax.text(pos[1][0], pos[1][1] + 0.4, "accept on $", ha="center", fontsize=7.5,
            color=PURPLE)
    ax.set_xlim(-0.6, 4.9); ax.set_ylim(-0.3, 2.3); ax.axis("off")
    _title(ax, "3 · LR(0) automaton for  E → E + n | n  (the parser's control)")


def lr_parse(tokens):
    states, syms, i, trace = [0], [], 0, []
    while True:
        a = ACTION[states[-1]].get(tokens[i])
        if a is None:
            trace.append(("error", len(syms))); break
        if a[0] == "s":
            states.append(a[1]); syms.append(tokens[i]); i += 1
            trace.append((f"shift {syms[-1]}", len(syms)))
        elif a[0] == "r":
            for _ in range(POPN[a[1]]):
                states.pop(); syms.pop()
            states.append(GOTO[(states[-1], "E")]); syms.append("E")
            trace.append((f"reduce r{a[1]}", len(syms)))
        else:
            trace.append(("accept", len(syms))); break
    return trace


def panel_lr_trace(ax):
    trace = lr_parse(["n", "+", "n", "+", "n", "$"])
    xs = range(len(trace)); depths = [d for _, d in trace]
    ax.step(xs, depths, where="post", color=TEAL, lw=2.0, zorder=3)
    ax.plot(xs, depths, "o", color=TEAL, ms=5, zorder=4)
    for k, (act, d) in enumerate(trace):
        col = GREEN if act.startswith("reduce") else (PURPLE if act == "accept" else FAN)
        ax.annotate(act, (k, d), (k, d + 0.35), fontsize=7, color=col, rotation=35,
                    ha="left", va="bottom")
    ax.set_xlim(-0.5, len(trace) - 0.3); ax.set_ylim(0, max(depths) + 1.6)
    _title(ax, "4 · LR parse of “n + n + n” — stack depth per step", "step",
           "stack depth")
    ax.text(0.02, 0.93, "shifts push, reduces pop — the shift/reduce sawtooth",
            transform=ax.transAxes, fontsize=8.2, color=MUTED)


def main():
    fig, axes = plt.subplots(2, 2, figsize=(15, 11))
    fig.suptitle("Utility programs as transition models — open systems with effects",
                 fontsize=17, color=INK, weight="bold", y=0.975)
    fig.text(0.5, 0.945, "not folds that funnel to an answer — these READ and WRITE; "
             "the answer is the effect trace, and an LR parser is a pushdown automaton",
             ha="center", fontsize=10, color=MUTED)
    panel_add2(axes[0, 0])
    panel_echo(axes[0, 1])
    panel_lr_automaton(axes[1, 0])
    panel_lr_trace(axes[1, 1])
    fig.tight_layout(rect=(0, 0, 1, 0.93))
    out = os.path.join(os.path.dirname(__file__), "results", "utility_programs.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
