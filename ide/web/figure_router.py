#!/usr/bin/env python3
"""Evident Web IDE — the figure/export router.

The render-and-export half of the API: turn a program into a picture, a file, or a delta.
Four endpoints, each a thin wrapper over an extracted helper, all sharing the `_LOCK` +
tempdir + `_export` pattern:

  POST /api/analyze   — the write→see loop: banner + dropped count + reachable stats + PNG
  POST /api/figure    — render a view as downloadable SVG (vector, publication-quality)
  POST /api/diff      — relational diff of two programs (states appeared/vanished + function diff)
  POST /api/smtlib    — the SMT-LIB the runtime emits (optionally k-step BMC unrolled)

Mounted onto the FastAPI app in `server.py` via `app.include_router(router)`. The Pydantic
request models (`Source`, `FigureReq`, `DiffReq`) live here with their handlers. `server.py`
re-imports `analyze` + `Source` for the direct-call unit test (`test_all_conditions_stats.py`).
"""
import tempfile

from config import REACH_LIMIT, _LOCK

from evident_viz import load as load_model

from fastapi import APIRouter
from pydantic import BaseModel

from analysis import _dropped_locs, _dynamics_response, _error_loc, _model_diff
from render import RENDERERS, _maybe_claim, _render_svg
from functionize import function_diff
from runtime_io import _export
from smtlib_tools import _ready_to_run

router = APIRouter()


class Source(BaseModel):
    source: str
    view: str | None = None
    scope: int | None = None        # reachable-exploration bound — the scope knob (#21/#84)
    unroll: int | None = None       # k-step transition unroll for /api/smtlib (#259/#19)
    all_conditions: bool = False    # state_graph: GLOBAL dynamics (every initial condition) vs from-init (diagram #1)
    entry: str | None = None        # which top-level fsm/claim to render — the entry picker (#290)
    verify_soundness: bool = False  # #332: on-demand abstract-vs-brute-force cross-check (no render)


@router.post("/api/analyze")
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


class FigureReq(BaseModel):
    source: str
    view: str              # which view to render as SVG


@router.post("/api/figure")
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


@router.post("/api/diff")
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


@router.post("/api/smtlib")
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
