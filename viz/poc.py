"""poc — render a diagram from a REAL Evident program's sampled interface."""
import json, subprocess, sys, os
import matplotlib; matplotlib.use("Agg"); import matplotlib.pyplot as plt

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def sample(evfile, schema, n):
    out = subprocess.check_output(
        [sys.executable, "evident.py", "sample", evfile, schema, "-n", str(n), "--json"],
        text=True, cwd=ROOT)
    return json.loads(out)

def main():
    ev = "ide/examples/circle.ev"
    fig, axes = plt.subplots(1, 2, figsize=(11, 5.4))
    for ax, schema, col in [(axes[0], "PointOnCircle", "#2868d2"),
                            (axes[1], "PointInDisk", "#2eb55f")]:
        pts = sample(ev, schema, 220)
        xs = [p["px"] for p in pts]; ys = [p["py"] for p in pts]
        ax.scatter(xs, ys, s=14, alpha=0.55, color=col, edgecolors="none")
        ax.set_aspect("equal"); ax.set_xlim(-1.4, 1.4); ax.set_ylim(-1.4, 1.4)
        ax.set_title(f"{schema}  ({len(pts)} samples)", fontsize=12, loc="left")
        ax.set_xlabel("px"); ax.set_ylabel("py")
        for s in ax.spines.values(): s.set_color("#d2d6de")
    fig.suptitle("Diagrams from a REAL Evident program (circle.ev) — sampled interface",
                 fontsize=14, weight="bold")
    fig.tight_layout(rect=(0,0,1,0.95))
    out = os.path.join(os.path.dirname(__file__), "results", "circle_from_program.png")
    fig.savefig(out, dpi=130, facecolor="white"); print("wrote", out)

if __name__ == "__main__": main()
