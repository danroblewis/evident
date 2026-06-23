"""reachability_forest — the ROOTING + graph construction for render_reachability_tree.

Two rooting strategies, chosen by `build`:

  - FINITE DISCRETE → root from the SET of initial conditions (every state with
    is_first_tick = true), hung off a synthetic ∅ root so the forest reads as one tree,
    and CLOSE the BFS at saturation (Model.closing_depth) — never a hard depth cap.
  - CONTINUOUS / unbounded / two-tick → the single-seed, depth-capped sample (the old
    behavior; the reachable set is infinite so the tree can't close).

The dynamics come entirely from the model's successor relation; nothing here is
hardcoded. Split out of render_reachability_tree so that file owns drawing only.
"""
import z3

import networkx as nx

from evident_viz import hashable_value

# Legibility caps so numeric / large systems still terminate and stay legible.
MAX_NODES = 60
MAX_DEPTH = 8

# The synthetic forest root over the initial-condition set. A tuple key (never produced
# by _key, which sorts (name, value) pairs) so it can't collide with a real state.
ROOT = ("∅", "initial-conditions")


def key(state):
    return tuple(sorted((k, hashable_value(v)) for k, v in state.items()))


def initial_states(m, limit=MAX_NODES):
    """All distinct INITIAL conditions — states with is_first_tick = true — via the same
    block-and-resolve enumeration successors() uses, deduped by key. For a single-seeded
    FSM this is one state; for a parametric / multi-init FSM it is the whole start SET."""
    s = m._base()
    if m.first_tick is not None:
        s.add(m.first_tick == True)  # noqa: E712
    out, seen = [], set()
    while len(out) < limit and s.check() == z3.sat:
        mod = s.model()
        st = m._read_state(mod)
        k = key(st)
        if k not in seen:
            seen.add(k)
            out.append(st)
        s.add(m._block_clause(mod))
    return out


def build_forest(m, inits):
    """Multi-source BFS forest rooted from the SET of initial conditions, CLOSING when the
    frontier empties (the finite-discrete path). Returns
    (G, states, depth, absorbing, root_k, truncated).

    A synthetic ROOT node (depth 0) edges to every initial condition (depth 1) so the
    forest reads as one tree. Real states keep their BFS depth = SHORTEST distance from
    the nearest init. No fixed MAX_DEPTH cap: the reachable set is finite (the discrete
    gate guaranteed it), so the frontier empties when the set CLOSES; MAX_NODES is the
    only legibility guard."""
    G = nx.DiGraph()
    G.add_node(ROOT)
    states = {ROOT: None}
    depth = {ROOT: 0}
    absorbing = set()
    truncated = False

    # Layer 1: the initial conditions, all children of the synthetic root.
    frontier = []
    for init in inits:
        k = key(init)
        if k in states:
            G.add_edge(ROOT, k)             # duplicate init value — re-link, don't re-add
            continue
        if len(G) >= MAX_NODES:
            truncated = True
            break
        states[k] = init
        depth[k] = 1
        G.add_node(k)
        G.add_edge(ROOT, k)
        frontier.append(k)

    while frontier:
        k = frontier.pop(0)
        st = states[k]
        succs = m.successors(st, limit=32)
        non_self = [s for s in succs if key(s) != k]
        if not succs or not non_self:
            absorbing.add(k)                # fixed point: no successor, or self-loop only
        for ns in succs:
            nk = key(ns)
            if nk == k:
                continue                    # don't draw self-loops in the tree
            if nk not in states:
                if len(G) >= MAX_NODES:
                    truncated = True
                    break
                states[nk] = ns
                depth[nk] = depth[k] + 1
                G.add_node(nk)
                G.add_edge(k, nk)           # first-discovery edge only
                frontier.append(nk)
            # else: cross/back edge — omitted to keep it a tree
        if truncated:
            break
    return G, states, depth, absorbing, ROOT, truncated


def pick_seed(m):
    """A start state that actually moves — the continuous/unbounded fallback's single seed.
    Prefer the program's initial_state; if it's a fixed point (successor == itself), fall
    back to a grid seed for numeric systems so the tree shows real dynamics."""
    init = m.initial_state()
    if init is not None:
        succ = m.successor(init)
        if succ is None or key(succ) != key(init):
            return init, "initial_state"
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
        if all(seed.get(n, 0) == 0 for n in numeric):
            seed[sorted(numeric)[0]] = 2800
        return seed, "grid seed"
    return init, "initial_state"


def build_tree(m, seed):
    """BFS tree from ONE seed, capped at MAX_DEPTH/MAX_NODES — the continuous/unbounded
    fallback (the reachable set is infinite, so the tree can't close; we sample it)."""
    G = nx.DiGraph()
    root_k = key(seed)
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
        non_self = [s for s in succs if key(s) != k]
        if not non_self:
            absorbing.add(k)                # self-loop-only => absorbing fixed point
        for ns in succs:
            nk = key(ns)
            if nk == k:
                continue
            if nk not in states:
                if len(G) >= MAX_NODES:
                    truncated = True
                    break
                states[nk] = ns
                depth[nk] = depth[k] + 1
                G.add_node(nk)
                G.add_edge(k, nk)           # first-discovery edge only
                frontier.append(nk)
    return G, states, depth, absorbing, root_k, truncated


def build(m):
    """Choose the rooting. For a finitely-enumerable DISCRETE system, root from the SET of
    initial conditions and CLOSE the BFS at saturation (Model.closing_depth). Otherwise
    fall back to the single-seed depth-capped sample. Returns
    (G, states, depth, absorbing, root_k, truncated, mode, closing_k, complete, seed_src)
    with G=None when there is nothing to root from."""
    if not m.has_two_tick:
        _s, _e, info = m.full_state_graph(limit=5000)
        if info.get("discrete") and not info.get("capped"):
            inits = initial_states(m)
            if inits:
                closing_k, complete = m.closing_depth(limit=5000)
                G, states, depth, absorbing, root_k, truncated = build_forest(m, inits)
                return (G, states, depth, absorbing, root_k, truncated,
                        "all-conditions", closing_k, complete, "all initial conditions")
    # Continuous / unbounded / two-tick → single-seed, depth-capped sample.
    seed, seed_src = pick_seed(m)
    if seed is None:
        return (None, {}, {}, set(), None, False, "fallback", 0, False, seed_src)
    G, states, depth, absorbing, root_k, truncated = build_tree(m, seed)
    return (G, states, depth, absorbing, root_k, truncated,
            "fallback", 0, False, seed_src)
