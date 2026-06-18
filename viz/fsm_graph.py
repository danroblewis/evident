"""fsm_graph — render a program's FSM as a state-transition diagram.

For a state machine whose transition relation is a (state, input, next-state) set,
draw it: nodes = states, directed arcs = transitions labelled by the input. Here we
read the adventure's declared transition relation `direction_exits` (room × direction
-> destination) — the literal state machine — and draw the map graph.

(This is the static-transition view. The executor-driven *trajectory* view — run the
FSM and trace the states it visits — is the next step.)

Run from the repo root:  python3 viz/fsm_graph.py
"""
import os
import re
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Circle, FancyArrowPatch

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INK = "#2a2c34"; MUTED = "#6b7080"; BLUE = "#2868d2"; EDGE = "#5a6478"
DIRCOL = {"north": "#2868d2", "south": "#2868d2", "east": "#1f9e6d",
          "west": "#1f9e6d", "up": "#c0612a", "down": "#c0612a"}


def parse_exits(world_src):
    m = re.search(r"direction_exits\s*=\s*\{(.*?)\}", world_src, re.S)
    tuples = re.findall(r'\("([^"]*)",\s*"([^"]*)",\s*"([^"]*)"\)', m.group(1))
    return [(r, d, dest) for r, d, dest in tuples if dest]   # drop blocked ("")


def main():
    src = open(os.path.join(ROOT, "programs/adventure2/world.ev")).read()
    edges = parse_exits(src)
    pos = {"entrance": (0, 0), "forest": (0, 1.5), "tower": (1.6, 1.5),
           "cave": (1.6, 0), "dungeon": (1.6, -1.5)}
    rooms = sorted(set(pos) | {r for r, _, _ in edges} | {d for *_, d in edges})

    fig, ax = plt.subplots(figsize=(8.5, 8))
    # each directed edge curves to the LEFT of its travel direction, so A->B and
    # B->A bulge to opposite sides and their labels never collide.
    for r, d, dest in edges:
        if r not in pos or dest not in pos:
            continue
        a, b = pos[r], pos[dest]
        rad = 0.22
        ax.add_patch(FancyArrowPatch(a, b, connectionstyle=f"arc3,rad={rad}",
                     arrowstyle="-|>", mutation_scale=20, lw=2.0,
                     color=DIRCOL.get(d, EDGE), shrinkA=24, shrinkB=24, zorder=2))
        mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = (dx * dx + dy * dy) ** 0.5 or 1
        nx, ny = -dy / L, dx / L                     # left normal of travel
        off = rad * L * 0.5 + 0.14
        ax.text(mx + nx * off, my + ny * off, d, fontsize=8.5,
                color=DIRCOL.get(d, EDGE), ha="center", va="center",
                bbox=dict(boxstyle="round,pad=0.12", fc="white", ec="none"), zorder=3)
    for room, (x, y) in pos.items():
        ax.add_patch(Circle((x, y), 0.3, fc="#eaf2fb", ec=BLUE, lw=2, zorder=4))
        ax.text(x, y, room, ha="center", va="center", fontsize=9.5, color=INK, zorder=5)

    ax.set_xlim(-0.9, 2.6); ax.set_ylim(-2.2, 2.2); ax.set_aspect("equal"); ax.axis("off")
    ax.set_title("adventure2 — the location state machine (transition relation "
                 "`direction_exits`)", fontsize=13, color=INK, weight="bold")
    ax.text(0.5, -0.04, f"{len(rooms)} states (rooms), "
            f"{len([e for e in edges if e[0] in pos and e[2] in pos])} transitions, "
            "labelled by the move that triggers them",
            transform=ax.transAxes, ha="center", fontsize=9, color=MUTED)
    out = os.path.join(ROOT, "viz", "results", "adventure_fsm.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white"); print("wrote", out)


if __name__ == "__main__":
    main()
