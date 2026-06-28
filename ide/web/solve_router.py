#!/usr/bin/env python3
"""Evident Web IDE — the analysis/solve router.

The interrogation half of the API: ask a claim/model a question and get an answer back —
no rendering. Six endpoints, each a thin wrapper over an extracted helper, all sharing the
`_LOCK` + tempdir + `_export` pattern:

  POST /api/solve      — SAT witness / UNSAT core / witness enumeration (over `evident query`)
  POST /api/optimize   — quantitative max/min of a numeric var (z3 Optimize)
  POST /api/invariant  — □ safety: does `var op value` hold on every reachable state?
  POST /api/temporal   — ◇/⤳/□◇ liveness over the reachable graph
  POST /api/query      — ∃ existential: does any reachable state satisfy a conjunction?
  POST /api/explore    — forward/backward reachability from a clicked diagram state

Mounted onto the FastAPI app in `server.py` via `app.include_router(router)`. The Pydantic
request models live here with their handlers.
"""
import tempfile

from config import _LOCK, effective_scope

from evident_viz import load as load_model

from fastapi import APIRouter
from pydantic import BaseModel

from runtime_io import _export, _run_query
from solve import _all_unsat_cores, _enumerate, _unsat_core
from symmetry import fold_witnesses
from optimize import _optimize
from smtlib_tools import _parse_predicate

router = APIRouter()


class SolveReq(BaseModel):
    source: str
    claim: str | None = None
    given: dict[str, str | int | float | bool] | None = None  # #466: domain-typed pins, not str-only
    enumerate: bool = False
    limit: int | None = None
    fold_symmetry: bool = False           # collapse value-symmetric witnesses (Ana #271)


@router.post("/api/solve")
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


@router.post("/api/optimize")
def optimize(req: OptimizeReq):
    """QUANTITATIVE query over a claim — z3 Optimize. Maximize/minimize a numeric var subject to
    the claim, returning the EXTREMAL value AND the optimizing assignment (the numeric vars). On an
    unsatisfiable or unbounded objective, returns satisfied=False (an honest "no finite extremum")."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        return _optimize(req.source, req.claim, req.var, req.direction, work)


class InvariantReq(BaseModel):
    source: str
    # SINGLE-VAR path (back-compat): var/op/value name one comparison.
    var: str | None = None
    op: str | None = None
    value: str | int | float | bool | None = None
    # MULTI-TERM path (#381): a CONJUNCTION (`terms`) or an IMPLICATION
    # (`antecedent` ⇒ `consequent`). Each is a list of [var, op, value] triples.
    terms: list | None = None
    antecedent: list | None = None
    consequent: list | None = None
    scope: int | None = None        # verification bound — the scope knob (#21/#84)


@router.post("/api/invariant")
def invariant(req: InvariantReq):
    """Assert-and-check a safety invariant over the reachable set: does the property hold on
    EVERY reachable state? Three shapes — a single `var op value`, a CONJUNCTION of `terms`, or
    an IMPLICATION `antecedent ⇒ consequent` (#381). Returns holds + (when finite & fully
    explored) a proof flag, or the first reachable counterexample state (with its trace). For an
    implication the counterexample is always a REAL reachable state where the antecedent holds
    and the consequent fails — never vacuous."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            scope = effective_scope(req)
            if req.antecedent is not None or req.consequent is not None:
                result = m.check_invariant_predicate(
                    antecedent=req.antecedent, consequent=req.consequent, limit=scope)
            elif req.terms is not None:
                result = m.check_invariant_predicate(terms=req.terms, limit=scope)
            else:
                result = m.check_invariant(req.var, req.op, req.value, limit=scope)
            return {"ok": True, **result}
        except Exception as e:
            return {"ok": False, "error": str(e)}


class TemporalReq(BaseModel):
    source: str
    terms: list                           # [[var, op, value], …] — the Q conjunction (#258)
    modality: str = "eventually"          # "eventually" (◇Q) | "leads_to" (P ⤳ Q) | "infinitely_often" (□◇Q)
    p_terms: list | None = None           # [[var, op, value], …] — the P conjunction, for leads_to
    fair: bool = False                    # WEAK-FAIRNESS mode — exclude unfair lassos (#269)
    scope: int | None = None              # verification bound — the scope knob (#21/#84)


@router.post("/api/temporal")
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


@router.post("/api/query")
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


@router.post("/api/explore")
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
