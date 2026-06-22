#!/usr/bin/env python3
"""render_cobweb.py — classic 1-D map cobweb plot for any Evident IR.

Usage:
    python3 viz/render_cobweb.py <smt2> <schema> <out_path>

Channel mapping (Cleveland-McGill / Mackinlay): a cobweb is a 1-D map, so both
AXES carry the SAME variable (x_n -> x_{n+1}) — the single most important
quantitative scalar. That uses up position; to ADD a dimension we FACET by a
low-cardinality CATEGORICAL var (an enum mode), one cobweb panel per value. That
is the honest small-multiples way to show a high-D model on a 1-D map.

  * primary scalar  = numeric_vars[0]  (else the top ranked var, ordinalized)
  * facet (panels)  = a categorical var with <= ~5 values, != the primary
  * the remaining carried vars are held at the initial / a neutral state

The map x_{n+1} = f(x_n) is sampled with m.successors() so SET-VALUED
(nondeterministic) transitions show their full fan; the dynamics ALWAYS come from
solving the transition, never hardcoded.
"""
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

sys.path.insert(0, "viz")
from evident_viz import load


# --------------------------------------------------------------------------
# Channel selection: primary scalar (the axis), facet var (the panels).
# --------------------------------------------------------------------------
def _pick_primary(m):
    """Return (var, mode): the scalar the cobweb is OVER.

    Prefer the top-ranked NUMERIC var (true 1-D map); fall back to the top
    ranked var as an enum ordinal. (None, None) only if there are no vars."""
    if m.numeric_vars:
        return m.numeric_vars[0], "int"
    for v in m.state_vars:
        if v["kind"] == "enum":
            return v, "enum-ordinal"
    return (m.state_vars[0], "enum-ordinal") if m.state_vars else (None, None)


def _facet_values(m, var):
    if var["kind"] == "enum":
        return list(m.enum_variants[var["name"]])
    return [False, True]


def _map_fingerprint(m, var, mode, base, grid):
    """A canonical fingerprint of the map x_{n+1} = f(x_n) under `base`. Two facet
    values that yield the SAME fingerprint produce IDENTICAL cobweb scatters — the
    facet doesn't enter the primary var's transition, so a panel per value is a
    duplicate (the `find` bug: s5=Unseen and s5=Visited are pixel-identical because
    holding-s5 doesn't change f). Used to dedup facet values before drawing."""
    xs, ys = _sample_map(m, var, mode, base, grid)
    return tuple(sorted(zip(xs, ys)))


def _distinct_facet_groups(m, var, mode, base, grid, facet):
    """Group facet values by the map they produce, keeping insertion order. Returns
    a list of (representative_values, fingerprint): values sharing a fingerprint are
    collapsed into one group (one panel labelled with all of them). A facet whose
    values ALL collapse to one group adds no information and should be dropped."""
    groups = []          # [(values, fingerprint)]
    index = {}           # fingerprint -> position in groups
    for val in _facet_values(m, facet):
        panel_base = dict(base)
        panel_base[facet["name"]] = val
        fp = _map_fingerprint(m, var, mode, panel_base, grid)
        if fp in index:
            groups[index[fp]][0].append(val)
        else:
            index[fp] = len(groups)
            groups.append(([val], fp))
    return groups


# --------------------------------------------------------------------------
# State construction + ordinal <-> value plumbing.
# --------------------------------------------------------------------------
def _base_state(m):
    """A neutral state holding the non-primary, non-facet vars fixed."""
    init = m.initial_state()
    if init is not None:
        return dict(init)
    state = {}
    for v in m.state_vars:
        if v["kind"] == "int":
            state[v["name"]] = 0
        elif v["kind"] == "bool":
            state[v["name"]] = False
        elif v["kind"] == "enum":
            state[v["name"]] = m.enum_variants[v["name"]][0]
        elif v["kind"] == "real":
            state[v["name"]] = 0.0
        else:
            state[v["name"]] = ""
    return state


def _to_ord(m, var, value):
    if var["kind"] == "enum":
        return m.enum_variants[var["name"]].index(value)
    if var["kind"] == "bool":
        return 1 if value else 0
    return value


