#!/usr/bin/env python3
"""render_phase_portrait.py — a phase-portrait (vector/direction field) renderer
for ANY Evident program's exported transition IR.

    python3 viz/render_phase_portrait.py <smt2> <schema> <out.png>

The picture is a difference-equation phase portrait: every state is a point in a
plane, and the field is the displacement successor(p) - p. The AXES carry the
two most expressive variables; a low-cardinality categorical, when present, is
lifted off the plane and used to FACET — one panel per value — which is the
honest way to ADD a dimension instead of cramming a 3rd variable onto a single
plot's color/jitter (Cleveland-McGill / Mackinlay: position is the strong
channel, facet is the dimension-adder for categoricals).

Channel mapping (via evident_viz):
  * AXES  = numeric_vars[:2] when the model has >=2 numerics (a true continuous
    field over value-space); otherwise the two most expressive vars of any kind,
    enums encoded as ordinals (enum_variants tick labels) and bools as 0/1.
  * COLOR = the derived STEP MAGNITUDE of the field (a coarse quantitative
    gradient — the one good quantitative use of hue). We keep this rather than
    recoloring by a variable; the variables ride the axes + facet.
  * FACET = a low-cardinality (<=~5) categorical, when one exists and is NOT
    already an axis. Each panel is the field/graph restricted to that value.

Two field regimes, both driven only by querying the transition via evident_viz:

  * NUMERIC (>=2 int/real vars): pin an arbitrary grid of points in value-space
    (we are NOT limited to reachable states), query successor() at each, draw the
    magnitude-colored field. Overlay trajectories from several seeds.

  * DISCRETE / MIXED (fewer than 2 numeric axis vars): there is no continuum, so
    we enumerate the reachable graph, project each visited state onto the two
    chosen (possibly ordinalized) axes, and draw the real transition arrows.
    Still a phase portrait — the arrows are the difference equation's image.

Degrades gracefully: <2 distinguishable axes, or an empty field, still emits a
titled figure (placeholder / projection).
"""
import sys
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load
# The numeric (continuous vector-field / orbit) machinery + the value<->plane
# projection primitives live in phase_portrait_field; this file owns channel
# planning, the discrete-graph regime, faceting layout, and orchestration.
from phase_portrait_field import (
    _numeric, _is_numeric, _axis_ticks, _cardinality,
    _orbit_extent, _robust_span, render_numeric_panel,
    _numeric_regime, _dwell_span, _reachable_extent, _FINITE_STATE_CAP,
    render_discrete_panel, _bounds_of,
)
# Interactive hover-overlay sidecar (#184 increment 3): only the MAIN scatter
# axis is hoverable — the numeric orbit's trajectory states / the discrete
# regime's reachable states (never the vector-field grid arrows). Un-faceted
# panels save PLAIN (figure_fraction); faceted panels save TIGHT (tight_fraction).
from overlay_points import write_points, figure_fraction, tight_fraction


# ----- channel assignment: axes (numeric-first) + a facet categorical -------
def plan_channels(m):
    """Decide axes, optional facet var, and regime from the ranked vars.

    Returns (axx, axy, facet_var, regime). The phase portrait NEEDS numeric axes
    for a true field, so when >=2 numerics exist we take numeric_vars[:2] for the
    plane directly (rather than assign_channels, which would put a categorical on
    y). The facet (when one exists) is the SUITABLE facet var from evident_viz —
    a low-cardinality categorical that stays ~constant within a run — so the
    dynamics live INSIDE a panel instead of being cut across panels. A var on the
    limit cycle (high within-run change rate) returns None: we then DON'T facet."""
    numeric = m.numeric_vars

    def facetable(exclude_names):
        """The suitable facet var (low-card, low within-run change rate), unless
        it's already consumed by an axis."""
        fv = m.facet_var()
        if fv is None or fv["name"] in exclude_names:
            return None
        return fv

    if len(numeric) >= 2:
        axx, axy = numeric[0], numeric[1]
        # a numeric field; facet by a suitable low-card categorical if one exists
        facet = facetable({axx["name"], axy["name"]})
        return axx, axy, facet, "numeric"

    # fewer than 2 numerics -> discrete/mixed projection. Facet by the suitable
    # low-cardinality categorical FIRST, then pick the two most expressive
    # remaining vars (numeric preferred) for the axes.
    facet = facetable(set())
    used = {facet["name"]} if facet is not None else set()
    axis_pool = [v for v in m.state_vars if v["name"] not in used]
    if len(axis_pool) < 2:
        # not enough left once we facet — don't facet, use the top-2 as axes
        facet = None
        axis_pool = list(m.state_vars)
    if len(axis_pool) < 2:
        return None, None, None, "degenerate"
    # axes: numerics first, then highest-cardinality categoricals
    num_axes = [v for v in axis_pool if _is_numeric(v)]
    cat_axes = sorted((v for v in axis_pool if not _is_numeric(v)),
                      key=lambda v: _cardinality(m, v), reverse=True)
    ordered = num_axes + cat_axes
    axx, axy = ordered[0], ordered[1]
    regime = "mixed" if num_axes else "discrete"
    return axx, axy, facet, regime


