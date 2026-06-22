#!/usr/bin/env python3
"""morse_graph_build.py — reachable transition-graph CONSTRUCTION for the
Morse-graph renderer.

Turns a loaded Evident model into a networkx DiGraph of state keys + transition
edges, by one of three honest strategies (the renderer picks which):

  * build_discrete_graph     — the EXACT reachable set for a discrete/mixed
                               finite-reachable program (m.reachable()).
  * build_numeric_orbit_graph — the real forward orbit from the encoded initial
                               state, quantized so a limit cycle closes into one
                               SCC (used when reachable() collapses to a point).
  * build_numeric_scan_graph — when the encoded seed is an UNSTABLE fixed point,
                               probe outward to the scale where dynamics come
                               alive, then walk off-origin orbits into the
                               attractor.

Every node is a state the program REALLY visits — none of these fabricate
off-domain grid seeds. Also hosts the node-label helpers (_key, _abbrev,
_fmt_val, _label_for_key); the renderer re-imports _abbrev/_fmt_val for the
drawing legend.
"""
import networkx as nx


def _key(m, state):
    return tuple(state[v["name"]] for v in m.state_vars)


def _abbrev(name):
    """Short tag for a (possibly dotted) state-var name: 'd.has_key' -> 'key',
    'state.balance' -> 'balance', 'state.mode' -> 'mode'."""
    leaf = name.split(".")[-1]
    for pre in ("has_", "is_"):
        if leaf.startswith(pre):
            leaf = leaf[len(pre):]
            break
    return leaf


def _fmt_val(val):
    if val is True:
        return "T"
    if val is False:
        return "F"
    if isinstance(val, float):
        return f"{val:.0f}"
    return str(val)


def _label_for_key(m, key):
    """Label a state by its RANKED vars (top var spelled out, bools as compact
    flags) rather than an anonymous positional tuple. The first ranked var (the
    most expressive, often the enum) gets named prominence; remaining vars become
    a compact key=val strip so the node reads from the axes of meaning, not
    position."""
    vs = m.state_vars
    if not vs:
        return "()"
    parts = [f"{_abbrev(vs[0]['name'])}={_fmt_val(key[0])}"]
    rest = []
    for v, val in zip(vs[1:], key[1:]):
        if v["kind"] == "bool":
            # only surface the True flags — terse and reads as "what's set"
            if val is True:
                rest.append(_abbrev(v["name"]))
        else:
            rest.append(f"{_abbrev(v['name'])}={_fmt_val(val)}")
    head = parts[0]
    if rest:
        return head + "\n[" + " ".join(rest) + "]"
    return head


def build_discrete_graph(m):
    """Exact reachable graph for discrete / mixed (finite-reachable) programs."""
    states, edges = m.reachable()
    G = nx.DiGraph()
    keys = [_key(m, s) for s in states]
    for k in keys:
        G.add_node(k)
    for (i, j) in edges:
        if keys[i] != keys[j]:
            G.add_edge(keys[i], keys[j])
        else:
            # self-loop = a fixed point; keep it so the SCC is nontrivial
            G.add_edge(keys[i], keys[j])
    return G, {k: _label_for_key(m, k) for k in keys}


def build_numeric_orbit_graph(m, steps=400):
    """Exact recurrence graph for a numeric system whose reachable() collapses
    (e.g. the encoded seed is a fixed point) — walk the ACTUAL forward orbit
    from the encoded initial state and quantize coincident points so a limit
    cycle closes into one SCC.

    This never invents off-domain seeds: every node corresponds to a state the
    program REALLY visits from its initial condition. Returns (G, labels) or
    (None, None) when the orbit does not settle (diverges / stays non-recurrent),
    in which case the caller renders an honest N/A card rather than fabricating
    a grid."""
    numeric_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    base = m.initial_state()
    if base is None:
        return None, None

    traj = m.trajectory(start=base, steps=steps)
    if not traj or len(traj) < 1:
        return None, None

    # Robust quantization step per numeric axis from the orbit's OWN spread
    # (not a hardcoded box). One cell ~ 1/40 of the realized range, min 1.
    cell = {}
    for v in numeric_vars:
        vals = [s[v["name"]] for s in traj]
        rng = (max(vals) - min(vals)) if vals else 0.0
        cell[v["name"]] = max(rng / 40.0, 1.0)

    def quant(state):
        parts = []
        for v in m.state_vars:
            val = state[v["name"]]
            if v["kind"] in ("int", "real"):
                parts.append(round(val / cell[v["name"]]))
            else:
                parts.append(val)
        return tuple(parts)

    G = nx.DiGraph()
    rep = {}   # cell-key -> representative real coords (for label)
    prev = None
    for s in traj:
        k = quant(s)
        rep.setdefault(k, tuple(s[v["name"]] for v in numeric_vars))
        G.add_node(k)
        if prev is not None and prev != k:
            G.add_edge(prev, k)
        elif prev == k:
            G.add_edge(k, k)   # fixed point self-loop
        prev = k

    if G.number_of_nodes() == 0:
        return None, None

    labels = {}
    for k, coords in rep.items():
        labels[k] = "(" + ", ".join(f"{c:.0f}" if isinstance(c, float)
                                    else str(c) for c in coords) + ")"
    return G, labels


