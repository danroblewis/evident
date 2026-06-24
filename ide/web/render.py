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


def _render_png(view, prefix, all_conditions=False):
    """Render the view PNG and return (bytes, points). `points` is the interactive
    hover-overlay sidecar (`<out>.points.json`, written by renderers that support it —
    currently solution_space): a list of {fx, fy, state}; [] when no sidecar exists.

    `all_conditions` requests state_graph's GLOBAL-dynamics graph (every initial
    condition). Threaded via the renderer module flag under the server's render _LOCK
    (the renderers ship a fixed 3-arg signature), then restored so it never leaks."""
    out = prefix + f".{view}.png"
    with _all_conditions(view, all_conditions):
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
    # A claim is static, so it gets the views that work without a run. claim_space = the solved
    # feasible region; solution_structure = what it DETERMINES (backbone / free / implied equalities).
    CLAIM_VIEWS = ["claim_space", "solution_structure"]
    view = view if view in CLAIM_VIEWS else "claim_space"
    png = b""
    try:
        if view == "solution_structure":
            import render_solution_structure as RSS
            RSS.render(smt2, schema, prefix + "_claim.png")
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
                      "fixed_points": [], "bounds": bounds, "reachable": 0, "capped": False,
                      "branching": 1},
        "dropped": dropped, "branching": 1, "states": 0, "edges": 0, "capped": False,
        "vars": list(bounds.keys()), "view": view, "views": CLAIM_VIEWS,
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
