#!/usr/bin/env python3
"""Evident Web IDE — M0 backend.

Wraps the Rust runtime (`evident export`) + the viz model-semantics layer
(`viz/evident_viz.py`) + the renderers, and serves the single-page front end. The one
endpoint that matters is POST /api/analyze: source text in, and out comes the
model-shape banner, the dropped-constraint honesty count, the reachable-set stats, and
the recommended view rendered to a PNG — i.e. everything the live write→see loop needs.

This module is the FastAPI wiring only — the `app`, middleware, request models, the
`@app.post` endpoints (each a thin wrapper over an extracted helper), and the index/static
serving. The work lives in sibling modules: `runtime_io` (`evident` subprocess calls),
`render` (the renderer registry + claim view), `analysis` (banner/recommend + dropped-locs),
`solve` (witness enumeration + unsat core), `smtlib_tools` (SMT-LIB export + query parse),
and `config` (shared paths + the serialization lock).

Run:  python3 -m uvicorn ide.web.server:app --host 0.0.0.0 --port 5173
(or:  python3 ide/web/server.py)
"""
import os
import re
import sys
import tempfile

from config import EVIDENT, REACH_LIMIT, STATIC, VIZ, _LOCK, effective_scope

sys.path.insert(0, VIZ)

import matplotlib  # noqa: E402
matplotlib.use("Agg")

from evident_viz import load as load_model  # noqa: E402

from fastapi import FastAPI  # noqa: E402
from fastapi.responses import Response  # noqa: E402
from fastapi.staticfiles import StaticFiles  # noqa: E402
from pydantic import BaseModel  # noqa: E402

from analysis import (  # noqa: E402
    _dropped_locs, _dynamics_response, _error_loc, _model_diff)
from render import (  # noqa: E402
    RENDERERS, VIEWS, _maybe_claim, _render_svg)
from functionize import function_diff  # noqa: E402
from runtime_io import _export, _run_query  # noqa: E402
from solve import _all_unsat_cores, _enumerate, _unsat_core  # noqa: E402
from symmetry import fold_witnesses  # noqa: E402
from optimize import _optimize  # noqa: E402
from smtlib_tools import _parse_predicate, _ready_to_run  # noqa: E402

app = FastAPI(title="Evident IDE")


@app.middleware("http")
async def _no_cache(request, call_next):
    # This is a live-iterated dev tool: never let a browser serve a stale app.js/css, or a
    # reviewer ends up auditing an old build. Force revalidation on every response.
    resp = await call_next(request)
    resp.headers["Cache-Control"] = "no-store, must-revalidate"
    return resp


class Source(BaseModel):
    source: str
    view: str | None = None
    scope: int | None = None        # reachable-exploration bound — the scope knob (#21/#84)
    unroll: int | None = None       # k-step transition unroll for /api/smtlib (#259/#19)
    all_conditions: bool = False    # state_graph: GLOBAL dynamics (every initial condition) vs from-init (diagram #1)
    entry: str | None = None        # which top-level fsm/claim to render — the entry picker (#290)
    verify_soundness: bool = False  # #332: on-demand abstract-vs-brute-force cross-check (no render)


@app.post("/api/analyze")
def analyze(req: Source):
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work, req.entry)
        if not ok:
            return {"ok": False, "error": msg, "dropped": dropped,
                    "error_loc": _error_loc(msg),
                    "dropped_locs": _dropped_locs(req.source, msg)}
        claim_resp = _maybe_claim(prefix, dropped, req.source, msg, req.view)  # a raw claim: solved space + structure
        if claim_resp is not None:
            return claim_resp
        try:
            return _dynamics_response(req, prefix, dropped, msg)
        except Exception as e:
            return {"ok": False, "error": f"analysis failed: {e}", "dropped": dropped}


class SolveReq(BaseModel):
    source: str
    claim: str | None = None
    given: dict[str, str] | None = None
    enumerate: bool = False
    limit: int | None = None
    fold_symmetry: bool = False           # collapse value-symmetric witnesses (Ana #271)


