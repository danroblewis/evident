#!/usr/bin/env python3
"""render_chord_diagram.py — chord/arc diagram of transition flow for ANY Evident IR.

CHANNEL MAPPING (Cleveland-McGill / Mackinlay): a chord diagram's nodes are a
single CATEGORICAL axis bent into a circle — the natural home for the model's
top categorical variable. We map channels by importance x type:

  - NODES (the position axis)  = categorical_vars[0]: the top-ranked enum/bool/
    string. room->room, mode->mode. This is the var the picture reads from.
  - ARC HUE (color channel)    = a SECOND categorical var, if one exists. Each
    arc is hued by the destination state's value of that var — color is
    excellent for categorical, so a low-cardinality bool/enum rides the color
    channel to ADD a dimension (does this room->room move LEAVE you escaped?
    does this mode->mode move DISPENSE?). When no second categorical exists
    (pure-numeric model), we keep the informative derived coloring: a
    weight gradient (transition count).
  - ARC WIDTH + OPACITY (size) = transition COUNT — a derived quantity that's
    genuinely informative (how much flow), kept on the size channel.
  - NODE SIZE                  = outgoing flow total.

Fallback when there is NO categorical var (vanderpol): bin the top numeric var
into ordinal bands and chord between bands — a numeric system still gets a
"which band flows to which band" picture, colored by the weight gradient.

Transitions come from *querying the transition relation* (never hardcoded):
  - has categorical structure: m.reachable() gives the exact transition edge
    set; we project each edge onto the node var (and the color var).
  - pure numeric: follow the ACTUAL orbit (m.trajectory) and bin its consecutive
    states onto the node var. The bin range is the REACHABLE extent
    (m.axis_bounds / the orbit), NEVER a hardcoded ±3000 box — that fabrication
    invents bands the program never enters. A finite/degenerate reachable set
    that is too small to bin meaningfully routes to an honest N/A placeholder.

Usage:
    python3 viz/render_chord_diagram.py <smt2> <schema> <out.png>
"""
import sys
import math
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import PathPatch, Circle
from matplotlib.path import Path
import numpy as np

sys.path.insert(0, "viz")
from evident_viz import load


# --- channel mapping: node var (categorical position) + arc-color var ----------

def pick_primary(m):
    """NODE channel = categorical_vars[0] (top enum/bool/string). Return
    (var_dict, node_labels, project_fn, mode_str). Falls back to binning the top
    numeric var when the model has no categorical structure at all."""
    cats = m.categorical_vars

    if cats:
        v = cats[0]
        if v["kind"] == "enum":
            labels = list(m.enum_variants[v["name"]])
            return v, labels, (lambda st: st[v["name"]]), "enum"
        if v["kind"] == "bool":
            labels = ["false", "true"]
            return v, labels, (lambda st: "true" if st[v["name"]] else "false"), "bool"
        # string: labels discovered dynamically from observed values
        return v, None, (lambda st: st[v["name"]]), "string"

    # no categorical var (pure numeric) — bin the top numeric var into bands
    v = m.numeric_vars[0]
    return v, None, None, "numeric"   # binning resolved after we know range


def pick_color_var(m, node_var):
    """COLOR channel = a SECOND categorical var (not the node var), if one exists.
    Returns (var_dict, color_labels, project_fn) or None. Color is excellent for
    categorical, so a low-cardinality bool/enum rides the color channel to ADD a
    dimension on top of the node->node flow."""
    for v in m.categorical_vars:
        if v["name"] == node_var["name"]:
            continue
        if v["kind"] == "enum":
            labels = list(m.enum_variants[v["name"]])
            return v, labels, (lambda st: st[v["name"]])
        if v["kind"] == "bool":
            return v, ["false", "true"], (lambda st: "true" if st[v["name"]] else "false")
        # string: dynamic labels resolved while gathering
        return v, None, (lambda st: st[v["name"]])
    return None


# --- gather transition flow as a dict {(src_label, dst_label): count} ----------

