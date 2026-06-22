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


# A qualitative palette big enough for the terminal sets we see.
PALETTE = [
    "#4477AA", "#EE6677", "#228833", "#CCBB44", "#66CCEE",
    "#AA3377", "#BBBBBB", "#FF8C00", "#117733", "#882255",
    "#44AA99", "#999933", "#332288", "#DDCC77", "#CC6677",
]


def _placeholder(out_path, fsm, reason):
    fig, ax = plt.subplots(figsize=(8, 6))
    ax.axis("off")
    ax.text(0.5, 0.58, "N/A for basin_map", ha="center", va="center",
            fontsize=20, weight="bold", color="#aa3333")
    ax.text(0.5, 0.44, reason, ha="center", va="center", fontsize=12,
            color="#333333", wrap=True)
    ax.set_title(f"{fsm} — basin_map", fontsize=14, weight="bold")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


# --------------------------------------------------------------------------
# axis / facet selection via the channel-mapping API
# --------------------------------------------------------------------------
def _choose_axes(m):
    """Return up to two state-var dicts for the x,y projection, via the channel
    API. The basin grid wants NUMERIC axes when available (position is the
    top-ranked channel and decodes quantitative best), so we prefer numeric_vars
    and fall back to assign_channels for the remaining slots (enum/bool ordinals).
    """
    numeric = m.numeric_vars
    chans = m.assign_channels(["x", "y"])
    axes = []
    # fill from numeric first (best for a continuous seed grid)
    for v in numeric:
        if v not in axes:
            axes.append(v)
        if len(axes) == 2:
            return axes
    # top up from the channel assignment (categorical ordinals as a last resort)
    for ch in ("x", "y"):
        v = chans[ch]
        if v is not None and v not in axes:
            axes.append(v)
        if len(axes) == 2:
            break
    return axes


def _choose_facet(m, axes, states):
    """Pick the SUITABLE facet variable via the shared faceting guard
    (m.facet_var): a low-cardinality categorical that stays ~constant within a
    run. Faceting by a var that CHANGES along the trajectory (e.g. vending's
    state.mode on the limit cycle) splits the dynamics across panels — the guard
    rejects those. Must not already be an axis. Returns (var, values) or
    (None, None).

    The returned `values` are ONLY those the facet var actually TAKES across the
    reachable `states` — a declared enum variant that no reachable state visits
    (e.g. find's state.s5 ∈ {Unseen, Pending, Visited} but every reachable state
    is Unseen) would otherwise render a permanently-empty panel. And if fewer
    than 2 distinct values occur, the var is constant: it carries zero faceting
    information, so DON'T facet at all."""
    axis_names = {a["name"] for a in axes}

    def values_of(v):
        if v["kind"] == "enum":
            return m.enum_variants[v["name"]]
        if v["kind"] == "bool":
            return [False, True]
        return None

    v = m.facet_var()
    if v is None or v["name"] in axis_names:
        return None, None
    vals = values_of(v)
    if vals is None:
        return None, None
    # Keep only declared values that some reachable state actually visits, in the
    # declared order. Suppresses empty panels (and rejects a constant facet var).
    present = {st[v["name"]] for st in states if v["name"] in st}
    vals = [x for x in vals if x in present]
    if len(vals) < 2:
        return None, None
    return v, vals


def _ordinal(m, var, value):
    """Project a state value onto a real number for plotting."""
    k = var["kind"]
    if k in ("int", "real"):
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        variants = m.enum_variants[var["name"]]
        return float(variants.index(value))
    if k == "string":
        return 0.0
    return 0.0


def _axis_label(var):
    return f"{var['name']}  [{var['kind']}]"