@app.post("/api/solve")
def solve(req: SolveReq):
    """Interrogate a claim. Default: SAT + a witness, or UNSAT (with a delta-debugged core).
    `given` pins variables (solve-for-X). `enumerate` walks distinct witnesses by blocking.
    All paths reuse `evident query` — the same encode+solve path as `test`."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        if req.enumerate:
            limit = max(1, min(req.limit or 10, 40))
            claim, sols, complete, err = _enumerate(req.source, req.claim, req.given, limit, work)
            if not sols and err:
                return {"ok": False, "error": err}
            resp = {"ok": True, "satisfied": bool(sols), "claim": claim, "solutions": sols,
                    "count": len(sols), "complete": complete, "limit": limit}
            if req.fold_symmetry:
                # Collapse value-symmetric witnesses to one canonical rep + orbit count. SOUND: folds
                # only PROVABLY-interchangeable enums (no value named, no ordering); a no-op otherwise.
                folded, folded_sets, raw = fold_witnesses(req.source, sols)
                resp["fold_requested"] = True
                resp["folded"] = folded
                resp["folded_count"] = len(folded)
                resp["folded_sets"] = folded_sets        # {enum: [values]} the fold broke ({} if none)
                resp["raw_count"] = raw
            return resp
        r = _run_query(req.source, req.claim, req.given, work)
        if r.get("ok") and r.get("satisfied") is False and not req.given:
            claim = r.get("claim") or req.claim
            r["core"] = _unsat_core(req.source, claim, work)             # one (back-compat)
            cores, complete = _all_unsat_cores(req.source, claim, work)  # every independent core
            r["cores"] = cores
            r["cores_complete"] = complete
        return r


class OptimizeReq(BaseModel):
    source: str
    claim: str | None = None
    var: str                              # the numeric variable to optimize
    direction: str = "max"                # "max" → maximize, "min" → minimize


@app.post("/api/optimize")
def optimize(req: OptimizeReq):
    """QUANTITATIVE query over a claim — z3 Optimize. Maximize/minimize a numeric var subject to
    the claim, returning the EXTREMAL value AND the optimizing assignment (the numeric vars). On an
    unsatisfiable or unbounded objective, returns satisfied=False (an honest "no finite extremum")."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        return _optimize(req.source, req.claim, req.var, req.direction, work)


class InvariantReq(BaseModel):
    source: str
    var: str
    op: str
    value: str | int | float | bool
    scope: int | None = None        # verification bound — the scope knob (#21/#84)


@app.post("/api/invariant")
def invariant(req: InvariantReq):
    """Assert-and-check a safety invariant over the reachable set: does `var op value` hold on
    EVERY reachable state? Returns holds + (when finite & fully explored) a proof flag, or the
    first reachable counterexample state (with the trace that reaches it)."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            return {"ok": True, **m.check_invariant(req.var, req.op, req.value, limit=effective_scope(req))}
        except Exception as e:
            return {"ok": False, "error": str(e)}


class TemporalReq(BaseModel):
    source: str
    terms: list                           # [[var, op, value], …] — the Q conjunction (#258)
    modality: str = "eventually"          # "eventually" (◇Q) | "leads_to" (P ⤳ Q) | "infinitely_often" (□◇Q)
    p_terms: list | None = None           # [[var, op, value], …] — the P conjunction, for leads_to
    fair: bool = False                    # WEAK-FAIRNESS mode — exclude unfair lassos (#269)
    scope: int | None = None              # verification bound — the scope knob (#21/#84)


@app.post("/api/temporal")
def temporal(req: TemporalReq):
    """Check a LIVENESS property over the reachable graph: ◇Q (eventually) / P⤳Q (leads-to) /
    □◇Q (infinitely often). Q (and P) are CONJUNCTIONS of var-op-value terms (#258). Returns holds +
    a counterexample state and the TRACE (a run that dodges Q forever); ◇ also returns `recurrent`
    (□◇ also holds) to flag a TRANSIENT ◇. With `fair=True` (#269) the check runs under WEAK FAIRNESS
    — unfair lassos that ignore an always-available path to Q are excluded, so it HOLDS whenever Q is
    reachable from every reachable (P-)state; the only fair counterexample is a TRAP (`trap=True`)."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            return {"ok": True, **m.check_temporal(
                req.terms, modality=req.modality, p_terms=req.p_terms, fair=req.fair,
                limit=effective_scope(req))}
        except Exception as e:
            return {"ok": False, "error": str(e)}


class QueryReq(BaseModel):
    source: str
    # Either a list of [var, op, value] triples (a conjunction), OR a raw predicate string the
    # server parses with the same regex the frontend uses. Provide one or the other.
    terms: list[list[str | int | float | bool]] | None = None
    predicate: str | None = None
    scope: int | None = None        # verification bound — the scope knob (#21/#84)


@app.post("/api/query")
def query(req: QueryReq):
    """Ad-hoc EXISTENTIAL query over the reachable set — the dual of /api/invariant. Instead of
    "does P hold on EVERY reachable state (□)", asks "does ANY reachable state satisfy the
    conjunction P₁ ∧ P₂ ∧ … (◇/∃)" — the Z3/Alloy `(assert)(check-sat)` move against the loaded
    model without editing source. Returns satisfiable + a witness state, the count of reachable
    states satisfying it, and the trace init→witness."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            terms = req.terms
            if terms is None:
                terms = _parse_predicate(req.predicate or "")
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            return {"ok": True, **m.query([tuple(t) for t in terms], limit=effective_scope(req))}
        except Exception as e:
            return {"ok": False, "error": str(e)}


class ExploreReq(BaseModel):
    source: str
    state: dict            # the clicked diagram point's carried-state assignment


@app.post("/api/explore")
def explore(req: ExploreReq):
    """EXPLORE from a clicked diagram state — "assume the machine is HERE". Returns
    what's reachable FORWARD from it (count + a sample) and the run that LEADS here
    (init→state trace), plus whether init is forward-reachable from here (a cycle
    back through start). Loads the model exactly like /api/query, then delegates to
    Model.explore, which finds the clicked state by `state_key` and runs the BFS."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            return {"ok": True, **m.explore(req.state, limit=effective_scope(req))}
        except Exception as e:
            return {"ok": False, "error": str(e)}


