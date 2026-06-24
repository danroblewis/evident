"""render_reachable_region.py — the ABSTRACT reachable region: where an FSM can ever be.

The dynamics-side counterpart to render_terminal_map. Instead of enumerating reachable states it
PLOTS a bounding box PROVEN to contain the reachable set by k-induction (reachable_region.py) — a
sound over-approximation, computed from the one-step relation, that decides BOUNDED vs UNBOUNDED
even on infinite state spaces (a free random walk) where enumeration can't run.

  * BOUNDED   — the reachable set sits inside the drawn box (solid = proven 1-inductive; hatched =
                per-var one-step range, not proven closed).
  * UNBOUNDED — at least one variable grows without bound; shown as the init state with outward
                arrows, not a misleading finite box.

Entry: render(model, out_path).
"""
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt                     # noqa: E402
from matplotlib.patches import Rectangle            # noqa: E402

from reachable_region import bounding_box           # noqa: E402

_GREEN = "#2e7d32"
_AMBER = "#b8860b"
_GREY = "#777777"
_BLUE = "#1565c0"


def _name(model):
    return getattr(model, "fsm", None) or "model"


def _short(v):
    return v["name"].split(".")[-1]


def render(model, out_path):
    r = bounding_box(model)
    verdict, box, unbounded, inductive = r["verdict"], r["box"], r["unbounded"], r["inductive"]
    numeric = [v for v in model.carried if v["kind"] in ("int", "real")]
    init = model.initial_state() or {}
    fig, ax = plt.subplots(figsize=(8.2, 5.6))

    if verdict == "bounded" and numeric:
        face = _GREEN if inductive else _AMBER
        hatch = None if inductive else "//"
        if len(numeric) >= 2:
            vx, vy = numeric[0], numeric[1]
            lox, hix = box[vx["name"]]; loy, hiy = box[vy["name"]]
            ax.add_patch(Rectangle((lox, loy), hix - lox or 0.001, hiy - loy or 0.001,
                                   facecolor=face, alpha=0.22, edgecolor=face, lw=2, hatch=hatch,
                                   zorder=2, label="proven reachable region"))
            ax.set_xlim(lox - 1, hix + 1); ax.set_ylim(loy - 1, hiy + 1)
            if vx["name"] in init and vy["name"] in init:
                ax.scatter([init[vx["name"]]], [init[vy["name"]]], marker="*", s=260,
                           color=_BLUE, edgecolor="black", zorder=4, label="initial state")
            ax.set_xlabel(_short(vx)); ax.set_ylabel(_short(vy))
            ax.grid(True, alpha=0.25); ax.legend(loc="upper right", fontsize=8)
        else:
            vx = numeric[0]; lo, hi = box[vx["name"]]
            ax.barh([0], [hi - lo or 0.001], left=lo, height=0.3, color=face, alpha=0.3,
                    edgecolor=face, lw=2, hatch=hatch, zorder=2)
            ax.annotate(f"[{lo}, {hi}]", ((lo + hi) / 2, 0.22), ha="center", fontsize=11,
                        fontweight="bold", color=face)
            if vx["name"] in init:
                ax.scatter([init[vx["name"]]], [0], marker="*", s=260, color=_BLUE,
                           edgecolor="black", zorder=4, label="initial state")
                ax.legend(loc="upper right", fontsize=8)
            ax.set_xlim(lo - 1, hi + 1); ax.set_ylim(-1, 1); ax.set_yticks([])
            ax.set_xlabel(_short(vx))
        for sp in ("top", "right"):
            ax.spines[sp].set_visible(False)
    elif verdict == "unbounded":
        # the reachable set grows without bound — show the init + outward arrows, not a fake box
        ix = init.get(numeric[0]["name"], 0) if numeric else 0
        iy = init.get(numeric[1]["name"], 0) if len(numeric) >= 2 else 0
        ax.scatter([ix], [iy], marker="*", s=300, color=_BLUE, edgecolor="black", zorder=4)
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ax.annotate("", xy=(ix + dx * 3, iy + dy * 3), xytext=(ix + dx * 0.6, iy + dy * 0.6),
                        arrowprops=dict(arrowstyle="-|>", color=_AMBER, lw=2))
        ax.set_xlim(ix - 4, ix + 4); ax.set_ylim(iy - 4, iy + 4)
        ax.text(0.5, 0.06, "grows without bound from the initial state ★",
                ha="center", transform=ax.transAxes, fontsize=10, color=_AMBER)
        if numeric:
            ax.set_xlabel(_short(numeric[0]))
        if len(numeric) >= 2:
            ax.set_ylabel(_short(numeric[1]))
        else:
            ax.set_yticks([])
        for sp in ("top", "right"):
            ax.spines[sp].set_visible(False)
    else:
        ax.text(0.5, 0.55, "?", ha="center", va="center", fontsize=72, color=_GREY,
                transform=ax.transAxes)
        ax.text(0.5, 0.32, r.get("note") or "no numeric state to bound", ha="center",
                va="center", fontsize=12, color=_GREY, transform=ax.transAxes)
        ax.set_xticks([]); ax.set_yticks([])
        for sp in ax.spines.values():
            sp.set_visible(False)

    ub = ", ".join(n.split(".")[-1] for n in unbounded)
    banners = {
        "bounded": ((f"BOUNDED — reachable set ⊆ the box, PROVEN 1-inductive (k=1) over the "
                     "one-step relation · sound; no enumeration" if inductive else
                     "BOUNDED by the per-var one-step range — shown as an over-approximation "
                     "(not proven closed under the transition; k>1 would tighten it)"),
                    _GREEN if inductive else _AMBER),
        "unbounded": (f"UNBOUNDED in {ub} — the reachable set has no finite bound; decided "
                      "abstractly (enumeration can't even run here)", _AMBER),
        "unknown": (r.get("note") or "no numeric carried state to bound", _GREY),
    }
    msg, col = banners[verdict]
    ax.set_title(f"{_name(model)} — reachable region  ·  {verdict.upper()}",
                 fontsize=13, fontweight="bold")
    fig.text(0.5, 0.02, msg, ha="center", va="bottom", fontsize=8.5, color=col, wrap=True)
    fig.tight_layout(rect=[0, 0.07, 1, 1])
    fig.savefig(out_path, dpi=120)
    plt.close(fig)
