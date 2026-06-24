"""The renderer registry + render helpers.

Imports every renderer once and calls it in-process (matplotlib stays warm). A
renderer exposes `render(smt2, schema, out_path)`; the few that only ship a CLI
`main()` are wrapped by patching argv around the call (serialized by `config._LOCK`,
so the global mutation is safe). Also owns `_maybe_claim`, which renders a raw
claim's SOLVED solution space (a claim has no run to step).

Requires `viz/` on `sys.path` (server inserts it before importing this module).
"""
import base64
import contextlib
import inspect
import json
import sys

from evident_viz import load as load_model
from functionize import function_summary

from analysis import _dropped_locs

# The full promised gallery — all sixteen views. A renderer exposes
# render(smt2, schema, out_path); the few that only ship a CLI main() are wrapped by
# patching argv around the call (serialized by _LOCK, so the global mutation is safe).
ALL_VIEWS = (
    "solution_space",                                  # the SOLVED boundary, not a run — lead view
    "terminal_map",                                    # the ABSTRACT end-state map (where it can rest), Z3 over the one-step relation — not a run
    "reachable_region",                                # the ABSTRACT reachable region (k-induction bounding box), bounded-vs-unbounded — not a run
    "time_series", "state_graph", "phase_portrait", "reachability_tree",
    "morse_graph", "occupancy_heatmap", "timing_diagram", "transition_matrix",
    "basin_map", "orbit_scatter", "scatter_matrix", "parallel_coords",
    "chord_diagram", "nullcline_field", "fixedpoint_map", "cobweb", "space_time",
    # functionizer family — the COMPILED structure (how the solver reduced the constraints to
    # per-variable functions), not the dynamics. Opt-in tabs; never auto-recommended.
    "function_graph", "function_residual", "function_guards", "function_behavior", "function_complexity",
)


def _render_via_main(mod):
    """A renderer that only ships a CLI main(smt2 schema out via argv): patch argv around
    the call (serialized by _LOCK, so the global mutation is safe)."""
    def render(smt2, schema, out_path):
        saved = sys.argv
        sys.argv = ["render", smt2, schema, out_path]
        try:
            mod.main()
        finally:
            sys.argv = saved
    return render


def _render_via_model(fn):
    """A renderer whose render() takes a loaded Model + out_path, not file paths."""
    def render(smt2, schema, out_path):
        fn(load_model(smt2, schema), out_path)
    return render


def _adapt(mod):
    """Normalize a renderer to render(smt2, schema, out_path), whatever shape it ships:
    render(smt2, schema, out) · render(model, out) · or a CLI main()."""
    if hasattr(mod, "render"):
        try:
            n = len(inspect.signature(mod.render).parameters)
        except (TypeError, ValueError):
            n = 3
        return mod.render if n >= 3 else _render_via_model(mod.render)
    if hasattr(mod, "main"):
        return _render_via_main(mod)
    return None


RENDERERS = {}            # view name -> render(smt2, schema, out_path)
for _v in ALL_VIEWS:
    try:
        _fn = _adapt(__import__(f"render_{_v}"))
        if _fn is not None:
            RENDERERS[_v] = _fn
        else:
            print(f"[server] render_{_v}: no render()/main()", file=sys.stderr)
    except Exception as e:                     # a renderer that won't import just isn't offered
        print(f"[server] render_{_v} unavailable: {e}", file=sys.stderr)

VIEWS = [v for v in ALL_VIEWS if v in RENDERERS]

# The functionizer family renders from the CHEAP decomposition (extract/guard_analysis/summary, all
# <100ms) and needs NO reachable-set solve — so these views get a fast path that skips the dynamics
# bundle entirely. Without it, opening a function tab on a nonlinear-Real sample waits out the whole
# (now timeout-bounded, but still slow) dynamics solve for nothing (Ana #301).
FUNCTION_VIEWS = {v for v in VIEWS if v.startswith("function_")}

# #285: each view's RIGOR class — the HONESTY marker. Is the content PROVEN over all conditions (abstract
# Z3), EXHAUSTIVE (the full bounded-discrete state graph), or SAMPLED (trajectories / a capped or
# continuous fallback)? So a viewer never mistakes a sampled cloud for a proof.
# claim views are Optimize-exact / static — no run, no cap; always proven.
_ALWAYS_PROVEN = {"claim_space", "solution_structure"}
# FSM abstract bound-views: proven ONLY when exhaustive. On a capped/continuous model the bound/region
# falls back to a SAMPLED cap (the chart's own subtitle says so), so the badge must NOT claim 'proven' —
# it must agree with the chart, never over-claim (Ana #353; #285's whole point).
_BOUND_VIEWS = {"solution_space", "terminal_map", "reachable_region"}
_ENUMERATE_VIEWS = {"state_graph", "basin_map", "fixedpoint_map", "transition_matrix", "timing_diagram",
                    "time_series", "reachability_tree", "orbit_scatter"}