# --------------------------------------------------------------------------
# DISCRETE: exact reachable graph -> SCC condensation -> terminal basins
# --------------------------------------------------------------------------
def _tarjan_scc(n, adj):
    """Return list of SCCs (each a list of node ids), via iterative Tarjan."""
    index = [None] * n
    low = [0] * n
    on_stack = [False] * n
    stack = []
    sccs = []
    counter = [0]

    for root in range(n):
        if index[root] is not None:
            continue
        work = [(root, 0)]
        while work:
            v, pi = work[-1]
            if pi == 0:
                index[v] = low[v] = counter[0]
                counter[0] += 1
                stack.append(v)
                on_stack[v] = True
            recursed = False
            neighbors = adj[v]
            while pi < len(neighbors):
                w = neighbors[pi]
                pi += 1
                if index[w] is None:
                    work[-1] = (v, pi)
                    work.append((w, 0))
                    recursed = True
                    break
                elif on_stack[w]:
                    low[v] = min(low[v], index[w])
            if recursed:
                continue
            work[-1] = (v, pi)
            # post-process v: update parent low later; first relax children lows
            for w in neighbors:
                if on_stack[w]:
                    low[v] = min(low[v], low[w])
            if low[v] == index[v]:
                comp = []
                while True:
                    w = stack.pop()
                    on_stack[w] = False
                    comp.append(w)
                    if w == v:
                        break
                sccs.append(comp)
            work.pop()
            if work:
                pv = work[-1][0]
                low[pv] = min(low[pv], low[v])
    return sccs


def _discrete_basins(m, out_path):
    states, edges = m.reachable()
    return _discrete_basins_on(m, out_path, states, edges)


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

    # For each SCC, which terminal SCC(s) can it reach? Pick a deterministic
    # "dominant" terminal (smallest reachable terminal id) for the basin color.
    # Reverse-topo over condensation.
    reach_term = [set() for _ in range(nscc)]
    # iterate to fixpoint (DAG, so a few passes suffice; do it robustly)
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
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
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
    fig.savefig(out_path, dpi=120, bbox_inches="tight",
                bbox_extra_artists=(leg,))
    plt.close(fig)
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
def _state_key_for_cluster(m, st, axes):
    """A coarse key for grouping attractor states (used to find regions)."""
    return tuple(round(_ordinal(m, v, st[v["name"]])) for v in m.state_vars)


def _iterate_to_attractor(m, seed_state, cache, resolved, max_steps=600):
    """Follow ONE successor chain to its attractor and return that attractor's
    phase-invariant SIGNATURE. Two memos make a grid of seeds tractable:
      `cache`    : state-key -> successor state (avoids re-solving z3).
      `resolved` : state-key -> attractor signature (once a chain settles, every
                   state along it is tagged with the attractor it reaches, so a
                   later chain that touches any of them short-circuits instantly).
    The first chain pays the full cost of walking onto the attractor; all later
    chains merge onto already-resolved territory and stop."""
    cur = seed_state
    history = []
    seen = {}
    for step in range(max_steps):
        k = m._key(cur)
        if k in resolved:
            sig = resolved[k]
            for h in history:
                resolved[m._key(h)] = sig
            return sig
        if k in seen:                       # closed a cycle
            cycle = history[seen[k]:]
            sig = _attractor_signature(m, cycle)
            for h in history:
                resolved[m._key(h)] = sig
            return sig
        seen[k] = step
        history.append(cur)
        if k in cache:
            nxt = cache[k]
        else:
            nxt = m.successor(cur)
            cache[k] = nxt
        if nxt is None:                     # dead-end / fixed point
            sig = _attractor_signature(m, [cur])
            for h in history:
                resolved[m._key(h)] = sig
            return sig
        cur = nxt
    # ran out of steps: signature from the tail (best effort)
    sig = _attractor_signature(m, history[-min(len(history), 12):])
    for h in history:
        resolved[m._key(h)] = sig
    return sig


def _baseline_fn(m):
    """Baseline value for a non-axis var: initial_state if present, else a neutral
    default (0 / false / first variant / "")."""
    init = m.initial_state()

    def baseline(v):
        if init is not None and v["name"] in init:
            return init[v["name"]]
        k = v["kind"]
        if k in ("int", "real"):
            return 0
        if k == "bool":
            return False
        if k == "enum":
            return m.enum_variants[v["name"]][0]
        if k == "string":
            return ""
        return 0

    return baseline


