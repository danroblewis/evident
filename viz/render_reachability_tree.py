#!/usr/bin/env python3
"""render_reachability_tree — BFS reachability TREE for any Evident IR.

Builds the breadth-first reachability tree from the initial state, keeping only
FIRST-DISCOVERY edges (so the result is a tree, not the full graph). Drawn
top-down with graphviz `dot`; each node sits at its BFS depth = the length of
the SHORTEST path from the root to that state. Absorbing / goal states (self-loop
fixed points, or states whose successors are all already-seen) are marked.

CLI:  python3 viz/render_reachability_tree.py <smt2> <schema> <out.png>

Reusable for ANY Evident program. The dynamics come entirely from querying the
transition relation through evident_viz (z3); nothing here is hardcoded.

Degradation: numeric systems have an unbounded reachable set, so the tree is
capped (node + depth limit) and we seed from a non-fixed-point grid point when
the program's own initial_state is a fixed point (e.g. vanderpol's origin).
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

import networkx as nx
from networkx.drawing.nx_pydot import graphviz_layout

VIZ_TYPE = "reachability_tree"

# Caps so numeric / large systems still terminate and stay legible.
MAX_NODES = 60
MAX_DEPTH = 8


def _key(state):
    return tuple(sorted(state.items()))


def _pick_seed(m):
    """A start state that actually moves. Prefer the program's initial_state;
    if it's a fixed point (successor == itself), fall back to a grid seed for
    numeric systems so the tree shows real dynamics."""
    init = m.initial_state()
    if init is not None:
        succ = m.successor(init)
        if succ is None or _key(succ) != _key(init):
            return init, "initial_state"
    # initial_state is missing or a fixed point. For numeric state, pin a seed
    # off the fixed point so the tree isn't a single self-looping node.
    names = {v["name"] for v in m.state_vars}
    numeric = {v["name"] for v in m.state_vars if v["kind"] in ("int", "real")}
    if numeric:
        seed = {}
        for v in m.state_vars:
            n = v["name"]
            if n in numeric:
                # van-der-pol-ish scale: a point on the limit cycle, off origin
                seed[n] = 2800 if (n.endswith(".x") or "x" in n.lower()) else 0
            elif v["kind"] == "bool":
                seed[n] = False
            elif v["kind"] == "enum":
                seed[n] = m.enum_variants[n][0]
            elif v["kind"] == "string":
                seed[n] = ""
        # make sure at least one numeric axis is nonzero
        if all(seed.get(n, 0) == 0 for n in numeric):
            any_n = sorted(numeric)[0]
            seed[any_n] = 2800
        return seed, "grid seed"
    return init, "initial_state"


def build_tree(m, seed):
    """BFS tree: nodes with depth, first-discovery edges, absorbing flags."""
    G = nx.DiGraph()
    root_k = _key(seed)
    states = {root_k: seed}
    depth = {root_k: 0}
    G.add_node(root_k)
    frontier = [root_k]
    absorbing = set()
    truncated = False

    while frontier and len(G) < MAX_NODES:
        k = frontier.pop(0)
        if depth[k] >= MAX_DEPTH:
            continue
        st = states[k]
        succs = m.successors(st, limit=32)
        if not succs:
            absorbing.add(k)
            continue
        # self-loop-only => absorbing fixed point
        non_self = [s for s in succs if _key(s) != k]
        if not non_self:
            absorbing.add(k)
        for ns in succs:
            nk = _key(ns)
            if nk == k:
                continue  # don't draw self-loops in the tree
            if nk not in states:
                if len(G) >= MAX_NODES:
                    truncated = True
                    break
                states[nk] = ns
                depth[nk] = depth[k] + 1
                G.add_node(nk)
                G.add_edge(k, nk)  # first-discovery edge only
                frontier.append(nk)
            # else: cross/back edge — omitted to keep it a tree
    return G, states, depth, absorbing, root_k, truncated


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    fsm = m.fsm

    seed, seed_src = _pick_seed(m)

    if seed is None:
        # nothing to seed from — placeholder figure
        fig, ax = plt.subplots(figsize=(10, 7))
        ax.axis("off")
        ax.set_title(f"{fsm} — {VIZ_TYPE}", fontsize=14, fontweight="bold")
        ax.text(0.5, 0.5,
                "N/A for this state: no initial state and no numeric seed",
                ha="center", va="center", fontsize=12,
                transform=ax.transAxes)
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    G, states, depth, absorbing, root_k, truncated = build_tree(m, seed)

    discrete = m.is_discrete()
    kind = "discrete" if discrete else "numeric/mixed"

    fig, ax = plt.subplots(figsize=(13, 9))
    ax.axis("off")

    cat = None
    cat_palette = {}
    cat_order = []

    if len(G) == 0:
        ax.text(0.5, 0.5, "empty reachable tree", ha="center", va="center",
                transform=ax.transAxes)
    else:
        try:
            pos = graphviz_layout(G, prog="dot")
        except Exception:
            pos = nx.spring_layout(G, seed=1)

        max_d = max(depth.values()) if depth else 0

        # COLOR channel: the top categorical var (enum/bool/string). One hue per
        # value, with a legend — this is what assign_channels routes to color, and
        # it reads far better than depth-shading for discrete state. If there's no
        # categorical var (pure-numeric systems like vanderpol), fall back to the
        # depth gradient (a legitimate coarse-quantitative use of color).
        cats = m.categorical_vars
        cat = cats[0] if cats else None

        # palette over the categorical var's domain
        if cat is not None:
            cname = cat["name"]
            if cat["kind"] == "enum":
                domain = list(m.enum_variants.get(cname, []))
            elif cat["kind"] == "bool":
                domain = [False, True]
            else:
                # string / other: collect observed values in stable order
                seen = []
                for nk in G.nodes():
                    val = states[nk][cname]
                    if val not in seen:
                        seen.append(val)
                domain = seen
            # also fold in any observed values not in the declared domain
            for nk in G.nodes():
                val = states[nk][cname]
                if val not in domain:
                    domain.append(val)
            cmap = plt.cm.tab10 if len(domain) <= 10 else plt.cm.tab20
            for i, val in enumerate(domain):
                cat_palette[val] = cmap(i % cmap.N)
                cat_order.append(val)

        node_colors = []
        edge_cols = []        # node border color: root/absorbing markers
        edge_widths = []
        for nk in G.nodes():
            if cat is not None:
                node_colors.append(cat_palette[states[nk][cat["name"]]])
            else:
                t = depth[nk] / max_d if max_d else 0
                node_colors.append(plt.cm.Blues(0.35 + 0.5 * t))
            # keep root / absorbing marking via the node border
            if nk == root_k:
                edge_cols.append("#2ecc71")    # root: green ring
                edge_widths.append(3.0)
            elif nk in absorbing:
                edge_cols.append("#e74c3c")    # absorbing/goal: red ring
                edge_widths.append(3.0)
            else:
                edge_cols.append("#333333")
                edge_widths.append(1.0)

        labels = {nk: m.label(states[nk]) for nk in G.nodes()}

        nx.draw_networkx_edges(G, pos, ax=ax, arrows=True,
                               arrowstyle="-|>", arrowsize=11,
                               edge_color="#888888", width=1.2)
        nx.draw_networkx_nodes(G, pos, ax=ax, node_color=node_colors,
                               node_size=900, edgecolors=edge_cols,
                               linewidths=edge_widths)
        nx.draw_networkx_labels(G, pos, labels=labels, ax=ax, font_size=6.5)

    subtitle = (f"BFS reachability tree (first-discovery edges)  ·  "
                f"{len(G)} nodes, depth {max(depth.values()) if depth else 0}  ·  "
                f"seed: {seed_src}  ·  state: {kind}")
    if truncated or len(G) >= MAX_NODES:
        subtitle += f"  ·  TRUNCATED at {MAX_NODES} nodes/{MAX_DEPTH} depth"
    if not discrete:
        subtitle += "  (numeric: reachable set unbounded — capped sample)"

    ax.set_title(f"{fsm} — {VIZ_TYPE}\n{subtitle}",
                 fontsize=12, fontweight="bold")

    # legend — COLOR encodes the top categorical var (one entry per value),
    # and the root / absorbing markers are shown as ringed swatches.
    from matplotlib.lines import Line2D
    legend = []
    if cat is not None:
        legend.append(Line2D([0], [0], marker="", color="w",
                             label=f"color = {cat['name']}"))
        for val in cat_order:
            legend.append(Line2D([0], [0], marker="o", color="w",
                                 markerfacecolor=cat_palette[val],
                                 markeredgecolor="#333333", markersize=10,
                                 label=f"  {val}"))
    else:
        legend.append(Line2D([0], [0], marker="o", color="w",
                             markerfacecolor="#5b9bd5", markeredgecolor="#333333",
                             markersize=10, label="reachable (shade = depth)"))
    legend.append(Line2D([0], [0], marker="o", color="w", markerfacecolor="#ffffff",
                         markeredgecolor="#2ecc71", markeredgewidth=2.5,
                         markersize=10, label="root (initial)"))
    legend.append(Line2D([0], [0], marker="o", color="w", markerfacecolor="#ffffff",
                         markeredgecolor="#e74c3c", markeredgewidth=2.5,
                         markersize=10, label="absorbing / goal"))
    ax.legend(handles=legend, loc="lower right", fontsize=8, framealpha=0.9)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def main(argv):
    if len(argv) != 4:
        print("usage: render_reachability_tree.py <smt2> <schema> <out.png>",
              file=sys.stderr)
        return 2
    render(argv[1], argv[2], argv[3])
    print(f"wrote {argv[3]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
