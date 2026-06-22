"""The renderer registry + render helpers.

Imports every renderer once and calls it in-process (matplotlib stays warm). A
renderer exposes `render(smt2, schema, out_path)`; the few that only ship a CLI
`main()` are wrapped by patching argv around the call (serialized by `config._LOCK`,
so the global mutation is safe). Also owns `_maybe_claim`, which renders a raw
claim's SOLVED solution space (a claim has no run to step).

Requires `viz/` on `sys.path` (server inserts it before importing this module).
"""
import base64
import inspect
import json
import sys

from evident_viz import load as load_model

from analysis import _dropped_locs

# The full promised gallery — all sixteen views. A renderer exposes
# render(smt2, schema, out_path); the few that only ship a CLI main() are wrapped by
# patching argv around the call (serialized by _LOCK, so the global mutation is safe).
ALL_VIEWS = (
    "solution_space",                                  # the SOLVED boundary, not a run — lead view
    "time_series", "state_graph", "phase_portrait", "reachability_tree",
    "morse_graph", "occupancy_heatmap", "timing_diagram", "transition_matrix",
    "basin_map", "orbit_scatter", "scatter_matrix", "parallel_coords",
    "chord_diagram", "nullcline_field", "fixedpoint_map", "cobweb",
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


def _render_png(view, prefix):
    """Render the view PNG and return (bytes, points). `points` is the interactive
    hover-overlay sidecar (`<out>.points.json`, written by renderers that support it —
    currently solution_space): a list of {fx, fy, state}; [] when no sidecar exists."""
    out = prefix + f".{view}.png"
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


def _maybe_claim(prefix, dropped, source="", msg=""):
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
    png = b""
    try:
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
        "vars": list(bounds.keys()), "view": "claim_space", "views": ["claim_space"],
        "png": base64.b64encode(png).decode() if png else None,
        "warnings": msg if dropped else "",
        "dropped_locs": _dropped_locs(source, msg) if dropped else [],
    }