# Per-model numeric domain, derived from the program's REACHABLE / VISITED set.
# Each entry is name -> (lo, hi). NEVER a hardcoded box: the domain is what the
# dynamics actually touch. For a degenerate-from-init fixed point (van der Pol's
# unstable origin) we widen by iterating off-init probe seeds and taking the
# extent those orbits VISIT — so the grid covers the limit cycle, but only out to
# where the program's own dynamics go, not a fabricated ±3000 plane.
_DOMAIN_CACHE = {}

# Off-init probe DIRECTIONS, expressed as fractions of a search radius. They are
# NOT absolute coordinates: the radius is grown geometrically until an orbit
# escapes the single fixed point, so we discover the attractor's scale from the
# dynamics instead of hardcoding it.
_PROBE_DIRS = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0),
               (0.7, 0.7), (-0.7, 0.7), (0.15, 0.0)]


def _numeric_axes(m):
    return [v for v in m.state_vars if v["kind"] in ("int", "real")]


def _visited_extent_from_probes(m, ax_x, ax_y):
    """Grow off-init probe seeds outward (geometric radius) and follow each
    orbit; return {name: (lo, hi)} of every numeric state var the orbits VISIT.
    Used only when the reachable-from-init set is a single fixed point but the
    program may have a surrounding continuous attractor (van der Pol). Returns {}
    if no probe escapes — i.e. a genuine lone fixed point, route to N/A."""
    init = m.initial_state() or {}
    nums = _numeric_axes(m)
    seen_vals = {v["name"]: [] for v in nums}
    escaped = False
    # axis-value-space directions for the two chosen axes; other numeric axes
    # stay at their init (the probe perturbs only the plotted plane).
    dirs = []
    for dx, dy in _PROBE_DIRS:
        dirs.append((dx, dy))
    for radius in (16, 64, 256, 1024, 2867):
        for dx, dy in dirs:
            st = dict(init) if init else {v["name"]: 0 for v in m.state_vars}
            st[ax_x["name"]] = int(round(radius * dx))
            if ax_y is not None and ax_y["kind"] in ("int", "real"):
                st[ax_y["name"]] = int(round(radius * dy))
            cur = st
            local = []
            keyseen = set()
            for _ in range(500):
                nxt = m.successor(cur)
                if nxt is None:
                    break
                k = m._key(nxt)
                if k in keyseen:
                    break
                keyseen.add(k)
                local.append(nxt)
                cur = nxt
            # did this orbit move away from the seed AND not collapse to a point?
            if local:
                for v in nums:
                    vals = [s[v["name"]] for s in local]
                    seen_vals[v["name"]].extend(vals)
                # escape = the orbit visited more than one distinct state
                if len({m._key(s) for s in local}) > 1:
                    escaped = True
        if escaped:
            break
    if not escaped:
        return {}
    out = {}
    for v in nums:
        vals = seen_vals[v["name"]]
        if vals and max(vals) > min(vals):
            out[v["name"]] = (float(min(vals)), float(max(vals)))
    return out