def view_rigor(view, capped=False, continuous=False):
    """The honesty class of a rendered view: 'proven' (abstract Z3 / static, exhaustive over all
    conditions), 'exhaustive' (the full bounded-discrete state graph), or 'sampled' (trajectories / a
    capped or continuous fallback). The bound-views AND the enumerate-views degrade to 'sampled' when the
    result capped or the model is continuous — only claim_space/solution_structure/function_* (Optimize-
    exact, no run) stay proven unconditionally (#285, #353: never badge 'proven' over a sampled chart)."""
    if view in _ALWAYS_PROVEN or view.startswith("function_"):
        return "proven"
    if view in _BOUND_VIEWS:
        return "proven" if not (capped or continuous) else "sampled"
    if view in _ENUMERATE_VIEWS:
        return "exhaustive" if not (capped or continuous) else "sampled"
    return "sampled"


@contextlib.contextmanager
def _all_conditions(view, on):
    """Set render_state_graph's ALL_CONDITIONS module flag for the duration of one render,
    then restore it. A no-op for any other view (only state_graph reads it). Safe because the
    server serializes renders under _LOCK, so the global never overlaps another request."""
    if view != "state_graph" or not on:
        yield
        return
    import render_state_graph as RSG
    saved = RSG.ALL_CONDITIONS
    RSG.ALL_CONDITIONS = True
    try:
        yield
    finally:
        RSG.ALL_CONDITIONS = saved


@contextlib.contextmanager
def _k_depth(view, k):
    """#327: set render_reachable_region's K_DEPTH module flag for one render (the same fixed-3-arg
    pattern as _all_conditions), capped at 64, then restore. A no-op for any other view or k≤1."""
    if view != "reachable_region" or not k or k <= 1:
        yield
        return
    import render_reachable_region as RRR
    saved = RRR.K_DEPTH
    RRR.K_DEPTH = max(1, min(int(k), 64))
    try:
        yield
    finally:
        RRR.K_DEPTH = saved


def _render_png(view, prefix, all_conditions=False, k=None):
    """Render the view PNG and return (bytes, points). `points` is the interactive
    hover-overlay sidecar (`<out>.points.json`, written by renderers that support it —
    currently solution_space): a list of {fx, fy, state}; [] when no sidecar exists.

    `all_conditions` requests state_graph's GLOBAL-dynamics graph (every initial
    condition); `k` requests reachable_region's k-induction depth (#327). Threaded via
    the renderer module flag under the server's render _LOCK (the renderers ship a fixed
    3-arg signature), then restored so they never leak."""
    out = prefix + f".{view}.png"
    with _all_conditions(view, all_conditions), _k_depth(view, k):
        RENDERERS[view](prefix + ".smt2", prefix + ".schema.json", out)
    with open(out, "rb") as f:
        png = f.read()
    points = []
    try:
        with open(out + ".points.json") as pf:
            loaded = json.load(pf)
            if isinstance(loaded, list):
                points = loaded
    except (OSError, ValueError):
        pass
    return png, points


def _render_svg(view, prefix):
    """Render the view as SVG (vector, publication-quality) and return the SVG text — the figure half
    of Ana #244. Same renderer code path as the PNG; matplotlib infers the format from the .svg out path."""
    out = prefix + f".{view}.svg"
    RENDERERS[view](prefix + ".smt2", prefix + ".schema.json", out)
    with open(out) as f:
        return f.read()


