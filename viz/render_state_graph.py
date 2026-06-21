#!/usr/bin/env python3
"""render_state_graph — the reachable state-transition graph of an Evident FSM.

Nodes are states, directed edges are transitions of the difference equation
`state = f(_state)`, queried entirely through evident_viz (z3) — never
hardcoded. Laid out with graphviz `dot` via networkx's nx_pydot (or at real
phase-space coordinates for a numeric system), rendered to PNG.

    python3 viz/render_state_graph.py <smt2> <schema> <out_path>

The honest reachable set, by program type:
  * FINITE (discrete OR a terminating numeric/mixed FSM — a clock that counts up
    to `done`, a cursor that walks a fixed input): the EXACT reachable graph via
    Model.reachable(). Almost every Evident demo is this — a small finite chain
    or DAG of 5-20 states. We show it exactly, never a guessed grid.
  * INPUT-DOMINATED (reachable() explodes because a free input fans out at each
    step — e.g. lru's requested cache key): the BFS over all possible inputs is a
    FABRICATION (states the real run never enters). We fall back to the
    deterministic TRAJECTORY — the actual single run — which is finite and small.
  * GENUINELY CONTINUOUS (vanderpol-shaped: an unbounded real/int phase flow):
    reachable() can't terminate, so we follow a handful of seeded trajectories and
    draw that finite subgraph, with axis limits taken from the PLOTTED points.

NODE LABELS: full state-tuples printed on every node overprint into an illegible
smear once nodes cluster (the absorbing-state pile-up). We never do that. Nodes
carry short IDs (S0, S1, …); a compact side legend maps each ID to its
selected-axis values. For larger graphs we drop per-node text entirely (the
layout IS the information) and label only the initial / terminal nodes.

COLOR encodes the top categorical var (enum/bool) by hue, with a legend.
Terminal / absorbing nodes (a state whose only successor is itself, or which has
no successor) keep a heavy ring so fixed points and sinks pop.
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


# A reachable BFS larger than this is treated as input-dominated / non-finite:
# we stop trusting it as "the honest reachable set" and fall back to the
# deterministic trajectory (the real run).
FINITE_CAP = 220


def _key(state):
    return tuple(sorted(state.items()))


def build_reachable_graph(m):
    """Exact reachable graph. Returns (G, states) or None if the reachable set is
    too large to be the honest finite set (input-dominated — caller falls back to
    the trajectory)."""
    states, edges = m.reachable(limit=FINITE_CAP)
    if not states or len(states) >= FINITE_CAP:
        return None
    G = nx.DiGraph()
    for i, st in enumerate(states):
        G.add_node(i, state=st)
    for a, b in edges:
        G.add_edge(a, b)
    return G, states


def build_trajectory_graph(m):
    """The deterministic single run as a path graph — the real, finite run when
    the full reachable BFS would fabricate by exploring every free input.

    Nodes are keyed by the INTERFACE state (the observable contract we plot), not
    the full carried set: an internal counter that keeps ticking after the
    interface state has settled must not split one absorbing state into hundreds
    of look-alike nodes (lru reaches its fixed interface state at tick 7 but a
    carried counter runs to 400)."""
    path = m.trajectory(steps=400)
    if not path:
        return None
    iface = [v["name"] for v in m.interface_vars]

    # If the FSM latches a 'done'-style bool true, the meaningful run ends there:
    # keep the FIRST terminal state and drop the tail (some programs keep ticking
    # an internal counter forever after done, which would otherwise stretch the
    # graph into a meaningless 400-node line).
    done_var = next((v["name"] for v in m.interface_vars
                     if v["kind"] == "bool" and "done" in v["name"].lower()), None)
    if done_var is not None:
        for i, st in enumerate(path):
            if st.get(done_var):
                path = path[:i + 1]
                break

    def ikey(st):
        return tuple((n, st.get(n)) for n in iface)

    index = {}
    states = []

    def node_for(st):
        k = ikey(st)
        if k not in index:
            index[k] = len(states)
            states.append(st)
        return index[k]

    G = nx.DiGraph()
    prev = None
    for st in path:
        j = node_for(st)
        if prev is not None:
            G.add_edge(prev, j)
        prev = j
    for i, st in enumerate(states):
        if i not in G:
            G.add_node(i, state=st)
        G.nodes[i]["state"] = states[i]
    return G, states


def build_seeded_graph(m, seeds, fan_limit=4, max_nodes=400):
    """Genuinely-continuous numeric flow (vanderpol): BFS the transition from
    seeded phase-space points, taking a small nondeterministic fan, capped so the
    picture stays a legible trajectory cloud."""
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
        for nxt in m.successors(states[i], limit=fan_limit):
            j = node_for(nxt)
            G.add_edge(i, j)
            if j not in visited and len(states) < max_nodes:
                frontier.append(j)
    for i, st in enumerate(states):
        if i not in G:
            G.add_node(i, state=st)
        G.nodes[i]["state"] = states[i]
    return G, states


def classify_terminal(G):
    """A node is terminal/absorbing if it has no successor, or its only successor
    is itself (a fixed point)."""
    terminal = set()
    for n in G.nodes():
        succ = set(G.successors(n))
        if not succ or succ == {n}:
            terminal.add(n)
    return terminal


def axis_pair(m):
    """The (x, y) numeric leaves for a phase layout: independent driver on X
    (math convention), the next-ranked numeric on Y. None if < 2 numeric leaves."""
    nums = [v["name"] for v in m.numeric_vars]
    if len(nums) < 2:
        return None
    indep = m.independence()
    driver = indep.get("driver")
    if driver in nums:
        ax = driver
        ay = next(n for n in nums if n != ax)
    else:
        ax, ay = nums[0], nums[1]
    return ax, ay


def robust_limits(vals, pad=0.08):
    """(lo, hi) for an axis from the PLOTTED values, rejecting ±sentinel outliers
    via an IQR fence so a single -1/±1e6 marker can't blow the axis out. Mirrors
    Model.axis_bounds but operates on the values we actually plot."""
    vals = sorted(v for v in vals if isinstance(v, (int, float)) and not isinstance(v, bool))
    if not vals:
        return None
    n = len(vals)
    q1, q3 = vals[n // 4], vals[(3 * n) // 4]
    iqr = q3 - q1
    if iqr > 0:
        lof, hif = q1 - 3 * iqr, q3 + 3 * iqr
        vals = [v for v in vals if lof <= v <= hif] or vals
    lo, hi = float(min(vals)), float(max(vals))
    if lo == hi:
        return (lo - 1.0, hi + 1.0)
    span = (hi - lo) * pad
    lo_out, hi_out = lo - span, hi + span
    if lo >= 0:                       # non-negative domain: clamp at 0 but keep a
        lo_out = -0.5 * span          # small margin so a node sitting at 0 isn't clipped
    return (lo_out, hi_out)


def color_by_categorical(m, G, states):
    """COLOR channel: map each node to a hue by the top-ranked categorical var.
    Returns (face_colors, legend_pairs, var_name) or (None, None, None)."""
    cats = m.categorical_vars
    if not cats:
        return None, None, None
    name = cats[0]["name"]
    if name in m.enum_variants and m.enum_variants[name]:
        domain = list(m.enum_variants[name])
    else:
        seen = []
        for n in G.nodes():
            val = str(states[n].get(name))
            if val not in seen:
                seen.append(val)
        if set(seen) <= {"True", "False"}:
            seen = sorted(seen, key=lambda s: s != "False")
        domain = seen
    for n in G.nodes():
        val = str(states[n].get(name))
        if val not in domain:
            domain.append(val)
    cmap = plt.get_cmap("tab10" if len(domain) <= 10 else "tab20")
    palette = {val: cmap(i % cmap.N) for i, val in enumerate(domain)}
    face = [palette[str(states[n].get(name))] for n in G.nodes()]
    legend = [(f"{name.split('.')[-1]} = {val}", palette[val]) for val in domain]
    return face, legend, name


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    title_type = "state_graph"

    # --- pick the honest finite reachable set ---------------------------------
    G = states = None
    if m.is_discrete():
        built = build_reachable_graph(m)
        if built is not None:
            G, states = built
            mode = "exact reachable graph"
    else:
        names = {v["name"] for v in m.state_vars}
        if {"state.x", "state.v"} <= names:           # vanderpol-shaped flow
            seeds = [{"state.x": x, "state.v": v} for x, v in
                     [(2800, 0), (400, 0), (0, 2700), (-1500, 1500),
                      (1500, -1500), (-2800, 0)]]
            G, states = build_seeded_graph(m, seeds, fan_limit=4, max_nodes=400)
            mode = "seeded trajectories (continuous flow)"
        else:
            # Terminating numeric/mixed FSM: the reachable set is finite. Use it
            # exactly. If the BFS explodes (a free input fans out — fabrication),
            # fall back to the deterministic trajectory (the real run).
            built = build_reachable_graph(m)
            if built is not None:
                G, states = built
                mode = "exact reachable graph"
            else:
                built = build_trajectory_graph(m)
                if built is not None:
                    G, states = built
                    mode = "deterministic run (trajectory)"

    if G is None:                                      # last resort
        built = build_trajectory_graph(m)
        if built is not None:
            G, states = built
            mode = "deterministic run (trajectory)"

    n_nodes = G.number_of_nodes() if G is not None else 0
    n_edges = G.number_of_edges() if G is not None else 0

    if n_nodes == 0:
        fig, ax = plt.subplots(figsize=(8, 6))
        ax.axis("off")
        ax.text(0.5, 0.5,
                f"N/A — not meaningful for {m.fsm}\n"
                f"(no reachable states to graph)",
                ha="center", va="center", fontsize=14, wrap=True)
        ax.set_title(f"{m.fsm} — {title_type}", fontsize=14, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return out_path, 0, 0, "n/a"

    terminal = classify_terminal(G)

    # --- layout ---------------------------------------------------------------
    pair = None if m.is_discrete() else axis_pair(m)
    phase_pos = None
    axis_labels = None
    if pair is not None:
        axx, axy = pair
        phase_pos = {n: (states[n][axx], states[n][axy]) for n in G.nodes()}
        # Only keep the phase layout if both axes actually vary — otherwise it
        # collapses to a line/point and the dot hierarchy reads better.
        xs = {states[n][axx] for n in G.nodes()}
        ys = {states[n][axy] for n in G.nodes()}
        if len(xs) >= 2 and len(ys) >= 2:
            axis_labels = (axx, axy)
        else:
            phase_pos = None

    if phase_pos is not None:
        pos = phase_pos
    else:
        try:
            pos = graphviz_layout(G, prog="dot")
        except Exception:
            pos = nx.spring_layout(G, seed=0)
        if pos:
            pos = {n: (x * 2.0, y) for n, (x, y) in pos.items()}

    # --- node-label strategy --------------------------------------------------
    # Never print the full state-tuple on every node. Small graphs get short IDs
    # (S0, S1 …) with a compact legend mapping each ID to its selected-axis
    # values; larger graphs drop per-node text and tag only init/terminal nodes.
    SHORT_ID_CAP = 26      # graphs at/under this get an S0.. id-and-legend scheme
    LEGEND_CAP = 30        # legend rows beyond this would overflow — skip it
    use_ids = n_nodes <= SHORT_ID_CAP

    init_key = _key(m.initial_state()) if m.initial_state() is not None else None
    init_node = next((n for n in G.nodes()
                      if _key(states[n]) == init_key), None) if init_key else None

    # Width/height scale with node count so things stay readable.
    w = min(max(11, n_nodes * 0.7), 30)
    h = min(max(7, n_nodes * 0.45), 22)
    fig, ax = plt.subplots(figsize=(w, h))

    self_loops = [(u, v) for u, v in G.edges() if u == v]
    plain_edges = [(u, v) for u, v in G.edges() if u != v]

    base_size = 1500 if n_nodes <= 24 else (700 if n_nodes <= 80 else 90)

    face_colors, color_legend, color_var = color_by_categorical(m, G, states)
    if face_colors is None:
        face_colors = ["#e8743b" if n in terminal else "#5b9bd5" for n in G.nodes()]

    edge_colors = ["#e8743b" if n in terminal else "#222222" for n in G.nodes()]
    line_widths = [2.4 if n in terminal else 0.6 for n in G.nodes()]

    nx.draw_networkx_nodes(G, pos, ax=ax, node_color=face_colors,
                           node_size=base_size, edgecolors=edge_colors,
                           linewidths=line_widths)
    nx.draw_networkx_edges(G, pos, ax=ax, edgelist=plain_edges,
                           arrows=True, arrowstyle="-|>", arrowsize=10,
                           edge_color="#888888", width=0.8,
                           connectionstyle="arc3,rad=0.06", node_size=base_size)
    if self_loops:
        nx.draw_networkx_edges(G, pos, ax=ax, edgelist=self_loops,
                               arrows=True, arrowstyle="-|>", arrowsize=10,
                               edge_color="#e8743b", width=1.2, node_size=base_size)

    id_legend = None
    if use_ids:
        # Short IDs on the nodes; legend maps id -> selected-axis values only.
        labels = {n: f"S{n}" for n in G.nodes()}
        nx.draw_networkx_labels(G, pos, labels=labels, ax=ax,
                                font_size=8 if n_nodes <= 24 else 6,
                                font_family="monospace")
        if n_nodes <= LEGEND_CAP:
            axis_names = [v["name"] for v in m.state_vars[:3]]
            short = [a.split(".")[-1] for a in axis_names]
            id_legend = []
            for n in sorted(G.nodes()):
                vals = ", ".join(str(states[n].get(a)) for a in axis_names)
                tag = ""
                if n == init_node:
                    tag = " (init)"
                elif n in terminal:
                    tag = " (terminal)"
                id_legend.append(f"S{n}: ({vals}){tag}")
            id_header = "(" + ", ".join(short) + ")"
    else:
        # Too many nodes for an id legend; tag only init + terminal nodes.
        tags = {}
        if init_node is not None:
            tags[init_node] = "init"
        for n in terminal:
            tags[n] = "term"
        if tags:
            nx.draw_networkx_labels(G, pos, labels=tags, ax=ax, font_size=7,
                                    font_family="monospace",
                                    font_color="#111111")

    subtitle = ""
    if color_var is not None:
        subtitle = f"  color: {color_var.split('.')[-1]}"
    ax.set_title(
        f"{m.fsm} — {title_type}  ({mode}; {n_nodes} states, {n_edges} edges)"
        + subtitle, fontsize=14, fontweight="bold")

    if axis_labels is not None:
        ax.set_xlabel(axis_labels[0])
        ax.set_ylabel(axis_labels[1])
        ax.axis("on")
        ax.grid(True, alpha=0.2)
        xb = robust_limits([states[n][axis_labels[0]] for n in G.nodes()])
        yb = robust_limits([states[n][axis_labels[1]] for n in G.nodes()])
        if xb:
            ax.set_xlim(*xb)
        if yb:
            ax.set_ylim(*yb)
    else:
        ax.axis("off")

    # --- legends --------------------------------------------------------------
    handles = []
    if color_legend is not None:
        handles = [mpatches.Patch(color=c, label=lbl) for lbl, c in color_legend]
        handles.append(mpatches.Patch(facecolor="white", edgecolor="#e8743b",
                                      linewidth=2.4, label="terminal / fixed point"))
    else:
        handles = [mpatches.Patch(color="#5b9bd5", label="state"),
                   mpatches.Patch(color="#e8743b", label="terminal / fixed point")]
    leg1 = ax.legend(handles=handles, loc="upper left",
                     bbox_to_anchor=(1.01, 1.0), fontsize=9, framealpha=0.95,
                     title="color")
    ax.add_artist(leg1)

    if id_legend is not None:
        # Compact node-id legend (short axis values, not the full tuple). Placed
        # outside the axes so it never overprints the graph.
        text = id_header + "\n" + "\n".join(id_legend)
        ax.text(1.01, 0.0, text, transform=ax.transAxes, va="bottom", ha="left",
                fontsize=7, family="monospace",
                bbox=dict(boxstyle="round", facecolor="#f7f7f7",
                          edgecolor="#cccccc", alpha=0.95))

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
