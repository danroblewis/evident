"""fsm_trajectories — trajectory-based phase portraits for Seq-state FSMs.

Point-sampling can't pin Seq state, so instead we RUN the program forward (fsm_trace)
and plot the trajectories its own state takes — the honest "draw the states it visits"
view. Output: viz/fsm/<name>.png
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle
from fsm_trace import trace

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "viz", "fsm")
PAL = ["#2868d2", "#2eb55f", "#eb9628", "#8a4fbf", "#de3c3c", "#0e8a8a"]
INK, MUTED = "#2a2c34", "#6b7080"


def _arrows(ax, xs, ys, color, every=6):
    for i in range(0, len(xs) - 1, every):
        if (xs[i], ys[i]) != (xs[i + 1], ys[i + 1]):
            ax.annotate("", xy=(xs[i + 1], ys[i + 1]), xytext=(xs[i], ys[i]),
                        arrowprops=dict(arrowstyle="-|>", color=color, lw=1.3, alpha=0.9))


def balls(steps=180, n=4):
    def cap(b):
        return [(b[f"state.balls.{i}"]["pos_y"], b[f"state.balls.{i}"]["vy"]) for i in range(n)]
    rows = trace("programs/balls_demo/balls.ev", steps=steps,
                 input_given={"input.dt": 16}, capture=cap)
    fig, ax = plt.subplots(figsize=(8.2, 7))
    ax.add_patch(Rectangle((20, -800), 520, 1600, fc="#e6f3ea", ec="#2eb55f",
                           lw=1.6, zorder=0))
    for bi in range(n):
        xs = [rows[s][bi][0] for s in range(len(rows))]
        ys = [rows[s][bi][1] for s in range(len(rows))]
        ax.plot(xs, ys, color=PAL[bi], lw=1.4, alpha=0.85, zorder=2, label=f"ball {bi}")
        _arrows(ax, xs, ys, PAL[bi])
        ax.scatter([xs[0]], [ys[0]], color=PAL[bi], s=40, zorder=3, edgecolors="white")
    ax.set_xlabel("pos_y  (height, px — floor at 540)", color=MUTED)
    ax.set_ylabel("vy  (vertical velocity, px/s)", color=MUTED)
    ax.axhline(0, color="#c9cdd6", lw=0.8, zorder=1)
    ax.set_title("balls.ev — bouncing-ball phase portrait (pos_y, vy)\n"
                 f"{len(rows)} steps traced through the runtime: fall → bounce (×−0.7) → climb",
                 fontsize=12, color=INK, loc="left")
    ax.legend(fontsize=8, loc="upper right")
    path = os.path.join(OUT, "balls__phase.png")
    os.makedirs(OUT, exist_ok=True)
    fig.savefig(path, dpi=130, facecolor="white"); plt.close(fig)
    return path


if __name__ == "__main__":
    print(balls())
