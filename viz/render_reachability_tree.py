#!/usr/bin/env python3
"""render_reachability_tree — reachability TREE/forest for any Evident IR.

For a FINITE DISCRETE system the tree is rooted from the SET of initial conditions
(every state with is_first_tick = true), not one seed, and it CLOSES: BFS stops when
a level adds no new reachable state (Model.closing_depth), so a terminating system
shows its full finite reach and a cyclic one stops exactly where the reachable set
saturates — never a misleading hard depth-8 cap. The roots hang off a single synthetic
∅ root so the forest reads as one tree; each real node sits at its BFS depth = the
SHORTEST distance from the nearest initial condition.

For a CONTINUOUS / unbounded system the reachable set is infinite, so we fall back to
the single-seed, depth-capped BFS (the old behavior) and say so honestly in the title.

Edges kept are FIRST-DISCOVERY edges only (so the result is a tree). Absorbing / goal
states (self-loop fixed points, or states whose successors are all already-seen) are
marked.

CLI:  python3 viz/render_reachability_tree.py <smt2> <schema> <out.png>

Reusable for ANY Evident program. The dynamics come entirely from querying the
transition relation through evident_viz (z3); nothing here is hardcoded.
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

# Interactive hover-overlay sidecar (#184 increment 4). The tree NODES are states
# (like state_graph), so each node's layout position + full state dict becomes a
# hover target. reachability_tree ALWAYS saves with bbox_inches="tight", so the
# per-node fractions use the tight-bbox mapping.
from overlay_points import write_points, tight_fraction, OVERLAY_CAP

# Rooting + graph construction (the SET-of-inits forest vs the single-seed fallback) lives
# in reachability_forest; this file owns drawing only.
from reachability_forest import build as _build, ROOT as _ROOT, MAX_NODES, MAX_DEPTH

VIZ_TYPE = "reachability_tree"


def _node_label_field_order(m):
    return ", ".join(v["name"].split(".")[-1] for v in m.interface_vars)


def _build_palette(m, states, real_nodes):
    """The COLOR channel: the top categorical var (enum/bool/string), one hue per value.
    Returns (cat, palette, order) — cat is None for pure-numeric systems (the caller then
    shades by depth). Folds in any observed value outside the declared domain."""
    cats = m.categorical_vars
    cat = cats[0] if cats else None
    if cat is None:
        return None, {}, []
    cname = cat["name"]
    if cat["kind"] == "enum":
        domain = list(m.enum_variants.get(cname, []))
    elif cat["kind"] == "bool":
        domain = [False, True]
    else:
        domain = []
        for nk in real_nodes:
            if states[nk][cname] not in domain:
                domain.append(states[nk][cname])
    for nk in real_nodes:
        if states[nk][cname] not in domain:
            domain.append(states[nk][cname])
    cmap = plt.cm.tab10 if len(domain) <= 10 else plt.cm.tab20
    palette = {val: cmap(i % cmap.N) for i, val in enumerate(domain)}
    return cat, palette, list(domain)


def _draw_nodes(ax, G, pos, states, depth, absorbing, root_k, mode,
                cat, cat_palette, real_nodes):
    """Draw edges + nodes + ID labels; the synthetic ∅ root is grey/green-ringed, real
    states take the categorical hue (or a depth shade), absorbing states get a red ring.
    Returns (node_id, overlay) — the S0/S1/… map and the per-node hover targets."""
    max_d = max(depth.values()) if depth else 0
    node_colors, edge_cols, edge_widths = [], [], []
    for nk in G.nodes():
        if nk == _ROOT:
            node_colors.append("#dddddd")            # synthetic forest root: grey
        elif cat is not None:
            node_colors.append(cat_palette[states[nk][cat["name"]]])
        else:
            t = depth[nk] / max_d if max_d else 0
            node_colors.append(plt.cm.Blues(0.35 + 0.5 * t))
        if nk == _ROOT or (mode == "fallback" and nk == root_k):
            edge_cols.append("#2ecc71"); edge_widths.append(3.0)   # root: green ring
        elif nk in absorbing:
            edge_cols.append("#e74c3c"); edge_widths.append(3.0)   # absorbing: red ring
        else:
            edge_cols.append("#333333"); edge_widths.append(1.0)

    # Short node IDs (S0, S1, …) in BFS-discovery order; the synthetic root shows ∅.
    node_id = {nk: f"S{i}" for i, nk in enumerate(real_nodes)}
    id_labels = {nk: node_id.get(nk, "∅") for nk in G.nodes()}

    nx.draw_networkx_edges(G, pos, ax=ax, arrows=True, arrowstyle="-|>",
                           arrowsize=11, edge_color="#888888", width=1.2)
    nx.draw_networkx_nodes(G, pos, ax=ax, node_color=node_colors, node_size=520,
                           edgecolors=edge_cols, linewidths=edge_widths)
    nx.draw_networkx_labels(G, pos, labels=id_labels, ax=ax, font_size=7.5,
                            font_weight="bold", font_color="#111111")

    overlay = [(ax, pos[nk][0], pos[nk][1], states[nk])
               for nk in real_nodes[:OVERLAY_CAP] if nk in pos]
    return node_id, overlay


def _subtitle(mode, n_roots, n_real, depth, real_nodes, closing_k, complete,
              truncated, discrete, kind, seed_src):
    """The honest one-line caption. all-conditions: how many init roots, how many states,
    and whether the tree CLOSED (saturated) vs truncated vs still-growing. fallback: the
    single-seed depth-capped sample (numeric/unbounded)."""
    max_real_depth = max((depth[nk] for nk in real_nodes), default=0)
    if mode == "all-conditions":
        # depth counts the synthetic root as level 0; real states sit at level ≥1, so the
        # tree's real height is max_real_depth-1 = the longest shortest-path from an init.
        height = max(max_real_depth - 1, 0)
        if complete and not truncated:
            close_txt = (f"CLOSES at depth {closing_k} (reachable set saturated — "
                         f"finite discrete system, fully enumerated)")
        elif truncated:
            close_txt = f"TRUNCATED at {MAX_NODES} nodes (legibility cap)"
        else:
            close_txt = f"capped at depth ≥ {closing_k} (still growing at limit)"
        return (f"reachability forest from ALL initial conditions  ·  "
                f"{n_roots} init condition(s), {n_real} states, tree height {height}  ·  "
                f"{close_txt}  ·  state: {kind}")
    sub = (f"BFS reachability tree (first-discovery edges)  ·  "
           f"{n_real} nodes, depth {max_real_depth}  ·  seed: {seed_src}  ·  state: {kind}")
    if truncated or n_real >= MAX_NODES:
        sub += f"  ·  TRUNCATED at {MAX_NODES} nodes/{MAX_DEPTH} depth"
    if not discrete:
        sub += "  (numeric: reachable set unbounded — capped sample)"
    return sub


def _draw_legends(fig, ax, m, mode, cat, cat_palette, cat_order, node_id, states):
    """The color/marker legend (lower-right) + the ID → full-state-tuple key (left margin),
    where the long tuples live once, readably, instead of overprinted on every node."""
    from matplotlib.lines import Line2D
    legend = []
    if cat is not None:
        legend.append(Line2D([0], [0], marker="", color="w", label=f"color = {cat['name']}"))
        for val in cat_order:
            legend.append(Line2D([0], [0], marker="o", color="w",
                                  markerfacecolor=cat_palette[val],
                                  markeredgecolor="#333333", markersize=10, label=f"  {val}"))
    else:
        legend.append(Line2D([0], [0], marker="o", color="w", markerfacecolor="#5b9bd5",
                             markeredgecolor="#333333", markersize=10,
                             label="reachable (shade = depth)"))
    root_label = ("∅ root → all initial conditions" if mode == "all-conditions"
                  else "root (initial)")
    legend.append(Line2D([0], [0], marker="o", color="w", markerfacecolor="#ffffff",
                         markeredgecolor="#2ecc71", markeredgewidth=2.5, markersize=10,
                         label=root_label))
    legend.append(Line2D([0], [0], marker="o", color="w", markerfacecolor="#ffffff",
                         markeredgecolor="#e74c3c", markeredgewidth=2.5, markersize=10,
                         label="absorbing / goal"))
    ax.legend(handles=legend, loc="lower right", fontsize=8, framealpha=0.9)

    if not node_id:
        return
    field_order = _node_label_field_order(m)
    ordered = sorted(node_id.items(), key=lambda kv: int(kv[1][1:]))
    lines = [f"{sid} = {m.label(states[nk])}" for nk, sid in ordered]
    cap = 40
    if len(lines) > cap:
        lines = lines[:cap] + [f"… (+{len(node_id) - cap} more)"]
    n = len(lines)
    fs = 8 if n <= 18 else (6.5 if n <= 30 else 5.5)
    fig.text(0.012, 0.5, f"node key  (fields: {field_order})\n" + "\n".join(lines),
             ha="left", va="center", fontsize=fs, family="monospace",
             bbox=dict(boxstyle="round", facecolor="#fbfbf6",
                       edgecolor="#cccccc", alpha=0.95))


def _placeholder(fsm, out_path):
    fig, ax = plt.subplots(figsize=(10, 7))
    ax.axis("off")
    ax.set_title(f"{fsm} — {VIZ_TYPE}", fontsize=14, fontweight="bold")
    ax.text(0.5, 0.5, "N/A for this state: no initial state and no numeric seed",
            ha="center", va="center", fontsize=12, transform=ax.transAxes)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, [])


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    (G, states, depth, absorbing, root_k, truncated,
     mode, closing_k, complete, seed_src) = _build(m)
    if G is None:
        return _placeholder(m.fsm, out_path)

    discrete = m.is_discrete()
    kind = "discrete" if discrete else "numeric/mixed"
    fig, ax = plt.subplots(figsize=(13, 9))
    ax.axis("off")

    real_nodes = [nk for nk in G.nodes() if states.get(nk) is not None]
    cat, cat_palette, cat_order = _build_palette(m, states, real_nodes)
    node_id, overlay = {}, []
    if len(G) == 0:
        ax.text(0.5, 0.5, "empty reachable tree", ha="center", va="center",
                transform=ax.transAxes)
    else:
        try:
            pos = graphviz_layout(G, prog="dot")
        except Exception:
            pos = nx.spring_layout(G, seed=1)
        node_id, overlay = _draw_nodes(ax, G, pos, states, depth, absorbing, root_k,
                                       mode, cat, cat_palette, real_nodes)

    n_roots = sum(1 for _ in G.successors(_ROOT)) if _ROOT in G else 0
    ax.set_title(f"{m.fsm} — {VIZ_TYPE}\n"
                 + _subtitle(mode, n_roots, len(real_nodes), depth, real_nodes,
                             closing_k, complete, truncated, discrete, kind, seed_src),
                 fontsize=12, fontweight="bold")
    _draw_legends(fig, ax, m, mode, cat, cat_palette, cat_order, node_id, states)

    # Compute per-node fractions BEFORE savefig, then write the sidecar after the image.
    points = tight_fraction(fig, overlay)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, points)


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
