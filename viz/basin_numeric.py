#!/usr/bin/env python3
"""basin_numeric.py — the NUMERIC / MIXED basin-map path.

Split out of render_basin_map.py so that module keeps only the dispatch +
discrete-graph path. This module owns everything specific to gridding a numeric
(or mixed) state space and seeding it: the reachable/visited-set domain
derivation, the off-init probe widening, the per-panel seed→attractor iteration,
and the panel drawing.

The entry point is `numeric_basins(m, out_path, ...)`. It calls back into the
discrete path (`discrete_basins` / `discrete_basins_on`) and shares the
enum-tick decoration (`decorate_enum_ticks`) — those are passed in by the caller
to keep the import acyclic.
"""
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

from basin_support import (  # noqa: E402
    PALETTE, _placeholder, _choose_axes, _choose_facet, _ordinal, _axis_label,
    _attractor_signature, _cluster, _sig_layout,
)
# Seed/grid DOMAIN derivation lives in its own module.
from basin_domain import (  # noqa: E402
    baseline_fn, numeric_domain, axis_grid,
)
# Interactive hover-overlay sidecar (#184 increment 3): each seed dot → its full
# start state. basin_map always saves tight, so use the tight-bbox mapping.
from overlay_points import write_points, tight_fraction  # noqa: E402


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



def _panel_basins(m, ax_x, ax_y, fixed, cache, resolved, dom):
    """Compute one panel of the basin map: seed a grid over the two axes, scaled
    to the reachable/visited `dom` (NO hardcoded box, NO off-plane probes — the
    grid already spans the actual attractor extent), holding `fixed`
    (name->value) constant, iterate each seed to its attractor signature. Returns
    (seeds, sigs, seed_states): seeds are (xv, yv) axis-value pairs, seed_states
    the FULL start-state dicts behind each plotted dot (for the hover overlay).
    `cache`/`resolved` are shared across panels so the z3 work is paid once."""
    baseline = baseline_fn(m)
    nx = 14 if ax_x["kind"] in ("int", "real") else None
    ny = 14 if (ax_y and ax_y["kind"] in ("int", "real")) else None
    gx, _bx = axis_grid(m, ax_x, nx or 8, dom)
    if ax_y is not None:
        gy, _by = axis_grid(m, ax_y, ny or 8, dom)
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

    seeds, sigs, seed_states = [], [], []
    for xv in gx:
        for yv in gy:
            st = mk_state(xv, yv)
            sig = _iterate_to_attractor(m, st, cache, resolved)
            seeds.append((xv, yv))
            sigs.append(sig)
            seed_states.append(st)
    return seeds, sigs, seed_states


def _draw_panel(ax, m, ax_x, ax_y, seeds, labels, centers, show_legend, dom,
                decorate_enum_ticks, seed_states=None, overlay=None):
    bx = axis_grid(m, ax_x, 8, dom)[1]
    by = axis_grid(m, ax_y, 8, dom)[1] if ax_y is not None else (-0.5, 0.5)
    xs = np.array([_ordinal(m, ax_x, s[0]) for s in seeds], float)
    if ax_y is not None:
        ys = np.array([_ordinal(m, ax_y, s[1]) for s in seeds], float)
    else:
        ys = np.zeros(len(seeds))
    labels = np.array(labels, int)
    # Each seed dot is hoverable → its full start state (#184 increment 3).
    if overlay is not None and seed_states is not None:
        overlay.extend((ax, float(xs[i]), float(ys[i]), seed_states[i])
                       for i in range(len(seed_states)))

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
    decorate_enum_ticks(ax, m, ax_x, ax_y)
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


def _exact_graph_route(m, out_path, states, distinct, finite, rcap,
                       discrete_basins, discrete_basins_on):
    """Decide whether the reachable structure can be drawn exactly (no numeric
    grid). Returns a done-note string if it handled the render, else None to let
    the caller fall through to the numeric seeded-grid path."""
    # FINITE reachable structure with >1 state (a terminating counter like wc):
    # plot ONLY the real reachable states, colored by the terminal SCC each can
    # reach — the exact-graph basin. No grid, no invented plane.
    if finite and distinct >= 2:
        return f"finite-reachable -> {discrete_basins(m, out_path)}"

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
                note = discrete_basins_on(m, out_path, pstates, pedges,
                                          projected_out=drop)
                return (f"counter-projected ({'/'.join(sorted(drop))}) -> {note}")
            if overflow or len(pstates) >= 2:
                _placeholder(out_path, m.fsm,
                             "reachable set is too large to enumerate even after "
                             f"projecting out the counter(s) "
                             f"{', '.join(sorted(drop))} — basin map N/A")
                return "too-large after projection (N/A)"
        # No monotone counter explains the blow-up: a genuinely CONTINUOUS reachable
        # set (a spiral sink / limit cycle whose from-init enumeration runs away —
        # #357/#465). This is NOT a fabrication case: fall through (return None) to the
        # numeric grid-sweep, which grids the PROVEN-bounds / probed-attractor extent
        # (numeric_domain) like phase_portrait does — every seed is a real point in the
        # reachable region, integrated forward to its real attractor. The caller still
        # routes to an honest N/A if no griddable numeric domain exists (a lone fixed
        # point with no surrounding attractor).
        return None
    return None