def _maybe_claim(prefix, dropped, source="", msg="", view="claim_space"):
    """If the export produced a CLAIM schema (no FSM), render its SOLVED solution space (exact
    z3-Optimize bounds + per-cell feasible region) and return the analyze response; else None so
    the FSM path runs. A claim has no run to step — this is the purest solved-boundary view."""
    import z3, json as _json
    try:
        sch = _json.load(open(prefix + ".schema.json"))
    except Exception:
        return None
    if "claim" not in sch or "fsm" in sch:
        return None
    smt2, schema = prefix + ".smt2", prefix + ".schema.json"
    import render_claim_space as RC
    feasible, bounds = True, {}
    try:
        _, body, consts = RC._load_claim(smt2, schema)
        s = z3.Solver(); s.add(body)
        feasible = s.check() == z3.sat
        for v in sch.get("vars", []):
            if v.get("kind") in ("int", "real") and v["name"] in consts:
                lo = RC._opt_bound(body, consts[v["name"]], False)
                hi = RC._opt_bound(body, consts[v["name"]], True)
                if lo is not None and hi is not None:
                    bounds[v["name"].split(".")[-1]] = [lo, hi]
    except Exception as e:
        print(f"[server] claim bounds failed: {type(e).__name__}: {e}", file=sys.stderr)
    # #341: the implied relations + their forcing-constraint proof cores, for the interrogable structure
    # panel (shown on either claim tab, independent of the active PNG view).
    decomp = {}                                    # #338: the FULL backbone/free/equalities/relations decomp
    if feasible:
        try:
            from claim_structure import solution_structure
            decomp = solution_structure(smt2, schema)
        except Exception as e:
            print(f"[server] claim structure failed: {type(e).__name__}: {e}", file=sys.stderr)
    # A claim is static, so it gets the views that work without a run. claim_space = the solved
    # feasible region; solution_structure = what it DETERMINES (backbone / free / implied equalities).
    CLAIM_VIEWS = ["claim_space", "solution_structure"]
    if len(bounds) >= 2:                            # #356: ≥2 numeric vars → also the witness-cloud sampling
        CLAIM_VIEWS += ["scatter_matrix", "parallel_coords"]   # views (the #284 claim renderers were dead-ended)
    view = view if view in CLAIM_VIEWS else "claim_space"
    png = b""
    try:
        if view == "solution_structure":
            import render_solution_structure as RSS
            RSS.render(smt2, schema, prefix + "_claim.png")
        elif view in ("scatter_matrix", "parallel_coords"):
            RENDERERS[view](smt2, schema, prefix + "_claim.png")   # #356: their claim paths sample the witnesses
        else:
            RC.render(smt2, schema, prefix + "_claim.png")
        png = open(prefix + "_claim.png", "rb").read()
    except Exception as e:
        print(f"[server] claim render failed: {type(e).__name__}: {e}", file=sys.stderr)
    return {
        "ok": True,
        "banner": ("a claim (a relation) — its SOLUTION SPACE, fully solved (no run; "
                   "press ⊨ Solve for one witness)" if feasible else
                   "a claim — UNSATISFIABLE (no assignment satisfies it; ⊨ Solve to see why)"),
        "structure": {"verdict": "satisfiable" if feasible else "unsatisfiable", "claim": True,
                      "fixed_points": [], "bounds": bounds, "relations": decomp.get("relations", []),
                      "backbone": decomp.get("backbone", []), "equalities": decomp.get("equalities", []),
                      "inequalities": decomp.get("inequalities", []),   # #338: full decomp, queryable as data
                      "reachable": 0, "capped": False, "branching": 1},
        "dropped": dropped, "branching": 1, "states": 0, "edges": 0, "capped": False,
        "vars": list(bounds.keys()), "view": view, "rigor": view_rigor(view), "views": CLAIM_VIEWS,
        "png": base64.b64encode(png).decode() if png else None,
        "warnings": msg if dropped else "",
        "dropped_locs": _dropped_locs(source, msg) if dropped else [],
    }


def _function_response(m, view, prefix, dropped, source, msg):
    """Render a functionizer-family view from the cheap decomposition alone — no reachable()/structure
    (Ana #301). Sibling of _maybe_claim: builds the analyze response for a model that needs no dynamics
    solve. The banner is the compiled-structure summary, not the dynamics shape."""
    summ = function_summary(m)
    banner = (f"compiled structure — {summ['n_func_carried']} of {summ['n_carried']} carried var(s) "
              f"functionized ({summ['pct']:.0f}%) · {summ['coupling']}"
              + (f" · {len(summ['cycles'])} feedback cycle(s)" if summ['cycles'] else ""))
    png, points = b"", []
    try:
        png, points = _render_png(view, prefix)
    except Exception as e:
        print(f"[server] render {view} failed: {type(e).__name__}: {e}", file=sys.stderr)
    return {
        "ok": True, "banner": banner, "structure": None, "dropped": dropped,
        "branching": 1, "states": 0, "edges": 0, "capped": False,
        "vars": [v["name"].split(".")[-1] for v in m.interface_vars]
                + [v["name"].split(".")[-1] for v in getattr(m, "derived", [])],
        "view": view, "views": VIEWS,
        "png": base64.b64encode(png).decode() if png else None, "points": points,
        "warnings": msg if dropped else "",
        "dropped_locs": _dropped_locs(source, msg) if dropped else [],
    }
