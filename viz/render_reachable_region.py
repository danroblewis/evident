"""render_reachable_region.py — the ABSTRACT reachable region: where an FSM can ever be.

The dynamics-side counterpart to render_terminal_map. Instead of enumerating the reachable set it
PLOTS a bounding box PROVEN to contain it by k-induction (reachable_region.py) — a sound
over-approximation, computed from the one-step relation, that decides BOUNDED vs UNBOUNDED even on
infinite state spaces (a free random walk) where full_state_graph() can't enumerate.

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
from render_common import (                          # noqa: E402
    GREEN as _GREEN, AMBER as _AMBER, GREY as _GREY, BLUE as _BLUE,
    short, model_name as _name, empty_panel, verdict_banner,
)


def _short(v):
    """Short name of a carried-var DICT (this renderer threads var dicts, not names)."""
    return short(v["name"])


def _draw_bounded(ax, numeric, box, inductive, init):
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


def _draw_unbounded(ax, numeric, init):
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
    ax.set_ylabel(_short(numeric[1])) if len(numeric) >= 2 else ax.set_yticks([])
    for sp in ("top", "right"):
        ax.spines[sp].set_visible(False)


def _draw_unknown(ax, note):
    label = note or "no numeric state to bound"
    if len(label) > 48:                                # the full reason rides in the banner below
        label = "bound undetermined"
    empty_panel(ax, "?", label, _GREY)


def _banner(verdict, unbounded, inductive, note):
    ub = ", ".join(n.split(".")[-1] for n in unbounded)
    if verdict == "bounded":
        return ((("BOUNDED — reachable set ⊆ the box, PROVEN 1-inductive (k=1) over the one-step "
                  "relation · sound; no enumeration") if inductive else
                 "BOUNDED by the per-var one-step range — an over-approximation (not proven closed "
                 "under the transition; k>1 would tighten it)"), _GREEN if inductive else _AMBER)
    if verdict == "unbounded":
        return (f"UNBOUNDED in {ub} — the reachable set has no finite bound; decided abstractly "
                "(enumeration can't even run here)", _AMBER)
    return (note or "no numeric carried state to bound", _GREY)


def render(model, out_path):
    r = bounding_box(model)
    verdict, box, unbounded, inductive = r["verdict"], r["box"], r["unbounded"], r["inductive"]
    numeric = [v for v in model.carried if v["kind"] in ("int", "real")]
    init = model.initial_state() or {}
    fig, ax = plt.subplots(figsize=(8.2, 5.6))
    if verdict == "bounded" and numeric:
        _draw_bounded(ax, numeric, box, inductive, init)
    elif verdict == "unbounded":
        _draw_unbounded(ax, numeric, init)
    else:
        _draw_unknown(ax, r.get("note"))
    msg, col = _banner(verdict, unbounded, inductive, r.get("note"))
    verdict_banner(fig, ax, out_path,
                   f"{_name(model)} — reachable region  ·  {verdict.upper()}", msg, col)