def _numeric_domain(m, ax_x, ax_y):
    """name -> (lo, hi) for each numeric state var, derived from the reachable /
    visited set — the honest plotting & seeding domain. Order of preference:
      1. model.axis_bounds(name) — the padded reachable extent (correct whenever
         the reachable set spans a real range: van der Pol's limit cycle when the
         sample walks it, vending balance 0..3, etc.).
      2. probe-visited extent — when the reachable-from-init set is a single fixed
         point, grow off-init orbits and take what they actually VISIT.
    A var with neither (a genuinely lone fixed point on that axis) is absent from
    the dict; callers treat that as 'no meaningful grid here'."""
    key = id(m)
    if key in _DOMAIN_CACHE:
        return _DOMAIN_CACHE[key]
    dom = {}
    nums = _numeric_axes(m)
    # Which numeric vars actually VARY in the reachable sample? axis_bounds pads a
    # flat (lo==hi) fixed-point var to ±1, which would masquerade as a real span —
    # so trust it only where the sample has ≥2 distinct values.
    sample = m._sample_states()
    varies = {}
    for v in nums:
        vals = [s[v["name"]] for s in sample if v["name"] in s]
        varies[v["name"]] = len(set(vals)) >= 2
    degenerate = []
    for v in nums:
        b = m.axis_bounds(v["name"])
        if b is not None and varies[v["name"]]:      # a real reachable span
            dom[v["name"]] = b
        else:
            degenerate.append(v)                     # flat in sample (fixed pt)
    if degenerate:                                   # try to find a surrounding
        probed = _visited_extent_from_probes(m, ax_x, ax_y)   # continuous attractor
        for v in degenerate:
            if v["name"] in probed:
                lo, hi = probed[v["name"]]
                pad = (hi - lo) * 0.08
                dom[v["name"]] = (lo - pad, hi + pad)
    _DOMAIN_CACHE[key] = dom
    return dom


def _axis_grid(m, v, n, dom):
    """Grid samples + display bounds for one axis variable, scaled to the
    reachable/visited domain `dom` (NEVER a hardcoded box). A numeric axis with no
    domain entry (a lone fixed point) gets a tiny window around its init value —
    there is nothing to grid, and the caller will have routed to N/A anyway."""
    k = v["kind"]
    if k in ("int", "real"):
        b = dom.get(v["name"])
        if b is None:
            init = m.initial_state() or {}
            c = float(init.get(v["name"], 0))
            return [c], (c - 1.0, c + 1.0)
        lo, hi = b
        steps = min(n, int(hi - lo) + 1) if k == "int" else n
        return list(np.linspace(lo, hi, max(2, steps))), (lo, hi)
    if k == "bool":
        return [False, True], (-0.5, 1.5)
    if k == "enum":
        variants = m.enum_variants[v["name"]]
        return list(variants), (-0.5, len(variants) - 0.5)
    return [""], (-0.5, 0.5)


def _panel_basins(m, ax_x, ax_y, fixed, cache, resolved, dom):
    """Compute one panel of the basin map: seed a grid over the two axes, scaled
    to the reachable/visited `dom` (NO hardcoded box, NO off-plane probes — the
    grid already spans the actual attractor extent), holding `fixed`
    (name->value) constant, iterate each seed to its attractor signature. Returns
    (seeds, sigs) where seeds are (xv, yv) axis-value pairs. `cache`/`resolved`
    are shared across panels so the z3 work is paid once."""
    baseline = _baseline_fn(m)
    nx = 14 if ax_x["kind"] in ("int", "real") else None
    ny = 14 if (ax_y and ax_y["kind"] in ("int", "real")) else None
    gx, _bx = _axis_grid(m, ax_x, nx or 8, dom)
    if ax_y is not None:
        gy, _by = _axis_grid(m, ax_y, ny or 8, dom)
    else:
        gy = [0]

    def mk_state(xv, yv):
        st = {v["name"]: baseline(v) for v in m.state_vars}
        for nm, val in fixed.items():
            st[nm] = val
        st[ax_x["name"]] = int(round(xv)) if ax_x["kind"] == "int" else xv
        if ax_y is not None:
            st[ax_y["name"]] = int(round(yv)) if ax_y["kind"] == "int" else yv
        return st

    seeds, sigs = [], []
    for xv in gx:
        for yv in gy:
            sig = _iterate_to_attractor(m, mk_state(xv, yv), cache, resolved)
            seeds.append((xv, yv))
            sigs.append(sig)
    return seeds, sigs


