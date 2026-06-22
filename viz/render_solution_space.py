"""solution_space — the SOLVED boundary of a program's variables, NOT a single run.

The trajectory views (time_series, phase_portrait) draw ONE orbit through state space.
This view draws the BOUNDARY of what is possible:
  * left  — each numeric variable's exact range over the whole reachable set, as a bar
            ("the abstract boundary of the variable"). Exact when the reachable set is
            finite and fully explored (an exhaustive solve); a lower bound when capped.
  * right — the feasible REGION of the two principal variables as a SET of points inside
            their bounding box, with fixed points / equilibria marked. The set, not a path.
"""
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle
from evident_viz import load

_SHORT = lambda n: n.split(".")[-1]


def _full(m, short_name):
    for v in m.carried:
        if _SHORT(v["name"]) == short_name:
            return v["name"]
    return short_name


def _na(out_path, title, msg):
    fig, ax = plt.subplots(figsize=(9, 6))
    ax.text(0.5, 0.5, msg, ha="center", va="center", transform=ax.transAxes, fontsize=13)
    ax.set_xticks([]); ax.set_yticks([])
    ax.set_title(title, fontsize=13)
    fig.tight_layout(); fig.savefig(out_path, dpi=120); plt.close(fig)
    return out_path


def render(smt2_path, schema_path, out_path):
    m = load(smt2_path, schema_path)
    states, edges = m.reachable(limit=400)
    struct = m.solution_structure(states=states, edges=edges)
    bounds = struct.get("bounds", {})            # {short: [lo, hi]} over the reachable set
    fps = struct.get("fixed_points", [])
    capped = struct.get("capped", False)
    verdict = struct.get("verdict", "")

    numeric = [_SHORT(v["name"]) for v in m.carried if v.get("kind") in ("int", "real")]
    numeric = [n for n in numeric if n in bounds]
    if not numeric:
        return _na(out_path, f"{m.fsm} — solution space",
                   "solution space needs a numeric variable\n(this program's state is categorical —\nsee state_graph for its boundary)")

    n = struct.get("reachable", len(states))
    # Honesty (Ana #112): the bounds are min/max over the reachable BFS. When the set is finite
    # and fully explored (not capped), that IS exact — every reachable state was seen. When capped,
    # it's a SAMPLE, not a proven bound — don't claim "solved", and don't assert a direction.
    boundtag = (f"sampled over {n} reachable states — not exhaustive (true range may differ)"
                if capped else f"exact — all {n} reachable states (exhaustively explored)")
    have2d = len(numeric) >= 2
    fig, axes = plt.subplots(1, 2 if have2d else 1,
                             figsize=(14 if have2d else 8.5, 6.5))
    axL = axes[0] if have2d else axes

    # --- left: each variable's solved boundary as a horizontal range bar ---
    ys = list(range(len(numeric)))
    for y, nm in zip(ys, numeric):
        lo, hi = bounds[nm]
        axL.plot([lo, hi], [y, y], lw=9, solid_capstyle="round", color="#58a6ff", alpha=0.5)
        axL.plot([lo, hi], [y, y], "|", color="#0f1419", markersize=16, markeredgewidth=2)
        axL.text(lo, y + 0.2, f"{lo:g}", ha="left", va="bottom", fontsize=9, color="#7d8590")
        axL.text(hi, y + 0.2, f"{hi:g}", ha="right", va="bottom", fontsize=9, color="#7d8590")
    axL.set_yticks(ys); axL.set_yticklabels(numeric)
    axL.set_ylim(-0.7, len(numeric) - 0.3)
    axL.set_xlabel("value spanned over the whole solution space")
    axL.set_title(f"variable boundaries — {boundtag}", fontsize=11)
    axL.grid(axis="x", alpha=0.2)

    # --- right: feasible region of the top-2 vars as a SET (no trajectory) + boundary box ---
    if have2d:
        axR = axes[1]
        vx, vy = numeric[0], numeric[1]
        fx, fy = _full(m, vx), _full(m, vy)
        pts = [(s.get(fx), s.get(fy)) for s in states]
        pts = [(x, y) for x, y in pts if isinstance(x, (int, float)) and isinstance(y, (int, float))]
        if pts:
            px, py = zip(*pts)
            axR.scatter(px, py, s=24, color="#58a6ff", alpha=0.5, edgecolors="none",
                        label="reachable set")
        (xlo, xhi), (ylo, yhi) = bounds[vx], bounds[vy]
        axR.add_patch(Rectangle((xlo, ylo), (xhi - xlo) or 1, (yhi - ylo) or 1, fill=False,
                                edgecolor="#7ee0c0", lw=1.6, ls="--", label="bounding box"))
        for f in fps:
            if vx in f and vy in f:
                axR.scatter([f[vx]], [f[vy]], marker="*", s=280, color="#c9a8ff",
                            edgecolors="#0f1419", zorder=5, label="fixed point")
        handles, labels = axR.get_legend_handles_labels()
        uniq = dict(zip(labels, handles))
        if uniq:
            axR.legend(uniq.values(), uniq.keys(), loc="best", fontsize=9)
        axR.set_xlabel(vx); axR.set_ylabel(vy)
        axR.set_title(f"reachable set ({vx}, {vy}) — every reachable combination + its extent",
                      fontsize=11)
        axR.grid(alpha=0.2)

    framing = "boundary exhaustively solved" if not capped else "boundary sampled (capped — not exhaustive)"
    fig.suptitle(f"{m.fsm} — solution space · {verdict} · {framing}", fontsize=13)
    fig.tight_layout(rect=[0, 0, 1, 0.96])
    fig.savefig(out_path, dpi=120); plt.close(fig)
    return out_path


def main(argv):
    if len(argv) < 4:
        print("usage: render_solution_space.py <smt2> <schema> <out.png>")
        return 2
    render(argv[1], argv[2], argv[3])
    return 0


if __name__ == "__main__":
    import sys
    raise SystemExit(main(sys.argv))
