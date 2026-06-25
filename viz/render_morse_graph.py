#!/usr/bin/env python3
"""render_morse_graph.py — Morse graph (recurrence skeleton) for any Evident IR.

Usage:
    python3 viz/render_morse_graph.py <smt2> <schema> <out.png>

The Morse graph is the *condensation DAG* of the reachable transition graph:
one node per strongly-connected component (SCC). Recurrent dynamics live inside
the nontrivial SCCs (cycles); the DAG between them encodes the gradient-like
flow of the system. Each node is classified:

    attractor  — no out-edges in the condensation (flow sinks here)
    repeller   — no in-edges  in the condensation (flow originates here)
    transient  — otherwise (flow passes through)

and marked as a CYCLE (SCC size > 1, recurrent set) vs a single state.

This is computed purely by querying the transition relation through
evident_viz (z3) — never hardcoded:
  * discrete / mixed programs: m.reachable() gives the exact graph.
  * numeric programs: reachable() from the (often fixed-point) initial state
    explores nothing, so we sample successors from a grid of seeds and quantize
    the resulting points into cells, building an approximate flow graph whose
    SCCs reveal the limit cycle / attracting set.
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load
# The SCC / Conley-index analysis (condensation DAG, role classification,
# recurrence-skeleton reduction) lives in morse_support; the reachable
# transition-graph construction (+ node-label helpers) lives in
# morse_graph_build. This file owns drawing + orchestration.
from morse_support import _tint_index, condense_and_classify, simplify_skeleton
from morse_graph_build import (
    _key, _abbrev, _fmt_val, _label_for_key,
    build_discrete_graph, build_numeric_orbit_graph, build_numeric_scan_graph,
)

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch
import networkx as nx
from networkx.drawing.nx_pydot import graphviz_layout


def draw_na_card(m, out_path, reason):
    """Render an honest 'not meaningful for this program' card instead of a
    fabricated graph."""
    fig, ax = plt.subplots(figsize=(12, 9))
    ax.axis("off")
    ax.text(0.5, 0.62, "N/A — Morse graph not meaningful here",
            ha="center", va="center", fontsize=20, fontweight="bold",
            color="#444444", transform=ax.transAxes)
    ax.text(0.5, 0.46, reason, ha="center", va="center", fontsize=12,
            color="#666666", wrap=True, transform=ax.transAxes)
    fig.suptitle(f"{m.fsm} — Morse graph (condensation of reachable "
                 f"transition graph)", fontsize=15, fontweight="bold", y=0.9)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    print(f"wrote {out_path}: N/A card ({reason})")


# ----------------------------------------------------------------------------
# Drawing
# ----------------------------------------------------------------------------

ROLE_COLOR = {                # #469: page-bright tones so the node fills read on the dark IDE page
    "attractor": "#3fb950",   # green — flow sinks here
    "repeller":  "#f85149",   # red   — flow sources
    "transient": "#58a6ff",   # blue  — pass-through
    "isolated":  "#a371f7",   # purple
}


def node_text(info_n, orig_labels):
    if "_text" in info_n:
        return info_n["_text"]
    members = info_n["members"]
    if info_n["size"] == 1:
        k = next(iter(members))
        return orig_labels.get(k, str(k))
    # cycle: show size + a couple of members (now ranked-var labels, not tuples)
    sample = list(members)[:2]
    txt = f"cycle ×{info_n['size']}\n"
    txt += "\n".join(orig_labels.get(k, str(k)) for k in sample)
    if info_n["size"] > 2:
        txt += "\n…"
    return txt


def _build_tint_map(C, info, tint_var):
    """Map each distinct dominant-categorical value present in the graph to a
    light fill color (the COLOR channel, excellent for categorical). Returns
    {value: hexcolor}. Empty when there's no tint var."""
    if tint_var is None:
        return {}
    vals = []
    for cn in C.nodes():
        d = info[cn].get("dom_cat")
        if d is not None and d not in vals:
            vals.append(d)
    if not vals:
        return {}
    import matplotlib.colors as mcolors
    base = plt.get_cmap("tab10")
    out = {}
    for i, v in enumerate(vals):
        r, g, b, _ = base(i % 10)
        # lighten toward white so the role-colored border + black text stay legible
        out[v] = mcolors.to_hex((0.55 + 0.45 * r, 0.55 + 0.45 * g, 0.55 + 0.45 * b))
    return out


