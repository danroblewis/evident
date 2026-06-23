#!/usr/bin/env python3
"""fixedpoint_basins.py — all-initial-conditions seeding + attractor basin coloring
for render_fixedpoint_map.py.

The DATA + ANALYSIS layer for the fixed-point map's "which attractor does each state
converge to?" partition. Two concerns live here, both kept OUT of the renderer so the
renderer is pure plotting:

  * `sample_all_conditions` — seed from ALL initial conditions (the global transition
    graph over every valid carried assignment) when finitely enumerable, else fall back
    to the from-init reachable / numeric-grid sample.
  * `basin_colors` / `_scatter_basins` — color every state by the terminal SCC (the
    attractor) it flows to, reusing basin_map's SCC condensation read-only.
"""
from fixedpoint_states import sample_states
# The terminal-SCC basin condensation is owned by basin_map / basin_support. We REUSE
# it READ-ONLY: a fixed-point map seeded from ALL initial conditions IS a basin partition
# with the attractors drawn as stars/orbits on top.
from render_basin_map import _condense_terminals
from basin_support import PALETTE

GREY, AMBER = "#9aa0b5", "#d29922"


# --------------------------------------------------------------------------
# seeding: ALL initial conditions (the basin partition) when enumerable
# --------------------------------------------------------------------------
def sample_all_conditions(model):
    """Return (states, mode, edges) seeded from ALL initial conditions when the
    discrete carried product is finitely enumerable, else the from-init fallback.

    A fixed-point / basin map's job is to partition the WHOLE state space by which
    attractor each STARTING state flows to. The from-init `reachable()` orbit is a
    SINGLE trajectory for a deterministic FSM — it only ever shows the one basin the
    seed falls into, so a bistable system would look mono-stable. So we root on
    `full_state_graph()` (every valid carried assignment, ignoring is_first_tick) and
    SCC-condense THAT. The dynamics (`successors`) and dedup (`_key`) are identical to
    `reachable()`; only the ROOT SET differs (all states vs the single seed).

    Falls back to `sample_states()` (from-init reachable, or numeric grid scan) when
    the model isn't finitely enumerable / exceeds the cap — `discrete=False`/`capped`."""
    states, idx_edges, info = model.full_state_graph(limit=5000)
    if info["discrete"] and not info["capped"] and states:
        # full_state_graph returns (from_index, to_index) edges; this renderer's
        # _draw_edges / find_attractors consume (state_dict, state_dict) pairs (the
        # same shape sample_states' reachable path returns), so materialize them.
        edges = [(states[i], states[j]) for i, j in idx_edges]
        return states, "all-conditions", edges
    return sample_states(model)          # from-init reachable / numeric grid fallback


# --------------------------------------------------------------------------
# basin coloring: which attractor does each state converge to?
# --------------------------------------------------------------------------
def basin_colors(states, edges):
    """Map each state index -> a basin color (the terminal SCC it flows to).

    `edges` are (state_dict, state_dict) pairs (the renderer's edge shape). We index
    them by object identity (`id`) and reuse basin_map's `_condense_terminals`: SCC-
    condense the transition graph, find terminal SCCs (the attractors), and for each
    state resolve which terminal it reaches. Returns (color_of[i], n_terminals):
      * one terminal reached  -> PALETTE[that terminal]  (it's in that basin)
      * >1 terminal reachable -> the multi-basin amber (nondeterministic — not one basin)
      * no terminal reachable -> None (grey; transient with no sink in-sample)
    Empty / no-edge graphs return (all-None, 0) so the caller draws plain dots."""
    n = len(states)
    if n == 0 or not edges:
        return [None] * n, 0
    idx = {id(s): i for i, s in enumerate(states)}
    idx_edges = [(idx[id(a)], idx[id(b)]) for a, b in edges
                 if id(a) in idx and id(b) in idx]
    if not idx_edges:
        return [None] * n, 0
    _eset, _sccs, scc_of, term_ids, term_index, reach_term = \
        _condense_terminals(n, idx_edges)
    colors = [None] * n
    for i in range(n):
        rt = reach_term[scc_of[i]]
        if not rt:
            continue
        if len(rt) > 1:
            colors[i] = AMBER            # multi-basin: reaches >1 attractor (honest, not faked)
        else:
            colors[i] = PALETTE[term_index[next(iter(rt))] % len(PALETTE)]
    return colors, len(term_ids)


def basin_of(states, edges):
    """The renderer's per-state basin lookup: {id(state) -> color} via `basin_colors`.
    Keyed by object identity so draw_panel can resolve each faceted sub-state's color
    back to the global partition. Empty when there's no edge graph (numeric grid scan)."""
    colors, _n = basin_colors(states, edges)
    return {id(states[i]): colors[i] for i in range(len(states))}


def scatter_basins(ax, states, proj, basin_of):
    """Background dots colored by the attractor each STATE converges to (the basin
    partition). One scatter call per distinct basin color so the legend reads one
    entry per terminal. AMBER marks states that reach >1 attractor (nondeterministic
    — not a single basin); None-basin states (transient, no sink in sample) draw GREY."""
    buckets = {}
    for s in states:
        c = basin_of.get(id(s)) or GREY
        buckets.setdefault(c, []).append(proj(s))

    def rank(c):                          # real basin colors first, amber/grey last
        return (2 if c == GREY else 1 if c == AMBER else 0, c)

    bi = 0
    for c in sorted(buckets, key=rank):
        pts = buckets[c]
        if c == GREY:
            lbl = f"no attractor in sample ({len(pts)})"
        elif c == AMBER:
            lbl = f"→ MULTIPLE attractors ({len(pts)})"
        else:
            bi += 1
            lbl = f"basin {bi} ({len(pts)})"
        ax.scatter([p[0] for p in pts], [p[1] for p in pts], s=44, c=c,
                   alpha=0.9, edgecolors="white", linewidths=0.4,
                   zorder=1, label=lbl)
