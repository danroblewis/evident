#!/usr/bin/env python3
"""state_graph_build.py — reachable / trajectory / seeded graph construction for
render_state_graph.py.

Builds the honest finite graph the renderer draws (exact reachable graph when it
closes below the cap, the deterministic trajectory when a free input would fan
out the BFS, or seeded phase-space trajectories for a continuous flow), plus the
terminal-node classifier and the legibility down-sampler. No plotting policy.
"""
import networkx as nx

from evident_viz import hashable_value


# A reachable BFS larger than this is treated as input-dominated / non-finite:
# we stop trusting it as "the honest reachable set" and fall back to the
# deterministic trajectory (the real run).
FINITE_CAP = 220

# A graph with more nodes than this can't be drawn legibly — the dots collapse
# into a single tangled smear. Beyond this we sample a representative connected
# subgraph and stamp a "showing N of M states" note.
READABLE_CAP = 48


def _key(state):
    return tuple(sorted((k, hashable_value(v)) for k, v in state.items()))


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


def build_global_graph(m):
    """The ALL-INITIAL-CONDITIONS graph: the transition graph over EVERY valid
    carried assignment (Model.full_state_graph), not the forward orbit of the one
    seeded init. Returns (G, states) for a finite discrete product that fits, or
    None when the model isn't finitely enumerable (real/string/unbounded) or the
    product exceeds FINITE_CAP — the caller then falls back to the from-init path."""
    states, edges, info = m.full_state_graph(limit=FINITE_CAP)
    if not info["discrete"] or info["capped"] or not states:
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
        return tuple((n, hashable_value(st.get(n))) for n in iface)

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


def sample_subgraph(G, states, init_node, cap=READABLE_CAP):
    """When G is too large to draw legibly, return a representative CONNECTED
    subgraph of `cap` nodes (and a 'showing N of M' note); otherwise return G
    unchanged with no note.

    We grow the sample by BFS from the initial node, so the kept nodes are a
    connected neighbourhood of the real run's start — for a deterministic
    trajectory that is exactly its first `cap` states (a faithful prefix), and
    for a branching reachable/seeded graph it is the start basin rather than an
    arbitrary slice. Returns (G2, states2, note, remap) where remap maps old
    node id -> new node id (None if a node was dropped)."""
    n = G.number_of_nodes()
    if n <= cap:
        return G, states, None, {i: i for i in G.nodes()}

    start = init_node if (init_node is not None and init_node in G) else (
        next(iter(G.nodes())) if n else None)
    keep = []
    seen = set()
    frontier = [start]
    # BFS over the UNDIRECTED view so we still reach predecessors of the start.
    und = G.to_undirected(as_view=True)
    while frontier and len(keep) < cap:
        node = frontier.pop(0)
        if node in seen:
            continue
        seen.add(node)
        keep.append(node)
        for nb in und.neighbors(node):
            if nb not in seen:
                frontier.append(nb)
    keep_set = set(keep)

    remap = {old: None for old in G.nodes()}
    G2 = nx.DiGraph()
    states2 = []
    for old in keep:
        new = len(states2)
        remap[old] = new
        states2.append(states[old])
        G2.add_node(new, state=states[old])
    for u, v in G.edges():
        if u in keep_set and v in keep_set:
            G2.add_edge(remap[u], remap[v])
    note = f"showing {len(keep)} of {n} states"
    return G2, states2, note, remap