# ----- axis decoration ------------------------------------------------------
def _decorate_axes(m, ax, axx, axy):
    tx = _axis_ticks(m, axx)
    if tx is not None:
        ax.set_xticks(tx[0])
        ax.set_xticklabels(tx[1], rotation=30, ha="right", fontsize=8)
    ty = _axis_ticks(m, axy)
    if ty is not None:
        ax.set_yticks(ty[0])
        ax.set_yticklabels(ty[1], fontsize=8)


def _axis_label(var):
    suffix = {"bool": " (0/1)", "enum": " (ordinal)"}.get(var["kind"], "")
    return f"{var['name']}{suffix}"


# ----- facet helpers --------------------------------------------------------
def _facet_values(m, facet_var):
    if facet_var["kind"] == "enum":
        return list(m.enum_variants[facet_var["name"]])
    if facet_var["kind"] == "bool":
        return [False, True]
    return None


# ----- top-level orchestration ----------------------------------------------
def render(smt2_path, schema_path, out_path):
    m = load(smt2_path, schema_path)
    axx, axy, facet_var, regime = plan_channels(m)

    if regime == "degenerate":
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        ax.text(0.5, 0.5,
                f"N/A for {len(m.state_vars)}-var state:\n"
                "phase portrait needs 2 axes",
                ha="center", va="center", transform=ax.transAxes, fontsize=13)
        ax.set_xticks([]); ax.set_yticks([])
        ax.set_title(f"{m.fsm} — phase portrait", fontsize=13)
        fig.tight_layout()
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        write_points(out_path, [])           # degenerate → no hoverable points
        return out_path

    facet_vals = _facet_values(m, facet_var) if facet_var is not None else None

    if regime == "numeric":
        # A continuous vector field is only HONEST when the reachable dynamics are
        # genuinely continuous/unbounded — a limit cycle, an open orbit. A
        # TERMINATING numeric program (a counter that marches 0..10 and halts) has
        # a small FINITE reachable set; gridding a guessed field over it fabricates
        # cycles/basins/fixed-point stars the program never enters (the bug).
        # Classify by the program's OWN reachable set, never a guessed box.
        kind = _numeric_regime(m, axx, axy)
        if kind == "finite":
            # finite reachable march -> the honest transition graph, NOT a field
            _render_discrete(m, axx, axy, facet_var, facet_vals, "mixed", out_path)
        elif kind == "bounded":
            # large/non-terminating reachable march but a BOUNDED real domain
            # (lru's caches, randomwalk's visit counters, life's clock): grid the
            # field over axis_bounds — the robust reachable extent — never the
            # perturbation-grown box that fabricated ±20000/±470000 axes.
            _render_numeric(m, axx, axy, facet_var, facet_vals, out_path,
                            extent_mode="reachable")
        elif kind == "continuous":
            # genuine continuum (vanderpol's limit cycle): the reachable set is a
            # lone fixed point, so the orbit must be discovered by perturbation.
            _render_numeric(m, axx, axy, facet_var, facet_vals, out_path,
                            extent_mode="orbit")
        else:  # "degenerate": no 2D field (a constant axis, or no orbit)
            _render_na(m, axx, axy, _na_reason(m, axx, axy), out_path)
    else:
        _render_discrete(m, axx, axy, facet_var, facet_vals, regime, out_path)
    return out_path


def _na_reason(m, axx, axy):
    """Why a 2-numeric program has no meaningful phase portrait — a constant axis
    (one variable never moves over the program's followed trajectory) or a lone
    fixed point. We judge constancy on the DWELL (the trajectory the program truly
    visits), not on m.reachable()'s relational fan: that fan can move an axis the
    real run holds fixed (randomwalk's v3/v4), which would mislabel the reason."""
    nx_, ny_ = axx["name"], axy["name"]
    dwell = _dwell_span(m, axx, axy)
    if dwell is not None:
        (xlo, xhi), (ylo, yhi) = dwell
        if xhi - xlo < 1e-9 or yhi - ylo < 1e-9:
            flat = nx_ if xhi - xlo < 1e-9 else ny_
            return (f"N/A — {flat} is constant across the reachable trajectory;\n"
                    "a phase portrait needs two varying axes")
    return ("N/A — reachable set is a single fixed point;\n"
            "no continuum and no orbit for a phase portrait")


