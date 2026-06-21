#!/usr/bin/env python3
"""render_state_graph — the reachable state-transition graph of an Evident FSM.

Nodes are states (labelled by Model.label), directed edges are transitions of
the difference equation `state = f(_state)`, queried entirely through
evident_viz (z3) — never hardcoded. Laid out with graphviz `dot` via
networkx's nx_pydot, rendered to PNG.

    python3 viz/render_state_graph.py <smt2> <schema> <out_path>

Works for ANY Evident IR pair, degrading by program type:
  * DISCRETE (all bool/enum/string): the EXACT reachable graph via
    Model.reachable() — finite, so every node/edge is shown.
  * NUMERIC / MIXED (has int/real): the reachable BFS would be unbounded, so
    we sample states visited along a handful of seeded trajectories and the
    nondeterministic fan out of each, then draw that finite subgraph.

Terminal / absorbing nodes (a state whose only successor is itself, or which
has no successor) are coloured distinctly so fixed points and sinks pop.
"""
import sys
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
import networkx as nx
from networkx.drawing.nx_pydot import graphviz_layout

sys.path.insert(0, "viz")
from evident_viz import load


def _key(state):
    return tuple(sorted(state.items()))


def build_discrete_graph(m):
    """Exact reachable graph for a purely discrete FSM."""
    states, edges = m.reachable()
    G = nx.DiGraph()
    for i, st in enumerate(states):
        G.add_node(i, label=m.label(st), state=st)
    for a, b in edges:
        G.add_edge(a, b)
    return G, states


def build_sampled_graph(m, seeds, steps=60, fan_limit=8, max_nodes=300):
    """For numeric / mixed FSMs: BFS the transition from seed states, taking the
    nondeterministic fan at each, capped so the picture stays legible."""
    init = m.initial_state()
    starts = []
    if init is not None:
        starts.append(init)
    starts.extend(seeds)

    index = {}
    states = []

    def node_for(st):
        k = _key(st)
        if k not in index:
            index[k] = len(states)
            states.append(st)
        return index[k]

    G = nx.DiGraph()
    frontier = []
    for s in starts:
        i = node_for(s)
        if i not in frontier:
            frontier.append(i)

    visited = set()
    while frontier and len(states) < max_nodes:
        i = frontier.pop(0)
        if i in visited:
            continue
        visited.add(i)
        succs = m.successors(states[i], limit=fan_limit)
        for nxt in succs:
            j = node_for(nxt)
            G.add_edge(i, j)
            if j not in visited and len(states) < max_nodes:
                frontier.append(j)

    # Make sure isolated seed nodes still appear.
    for i, st in enumerate(states):
        if i not in G:
            G.add_node(i)
    for i in list(G.nodes()):
        G.nodes[i]["label"] = m.label(states[i])
        G.nodes[i]["state"] = states[i]
    return G, states


def classify_terminal(G, states):
    """A node is terminal/absorbing if it has no successor, or its only
    successor is itself (a fixed point)."""
    terminal = set()
    for n in G.nodes():
        succ = set(G.successors(n))
        if not succ or succ == {n}:
            terminal.add(n)
    return terminal