def _expanding_orbit_radius(m, xvar, yvar, center):
    """Probe OUTWARD from the fixed point `center` along +x to find the smallest
    radius at which the dynamics expand into a non-trivial orbit (length > 2),
    rather than sitting at the fixed point. Returns that radius, or None if no
    probe out to a generous cap expands. Mirrors the shared probe the other
    numeric renderers use (orbit_scatter / phase_portrait) — self-scaling per
    program instead of a hardcoded box."""
    cx = center.get(xvar["name"], 0)
    cy = center.get(yvar["name"], 0)
    r = 1
    while r <= 1 << 16:          # generous cap; geometric so ~17 probes max
        seed = {v["name"]: center.get(v["name"], 0) for v in m.state_vars}
        seed[xvar["name"]] = int(round(cx + r))
        seed[yvar["name"]] = int(round(cy))
        if len(m.trajectory(start=seed, steps=64)) > 2:
            return r
        r *= 2
    return None


def build_numeric_scan_graph(m, steps=400):
    """Recurrence graph for a numeric system whose ENCODED initial state is a
    degenerate fixed point (e.g. vanderpol's solver-picked (0,0) — the UNSTABLE
    equilibrium), so walking that single orbit reveals no dynamics. We probe
    outward from the fixed point (the same seed-scan the other numeric renderers
    use) to discover the scale at which the dynamics come alive, then walk real
    forward orbits from a spread of off-origin seeds at that scale and quantize
    coincident points so the limit cycle closes into one SCC.

    Every node is still a state the program REALLY visits — we only changed the
    STARTING point from the dead fixed point to off-origin seeds the program's
    own dynamics carry into the attractor. Returns (G, labels, nseeds) or
    (None, None, 0) when no seed expands (a genuine fixed point)."""
    numeric_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    if len(numeric_vars) < 2:
        return None, None, 0
    xvar, yvar = numeric_vars[0], numeric_vars[1]
    center = m.initial_state() or {v["name"]: 0 for v in m.state_vars}
    cx = center.get(xvar["name"], 0)
    cy = center.get(yvar["name"], 0)
    r = _expanding_orbit_radius(m, xvar, yvar, center)
    if r is None:
        return None, None, 0

    # Spread of starts at the discovered scale: along each axis + an off-axis
    # diagonal, so a closed orbit is entered from several angles and the
    # recurrent set is well covered.
    offsets = [(r, 0), (0, r), (-r, r), (r // 2, 0)]
    orbits = []
    for dx, dy in offsets:
        seed = {v["name"]: center.get(v["name"], 0) for v in m.state_vars}
        seed[xvar["name"]] = int(round(cx + dx))
        seed[yvar["name"]] = int(round(cy + dy))
        orb = m.trajectory(start=seed, steps=steps)
        if len(orb) > 2:
            orbits.append(orb)
    if not orbits:
        return None, None, 0

    # Robust quantization step per numeric axis from the orbits' OWN spread (not
    # a hardcoded box). One cell ~ 1/40 of the realized range, min 1.
    allpts = [s for orb in orbits for s in orb]
    cell = {}
    for v in numeric_vars:
        vals = [s[v["name"]] for s in allpts]
        rng = (max(vals) - min(vals)) if vals else 0.0
        cell[v["name"]] = max(rng / 40.0, 1.0)

    def quant(state):
        parts = []
        for v in m.state_vars:
            val = state[v["name"]]
            if v["kind"] in ("int", "real"):
                parts.append(round(val / cell[v["name"]]))
            else:
                parts.append(val)
        return tuple(parts)

    G = nx.DiGraph()
    rep = {}
    for orb in orbits:
        prev = None
        for s in orb:
            k = quant(s)
            rep.setdefault(k, tuple(s[v["name"]] for v in numeric_vars))
            G.add_node(k)
            # Only emit an edge on a genuine cell-to-cell MOVE. A quantized
            # continuous orbit that lingers in one cell for a step is NOT a
            # fixed point — emitting a self-loop there would mislabel hundreds
            # of cells as recurrent. Real recurrence shows up as the orbit
            # returning to a cell later (closing the limit-cycle SCC).
            if prev is not None and prev != k:
                G.add_edge(prev, k)
            prev = k

    if G.number_of_nodes() <= 1:
        return None, None, 0

    labels = {}
    for k, coords in rep.items():
        labels[k] = "(" + ", ".join(f"{c:.0f}" if isinstance(c, float)
                                    else str(c) for c in coords) + ")"
    return G, labels, len(orbits)