def _draw_panel(ax, m, ax_x, ax_y, seeds, labels, centers, show_legend, dom):
    bx = _axis_grid(m, ax_x, 8, dom)[1]
    by = _axis_grid(m, ax_y, 8, dom)[1] if ax_y is not None else (-0.5, 0.5)
    xs = np.array([_ordinal(m, ax_x, s[0]) for s in seeds], float)
    if ax_y is not None:
        ys = np.array([_ordinal(m, ax_y, s[1]) for s in seeds], float)
    else:
        ys = np.zeros(len(seeds))
    labels = np.array(labels, int)

    marker_size = 36 if (ax_x["kind"] in ("int", "real")) else 140
    for ci in sorted(set(labels)):
        mask = labels == ci
        color = PALETTE[ci % len(PALETTE)]
        desc = _describe_region(m, centers[ci])
        ax.scatter(xs[mask], ys[mask], s=marker_size, color=color,
                   edgecolors="none", marker="s",
                   label=f"basin {ci}: {desc}")

    if ax_x["kind"] in ("int", "real"):
        cxs, cys = [], []
        for cvec in centers:
            cx, cy = _center_axis_coords(m, cvec, ax_x, ax_y)
            if cx is not None and bx[0] <= cx <= bx[1]:
                cxs.append(cx)
                cys.append(cy if cy is not None else 0.0)
        if cxs:
            ax.scatter(cxs, cys, s=60, color="black", marker="*",
                       zorder=5, label="attractor")

    ax.set_xlabel(_axis_label(ax_x))
    ax.set_ylabel(_axis_label(ax_y) if ax_y else "(single axis)")
    ax.set_xlim(bx)
    if ax_y is not None:
        ax.set_ylim(by)
    _decorate_enum_ticks(ax, m, ax_x, ax_y)
    if show_legend:
        ax.legend(loc="center left", bbox_to_anchor=(1.01, 0.5), fontsize=8,
                  title="attractor basin", frameon=True)


# --------------------------------------------------------------------------
# Monotone-counter projection: recover a FINITE discrete basin that a
# free-running clock has inflated past enumeration.
# --------------------------------------------------------------------------
# A program like Conway's life on a small board is a finite difference equation
# — the BOARD has only 2^N configurations, so it must fall into a fixed point or
# a limit cycle. But the IR also carries `gen` (a generation counter that
# increments forever) and a derived `pop`. Because every (board, gen) pair is a
# distinct state, reachable() never terminates: it hits the cap, the program
# LOOKS unenumerable, and the old code fell through to a seeded numeric grid that
# FABRICATED basins by binning `gen` into ranges. The truth is one basin (the
# blinker's single period-2 orbit). Projecting the monotone counter(s) out of the
# state key collapses (board, gen) back onto the finite board graph, on which the
# exact terminal-SCC basin is well-defined.


def _monotone_counters(m, traj):
    """Int interface vars that STRICTLY increase by a fixed step every tick along
    the trajectory — free-running clocks (life's `gen`). These inflate the
    reachable set without changing the underlying dynamics, so they're projected
    out of the state key before re-running BFS. Returns a set of names."""
    out = set()
    if len(traj) < 4:
        return out
    for v in m.carried:
        if v["kind"] != "int":
            continue
        seq = [s[v["name"]] for s in traj if v["name"] in s]
        if len(seq) < 4:
            continue
        diffs = {seq[i + 1] - seq[i] for i in range(len(seq) - 1)}
        if diffs == {1}:                     # +1 every tick: a generation clock
            out.add(v["name"])
    return out


def _projected_reachable(m, drop, cap=2048):
    """BFS the reachable graph, but key states by their NON-`drop` fields (so a
    free-running counter no longer makes every step a fresh state). Returns
    (states, edges, overflow): states are representative full states (first seen
    per projected key), edges index into them, overflow is True if the projected
    graph itself exceeded `cap` (genuinely too large -> caller routes to N/A)."""
    init = m.initial_state()
    if init is None:
        return [], [], False

    def pkey(st):
        return tuple(sorted((k, v) for k, v in st.items() if k not in drop))

    states = [init]
    index = {pkey(init): 0}
    edges = set()
    frontier = [0]
    overflow = False
    while frontier:
        i = frontier.pop()
        for nxt in m.successors(states[i]):
            k = pkey(nxt)
            if k not in index:
                if len(states) >= cap:
                    overflow = True
                    continue
                index[k] = len(states)
                states.append(nxt)
                frontier.append(index[k])
            edges.add((i, index[k]))
    return states, list(edges), overflow