def gather_flow(m, var, labels, proj, mode, color):
    """Returns (labels, flow, numrange, arc_cat, color_labels) where flow maps
    (src,dst)->count and arc_cat maps (src,dst)->the majority color-var category
    of the destination (None when no color var)."""
    flow = {}
    cat_votes = {}            # (src,dst) -> {color_label: count}
    nbins = 8
    cproj = color[2] if color else None

    def bump(a, b, dst_state=None):
        flow[(a, b)] = flow.get((a, b), 0) + 1
        if cproj is not None and dst_state is not None:
            d = cat_votes.setdefault((a, b), {})
            lab = cproj(dst_state)
            d[lab] = d.get(lab, 0) + 1

    def finish_cat():
        return {k: max(v, key=v.get) for k, v in cat_votes.items()}

    if mode in ("enum", "bool", "string"):
        states, edges = m.reachable()
        if mode == "string":
            seen = []
            for st in states:
                lab = proj(st)
                if lab not in seen:
                    seen.append(lab)
            labels = seen if seen else ["(none)"]
        # resolve dynamic string color labels from observed dst values
        clabels = color[1] if color else None
        if color is not None and clabels is None:
            seen = []
            for st in states:
                lab = cproj(st)
                if lab not in seen:
                    seen.append(lab)
            clabels = seen
        for (i, j) in edges:
            bump(proj(states[i]), proj(states[j]), states[j])
        return labels, flow, None, finish_cat(), clabels

    # numeric: bin the primary var over its REACHABLE extent, and chord between
    # the consecutive states of the ACTUAL orbit (never a fabricated grid sweep
    # over a guessed ±3000 box). orbit() returns the real visited successor chain.
    orbit = orbit_states(m, var)
    lo, hi = orbit_extent(orbit, var["name"])
    if lo is None or (hi - lo) <= 1:
        # reachable set is a single point / degenerate — nothing to chord.
        return [], flow, None, {}, None
    edges_bin = np.linspace(lo, hi, nbins + 1)
    centers = (edges_bin[:-1] + edges_bin[1:]) / 2.0
    labels = [bin_label(centers[k]) for k in range(nbins)]

    def to_bin(val):
        k = int(np.clip(np.searchsorted(edges_bin, val, side="right") - 1, 0, nbins - 1))
        return labels[k]

    for cur, nxt in zip(orbit, orbit[1:]):
        bump(to_bin(cur[var["name"]]), to_bin(nxt[var["name"]]), nxt)

    return labels, flow, (lo, hi), finish_cat(), (color[1] if color else None)


def orbit_states(m, var):
    """The ACTUAL visited states for the numeric node var, as a successor chain.
    Prefers the orbit from the initial state; if that is a degenerate single point
    (an unstable fixed point at the seed — vanderpol's origin), probe a few
    off-origin seeds and keep the richest real orbit. Every state here comes from
    m.successor / m.trajectory — never a hardcoded box."""
    best = m.trajectory(steps=400)
    if _spread(best, var["name"]) > 1:
        return best
    # degenerate from the default init: the attractor may live off the seed.
    # Probe a handful of small off-origin seeds (still REAL dynamics) to recover
    # a limit-cycle / spiral orbit. We do NOT widen to a guessed plotting box;
    # these seeds just kick the system off an unstable fixed point.
    nums = m.numeric_vars
    base = m.initial_state() or {}
    for kick in (16, 64, 256, 1024):
        seed = dict(base)
        if nums:
            seed[nums[0]["name"]] = (base.get(nums[0]["name"], 0) or 0) + kick
        cand = m.trajectory(start=seed, steps=400)
        if _spread(cand, var["name"]) > _spread(best, var["name"]):
            best = cand
    return best


def _spread(states, name):
    vals = [s[name] for s in states if name in s]
    return (max(vals) - min(vals)) if vals else 0


def orbit_extent(orbit, name):
    """(lo, hi) of the node var over the ACTUAL orbit, padded slightly. None when
    the orbit is empty/degenerate — the caller then routes to an honest N/A."""
    vals = [s[name] for s in orbit if name in s]
    if not vals:
        return None, None
    lo, hi = float(min(vals)), float(max(vals))
    if hi - lo <= 1:
        return lo, hi
    pad = (hi - lo) * 0.02
    return lo - pad, hi + pad


