#!/usr/bin/env python3
"""morse_support.py — the SCC / Conley-index analysis for the Morse-graph
renderer.

Given a reachable transition graph (a networkx DiGraph of state keys), this
module computes the *condensation DAG* (one node per strongly-connected
component), classifies each SCC by its Conley-index role in the gradient-like
flow (attractor / repeller / transient / isolated, plus the recurrent CYCLE
flag), and — for large graphs — collapses the singleton-transient cloud down to
a legible recurrence skeleton.

It is path-agnostic and draw-agnostic: it takes a graph (+ an optional model for
the categorical-tint index) and returns analysis structures. The renderer
(`render_morse_graph.py`) owns graph construction, drawing, and orchestration;
this module owns the topology.
"""
import networkx as nx


def _tint_index(m):
    """Index into the state-key tuple of the variable used to TINT Morse nodes:
    the top-ranked CATEGORICAL var (color is excellent for categorical). Returns
    (index, var) or (None, None) when the model has no categorical var (e.g. a
    pure-numeric system like vanderpol) — then nodes stay white-filled and read
    by role/border alone."""
    cats = m.categorical_vars
    if not cats:
        return None, None
    name = cats[0]["name"]
    names = [v["name"] for v in m.state_vars]
    if name not in names:
        return None, None
    return names.index(name), cats[0]


def condense_and_classify(G, tint_idx=None):
    """Return (C, info) where C is the condensation DAG and info maps each
    condensation node -> dict(role, is_cycle, size, members, label). When
    tint_idx is given, info[n]['dom_cat'] = the dominant value of that categorical
    var among the SCC's member states (the node-fill tint)."""
    sccs = list(nx.strongly_connected_components(G))
    C = nx.condensation(G, scc=sccs)   # node attr 'members' = set of orig nodes

    info = {}
    for cn in C.nodes():
        members = C.nodes[cn]["members"]
        size = len(members)
        # an SCC is recurrent (cycle) if size>1 OR a single node with a self-loop
        is_cycle = size > 1
        if size == 1:
            only = next(iter(members))
            if G.has_edge(only, only):
                is_cycle = True
        indeg = C.in_degree(cn)
        outdeg = C.out_degree(cn)
        if outdeg == 0 and indeg > 0:
            role = "attractor"
        elif indeg == 0 and outdeg > 0:
            role = "repeller"
        elif indeg == 0 and outdeg == 0:
            role = "isolated"
        else:
            role = "transient"
        dom = None
        if tint_idx is not None:
            counts = {}
            for k in members:
                if isinstance(k, tuple) and len(k) > tint_idx:
                    val = k[tint_idx]
                    counts[val] = counts.get(val, 0) + 1
            if counts:
                dom = max(counts, key=counts.get)
        info[cn] = dict(role=role, is_cycle=is_cycle, size=size,
                        members=members, dom_cat=dom)
    return C, info


def simplify_skeleton(C, info, labels):
    """Collapse a large condensation down to its recurrence skeleton for
    legibility. Keep every recurrent (cycle) SCC plus the repeller/attractor
    boundary nodes; merge the cloud of singleton transients into one summary
    node so the limit set stays readable.

    Returns (C2, info2, labels2, note_str)."""
    keep = set()
    for cn in C.nodes():
        i = info[cn]
        if i["is_cycle"] or i["role"] in ("repeller", "attractor", "isolated"):
            keep.add(cn)
    merged = [cn for cn in C.nodes() if cn not in keep]
    if not merged:
        return C, info, labels, ""

    C2 = nx.DiGraph()
    SUMMARY = ("__transient_cloud__",)
    # collect orig members for relabeling
    new_labels = {}
    new_info = {}
    for cn in keep:
        C2.add_node(cn)
        new_info[cn] = info[cn]
        i = info[cn]
        if i["size"] == 1:
            k = next(iter(i["members"]))
            new_labels[cn] = labels.get(k, str(k))
        else:
            new_labels[cn] = None  # node_text will summarize via members

    C2.add_node(SUMMARY)
    new_info[SUMMARY] = dict(role="transient", is_cycle=False,
                             size=len(merged), members={SUMMARY}, dom_cat=None)
    new_labels[SUMMARY] = f"{len(merged)} transient\ncells\n(flow-through)"

    def remap(n):
        return n if n in keep else SUMMARY

    for (u, v) in C.edges():
        a, b = remap(u), remap(v)
        if a != b:
            C2.add_edge(a, b)

    # recompute roles for kept nodes after merge (degrees changed)
    for cn in C2.nodes():
        if cn == SUMMARY:
            continue
        indeg = C2.in_degree(cn)
        outdeg = C2.out_degree(cn)
        i = new_info[cn]
        if outdeg == 0 and indeg > 0:
            role = "attractor"
        elif indeg == 0 and outdeg > 0:
            role = "repeller"
        elif indeg == 0 and outdeg == 0:
            role = "isolated"
        else:
            role = "transient"
        # a recurrent SCC with no outflow is still an attractor (a real attractor)
        new_info[cn] = dict(i, role=role)

    # store explicit-label override on info so node_text can use it
    for cn in C2.nodes():
        if new_labels[cn] is not None:
            new_info[cn] = dict(new_info[cn], _text=new_labels[cn])

    note = (f"  —  skeleton view: {len(keep)} recurrent/boundary SCCs kept, "
            f"{len(merged)} transient SCCs merged")
    return C2, new_info, labels, note