class FigureReq(BaseModel):
    source: str
    view: str              # which view to render as SVG


@app.post("/api/figure")
def figure(req: FigureReq):
    """Render a view as SVG (vector, publication-quality) for download — the figure half of Ana #244.
    Same export + renderer path as /api/analyze; matplotlib infers SVG from the .svg out path."""
    if req.view not in RENDERERS:
        return {"ok": False, "error": f"unknown view {req.view}"}
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            return {"ok": True, "svg": _render_svg(req.view, prefix)}
        except Exception as e:
            return {"ok": False, "error": str(e)}


class DiffReq(BaseModel):
    source_a: str          # the PINNED program (A)
    source_b: str          # the LIVE program (B) — the same model with a constraint changed


@app.post("/api/diff")
def diff(req: DiffReq):
    """The relational analog of a text diff between two programs that share a var set: which
    reachable states APPEARED (in B not A), VANISHED (in A not B), and how many stayed COMMON.
    States align by `state_key` — the reachable-graph identity — so the delta is on the model's
    behavior, not its source. A and B must carry the same variables (else a clear error)."""
    def _load_for_diff(source, work):
        """Export+load one source into a Model, or return (None, error). Reused for A and B."""
        ok, prefix, dropped, msg = _export(source, work)
        if not ok:
            return None, msg
        return load_model(prefix + ".smt2", prefix + ".schema.json"), None

    with _LOCK, tempfile.TemporaryDirectory() as wa, tempfile.TemporaryDirectory() as wb:
        try:
            ma, err = _load_for_diff(req.source_a, wa)
            if err:
                return {"ok": False, "error": f"pinned program A: {err}"}
            mb, err = _load_for_diff(req.source_b, wb)
            if err:
                return {"ok": False, "error": f"live program B: {err}"}
            # Behavior delta (reachable states) PLUS compiled-structure delta (which per-variable
            # functions appeared/vanished/changed) — one diff, both layers (Ana #318). The structure
            # diff is cheap and var-set-tolerant, so it's attached even when the state diff is thin.
            result = _model_diff(ma, mb, REACH_LIMIT)
            result["function_diff"] = function_diff(ma, mb)
            return result
        except Exception as e:
            return {"ok": False, "error": f"diff failed: {e}"}


@app.post("/api/smtlib")
def smtlib(req: Source):
    """Return the SMT-LIB the runtime emits for this program — so a user can re-run the exact
    encoding in z3 directly, diff two encodings, or paste a model/core into notes (Ana #200)."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            if req.unroll:               # k-step BMC unroll (#259/#19) instead of the single tick
                m = load_model(prefix + ".smt2", prefix + ".schema.json")
                smt = m.unroll_smt2(max(1, min(req.unroll, 64)))
                if smt is None:
                    return {"ok": False, "error": "nothing to unroll — no carried-state transition"}
                return {"ok": True, "smtlib": smt, "dropped": dropped, "unrolled": req.unroll}
            with open(prefix + ".smt2") as f:
                raw = f.read()
            return {"ok": True, "smtlib": _ready_to_run(raw), "dropped": dropped}
        except Exception as e:
            return {"ok": False, "error": str(e)}


_NOCACHE = {"Cache-Control": "no-store, no-cache, must-revalidate, max-age=0",
           "Pragma": "no-cache", "Expires": "0"}


@app.get("/")
def index():
    # Serve index.html with every app.js/app.css reference stamped by its file mtime, so a
    # changed asset ALWAYS busts the browser cache (a no-store header alone does not evict an
    # already-cached entry — which was silently feeding reviewers a stale build). Index
    # itself is hard no-cache so the browser re-pulls the current stamps.
    with open(os.path.join(STATIC, "index.html")) as f:
        html = f.read()

    def stamp(m):
        name = m.group(1)
        try:
            v = int(os.path.getmtime(os.path.join(STATIC, name)))
        except OSError:
            v = 0
        return f"{name}?v={v}"

    # app.css, app.js, and the app-<concern>.js split files (Dijkstra's app.js split).
    html = re.sub(r'(app(?:-[\w-]+)?\.(?:js|css))(?:\?v=[^"\']*)?', stamp, html)
    return Response(html, media_type="text/html", headers=_NOCACHE)


app.mount("/static", StaticFiles(directory=STATIC), name="static")


if __name__ == "__main__":
    import uvicorn
    print(f"[server] runtime: {EVIDENT}")
    print(f"[server] views: {VIEWS}")
    uvicorn.run(app, host="0.0.0.0", port=5173, log_level="warning")