def _from_ord(m, var, o):
    if var["kind"] == "enum":
        variants = m.enum_variants[var["name"]]
        o = max(0, min(len(variants) - 1, int(round(o))))
        return variants[o]
    if var["kind"] == "bool":
        return bool(int(round(o)))
    return int(round(o))


def _orbit_values(m, var, base):
    """The DISTINCT values the primary var actually takes along the REAL orbit — the
    single successor-chain (trajectory) starting from `base`, with the non-primary
    vars carried by the transition itself.

    This is the honest domain for a cobweb. `axis_bounds`/`reachable` over-approximate:
    they leave the OTHER carried vars free, so they report every value the var COULD
    take across all branches (lru's `k0` -> {-1,1,9,10,...,75}), not the values it
    visits on the orbit the staircase actually follows (lru's `k0` -> {-1, 1}).
    Gridding the over-approximation fabricates a y=x continuum the program never
    enters. Returns a sorted list of orbit values (possibly empty)."""
    name = var["name"]
    traj = m.trajectory(start=base, steps=400)
    seen = []
    s = set()
    for st in traj:
        v = st.get(name)
        if v is not None and v not in s:
            s.add(v)
            seen.append(v)
    return sorted(seen)


def _numeric_range(m, var, base):
    """Determine the cobweb x-range for a numeric var FROM THE REAL ORBIT, never a
    hardcoded ±3000 box AND never the over-approximated reachable extent (both are
    fabrication: gridding values the staircase never visits invents a map continuum /
    y=x line the program never enters — the lru `k0` bug).

    Grid over the distinct values the primary var takes along the trajectory, padded
    by one step on each side so the map's local neighborhood is visible. When the orbit
    only ever sits at a couple of values (lru's `k0 ∈ {-1, 1}`), the grid is just those
    points — no fake continuum. Falls back to `axis_bounds` only when the orbit is empty
    (no trajectory), and to a wide window only when even that is unbounded.

    Returns (grid_values, is_bounded). Returns (None, None) when no honest range exists
    so the caller routes to the N/A path instead of fabricating."""
    name = var["name"]
    orbit = _orbit_values(m, var, base)
    if orbit:
        olo, ohi = orbit[0], orbit[-1]
        span = ohi - olo
        # Grid the orbit's own extent, padded by ONE unit on each side (so the
        # neighborhood of each visited value shows), capped at a readable resolution.
        # No wide window: a 2-value orbit grids ~4 points, not 0..75.
        ilo, ihi = int(round(olo)) - 1, int(round(ohi)) + 1
        ispan = ihi - ilo
        if ispan <= 0:
            return [int(round(olo))], True
        if ispan <= 200:
            return list(range(ilo, ihi + 1)), True
        n = 161
        return [ilo + ispan * i // (n - 1) for i in range(n)], True

    # No orbit (no trajectory at all) — fall back to the reachable extent.
    bounds = m.axis_bounds(name, pad=0.0)
    if bounds is None:
        # genuinely unbounded continuous dynamics with no finite reachable sample:
        # the ONLY case a generous window is honest. (Rare — most numeric Evident
        # FSMs have a finite reachable set.)
        lo, hi, n = -3200, 3200, 121
        grid = [lo + (hi - lo) * i // (n - 1) for i in range(n)]
        return grid, False
    lo, hi = bounds
    ilo, ihi = int(round(lo)), int(round(hi))
    span = ihi - ilo
    if span <= 0:
        return [ilo], True
    if span <= 400:                          # bounded reachable counter: grid exactly
        return list(range(ilo, ihi + 1)), True
    # bounded but wide: sample the reachable extent at fixed resolution (no padding
    # beyond what axis_bounds already added).
    n = 161
    grid = [ilo + span * i // (n - 1) for i in range(n)]
    return grid, True


# --------------------------------------------------------------------------
# Map + staircase sampling (set-valued aware).
# --------------------------------------------------------------------------
def _sample_map(m, var, mode, base, grid):
    """Sample x_{n+1} in f(x_n) for x_n over `grid`. Uses successors() so the
    FAN of a nondeterministic map shows all branches. Returns parallel
    (xs, ys) in ordinal space."""
    name = var["name"]
    xs, ys = [], []
    for x in grid:
        state = dict(base)
        state[name] = _from_ord(m, var, x)
        for nxt in m.successors(state):
            xs.append(x)
            ys.append(_to_ord(m, var, nxt[name]))
    return xs, ys


def _staircase(m, var, mode, base, seed, steps=60):
    """A cobweb staircase orbit following one successor chain from `seed`."""
    name = var["name"]
    px, py = [], []
    x = seed
    px.append(x); py.append(x)            # start on the diagonal
    seen = set()
    for _ in range(steps):
        state = {**base, name: _from_ord(m, var, x)}
        nxt = m.successor(state)
        if nxt is None:
            break
        y = _to_ord(m, var, nxt[name])
        px.append(x); py.append(y)        # vertical to the map
        px.append(y); py.append(y)        # horizontal to the diagonal
        key = round(y, 6)
        if key in seen:
            break
        seen.add(key)
        x = y
    return px, py


def _seed_for(mode, lo, hi, bounded, base, var, m):
    """Seed the cobweb staircase from a REACHABLE state — the initial state's value
    of this var (clamped into the gridded range), never a fabricated wide start
    (the old `seed=2000` invented an orbit through a region the program never
    enters)."""
    seed = _to_ord(m, var, base[var["name"]])
    if mode == "int":
        return max(lo, min(hi, seed))
    return seed


# --------------------------------------------------------------------------
# Drawing.
# --------------------------------------------------------------------------
def _draw_panel(ax, m, var, mode, base, grid, bounded, panel_label=None):
    xs, ys = _sample_map(m, var, mode, base, grid)
    if not xs:
        ax.text(0.5, 0.5, "transition unsat\nover sampled range",
                ha="center", va="center", fontsize=10, transform=ax.transAxes)
        ax.set_axis_off()
        return False

    lo = min(min(xs), min(ys))
    hi = max(max(xs), max(ys))
    pad = (hi - lo) * 0.05 + 0.5
    lo -= pad; hi += pad

    # The map x_{n+1} = f(x_n). Markers handle set-valued fans (multiple y per x).
    ax.plot(xs, ys, "o", color="#1f77b4", ms=4, label=r"$x_{n+1}=f(x_n)$")
    if mode == "int" and not bounded:
        # connect the single-valued continuous branch for readability
        if len(set(xs)) == len(xs):
            ax.plot(xs, ys, "-", color="#1f77b4", lw=1, alpha=0.4)

    ax.plot([lo, hi], [lo, hi], "--", color="#888", lw=1, label="y = x")

    seed = _seed_for(mode, lo, hi, bounded, base, var, m)
    px, py = _staircase(m, var, mode, base, seed)
    if len(px) > 1:
        ax.plot(px, py, "-", color="#d62728", lw=1.3, alpha=0.85,
                label=f"orbit (seed={_from_ord(m, var, seed)})")
        ax.plot(px[0], py[0], "o", color="#d62728", ms=6)

    ax.set_xlim(lo, hi)
    ax.set_ylim(lo, hi)
    ax.set_aspect("equal", adjustable="box")

    if mode == "enum-ordinal":
        variants = m.enum_variants[var["name"]]
        ax.set_xticks(range(len(variants)))
        ax.set_xticklabels(variants, rotation=45, ha="right", fontsize=7)
        ax.set_yticks(range(len(variants)))
        ax.set_yticklabels(variants, fontsize=7)

    ax.grid(True, alpha=0.2)
    if panel_label is not None:
        ax.set_title(panel_label, fontsize=10)
    return True


def _na_card(out_path, fsm, msg):
    """Honest placeholder instead of fabricating structure over a guessed range."""
    fig, ax = plt.subplots(figsize=(7.5, 7.5))
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13, wrap=True)
    ax.set_axis_off()
    ax.set_title(f"{fsm}  —  cobweb")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def _reachable_count(m):
    """Number of distinct reachable states (capped) — for the degeneracy guard."""
    try:
        states, _ = m.reachable(limit=64)
        return len(states)
    except Exception:
        return None


def _held_alts(m, hv):
    """A few alternative previous-values for a HELD companion var, to perturb it."""
    if hv["kind"] == "enum":
        return list(m.enum_variants[hv["name"]])
    if hv["kind"] == "bool":
        return [False, True]
    # numeric companion: sample a handful of values it actually takes on the orbit. Must accept
    # float (Real) too — restricting to int silently returned [] for a continuous companion like
    # the oscillator's `pos`, so the non-autonomy probe never perturbed it and drew a misleading
    # 1-D slice of a genuinely 2-D coupled system (Marek #38).
    vals = sorted({s.get(hv["name"]) for s in m._sample_states()
                   if isinstance(s.get(hv["name"]), (int, float)) and not isinstance(s.get(hv["name"]), bool)})
    return vals[:4]


def _depends_on_held(m, var, base, grid):
    """Is the candidate scalar's successor a function of a HELD companion var, rather
    than a self-contained 1-D map?

    The cobweb scans x_n -> x_{n+1} with the OTHER carried vars pinned at `base`. That
    is only honest when f(x) = state.X(_state.X) — when the scalar's next value is
    determined by its OWN previous value alone. If instead the next value depends on a
    held companion (vending's balance(n+1) is driven by the held state.mode, not by
    balance(n)), the scanned map is a LIE: holding mode=Idle makes f(balance)=0 for
    EVERY balance, a flat line that wrongly implies balance always collapses to 0.

    Probe: for a few x_n on the grid, perturb each held companion ONE at a time
    (holding the scalar and the rest) and check whether the scalar's successor MOVES.
    If any perturbation changes it, the scalar is non-autonomous over these axes and
    the 1-D cobweb is not meaningful. Returns (True, (x, held_name, alt)) on the first
    witness, else (False, None)."""
    name = var["name"]
    # use ALL interface vars, not the DEDUPED state_vars — csv_stats' cursor/count/sum
    # are partition-equivalent on the trajectory so dedup collapses them, dropping cursor
    # (the real driver of sum) from the held set and hiding the non-autonomy.
    held = [v for v in m.interface_vars if v["name"] != name]
    if not held:
        return False, None
    # Probe bases: grid scan-points at the neutral base, PLUS a few REAL reachable
    # states. A single neutral base can sit where a companion's influence is masked
    # (e.g. csv_stats' sum is driven by the held cursor, but at a past-EOF/neutral
    # cursor the scalar is frozen so the dependence hides) — probing reachable states
    # catches it where the dependence is live.
    probes = []
    if grid:
        n = len(grid)
        for x in sorted({grid[0], grid[n // 2], grid[-1]}):
            st = dict(base)
            st[name] = _from_ord(m, var, x)
            probes.append(st)
    probes += m._sample_states()[:5]
    for st in probes:
        ref = m.successor(st)
        if ref is None:
            continue
        refv = ref.get(name)
        for hv in held:
            for alt in _held_alts(m, hv):
                if alt == st.get(hv["name"]):
                    continue
                pert = m.successor({**st, hv["name"]: alt})
                if pert is not None and pert.get(name) != refv:
                    return True, (st.get(name), hv["name"], alt)
    return False, None


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    var, mode = _pick_primary(m)

    if var is None:
        _na_card(out_path, m.fsm, "N/A: no state var to cobweb")
        return

    # Degeneracy guard: a cobweb staircases an ORBIT over a 1-D map. A reachable set
    # of one or two states (e.g. a fixed point at the origin — van der Pol seeded at
    # (0,0)) has no orbit to trace and no real axis extent; gridding a map over it
    # would fabricate a fixed-point continuum the program never enters. Render an
    # honest N/A instead. (Numeric vars only — a discrete/enum mode legitimately
    # cobwebs over its few categorical values.)
    if mode == "int":
        nstates = _reachable_count(m)
        if nstates is not None and nstates <= 2:
            _na_card(out_path, m.fsm,
                     f"N/A — reachable set is {nstates} "
                     f"state{'s' if nstates != 1 else ''} (degenerate / fixed "
                     f"point);\ncobweb (a 1-D map orbit) not meaningful.")
            return

    base = _base_state(m)
    if mode == "int":
        grid, bounded = _numeric_range(m, var, base)
        if grid is None:                    # axis_bounds None AND no fallback fired
            _na_card(out_path, m.fsm,
                     f"N/A — no numeric range for {var['name']}; "
                     "cobweb not meaningful.")
            return
        # Non-autonomy guard: a cobweb is a 1-D map x_{n+1}=f(x_n). If the scalar's
        # next value is actually driven by a HELD companion var (vending's balance is
        # driven by state.mode, not by balance itself), scanning the scalar with the
        # companion pinned fabricates a false, misleading map — a flat f(x)=0 line that
        # claims balance always collapses to 0. Emit an honest N/A instead of drawing it.
        dep, why = _depends_on_held(m, var, base, grid)
        if dep:
            x, hname, _alt = why
            _na_card(out_path, m.fsm,
                     f"N/A — not a 1-D autonomous map.\n\n"
                     f"{var['name']}(n+1) is driven by the held companion "
                     f"'{hname}', not by {var['name']}(n) alone:\nperturbing "
                     f"'{hname}' changes {var['name']}'s successor.\nScanning "
                     f"{var['name']} with '{hname}' pinned gives a false map\n"
                     f"(a flat line implying {var['name']} always collapses).\n"
                     f"A cobweb is only meaningful for a self-contained 1-D map.")
            return
    else:
        grid = list(range(len(m.enum_variants[var["name"]])))
        bounded = True

    facet = m.facet_var()
    if facet is not None and facet["name"] == var["name"]:
        facet = None
    sup = f"{m.fsm}  —  cobweb on  {var['name']}"
    if mode == "enum-ordinal":
        sup += "  (enum -> ordinal)"

    # Drop a facet that doesn't ENTER the primary var's map: if every facet value
    # yields the same cobweb (find's s5 — Unseen/Visited are identical), faceting just
    # duplicates one panel N times. Collapse identical panels; only keep the facet if
    # >= 2 genuinely-distinct maps survive. Otherwise fall through to a single panel.
    groups = None
    if facet is not None:
        groups = _distinct_facet_groups(m, var, mode, base, grid, facet)
        if len(groups) < 2:
            facet = None

    # ---- faceted: one cobweb panel per DISTINCT map (collapsed facet values) ----
    if facet is not None:
        n = len(groups)
        ncol = min(n, 3)
        nrow = (n + ncol - 1) // ncol
        fig, axes = plt.subplots(nrow, ncol, figsize=(4.6 * ncol, 4.6 * nrow),
                                 squeeze=False)
        for k, (vals, _fp) in enumerate(groups):
            ax = axes[k // ncol][k % ncol]
            panel_base = dict(base)
            panel_base[facet["name"]] = vals[0]
            label = f"{facet['name']} = " + (
                str(vals[0]) if len(vals) == 1
                else "{" + ", ".join(str(v) for v in vals) + "}")
            _draw_panel(ax, m, var, mode, panel_base, grid,
                        bounded, panel_label=label)
        # blank any unused cells
        for k in range(n, nrow * ncol):
            axes[k // ncol][k % ncol].set_axis_off()
        held = [v["name"] for v in m.state_vars
                if v["name"] not in (var["name"], facet["name"])]
        sub = sup + f"   faceted by {facet['name']}"
        if held:
            sub += "   (held: " + ", ".join(held) + ")"
        fig.suptitle(sub, fontsize=12)
        # shared axis labels
        for r in range(nrow):
            axes[r][0].set_ylabel(var["name"] + "  (n+1)")
        for c in range(ncol):
            axes[nrow - 1][c].set_xlabel(var["name"] + "  (n)")
        handles, labels = axes[0][0].get_legend_handles_labels()
        if handles:
            fig.legend(handles, labels, loc="lower center", ncol=len(labels),
                       fontsize=9, bbox_to_anchor=(0.5, -0.04))
        fig.tight_layout(rect=(0, 0.06, 1, 0.94))
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    # ---- single panel ----
    fig, ax = plt.subplots(figsize=(7.5, 7.5))
    ok = _draw_panel(ax, m, var, mode, base, grid, bounded)
    held = [v["name"] for v in m.state_vars if v["name"] != var["name"]]
    if ok:
        ax.set_xlabel(var["name"] + ("  (n)" if mode == "int" else "  ordinal (n)"))
        ax.set_ylabel(var["name"] + ("  (n+1)" if mode == "int" else "  ordinal (n+1)"))
        ax.legend(loc="upper left", fontsize=9)
    sub = sup
    if held:
        sub += "   (held: " + ", ".join(held) + ")"
    ax.set_title(sub, fontsize=11)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_cobweb.py <smt2> <schema> <out_path>", file=sys.stderr)
        sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