def bin_label(c):
    if abs(c) >= 1000:
        return f"{c/1000:+.1f}k"
    return f"{c:+.0f}"


# --- draw -----------------------------------------------------------------------

def draw(m, viz_title, out_path):
    var, labels, proj, mode = pick_primary(m)
    color = pick_color_var(m, var)
    labels, flow, numrange, arc_cat, color_labels = gather_flow(
        m, var, labels, proj, mode, color)

    n = len(labels)
    if n == 0:
        if mode == "numeric":
            orbit = orbit_states(m, var)
            npts = len({tuple(sorted(s.items())) for s in orbit})
            return placeholder(
                m, viz_title, out_path,
                f"reachable set is {npts} point{'s' if npts != 1 else ''} / degenerate — "
                f"chord flow over '{var['name']}' not meaningful")
        return placeholder(m, viz_title, out_path, "no values for primary var")

    # node angles around the circle (top, clockwise)
    angles = {lab: (math.pi / 2 - 2 * math.pi * i / n) for i, lab in enumerate(labels)}
    R = 1.0
    pos = {lab: (R * math.cos(a), R * math.sin(a)) for lab, a in angles.items()}

    fig, ax = plt.subplots(figsize=(8.5, 9))
    ax.set_aspect("equal")
    ax.axis("off")

    maxw = max(flow.values()) if flow else 1

    # COLOR channel: discrete hue per color-var category (if a second categorical
    # var exists), else the weight gradient (derived: transition count).
    use_cat_color = bool(color) and bool(color_labels)
    if use_cat_color:
        qual = plt.get_cmap("tab10")
        cat_color = {lab: qual(i % 10) for i, lab in enumerate(color_labels)}

        def arc_rgba(a, b, frac):
            lab = arc_cat.get((a, b))
            return cat_color.get(lab, (0.5, 0.5, 0.5, 1.0))
    else:
        grad = plt.get_cmap("viridis")

        def arc_rgba(a, b, frac):
            return grad(0.15 + 0.8 * frac)

    # outgoing total per node sets node size
    out_tot = {lab: 0 for lab in labels}
    for (a, b), w in flow.items():
        out_tot[a] = out_tot.get(a, 0) + w
    max_node = max(out_tot.values()) if out_tot else 1

    # draw arcs (sorted light->heavy so heavy arcs sit on top)
    for (a, b), w in sorted(flow.items(), key=lambda kv: kv[1]):
        if a not in pos or b not in pos:
            continue
        x0, y0 = pos[a]
        x1, y1 = pos[b]
        frac = w / maxw
        lw = 0.8 + 6.5 * frac
        alpha = (0.55 + 0.4 * frac) if use_cat_color else (0.30 + 0.6 * frac)
        color_rgba = arc_rgba(a, b, frac)
        if a == b:
            self_loop(ax, x0, y0, lw, color_rgba, alpha)
        else:
            # quadratic Bezier bending toward the circle center for the chord look
            cx, cy = (x0 + x1) * 0.18, (y0 + y1) * 0.18
            path = Path([(x0, y0), (cx, cy), (x1, y1)],
                        [Path.MOVETO, Path.CURVE3, Path.CURVE3])
            ax.add_patch(PathPatch(path, fill=False, lw=lw, edgecolor=color_rgba,
                                   alpha=alpha, capstyle="round"))
            # arrowhead near the destination
            draw_arrowhead(ax, cx, cy, x1, y1, color_rgba, alpha, frac)

    # draw nodes + labels
    for lab in labels:
        x, y = pos[lab]
        sz = 0.04 + 0.06 * (out_tot.get(lab, 0) / max_node)
        ax.add_patch(Circle((x, y), sz, facecolor="#222831", edgecolor="white",
                            lw=1.2, zorder=5))
        # label outside the circle
        a = angles[lab]
        lx, ly = 1.18 * math.cos(a), 1.18 * math.sin(a)
        ha = "left" if math.cos(a) > 0.1 else ("right" if math.cos(a) < -0.1 else "center")
        ax.text(lx, ly, str(lab), ha=ha, va="center", fontsize=11,
                fontweight="bold", color="#222831", zorder=6)

    ax.set_xlim(-1.45, 1.45)
    ax.set_ylim(-1.45, 1.45)

    sub = f"nodes: {var['name']}"
    if mode == "numeric":
        sub += f"  (binned, range [{numrange[0]:.0f}, {numrange[1]:.0f}])"
    elif mode == "bool":
        sub += "  (top var is a bool)"
    if use_cat_color:
        sub += f"   |   arc hue: {color[0]['name']} (of destination)"
    ax.set_title(f"{m.fsm}  —  {viz_title}\n{sub}",
                 fontsize=14, fontweight="bold", pad=18)

    # discrete-hue legend for the color channel (the second categorical var)
    if use_cat_color:
        from matplotlib.lines import Line2D
        handles = [Line2D([0], [0], color=cat_color[lab], lw=4, label=str(lab))
                   for lab in color_labels]
        ax.legend(handles=handles, title=color[0]["name"], loc="upper left",
                  bbox_to_anchor=(-0.02, 1.0), fontsize=9, title_fontsize=10,
                  framealpha=0.9)

    legend = (f"arc width/opacity = transition count (max {maxw});  "
              f"node size = outgoing flow")
    if use_cat_color:
        legend += ";  arc hue = destination's " + color[0]["name"]
    else:
        legend += ";  arc hue = weight gradient"
    fig.text(0.5, 0.025, legend, ha="center", fontsize=9, color="#555")

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def self_loop(ax, x, y, lw, color, alpha):
    # a small loop petal pointing radially outward from the node
    r = math.hypot(x, y) or 1.0
    ux, uy = x / r, y / r
    off = 0.16
    c1 = (x + ux * off - uy * off, y + uy * off + ux * off)
    c2 = (x + ux * off + uy * off, y + uy * off - ux * off)
    path = Path([(x, y), c1, c2, (x, y)],
                [Path.MOVETO, Path.CURVE4, Path.CURVE4, Path.CURVE4])
    ax.add_patch(PathPatch(path, fill=False, lw=lw, edgecolor=color,
                           alpha=alpha, capstyle="round"))