def numeric_basins(m, out_path, discrete_basins, discrete_basins_on,
                   decorate_enum_ticks):
    """NUMERIC / MIXED basin path. `discrete_basins` / `discrete_basins_on` are
    the exact-graph fallbacks (passed in to keep the import acyclic), and
    `decorate_enum_ticks` paints categorical axis ticks on each panel."""
    axes = _choose_axes(m)
    if len(axes) < 1:
        _placeholder(out_path, m.fsm, "no axes available")
        return "numeric: no axes"
    ax_x = axes[0]
    ax_y = axes[1] if len(axes) > 1 else None

    # --- reachable-set routing (the fabrication fix) -----------------------
    # A basin map seeded over a guessed plane invents cycles/basins a terminating
    # program never enters. So first ask what the program ACTUALLY reaches; the
    # exact-graph router handles the finite/capped cases and returns a done-note,
    # or None to fall through to the numeric grid.
    rcap = 1200
    states, _edges = m.reachable(limit=rcap)
    distinct = len({m._key(s) for s in states})
    finite = 0 < len(states) < rcap
    routed = _exact_graph_route(m, out_path, states, distinct, finite, rcap,
                                discrete_basins, discrete_basins_on)
    if routed is not None:
        return routed

    # Single reachable state: the reachable-from-init set is one fixed point. There
    # may still be a surrounding continuous attractor (van der Pol's init sits on
    # the unstable origin), discovered by off-init probes inside _numeric_domain.
    dom = numeric_domain(m, ax_x, ax_y)
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
        return _basins_single(m, out_path, ax_x, ax_y, dom, kind, cache,
                              resolved, decorate_enum_ticks)
    return _basins_faceted(m, out_path, ax_x, ax_y, dom, kind, cache, resolved,
                           facet_var, facet_vals, decorate_enum_ticks)


def _basins_single(m, out_path, ax_x, ax_y, dom, kind, cache, resolved,
                   decorate_enum_ticks):
    """Single (un-faceted) panel: one grid of seeds → cluster → draw."""
    seeds, sigs, seed_states = _panel_basins(m, ax_x, ax_y, {}, cache, resolved, dom)
    labels, centers = _cluster(sigs)
    fig, ax = plt.subplots(figsize=(9, 7))
    overlay = []
    _draw_panel(ax, m, ax_x, ax_y, seeds, labels, centers,
                show_legend=True, dom=dom,
                decorate_enum_ticks=decorate_enum_ticks,
                seed_states=seed_states, overlay=overlay)
    ax.set_title(f"{m.fsm} — basin_map ({kind}: {len(centers)} basins on "
                 f"{len(seeds)}-seed grid)", fontsize=13, weight="bold")
    points = tight_fraction(fig, overlay)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, points)
    return f"{kind}: {len(seeds)} seeds -> {len(centers)} basins"


def _basins_faceted(m, out_path, ax_x, ax_y, dom, kind, cache, resolved,
                    facet_var, facet_vals, decorate_enum_ticks):
    """One panel per facet value, holding facet_var fixed. Compute all panels
    first so we cluster every signature TOGETHER (consistent colors)."""
    panels = []        # (label_str, seeds, sigs, seed_states)
    all_sigs = []
    for fv in facet_vals:
        seeds, sigs, seed_states = _panel_basins(
            m, ax_x, ax_y, {facet_var["name"]: fv}, cache, resolved, dom)
        disp = fv if facet_var["kind"] != "bool" else ("true" if fv else "false")
        panels.append((str(disp), seeds, sigs, seed_states))
        all_sigs.extend(sigs)
    labels_all, centers = _cluster(all_sigs)

    npan = len(panels)
    fig, axarr = plt.subplots(1, npan, figsize=(5.2 * npan, 6.0),
                              squeeze=False)
    off = 0
    overlay = []
    for i, (disp, seeds, sigs, seed_states) in enumerate(panels):
        lbl = labels_all[off:off + len(sigs)]
        off += len(sigs)
        ax = axarr[0][i]
        _draw_panel(ax, m, ax_x, ax_y, seeds, lbl, centers,
                    show_legend=(i == npan - 1), dom=dom,
                    decorate_enum_ticks=decorate_enum_ticks,
                    seed_states=seed_states, overlay=overlay)
        ax.set_title(f"{facet_var['name']} = {disp}", fontsize=11,
                     weight="bold")
    fig.suptitle(f"{m.fsm} — basin_map ({kind}: faceted by "
                 f"{facet_var['name']}, {len(centers)} basins)",
                 fontsize=13, weight="bold")
    points = tight_fraction(fig, overlay)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, points)
    return (f"{kind}: faceted by {facet_var['name']} ({npan} panels), "
            f"{len(centers)} basins")