def _render_na(m, axx, axy, msg, out_path):
    fig, ax = plt.subplots(figsize=(8.5, 7.5))
    ax.text(0.5, 0.5, msg, ha="center", va="center",
            transform=ax.transAxes, fontsize=13)
    ax.set_xticks([]); ax.set_yticks([])
    ax.set_title(f"{m.fsm} — phase portrait", fontsize=13)
    fig.tight_layout()
    fig.savefig(out_path, dpi=120)
    plt.close(fig)
    write_points(out_path, [])               # N/A → no hoverable points


def _panel_grid(n):
    cols = min(n, 3)
    rows = (n + cols - 1) // cols
    return rows, cols


def _render_numeric(m, axx, axy, facet_var, facet_vals, out_path,
                    extent_mode="orbit"):
    """`extent_mode` selects the field DOMAIN — the one lever that decides whether
    the picture is honest or fabricated:
      * "reachable" — grid over axis_bounds (the robust reachable extent). Used
        for a bounded march (lru/randomwalk); never invents an off-domain box.
      * "orbit"     — grow a perturbation orbit off the fixed point. Used ONLY
        for a genuine continuum whose reachable set is a single point (vanderpol).
    """
    init = m.initial_state() or {v["name"]: 0 for v in m.state_vars}

    def _extent(pin):
        if extent_mode == "reachable":
            return _reachable_extent(m, axx, axy)
        return _orbit_extent(m, axx, axy, pin)

    subtitle = ("(numeric vector field, reachable extent)"
                if extent_mode == "reachable"
                else "(numeric vector field, reachable-orbit extent)")

    # In reachable mode the grid covers axis_bounds, but the dynamics may dwell in
    # a sub-region; let the panel snap its frame to the data it plotted. (Orbit
    # mode keeps its symmetric box — the limit cycle must read centred.)
    fit = (extent_mode == "reachable")

    if facet_var is None:
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        pin = {v["name"]: init[v["name"]] for v in m.state_vars}
        extent = _extent(pin)
        overlay = []
        render_numeric_panel(m, ax, axx, axy, pin, draw_colorbar=True,
                             extent=extent, fit_to_data=fit, overlay=overlay)
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        ax.set_title(f"{m.fsm} — phase portrait\n" + subtitle, fontsize=13)
        fig.tight_layout()
        points = figure_fraction(fig, overlay)   # plain savefig → figure-relative
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        write_points(out_path, points)
        return

    rows, cols = _panel_grid(len(facet_vals))
    fig, axes = plt.subplots(rows, cols, figsize=(5.2 * cols, 4.8 * rows),
                             squeeze=False)
    flat = [axes[r][c] for r in range(rows) for c in range(cols)]
    last_q = None
    # Faceted panels MUST share one frame so they're comparable; in reachable mode
    # snap that shared frame to the union of all panels' plotted data (so an empty
    # upper grid isn't carried across every panel).
    panel_xlims, panel_ylims = [], []
    overlay = []
    for idx, fval in enumerate(facet_vals):
        ax = flat[idx]
        pin = {v["name"]: init[v["name"]] for v in m.state_vars}
        pin[facet_var["name"]] = fval
        extent = _extent(pin)
        q = render_numeric_panel(m, ax, axx, axy, pin, draw_colorbar=False,
                                 extent=extent, fit_to_data=fit, overlay=overlay)
        if fit:
            panel_xlims.append(ax.get_xlim()); panel_ylims.append(ax.get_ylim())
        if q is not None:
            last_q = q
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        ax.set_title(f"{facet_var['name']} = {fval}", fontsize=11)
    # unify the per-panel data-fit frames so the small multiples stay comparable
    if fit and panel_xlims:
        sxlo = min(l for l, _ in panel_xlims); sxhi = max(h for _, h in panel_xlims)
        sylo = min(l for l, _ in panel_ylims); syhi = max(h for _, h in panel_ylims)
        for idx in range(len(facet_vals)):
            flat[idx].set_xlim(sxlo, sxhi); flat[idx].set_ylim(sylo, syhi)
    for j in range(len(facet_vals), len(flat)):
        flat[j].axis("off")
    if last_q is not None:
        fig.colorbar(last_q, ax=axes.ravel().tolist(), fraction=0.025,
                     pad=0.02, label="step magnitude")
    fig.suptitle(f"{m.fsm} — phase portrait  (faceted by {facet_var['name']})",
                 fontsize=14)
    points = tight_fraction(fig, overlay)        # tight savefig → crop-relative
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, points)