def _numeric_basins(m, out_path):
    axes = _choose_axes(m)
    if len(axes) < 1:
        _placeholder(out_path, m.fsm, "no axes available")
        return "numeric: no axes"
    ax_x = axes[0]
    ax_y = axes[1] if len(axes) > 1 else None

    # --- reachable-set routing (the fabrication fix) -----------------------
    # A basin map seeded over a guessed plane invents cycles/basins a terminating
    # program never enters. So first ask what the program ACTUALLY reaches.
    rcap = 1200
    states, _edges = m.reachable(limit=rcap)
    distinct = len({m._key(s) for s in states})
    finite = 0 < len(states) < rcap

    # FINITE reachable structure with >1 state (a terminating counter like wc):
    # plot ONLY the real reachable states, colored by the terminal SCC each can
    # reach — the exact-graph basin. No grid, no invented plane.
    if finite and distinct >= 2:
        note = _discrete_basins(m, out_path)
        return f"finite-reachable -> {note}"

    # CAPPED reachable set: the BFS never terminated. Before reaching for a
    # numeric grid, check whether the set is unbounded ONLY because a monotone
    # counter (life's `gen`) tags every step as a fresh state. If so, project that
    # counter out and re-run BFS on the underlying state space — a small board's
    # dynamics are finite and MUST settle into a fixed point or limit cycle. On the
    # projected graph the exact terminal-SCC basin is well-defined (life: a single
    # period-2 orbit = one basin), so route to the same exact-graph machinery the
    # discrete path uses. If even the projected graph is too large, render honest
    # N/A — never a seeded grid that fabricates basins.
    if not finite and len(states) >= rcap:
        traj = m.trajectory(steps=64)
        drop = _monotone_counters(m, traj)
        if drop:
            pstates, pedges, overflow = _projected_reachable(m, drop)
            if not overflow and len(pstates) >= 2:
                note = _discrete_basins_on(m, out_path, pstates, pedges,
                                           projected_out=drop)
                return (f"counter-projected ({'/'.join(sorted(drop))}) -> {note}")
            if overflow or len(pstates) >= 2:
                _placeholder(out_path, m.fsm,
                             "reachable set is too large to enumerate even after "
                             f"projecting out the counter(s) "
                             f"{', '.join(sorted(drop))} — basin map N/A")
                return "too-large after projection (N/A)"
        # No monotone counter explains the blow-up: a genuinely large/continuous
        # reachable set. Don't fabricate a grid of basins — honest N/A.
        _placeholder(out_path, m.fsm,
                     f"reachable set exceeds {rcap} states with no monotone "
                     "counter to project out — too large to enumerate; basin "
                     "map N/A")
        return "too-large reachable (N/A)"

    # Single reachable state: the reachable-from-init set is one fixed point. There
    # may still be a surrounding continuous attractor (van der Pol's init sits on
    # the unstable origin), discovered by off-init probes inside _numeric_domain.
    dom = _numeric_domain(m, ax_x, ax_y)
    has_numeric_domain = any(v["name"] in dom for v in axes
                             if v["kind"] in ("int", "real"))
    if distinct <= 1 and not has_numeric_domain:
        _placeholder(out_path, m.fsm,
                     f"reachable set is {distinct} point(s) — a lone fixed point "
                     "with no surrounding attractor; basin map not meaningful")
        return "degenerate: lone fixed point (N/A)"

    facet_var, facet_vals = _choose_facet(m, axes, states)
    kind = "numeric" if all(v["kind"] in ("int", "real")
                            for v in m.state_vars) else "mixed"

    # Shared across panels: the z3-backed successor cache, and a SINGLE cluster
    # so a basin's color/label means the same thing in every panel.
    cache, resolved = {}, {}

    if facet_var is None:
        seeds, sigs = _panel_basins(m, ax_x, ax_y, {}, cache, resolved, dom)
        labels, centers = _cluster(sigs)
        fig, ax = plt.subplots(figsize=(9, 7))
        _draw_panel(ax, m, ax_x, ax_y, seeds, labels, centers,
                    show_legend=True, dom=dom)
        ax.set_title(f"{m.fsm} — basin_map ({kind}: {len(centers)} basins on "
                     f"{len(seeds)}-seed grid)", fontsize=13, weight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return f"{kind}: {len(seeds)} seeds -> {len(centers)} basins"

    # FACETED: one panel per facet value, holding facet_var fixed. Compute all
    # panels first so we cluster every signature TOGETHER (consistent colors).
    panels = []        # (label_str, seeds, sigs)
    all_sigs = []
    for fv in facet_vals:
        seeds, sigs = _panel_basins(m, ax_x, ax_y, {facet_var["name"]: fv},
                                    cache, resolved, dom)
        disp = fv if facet_var["kind"] != "bool" else ("true" if fv else "false")
        panels.append((str(disp), seeds, sigs))
        all_sigs.extend(sigs)
    labels_all, centers = _cluster(all_sigs)

    npan = len(panels)
    fig, axarr = plt.subplots(1, npan, figsize=(5.2 * npan, 6.0),
                              squeeze=False)
    off = 0
    for i, (disp, seeds, sigs) in enumerate(panels):
        lbl = labels_all[off:off + len(sigs)]
        off += len(sigs)
        ax = axarr[0][i]
        _draw_panel(ax, m, ax_x, ax_y, seeds, lbl, centers,
                    show_legend=(i == npan - 1), dom=dom)
        ax.set_title(f"{facet_var['name']} = {disp}", fontsize=11,
                     weight="bold")
    fig.suptitle(f"{m.fsm} — basin_map ({kind}: faceted by "
                 f"{facet_var['name']}, {len(centers)} basins)",
                 fontsize=13, weight="bold")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    return (f"{kind}: faceted by {facet_var['name']} ({npan} panels), "
            f"{len(centers)} basins")


def _attractor_signature(m, cycle):
    """A PHASE-INVARIANT signature for an attractor (cycle of states). Two
    trajectories that land on the same limit cycle must get the same signature
    regardless of where on the cycle they happened to stop — otherwise every
    phase reads as its own 'basin'. So for numeric axes we use the cycle's
    geometric centroid and its mean radius about that centroid (both phase
    invariant), NOT the raw per-state means. Discrete axes (enum/bool) use the
    set of values visited, since those genuinely distinguish attractors."""
    if not cycle:
        return ("num", 0.0, 0.0, 0.0)
    num_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    disc_vars = [v for v in m.state_vars if v["kind"] in ("enum", "bool",
                                                          "string")]
    sig = []
    # numeric: centroid magnitude + mean radius (size of the orbit)
    for v in num_vars:
        vals = [_ordinal(m, v, st[v["name"]]) for st in cycle]
        sig.append(sum(vals) / len(vals))          # centroid coord
    if num_vars:
        cx = [sum(_ordinal(m, v, st[v["name"]]) for st in cycle) / len(cycle)
              for v in num_vars]
        radii = []
        for st in cycle:
            d = sum((_ordinal(m, v, st[v["name"]]) - cx[i]) ** 2
                    for i, v in enumerate(num_vars)) ** 0.5
            radii.append(d)
        sig.append(sum(radii) / len(radii))        # mean orbit radius
    # discrete: the SET of visited values, projected to an ordinal multiset key
    for v in disc_vars:
        visited = sorted(set(round(_ordinal(m, v, st[v["name"]]))
                             for st in cycle))
        # encode the visited-set as separated coords (one big number per set)
        sig.append(float(sum(val * (1000 ** i)
                             for i, val in enumerate(visited))))
    sig.append(float(len(cycle)))
    return tuple(sig)


def _cluster(sigs, tol=400.0):
    """Greedy clustering of signature vectors. Numeric coords use absolute tol;
    this is enough to separate a limit cycle from a fixed point and distinct
    discrete attractors. Returns (labels, centers)."""
    centers = []
    labels = []
    for s in sigs:
        best = -1
        bestd = None
        for i, c in enumerate(centers):
            d = _sig_dist(s, c)
            if bestd is None or d < bestd:
                bestd = d
                best = i
        if best >= 0 and bestd <= tol:
            labels.append(best)
            # online mean update
            c = centers[best]
            centers[best] = tuple((a + b) / 2 for a, b in zip(c, s))
        else:
            centers.append(s)
            labels.append(len(centers) - 1)
    return labels, centers


def _sig_dist(a, b):
    n = min(len(a), len(b))
    return sum(abs(a[i] - b[i]) for i in range(n))


def _sig_layout(m):
    """Index map into a signature vector (see _attractor_signature):
      [ num centroid coords (one per numeric var, in state-var order),
        mean orbit radius (only if any numeric var),
        discrete set-codes (one per enum/bool/string var),
        cycle length ]"""
    num_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    disc_vars = [v for v in m.state_vars if v["kind"] in ("enum", "bool",
                                                          "string")]
    num_idx = {v["name"]: i for i, v in enumerate(num_vars)}
    base = len(num_vars)
    radius_idx = base if num_vars else None
    disc_start = base + (1 if num_vars else 0)
    disc_idx = {v["name"]: disc_start + i for i, v in enumerate(disc_vars)}
    return num_idx, radius_idx, disc_idx, num_vars, disc_vars


def _center_axis_coords(m, cvec, ax_x, ax_y):
    """Map an attractor signature's centroid back to plot coords on the chosen
    numeric axes (used to overlay the attractor location)."""
    num_idx, _, _, _, _ = _sig_layout(m)

    def coord(ax):
        if ax is None or ax["name"] not in num_idx:
            return None
        i = num_idx[ax["name"]]
        return cvec[i] if i < len(cvec) else None

    return coord(ax_x), coord(ax_y)


def _describe_region(m, center):
    """Short human description of a cluster center signature."""
    num_idx, radius_idx, disc_idx, num_vars, disc_vars = _sig_layout(m)
    cyclelen = center[-1] if center else 0
    radius = center[radius_idx] if (radius_idx is not None
                                    and radius_idx < len(center)) else 0.0
    parts = []
    for v in num_vars:
        i = num_idx[v["name"]]
        val = center[i] if i < len(center) else 0.0
        parts.append(f"{v['name']}≈{val:.0f}")
    if num_vars:
        parts.append(f"r≈{radius:.0f}")
    for v in disc_vars:
        i = disc_idx[v["name"]]
        code = int(center[i]) if i < len(center) else 0
        # decode the visited-set ordinal multiset
        vals = []
        c = code
        while c > 0:
            vals.append(c % 1000)
            c //= 1000
        if not vals:
            vals = [0]
        if v["kind"] == "enum":
            variants = m.enum_variants[v["name"]]
            names = "/".join(variants[min(len(variants) - 1, j)] for j in vals)
            parts.append(f"{v['name']}∈{{{names}}}")
        elif v["kind"] == "bool":
            parts.append(f"{v['name']}∈{{{'/'.join('T' if j else 'F' for j in vals)}}}")
        else:
            parts.append(f"{v['name']}#{len(vals)}")
    kind = "cycle" if (cyclelen > 1.5 or radius > 150) else "fixed"
    return ", ".join(parts) + f" ({kind})"


# --------------------------------------------------------------------------
def render(smt2, schema, out_path):
    m = load(smt2, schema)
    if m.is_discrete():
        return _discrete_basins(m, out_path)
    return _numeric_basins(m, out_path)


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