def draw_arrowhead(ax, cx, cy, x1, y1, color, alpha, frac):
    # direction approaching the destination along the bezier (control->end)
    dx, dy = x1 - cx, y1 - cy
    d = math.hypot(dx, dy) or 1.0
    dx, dy = dx / d, dy / d
    # back the head off the node a little
    bx, by = x1 - dx * 0.07, y1 - dy * 0.07
    size = 0.035 + 0.03 * frac
    px, py = -dy, dx
    p1 = (bx, by)
    p2 = (bx - dx * size + px * size * 0.6, by - dy * size + py * size * 0.6)
    p3 = (bx - dx * size - px * size * 0.6, by - dy * size - py * size * 0.6)
    ax.add_patch(plt.Polygon([p1, p2, p3], closed=True, color=color, alpha=alpha,
                             zorder=4))


def placeholder(m, viz_title, out_path, reason):
    fig, ax = plt.subplots(figsize=(8.5, 9))
    ax.axis("off")
    ax.text(0.5, 0.5, f"N/A for this state:\n{reason}", ha="center", va="center",
            fontsize=14, transform=ax.transAxes)
    ax.set_title(f"{m.fsm}  —  {viz_title}", fontsize=14, fontweight="bold")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def main():
    if len(sys.argv) != 4:
        print("usage: render_chord_diagram.py <smt2> <schema> <out.png>", file=sys.stderr)
        sys.exit(2)
    smt2, schema, out = sys.argv[1:4]
    m = load(smt2, schema)
    try:
        draw(m, "chord_diagram", out)
    except Exception as e:
        import traceback
        traceback.print_exc()
        placeholder(m, "chord_diagram", out, f"render error: {e}")


if __name__ == "__main__":
    main()
