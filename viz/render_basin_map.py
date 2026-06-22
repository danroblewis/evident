#!/usr/bin/env python3
"""render_basin_map.py — basin-of-attraction map for ANY Evident IR.

Usage:
    python3 viz/render_basin_map.py <smt2> <schema> <out_path>

Idea: every start state, when you keep applying the transition, eventually
settles into a *terminal* set — a fixed point, a limit cycle, or a terminal
strongly-connected component (SCC). The "basin" of a terminal is every start
state that flows there. This renderer colors a 2-D projection of state space
by *which terminal* each start ends up in.

  * DISCRETE programs (all bool/enum/string): we have the exact reachable
    graph from evident_viz.reachable(). We condense it into SCCs, find the
    terminal SCCs (no outgoing edge to another SCC), and color every reachable
    state by the terminal SCC it can reach. Two state axes are chosen for the
    scatter; the rest collapse (a point may carry several colors -> drawn as a
    small multi-wedge, but we keep it simple with the dominant terminal).

  * NUMERIC programs: the plotting / seed / grid domain is derived from the
    program's REACHABLE states (model.axis_bounds / model.reachable / the
    iterated probe-visited set) — NEVER a hardcoded ±3000 box. The old code
    seeded a fixed ±3200 plane, which fabricated cycles/basins/fixed-point stars
    on programs whose state never leaves a tiny region (e.g. a counter that runs
    to 10 and halts). The honest routing now is:
      - reachable set is FINITE with ≥2 states  -> plot ONLY those real states,
        colored by the terminal SCC each can reach (the exact-graph basin, same
        machinery the discrete path uses). No grid, no invented plane.
      - reachable set is a single fixed point   -> probe off-init seeds to look
        for a surrounding continuous attractor (e.g. van der Pol's limit cycle,
        whose init sits exactly on the unstable origin so BFS-from-init sees one
        point). If probes reveal an attractor, grid + plot over the ACTUAL
        VISITED extent (van der Pol: ~±2.5 in real units). If nothing surrounds
        the fixed point, render an honest N/A card rather than a fabricated plane.

  * MIXED programs: same as numeric but enum/bool axes are projected to
    ordinals (enum -> index in variant list, bool -> 0/1) when chosen as an
    axis, and held at their initial value otherwise.

Everything dynamic comes from querying the transition via evident_viz; nothing
about any specific program is hardcoded, and no axis is gridded outside the
reachable / actually-visited set.
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load  # noqa: E402

import matplotlib  # noqa: E402
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
from matplotlib.patches import Patch  # noqa: E402
import numpy as np  # noqa: E402

# Shared, path-agnostic primitives live in the sibling support module.
from basin_support import (  # noqa: E402
    PALETTE, _placeholder, _choose_axes, _choose_facet, _ordinal, _axis_label,
    _tarjan_scc,
)
# The numeric/mixed basin path lives in its own module (this file keeps the
# dispatch + the exact-graph discrete path).
from basin_numeric import numeric_basins  # noqa: E402
# Interactive hover-overlay sidecar (#184 increment 3). basin_map ALWAYS saves
# with bbox_inches="tight", so per-dot fractions use the tight-bbox mapping.
from overlay_points import write_points, tight_fraction  # noqa: E402


# --------------------------------------------------------------------------
# DISCRETE: exact reachable graph -> SCC condensation -> terminal basins
# --------------------------------------------------------------------------


def _discrete_basins(m, out_path):
    states, edges = m.reachable()
    return _discrete_basins_on(m, out_path, states, edges)


def _condense_terminals(n, edges):
    """SCC-condense a reachable graph (n nodes, `edges` index pairs) and compute,
    for each SCC, the set of terminal SCCs it can reach. Returns
    (eset, sccs, scc_of, term_ids, term_index, reach_term) — the de-duped edge
    set, the SCC membership lists, the node->scc map, the terminal-SCC ids +
    their dense index, and the reach_term[scc] -> set(terminal-scc) map."""
    adj = [[] for _ in range(n)]
    eset = set()
    for a, b in edges:
        if a != b and (a, b) not in eset:
            adj[a].append(b)
            eset.add((a, b))

    sccs = _tarjan_scc(n, adj)
    scc_of = [0] * n
    for sid, comp in enumerate(sccs):
        for node in comp:
            scc_of[node] = sid
    nscc = len(sccs)

    # condensation DAG
    cadj = [set() for _ in range(nscc)]
    for a, b in eset:
        if scc_of[a] != scc_of[b]:
            cadj[scc_of[a]].add(scc_of[b])

    terminal = [len(cadj[s]) == 0 for s in range(nscc)]
    term_ids = [s for s in range(nscc) if terminal[s]]
    term_index = {s: i for i, s in enumerate(term_ids)}

    # For each SCC, which terminal SCC(s) can it reach? Iterate to fixpoint over
    # the condensation DAG (a few passes suffice; done robustly).
    reach_term = [set() for _ in range(nscc)]
    changed = True
    while changed:
        changed = False
        for s in range(nscc):
            before = len(reach_term[s])
            if terminal[s]:
                reach_term[s].add(s)
            for t in cadj[s]:
                reach_term[s] |= reach_term[t]
            if len(reach_term[s]) != before:
                changed = True
    return eset, sccs, scc_of, term_ids, term_index, reach_term


def _discrete_basins_on(m, out_path, states, edges, projected_out=None):
    """Exact terminal-SCC basin map over a PRE-COMPUTED reachable graph (states +
    edges). Used both for genuinely-discrete programs (states from m.reachable())
    and for the counter-projected path (states from _projected_reachable, with the
    free-running counter collapsed out — `projected_out` names it for the title)."""
    if not states:
        _placeholder(out_path, m.fsm,
                     "no reachable states (initial_state() is None)")
        return "discrete: empty"

    n = len(states)
    eset, sccs, scc_of, term_ids, term_index, reach_term = \
        _condense_terminals(n, edges)
    nscc = len(sccs)

    def basin_color_idx(node):
        rt = reach_term[scc_of[node]]
        if not rt:
            return -1
        if len(rt) > 1:        # reaches MULTIPLE terminals — it is NOT in one basin (Ana #76).
            return -2          # a nondeterministic state can flow to >1 attractor; don't fabricate.
        return term_index[next(iter(rt))]

    # axes (channel API: position is the top-ranked channel)
    axes = _choose_axes(m)
    if len(axes) == 0:
        _placeholder(out_path, m.fsm, "no state variables to project")
        return "discrete: no axes"
    ax_x = axes[0]
    ax_y = axes[1] if len(axes) > 1 else None

    # FACET by a low-cardinality categorical that isn't an axis — adds a 3rd
    # dimension as small multiples instead of clobbering the 2-axis projection.
    facet_var, facet_vals = _choose_facet(m, axes, states)

    # project every state onto the chosen axes + basin color, once.
    xs = np.array([_ordinal(m, ax_x, st[ax_x["name"]]) for st in states], float)
    ys = np.array([_ordinal(m, ax_y, st[ax_y["name"]]) if ax_y else 0.0
                   for st in states], float)
    cidx = np.array([basin_color_idx(node) for node in range(n)], int)
    rng = np.random.default_rng(7)
    jx = (rng.random(n) - 0.5) * 0.22
    jy = (rng.random(n) - 0.5) * 0.22

    def basin_label(ci):
        if ci == -2:           # multi-basin: reaches >1 attractor (nondeterministic) — honest, not faked
            return "#d29922", "→ MULTIPLE attractors (nondeterministic — not a single basin)"
        if ci < 0:
            return "#000000", "no terminal"
        color = PALETTE[ci % len(PALETTE)]
        rep_scc = term_ids[ci]
        rep_node = sccs[rep_scc][0]
        cyc = "cycle" if len(sccs[rep_scc]) > 1 else "fixed pt"
        return color, f"→ {m.label(states[rep_node])} ({cyc})"

    overlay = []   # (ax, dot_x, dot_y, full_state) per plotted dot — hover sidecar

    def draw(ax, node_ids):
        nodeset = set(node_ids)
        for a, b in eset:
            if a in nodeset and b in nodeset:
                ax.plot([xs[a] + jx[a], xs[b] + jx[b]],
                        [ys[a] + jy[a], ys[b] + jy[b]],
                        color="#cccccc", lw=0.5, alpha=0.5, zorder=1)
        for ci in sorted(set(cidx[node_ids])):
            mask = np.array([nd for nd in node_ids if cidx[nd] == ci], int)
            color, _lbl = basin_label(ci)
            ax.scatter(xs[mask] + jx[mask], ys[mask] + jy[mask], s=90,
                       color=color, edgecolors="black", linewidths=0.5,
                       zorder=3)
        overlay.extend((ax, xs[nd] + jx[nd], ys[nd] + jy[nd], states[nd])
                       for nd in node_ids)
        ax.set_xlabel(_axis_label(ax_x))
        ax.set_ylabel(_axis_label(ax_y) if ax_y else "(single axis)")
        _decorate_enum_ticks(ax, m, ax_x, ax_y)
        ax.grid(True, alpha=0.25)

    # one shared legend covering every terminal basin (faceting splits the nodes
    # across panels, so a per-panel legend would only show that panel's basins).
    def legend_handles():
        h = []
        for ci in sorted(set(cidx)):
            color, lbl = basin_label(ci)
            h.append(Patch(facecolor=color, edgecolor="black", label=lbl))
        return h

    if facet_var is None:
        fig, ax = plt.subplots(figsize=(9, 7))
        draw(ax, list(range(n)))
        ax.legend(handles=legend_handles(), loc="center left",
                  bbox_to_anchor=(1.01, 0.5), fontsize=8,
                  title="terminal basin", frameon=True)
        ax.set_title(f"{m.fsm} — basin_map (discrete: {nscc} SCCs, "
                     f"{len(term_ids)} terminal)", fontsize=13, weight="bold")
        points = tight_fraction(fig, overlay)
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        write_points(out_path, points)
        return (f"discrete: {n} reachable states, {nscc} SCCs, "
                f"{len(term_ids)} terminal basins")

    # one panel per facet value
    npan = len(facet_vals)
    fig, axarr = plt.subplots(1, npan, figsize=(5.0 * npan, 5.6),
                              squeeze=False, sharex=True, sharey=True)
    for i, fv in enumerate(facet_vals):
        node_ids = [nd for nd, st in enumerate(states)
                    if st[facet_var["name"]] == fv]
        disp = fv if facet_var["kind"] != "bool" else ("true" if fv else "false")
        ax = axarr[0][i]
        draw(ax, node_ids)
        ax.set_title(f"{facet_var['name']} = {disp}  ({len(node_ids)} states)",
                     fontsize=11, weight="bold")
    leg = fig.legend(handles=legend_handles(), loc="center left",
                     bbox_to_anchor=(1.0, 0.5), fontsize=8,
                     title="terminal basin", frameon=True)
    fig.suptitle(f"{m.fsm} — basin_map (discrete: {nscc} SCCs, "
                 f"{len(term_ids)} terminal; faceted by {facet_var['name']})",
                 fontsize=13, weight="bold")
    points = tight_fraction(fig, overlay)
    fig.savefig(out_path, dpi=120, bbox_inches="tight",
                bbox_extra_artists=(leg,))
    plt.close(fig)
    write_points(out_path, points)
    return (f"discrete: {n} reachable states, {nscc} SCCs, "
            f"{len(term_ids)} terminal basins; faceted by "
            f"{facet_var['name']} ({npan} panels)")


def _decorate_enum_ticks(ax, m, ax_x, ax_y):
    if ax_x["kind"] == "enum":
        vs = m.enum_variants[ax_x["name"]]
        ax.set_xticks(range(len(vs)))
        ax.set_xticklabels(vs, rotation=30, ha="right", fontsize=8)
    elif ax_x["kind"] == "bool":
        ax.set_xticks([0, 1])
        ax.set_xticklabels(["false", "true"])
    if ax_y is not None:
        if ax_y["kind"] == "enum":
            vs = m.enum_variants[ax_y["name"]]
            ax.set_yticks(range(len(vs)))
            ax.set_yticklabels(vs, fontsize=8)
        elif ax_y["kind"] == "bool":
            ax.set_yticks([0, 1])
            ax.set_yticklabels(["false", "true"])


# --------------------------------------------------------------------------
# NUMERIC / MIXED: grid of seeds -> iterate to convergence -> cluster regions
# --------------------------------------------------------------------------
# The whole numeric/mixed path lives in basin_numeric.py; `render()` dispatches
# to it. The discrete-graph helpers above (_discrete_basins / _discrete_basins_on)
# and _decorate_enum_ticks are passed in so the import stays acyclic.
# --------------------------------------------------------------------------
def render(smt2, schema, out_path):
    m = load(smt2, schema)
    if m.is_discrete():
        return _discrete_basins(m, out_path)
    return numeric_basins(m, out_path, _discrete_basins, _discrete_basins_on,
                          _decorate_enum_ticks)


def main(argv):
    if len(argv) != 4:
        print("usage: render_basin_map.py <smt2> <schema> <out_path>",
              file=sys.stderr)
        return 2
    smt2, schema, out_path = argv[1], argv[2], argv[3]
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    try:
        note = render(smt2, schema, out_path)
        print(f"[basin_map] {out_path}: {note}")
    except Exception as e:
        import traceback
        traceback.print_exc()
        m_fsm = "unknown"
        try:
            m_fsm = load(smt2, schema).fsm
        except Exception:
            pass
        _placeholder(out_path, m_fsm, f"render error: {type(e).__name__}: {e}")
        print(f"[basin_map] {out_path}: placeholder ({e})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