def _render_discrete(m, axx, axy, facet_var, facet_vals, regime, out_path):
    states, edges = m.reachable(limit=3000)
    if not states:
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        ax.text(0.5, 0.5, "N/A: no reachable states\n(initial_state is None)",
                ha="center", va="center", transform=ax.transAxes, fontsize=12)
        ax.set_title(f"{m.fsm} — phase portrait", fontsize=13)
        fig.tight_layout()
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        write_points(out_path, [])           # no reachable states → no points
        return

    init_key = m._key(states[0])
    bounds = _bounds_of(m, states, axx, axy)

    if facet_var is None:
        fig, ax = plt.subplots(figsize=(8.5, 7.5))
        overlay = []
        render_discrete_panel(m, ax, axx, axy, states, edges, init_key, bounds,
                              overlay=overlay)
        ax.legend(loc="upper right", fontsize=8)
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        ax.text(0.02, 0.98,
                f"{len(states)} reachable states, {len(edges)} transitions",
                transform=ax.transAxes, fontsize=8, color="gray", va="top")
        ax.set_title(f"{m.fsm} — phase portrait\n(discrete transition graph)",
                     fontsize=13)
        fig.tight_layout()
        points = figure_fraction(fig, overlay)   # plain savefig → figure-relative
        fig.savefig(out_path, dpi=120)
        plt.close(fig)
        write_points(out_path, points)
        return

    # FACET: one panel per facet value. A state belongs to a panel by its facet
    # value; an edge stays IN the panel only if both endpoints share it (a
    # cross-facet edge would need a 3rd axis to draw honestly, so we annotate
    # the count instead of drawing a misleading in-plane arrow).
    fname = facet_var["name"]
    # Only facet over values that actually occur in the reachable set. An enum
    # may declare variants the program never reaches (find's s5: Unseen declared
    # but never visited) — drawing an empty panel for each is noise, not a view.
    present = {s[fname] for s in states}
    facet_vals = [v for v in facet_vals if v in present]
    rows, cols = _panel_grid(len(facet_vals))
    fig, axes = plt.subplots(rows, cols, figsize=(5.4 * cols, 4.8 * rows),
                             squeeze=False)
    flat = [axes[r][c] for r in range(rows) for c in range(cols)]

    overlay = []
    for idx, fval in enumerate(facet_vals):
        ax = flat[idx]
        keep = [i for i, s in enumerate(states) if s[fname] == fval]
        remap = {gi: li for li, gi in enumerate(keep)}
        sub_states = [states[gi] for gi in keep]
        sub_edges = [(remap[a], remap[b]) for (a, b) in edges
                     if a in remap and b in remap]
        crossing = sum(1 for (a, b) in edges
                       if (a in remap) != (b in remap)
                       and (a in remap or b in remap))
        render_discrete_panel(m, ax, axx, axy, sub_states, sub_edges,
                              init_key, bounds, overlay=overlay)
        ax.set_xlabel(_axis_label(axx)); ax.set_ylabel(_axis_label(axy))
        _decorate_axes(m, ax, axx, axy)
        ax.grid(True, ls=":", alpha=0.3)
        note = f"{len(sub_states)} states"
        if crossing:
            note += f", {crossing} cross-facet"
        ax.text(0.02, 0.98, note, transform=ax.transAxes, fontsize=7,
                color="gray", va="top")
        ax.set_title(f"{fname} = {fval}", fontsize=11)

    # one shared legend
    handles, labels = flat[0].get_legend_handles_labels()
    for j in range(len(facet_vals), len(flat)):
        flat[j].axis("off")
    if handles:
        fig.legend(handles, labels, loc="lower center", ncol=len(labels),
                   fontsize=9, frameon=True)
    fig.suptitle(
        f"{m.fsm} — phase portrait  (faceted by {fname}; "
        f"{len(states)} states, {len(edges)} transitions)", fontsize=13)
    fig.tight_layout(rect=(0, 0.05, 1, 0.96))
    points = figure_fraction(fig, overlay)   # plain savefig → figure-relative
    fig.savefig(out_path, dpi=120)
    plt.close(fig)
    write_points(out_path, points)


def main(argv):
    if len(argv) != 4:
        print("usage: render_phase_portrait.py <smt2> <schema> <out.png>",
              file=sys.stderr)
        return 2
    out = render(argv[1], argv[2], argv[3])
    size = os.path.getsize(out)
    print(f"wrote {out} ({size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
