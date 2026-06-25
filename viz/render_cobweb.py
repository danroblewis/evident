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
# Channel selection + map/orbit sampling (the data layer) live in the sibling
# module; this file keeps the drawing + dispatch.
from cobweb_sample import (
    _pick_primary, _base_state, _numeric_range, _distinct_facet_groups,
    _depends_on_held, _reachable_count, _sample_map, _staircase, _seed_for,
    _from_ord,
)
from axis_select import resolve_axes, write_axes


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



def _resolve_grid(m, out_path, var, mode):
    """Run the cobweb's honesty guards (degeneracy, numeric range, non-autonomy)
    and return (base, grid, bounded) to draw, or None if it already emitted an N/A
    card. NB: the order of model queries here is load-bearing — `_reachable_count`
    runs BEFORE `_base_state` (matching the original flow) so that for a
    nondeterministic transition the downstream witnesses are unchanged."""
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
            return None

    base = _base_state(m)
    if mode != "int":
        return base, list(range(len(m.enum_variants[var["name"]]))), True

    grid, bounded = _numeric_range(m, var, base)
    if grid is None:                    # axis_bounds None AND no fallback fired
        _na_card(out_path, m.fsm,
                 f"N/A — no numeric range for {var['name']}; "
                 "cobweb not meaningful.")
        return None
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
        return None
    return base, grid, bounded


def _render_faceted(m, out_path, var, mode, base, grid, bounded, facet, groups, sup):
    """One cobweb panel per DISTINCT map (facet values sharing a map collapse)."""
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


def render(smt2, schema, out_path, x_var=None, y_var=None):
    m = load(smt2, schema)
    var, mode = _pick_primary(m)

    if var is None:
        _na_card(out_path, m.fsm, "N/A: no state var to cobweb")
        return
    # #445: a cobweb is 1-D (x_n → x_{n+1} of ONE scalar), so only x_var applies — honor it when it
    # names a real numeric var, else keep _pick_primary's auto-pick. The view's y axis IS x (the same
    # scalar one tick later), so echo y = x; y_var is ignored.
    if x_var:
        var, _y, axinfo = resolve_axes(m, x_var, None, var, var)
        axinfo["y"] = axinfo["x"]
        write_axes(out_path, axinfo)

    resolved = _resolve_grid(m, out_path, var, mode)
    if resolved is None:
        return
    base, grid, bounded = resolved

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

    if facet is not None:
        _render_faceted(m, out_path, var, mode, base, grid, bounded, facet,
                        groups, sup)
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
