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

from evident_viz import load                          # noqa: E402
from reachable_region import bounding_box, k_induction_box  # noqa: E402
from render_common import (                          # noqa: E402
    GREEN as _GREEN, AMBER as _AMBER, GREY as _GREY, BLUE as _BLUE,
    short, model_name as _name, empty_panel, verdict_banner,
)
from axis_select import resolve_axes, write_axes      # noqa: E402
import region_data                                    # noqa: E402


def _axes(model, x_var, y_var):
    """The ordered numeric carried vars to plot, honoring an explicit x_var/y_var request
    (#445) and falling back to the rank order. Returns (numeric_ordered, info) where info is the
    axes.json echo. Default is the existing rank order, so the auto-pick path is unchanged."""
    numeric = [v for v in model.carried if v["kind"] in ("int", "real")]
    if not numeric:
        return numeric, {"x": None, "y": None, "requested": {"x": x_var, "y": y_var},
                         "fell_back": bool(x_var or y_var)}
    dflt_x = numeric[0]
    dflt_y = numeric[1] if len(numeric) >= 2 else None
    x, y, info = resolve_axes(model, x_var, y_var, dflt_x, dflt_y, candidates=numeric)
    ordered = [x] + ([y] if y is not None else [])
    # keep any remaining numeric vars after the two plotted axes (1-D box list path reads only [0])
    ordered += [v for v in numeric if v not in ordered]
    return ordered, info


def _short(v):
    """Short name of a carried-var DICT (this renderer threads var dicts, not names)."""
    return short(v["name"])


def _draw_bounded(ax, numeric, box, inductive, init):
    face = _GREEN if inductive else _AMBER
    hatch = None if inductive else "//"
    if len(numeric) >= 2:
        vx, vy = numeric[0], numeric[1]
        # box is keyed by SHORT name (bounding_box / solved_bounds both key short); init is keyed
        # by FULL name (initial_state). Mixing them KeyError'd on every model whose var has a
        # dotted full name — i.e. any state.X carried var (#428: a FALSE crash, not a real N/A).
        lox, hix = box[_short(vx)]; loy, hiy = box[_short(vy)]
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
        vx = numeric[0]; lo, hi = box[_short(vx)]      # box keyed short; see the 2-axis note above
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


K_DEPTH = 1   # #327: k-induction depth, threaded by render.py's _k_depth ctx (like state_graph's ALL_CONDITIONS)


def _render_k(model, out_path, k, numeric, axinfo):
    """#327: the reachable box at k-induction depth k — solved_bounds(k) drawn, with an honest CLOSED
    (proven inductive — provably contains every reachable state) vs OPEN (k-step extent only — raise k,
    or it's unbounded) banner. Raise k and watch the box close (a saturating counter) or keep growing
    (a free counter that never closes)."""
    kr = k_induction_box(model, k)
    box, closed = kr["box"], kr["closed"]
    init = model.initial_state() or {}
    region_data.write(out_path, region_data.build(
        model, {"verdict": "bounded" if closed else "indeterminate", "inductive": closed,
                "box": {nm: tuple(b) for nm, b in box.items()}, "unbounded": [], "note": kr.get("note")},
        (axinfo["x"], axinfo["y"])))
    write_axes(out_path, axinfo)
    fig, ax = plt.subplots(figsize=(8.2, 5.6))
    if box:
        _draw_bounded(ax, numeric, box, closed, init)
    else:
        _draw_unknown(ax, kr.get("note"))
    if closed:
        msg, col = (f"PROVEN CLOSED at k={k} (≈{kr['horizon']}-step unrolling) — the box is inductive: it "
                    "provably contains EVERY reachable state · sound, no enumeration", _GREEN)
    else:
        msg, col = (f"k-step extent at k={k} (≈{kr['horizon']}-step unrolling) — NOT yet proven closed; "
                    "raise k to tighten/close, or the set is unbounded (a box that grows with every k)", _AMBER)
    verdict_banner(fig, ax, out_path,
                   f"{_name(model)} — reachable region  ·  k-INDUCTION k={k}", msg, col)


def render(smt2, schema, out_path, x_var=None, y_var=None):
    # DUAL CONTRACT: the IDE adapter (ide/web/render.py) calls EVERY renderer as
    # render(smt2, schema, out, x_var=, y_var=) — so we keep that signature and load the model
    # INTERNALLY (never take a Model object), while ALSO writing <out>.data.json (the abstract
    # substrate the golden suite asserts on). Declaring x_var/y_var makes _takes_axes() true, so the
    # IDE threads explicit projection axes straight through (#445) — no _render_via_model adapter.
    model = load(smt2, schema)
    numeric, axinfo = _axes(model, x_var, y_var)       # #445: honor explicit axes, else rank order
    if K_DEPTH and K_DEPTH > 1:
        return _render_k(model, out_path, K_DEPTH, numeric, axinfo)
    r = bounding_box(model)
    verdict, box, unbounded, inductive = r["verdict"], r["box"], r["unbounded"], r["inductive"]
    region_data.write(out_path, region_data.build(model, r, (axinfo["x"], axinfo["y"])))
    write_axes(out_path, axinfo)
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