def numeric_axis_pos(m, G, states):
    """If the state has >=2 numeric leaves, lay nodes out at their actual
    (first-numeric, second-numeric) phase-space coordinates instead of a dot
    tree — so the transition edges trace the real trajectory/limit cycle.
    Returns a pos dict, or None if there aren't two numeric leaves."""
    nums = [v["name"] for v in m.state_vars if v["kind"] in ("int", "real")]
    if len(nums) < 2:
        return None
    ax, ay = nums[0], nums[1]
    return {n: (states[n][ax], states[n][ay]) for n in G.nodes()}, ax, ay


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    title_type = "state_graph"
    axis_labels = None

    if m.is_discrete():
        G, states = build_discrete_graph(m)
        mode = "exact reachable graph"
    else:
        # Per-sample seeds for the known numeric/mixed samples; for an unknown
        # numeric IR we still seed from initial_state + a coarse origin grid.
        seeds = []
        names = {v["name"] for v in m.state_vars}
        if {"state.x", "state.v"} <= names:        # vanderpol-shaped
            for x, v in [(2800, 0), (400, 0), (0, 2700), (-1500, 1500),
                         (1500, -1500), (-2800, 0)]:
                seeds.append({"state.x": x, "state.v": v})
            G, states = build_sampled_graph(m, seeds, steps=80, fan_limit=4,
                                            max_nodes=400)
            mode = "sampled trajectories"
        else:
            init = m.initial_state()
            if init is not None:
                seeds.append(init)
            G, states = build_sampled_graph(m, seeds, steps=80, fan_limit=8,
                                            max_nodes=300)
            mode = "sampled reachable graph"

    n_nodes = G.number_of_nodes()
    n_edges = G.number_of_edges()

    # Empty / nothing-to-draw -> titled placeholder.
    if n_nodes == 0:
        fig, ax = plt.subplots(figsize=(8, 6))
        ax.axis("off")
        ax.text(0.5, 0.5,
                f"N/A for {m.fsm}: no reachable states\n(initial_state is None)",
                ha="center", va="center", fontsize=14, wrap=True)
        ax.set_title(f"{m.fsm} — {title_type}", fontsize=14, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return out_path, n_nodes, n_edges, mode

    terminal = classify_terminal(G, states)

    # Layout. For a numeric/mixed system with >=2 numeric leaves, place each
    # node at its real phase-space coordinate so edges trace the trajectory and
    # any limit cycle is visible. Otherwise use graphviz `dot` (discrete graphs
    # read as a clean hierarchy), falling back to spring if dot chokes.
    phase = None if m.is_discrete() else numeric_axis_pos(m, G, states)
    if phase is not None:
        pos, axx, axy = phase
        axis_labels = (axx, axy)
        big = n_nodes > 60   # too many nodes for per-node text labels
    else:
        big = n_nodes > 60
        try:
            pos = graphviz_layout(G, prog="dot")
        except Exception:
            pos = nx.spring_layout(G, seed=0)
        # Stretch x apart so the wide tuple labels of sibling nodes on the
        # same rank stop colliding, without distorting the dot hierarchy.
        if pos:
            pos = {n: (x * 2.6, y) for n, (x, y) in pos.items()}

    # Scale figure with graph size so labels stay readable.
    if phase is not None:
        w = min(max(10, n_nodes * 0.8), 42)
        h = min(max(7, n_nodes * 0.5), 32)
    else:
        # Discrete dot layout: wide aspect to give horizontal label room.
        w = min(max(16, n_nodes * 1.2), 46)
        h = min(max(8, n_nodes * 0.55), 30)
    fig, ax = plt.subplots(figsize=(w, h))

    node_colors = ["#e8743b" if n in terminal else "#5b9bd5" for n in G.nodes()]
    self_loops = [(u, v) for u, v in G.edges() if u == v]
    plain_edges = [(u, v) for u, v in G.edges() if u != v]

    node_size = 1600 if n_nodes <= 30 else (900 if n_nodes <= 80 else 120)
    font_size = 8 if n_nodes <= 30 else (6 if n_nodes <= 80 else 4)

    nx.draw_networkx_nodes(G, pos, ax=ax, node_color=node_colors,
                           node_size=node_size, edgecolors="#222222",
                           linewidths=0.6)
    nx.draw_networkx_edges(G, pos, ax=ax, edgelist=plain_edges,
                           arrows=True, arrowstyle="-|>", arrowsize=10,
                           edge_color="#888888", width=0.8,
                           connectionstyle="arc3,rad=0.06",
                           node_size=node_size)
    if self_loops:
        nx.draw_networkx_edges(G, pos, ax=ax, edgelist=self_loops,
                               arrows=True, arrowstyle="-|>", arrowsize=10,
                               edge_color="#e8743b", width=1.2,
                               node_size=node_size)

    # Per-node text labels only when the graph is small enough to read them;
    # otherwise (big numeric trajectory clouds) the positions ARE the label.
    if not big:
        labels = {n: G.nodes[n]["label"] for n in G.nodes()}
        nx.draw_networkx_labels(G, pos, labels=labels, ax=ax,
                                font_size=font_size, font_family="monospace")

    ax.set_title(
        f"{m.fsm} — {title_type}  ({mode}; {n_nodes} states, {n_edges} edges)",
        fontsize=14, fontweight="bold")
    if axis_labels is not None:
        ax.set_xlabel(axis_labels[0])
        ax.set_ylabel(axis_labels[1])
        ax.axis("on")
        ax.grid(True, alpha=0.2)
    else:
        ax.axis("off")

    legend = [
        mpatches.Patch(color="#5b9bd5", label="state"),
        mpatches.Patch(color="#e8743b", label="terminal / fixed point"),
    ]
    ax.legend(handles=legend, loc="upper right", fontsize=9, framealpha=0.9)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    return out_path, n_nodes, n_edges, mode


def main(argv):
    if len(argv) != 4:
        print("usage: render_state_graph.py <smt2> <schema> <out_path>",
              file=sys.stderr)
        return 2
    out, n, e, mode = render(argv[1], argv[2], argv[3])
    print(f"wrote {out}: {n} nodes, {e} edges ({mode})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
