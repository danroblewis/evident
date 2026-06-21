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


def _pick_facet(m, primary, max_card=5):
    """A low-cardinality categorical var to facet by (one panel per value),
    or None. Prefers an enum (named panels) over a bool; never the primary."""
    best = None
    for v in m.categorical_vars:
        if v["name"] == primary["name"]:
            continue
        card = len(m.enum_variants[v["name"]]) if v["kind"] == "enum" else 2
        if card > max_card:
            continue
        # enum beats bool (named panels read better); fewer panels preferred
        rank = (0 if v["kind"] == "enum" else 1, card)
        if best is None or rank < best[0]:
            best = (rank, v, card)
    return best[1] if best else None


def _facet_values(m, var):
    if var["kind"] == "enum":
        return list(m.enum_variants[var["name"]])
    return [False, True]


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


def _numeric_range(m, var, base):
    """Determine the cobweb x-range for a numeric var. If the system is bounded
    and small (a counter-like state, found via reachable/sampled states), grid
    that exact range; otherwise span a generous window for big-scale fixed-point
    systems (van der Pol etc.). Returns (grid_values, is_bounded)."""
    name = var["name"]
    vals = set()
    try:
        states, _ = m.reachable(limit=400)
        for s in states:
            v = s.get(name)
            if isinstance(v, (int, float)):
                vals.add(int(v))
    except Exception:
        pass
    # A genuinely BOUNDED counter (vending: balance 0..3) reaches several
    # distinct small values. A big fixed-point system (van der Pol) seeded at the
    # origin reaches only the fixed point — 1-3 clustered points — which we must
    # NOT mistake for a small range. Require a real spread of distinct values.
    if len(vals) >= 4:
        lo, hi = min(vals), max(vals)
        span = hi - lo
        if span <= 64:                      # small bounded counter: grid it exactly
            pad = max(1, span // 2)
            grid = list(range(lo - pad, hi + pad + 1))
            return grid, True
    # large-scale / fixed-point-at-origin: generous symmetric window.
    lo, hi, n = -3200, 3200, 121
    grid = [lo + (hi - lo) * i // (n - 1) for i in range(n)]
    return grid, False


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
    if mode == "int":
        if bounded:
            return _to_ord(m, var, base.get(var["name"], lo))
        seed = 2000                       # near the limit cycle for big systems
        return seed if lo <= seed <= hi else int((lo + hi) / 2)
    return _to_ord(m, var, base[var["name"]])


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


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    var, mode = _pick_primary(m)

    if var is None:
        fig, ax = plt.subplots(figsize=(7.5, 7.5))
        ax.text(0.5, 0.5, "N/A: no state var to cobweb",
                ha="center", va="center", fontsize=13)
        ax.set_axis_off()
        ax.set_title(f"{m.fsm}  —  cobweb")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    base = _base_state(m)
    if mode == "int":
        grid, bounded = _numeric_range(m, var, base)
    else:
        grid = list(range(len(m.enum_variants[var["name"]])))
        bounded = True

    facet = _pick_facet(m, var)
    sup = f"{m.fsm}  —  cobweb on  {var['name']}"
    if mode == "enum-ordinal":
        sup += "  (enum -> ordinal)"

    # ---- faceted: one cobweb panel per categorical value (adds a dimension) ----
    if facet is not None:
        values = _facet_values(m, facet)
        n = len(values)
        ncol = min(n, 3)
        nrow = (n + ncol - 1) // ncol
        fig, axes = plt.subplots(nrow, ncol, figsize=(4.6 * ncol, 4.6 * nrow),
                                 squeeze=False)
        for k, val in enumerate(values):
            ax = axes[k // ncol][k % ncol]
            panel_base = dict(base)
            panel_base[facet["name"]] = val
            label = f"{facet['name']} = {val}"
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