def draw(C, info, orig_labels, fsm, viz_type, out_path, subtitle="",
         tint_var=None):
    fig, ax = plt.subplots(figsize=(12, 9))
    tint_map = _build_tint_map(C, info, tint_var)

    if C.number_of_nodes() == 0:
        ax.text(0.5, 0.5, "empty graph", ha="center", va="center",
                fontsize=18, transform=ax.transAxes)
        ax.axis("off")
        fig.suptitle(f"{fsm} — {viz_type}", fontsize=16, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    # layout the condensation DAG top-down with graphviz dot
    try:
        pos = graphviz_layout(C, prog="dot")
    except Exception:
        pos = nx.spring_layout(C, seed=7)

    # normalize positions into axes coords for manual boxed drawing
    xs = [p[0] for p in pos.values()]
    ys = [p[1] for p in pos.values()]
    minx, maxx = min(xs), max(xs)
    miny, maxy = min(ys), max(ys)
    spanx = max(maxx - minx, 1)
    spany = max(maxy - miny, 1)

    def norm(p):
        # center a single node (and any degenerate axis) instead of pinning to a
        # corner; spread multi-node layouts across the [0,1] box.
        nx_ = 0.5 if (maxx - minx) < 1e-9 else (p[0] - minx) / spanx
        ny_ = 0.5 if (maxy - miny) < 1e-9 else (p[1] - miny) / spany
        return (nx_, ny_)

    npos = {n: norm(p) for n, p in pos.items()}

    # edges first
    for (u, v) in C.edges():
        x0, y0 = npos[u]
        x1, y1 = npos[v]
        ax.annotate("", xy=(x1, y1), xytext=(x0, y0),
                    arrowprops=dict(arrowstyle="-|>", color="#888888",
                                    lw=1.4, shrinkA=22, shrinkB=22,
                                    connectionstyle="arc3,rad=0.05"))

    # nodes as boxes
    for n in C.nodes():
        x, y = npos[n]
        i = info[n]
        color = ROLE_COLOR[i["role"]]
        txt = node_text(i, orig_labels)
        # cycle nodes get a thicker, double border
        lw = 3.0 if i["is_cycle"] else 1.6
        box = FancyBboxPatch((x - 0.001, y - 0.001), 0.002, 0.002,
                             boxstyle="round,pad=0.5",
                             linewidth=0, facecolor="none")
        ax.add_patch(box)
        fc = tint_map.get(i.get("dom_cat"), "white")
        bbox = dict(boxstyle="round,pad=0.35",
                    fc=fc, ec=color, lw=lw)
        ax.text(x, y, txt, ha="center", va="center", fontsize=8.5,
                color="#111111", bbox=bbox, zorder=5)
        # role marker dot above the box
        ax.scatter([x], [y], s=0)  # keep autoscale honest

    ax.set_xlim(-0.08, 1.08)
    ax.set_ylim(-0.08, 1.08)
    ax.axis("off")

    # legend
    from matplotlib.lines import Line2D
    legend_elems = [
        Line2D([0], [0], marker="s", color="w", label=role,
               markerfacecolor="white", markeredgecolor=col, markeredgewidth=2,
               markersize=12)
        for role, col in ROLE_COLOR.items()
    ]
    legend_elems.append(
        Line2D([0], [0], marker="s", color="w",
               label="cycle SCC (thick border)",
               markerfacecolor="white", markeredgecolor="#333333",
               markeredgewidth=3, markersize=12))
    role_legend = ax.legend(handles=legend_elems, loc="lower center",
                            bbox_to_anchor=(0.5, -0.06), ncol=5,
                            frameon=False, fontsize=9)
    ax.add_artist(role_legend)

    # secondary legend: node FILL = dominant value of the top categorical var
    # (the added COLOR channel — keeps the border=role encoding intact).
    if tint_map:
        from matplotlib.patches import Patch
        tag = _abbrev(tint_var["name"]) if tint_var else "category"
        tint_elems = [
            Patch(facecolor=col, edgecolor="#666666", label=f"{tag}={_fmt_val(val)}")
            for val, col in tint_map.items()
        ]
        # outside the axes (right margin) so it never overlaps a DAG node, which
        # can sit anywhere in the [0,1] layout box.
        ax.legend(handles=tint_elems, loc="upper left",
                  bbox_to_anchor=(1.01, 1.0),
                  title=f"node fill:\ndominant {tag}", title_fontsize=9,
                  ncol=1, frameon=True, fontsize=8.5, framealpha=0.95)

    title = f"{fsm} — Morse graph (condensation of reachable transition graph)"
    fig.suptitle(title, fontsize=15, fontweight="bold", y=0.98)
    if subtitle:
        ax.set_title(subtitle, fontsize=10, color="#444444", pad=14)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


# ----------------------------------------------------------------------------

def render(smt2, schema, out_path):
    """In-process entry — the server imports this and calls it like every other renderer
    (same (smt2, schema, out_path) signature). Mirrors main()'s flow exactly."""
    out = out_path
    m = load(smt2, schema)

    if m.is_discrete():
        G, labels = build_discrete_graph(m)
        sub = (f"{G.number_of_nodes()} reachable states, "
               f"{G.number_of_edges()} transitions  (exact)")
    else:
        numeric_vars = [v for v in m.state_vars
                        if v["kind"] in ("int", "real")]
        if numeric_vars:
            # mixed or pure numeric: use the EXACT reachable set (finite mixed
            # systems like vending/wc terminate). If reachable() collapses to a
            # single state, the encoded seed sits at a fixed point — walk the
            # real forward orbit (which may close into a limit cycle). We NEVER
            # invent off-domain grid seeds: every plotted node is a state the
            # program really visits.
            try:
                states, edges = m.reachable(limit=4000)
            except Exception:
                states, edges = [], []
            if len(states) > 1 and len(states) < 2000:
                G = nx.DiGraph()
                keys = [_key(m, s) for s in states]
                lab = {}
                for s, k in zip(states, keys):
                    G.add_node(k)
                    lab[k] = _label_for_key(m, k)
                for (i, j) in edges:
                    G.add_edge(keys[i], keys[j])
                labels = lab
                sub = (f"{G.number_of_nodes()} reachable states, "
                       f"{G.number_of_edges()} transitions  (exact, mixed)")
            else:
                # reachable() either collapsed to ONE state (the encoded seed sits at a fixed
                # point) OR overflowed the cap (a CONTINUOUS field — a spiral sink / limit cycle —
                # whose from-init enumeration runs away; #357/#465). Both cases are handled by
                # walking the REAL forward flow over the proven-bounds grid, not the discrete
                # enumeration: first the orbit from the seed, then the off-fixed-point seed-scan
                # (which grids the dynamics' own scale). Every node is still a state the program
                # really visits — we change the START point, never fabricate off-domain states.
                G, labels = build_numeric_orbit_graph(m)
                if G is not None and G.number_of_nodes() > 1:
                    sub = (f"{G.number_of_nodes()} states on the real forward "
                           f"orbit from the initial condition  (forward flow)")
                else:
                    G, labels, nseeds = build_numeric_scan_graph(m)
                    if G is not None and G.number_of_nodes() > 1:
                        sub = (f"{G.number_of_nodes()} states on real forward orbits from "
                               f"{nseeds} grid seeds over the proven bounds — the continuous "
                               f"flow's recurrent set  (grid-sweep)")
                    elif len(states) == 1:
                        # genuine fixed point: plot the one real reachable state.
                        G = nx.DiGraph()
                        keys = [_key(m, s) for s in states]
                        G.add_node(keys[0])
                        G.add_edge(keys[0], keys[0])
                        labels = {keys[0]: _label_for_key(m, keys[0])}
                        sub = ("1 reachable state — the encoded seed is a fixed "
                               "point (no further dynamics)  (exact)")
                    else:
                        draw_na_card(m, out,
                                     "the continuous flow has no recurrent set the grid-sweep "
                                     "could resolve (every probed orbit diverged) — a Morse "
                                     "graph would require fabricating off-domain states.")
                        return
        else:
            G, labels = build_discrete_graph(m)
            sub = f"{G.number_of_nodes()} reachable states"

    tint_idx, tint_var = _tint_index(m)
    C, info = condense_and_classify(G, tint_idx=tint_idx)
    if C.number_of_nodes() > 24:
        C, info, labels, note = simplify_skeleton(C, info, labels)
        sub = sub + note
    if tint_var is not None:
        sub = sub + f"  ·  fill = dominant {_abbrev(tint_var['name'])}"
    draw(C, info, labels, m.fsm, "morse_graph", out, subtitle=sub,
         tint_var=tint_var)
    print(f"wrote {out}: {C.number_of_nodes()} SCC nodes, "
          f"{C.number_of_edges()} condensation edges")


def main():
    if len(sys.argv) != 4:
        print("usage: render_morse_graph.py <smt2> <schema> <out.png>",
              file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])


if __name__ == "__main__":
    main()
