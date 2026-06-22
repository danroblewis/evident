#!/usr/bin/env python3
"""Evident Web IDE — M0 backend.

Wraps the Rust runtime (`evident export`) + the viz model-semantics layer
(`viz/evident_viz.py`) + the renderers, and serves the single-page front end. The one
endpoint that matters is POST /api/analyze: source text in, and out comes the
model-shape banner, the dropped-constraint honesty count, the reachable-set stats, and
the recommended view rendered to a PNG — i.e. everything the live write→see loop needs.

Run:  python3 -m uvicorn ide.web.server:app --host 0.0.0.0 --port 5173
(or:  python3 ide/web/server.py)
"""
import base64
import inspect
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
VIZ = os.path.join(ROOT, "viz")
STATIC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "static")
EVIDENT = os.path.join(ROOT, "runtime", "target", "release", "evident")
sys.path.insert(0, VIZ)

import matplotlib  # noqa: E402
matplotlib.use("Agg")
import networkx as nx  # noqa: E402  (SCC detection for the cyclic-vs-terminating banner)

from evident_viz import load as load_model  # noqa: E402

from fastapi import FastAPI  # noqa: E402
from fastapi.responses import FileResponse, Response  # noqa: E402
from fastapi.staticfiles import StaticFiles  # noqa: E402
from pydantic import BaseModel  # noqa: E402

# --- renderers: import once, call in-process (matplotlib stays warm) ----------------
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
REACH_LIMIT = 400                              # bounded exploration cap for the live stats
_LOCK = threading.Lock()                       # matplotlib + z3 are not thread-safe; serialize

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


def _export(source: str, work: str):
    """Write source, run `evident export`. Returns (ok, prefix, dropped, message)."""
    ev = os.path.join(work, "prog.ev")
    with open(ev, "w") as f:
        f.write(source)
    prefix = os.path.join(work, "prog")
    r = subprocess.run([EVIDENT, "export", ev, "--out", prefix],
                       capture_output=True, text=True, timeout=30, cwd=ROOT)
    err = (r.stderr or "") + (r.stdout or "")
    dropped = sum(1 for ln in err.splitlines() if "dropped" in ln.lower())
    # Strip the internal temp-dir plumbing from anything shown to the user (Sam/Marek #190):
    # "export: load /tmp/tmpXXX/prog.ev: …" → "…", and drop the "wrote …prog.smt2" success noise.
    err = (err.replace(ev + ":", "").replace(ev, "your program")
              .replace(prefix + ".smt2", "the model").replace(prefix + ".schema.json", "the schema")
              .replace(work + "/", "").replace("export: ", ""))
    err = "\n".join(ln for ln in err.splitlines() if not ln.lstrip().startswith("wrote ")).strip()
    if r.returncode != 0 or not os.path.exists(prefix + ".smt2"):
        return False, prefix, dropped, err[-1200:] or "export failed"
    return True, prefix, dropped, err


_LOC_RE = re.compile(r"\bline (\d+), col (\d+)\b")


def _error_loc(msg: str):
    """Pull a 1-based (line, col) out of a parse/lex error message — the runtime
    formats them as 'parse error at line N, col N: …'. Returns None when absent."""
    m = _LOC_RE.search(msg or "")
    return {"line": int(m.group(1)), "col": int(m.group(2))} if m else None


def _banner(m, max_branch=1, recurrent=1):
    """The model-shape line, from the functional-dependency analysis. Two reachable-graph
    facts override the dependency verdict: BRANCHING (a state with ≥2 successors is
    nondeterministic no matter what), and a RECURRENT cycle (a ≥2-state SCC is
    eventually-periodic, not a terminating chain — so the banner must say 'cyclic')."""
    try:
        ind = m.independence()
    except Exception:
        return "model shape: (unavailable)"
    short = lambda n: n.split(".")[-1]
    if max_branch >= 2:
        drv = ind.get("driver")
        hint = f"; candidate driver of the deterministic part: {short(drv)}" if drv else ""
        return (f"Nondeterministic — up to {max_branch} successors from some state "
                f"(a free choice fans out){hint}")
    if ind["verdict"] == "driven" and ind.get("driver"):
        drv = short(ind["driver"])
        deps = [short(d) for d in ind.get("dependents", [])[:4]]
        if deps:
            return (f"Driven pipeline — independent variable: {drv}"
                    f" — computed from it: {', '.join(deps)}")
        if recurrent >= 2:
            return (f"Cyclic — {drv} cycles through a recurrent loop of {recurrent} states "
                    f"(eventually periodic, no fixpoint)")
        return f"Driven — {drv} advances on its own clock (a deterministic recurrence)"
    if ind["verdict"] == "nondeterministic":
        return "Nondeterministic — the free choice is the input, not a state variable"
    # A relational (no single driver) machine whose reachable graph has a real recurrent
    # SCC is a CYCLE, not just a tangle: the variables co-determine in a loop and the orbit
    # eventually repeats. Say 'cyclic' (traffic: light+timer recur every N ticks) rather than
    # the static 'genuinely relational' phrasing, which read as terminating.
    if recurrent >= 2:
        return (f"Cyclic — {recurrent} states recur; the variables co-determine in a loop "
                f"(eventually periodic, no fixpoint)")
    return "Genuinely relational — no independent variable (a cycle; every variable co-determines)"


def _recommend(m, n_states, max_branch, discrete):
    """Pick the lead view from the model's shape:
      - a SMALL DISCRETE machine → state_graph: it draws the whole structure at once —
        branch out-edges AND back-edges/cycles — which a tree would hide and a noodle
        would bury. (A 3-state vending loop reads as a loop here, not a fanned tree.)
      - otherwise, any BRANCHING (some state has ≥2 successors) → reachability_tree, so the
        fan is visible where the full graph would be an unreadable noodle. Keyed on the
        branching factor (not edge count) so it still fires when a large reachable set hits
        the exploration cap (n_edges ≈ n_states).
      - otherwise, a DETERMINISTIC system with ≥2 interacting NUMERIC variables →
        phase_portrait: the compelling view is the orbit in (var₁, var₂) space, not a pair
        of separate time-series lines. The oscillator spirals in (pos, vel); a time series
        would split that single trajectory across two flat plots and hide the spiral. Gated
        on ¬discrete (a tiny discrete machine reads as state_graph above) and on the
        deterministic path (max_branch < 2) so the genuinely-branching numeric systems —
        vending, pick — still go to reachability_tree above, not here.
      - otherwise the time series: a deterministic numeric ramp/trajectory reads as a clean
        line, faithful and fast for almost everything.

      BUT lead with solution_space whenever there's a numeric variable: the DEFAULT picture
      should be the BOUNDARY of what the variables can be (the solved range of each var + the
      feasible set + fixed points), not one trajectory through it. The dynamics views are one
      tab click away. (Purely categorical machines have no numeric boundary, so they fall
      through to state_graph below.)"""
    if "solution_space" in VIEWS and m.numeric_vars:
        return "solution_space"
    if "state_graph" in VIEWS and discrete and n_states <= 30:
        return "state_graph"
    if "reachability_tree" in VIEWS and max_branch >= 2:
        return "reachability_tree"
    if "phase_portrait" in VIEWS and not discrete and len(m.numeric_vars) >= 2:
        return "phase_portrait"
    return "time_series" if "time_series" in VIEWS else (VIEWS[0] if VIEWS else None)


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


def _maybe_claim(prefix, dropped):
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
        "png": base64.b64encode(png).decode() if png else None, "warnings": "",
    }


@app.post("/api/analyze")
def analyze(req: Source):
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg, "dropped": dropped,
                    "error_loc": _error_loc(msg)}
        claim_resp = _maybe_claim(prefix, dropped)     # a raw claim renders its solved solution space
        if claim_resp is not None:
            return claim_resp
        try:
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            states, edges = m.reachable(limit=REACH_LIMIT)
            n_states, n_edges = len(states), len(edges)
            out_deg = Counter(src for src, _ in edges)
            max_branch = max(out_deg.values()) if out_deg else 1
            capped = n_states >= REACH_LIMIT      # the reachable set didn't fit the cap
            # largest recurrent SCC: ≥2 distinguishes eventually-periodic (vending) from a
            # terminating-driven chain (counter), which the banner must not flatten.
            recurrent = 1
            if edges:
                g = nx.DiGraph(); g.add_edges_from(edges)
                recurrent = max((len(c) for c in nx.strongly_connected_components(g)), default=1)
            try:
                structure = m.solution_structure(states=states, edges=edges)
            except Exception as _e:
                print(f"[server] structure failed: {type(_e).__name__}: {_e}", file=sys.stderr)
                structure = None
            if dropped:
                structure = None        # a BROKEN model has no trustworthy verdict — don't show a
                                        # green "Terminates" card under the red "under-constrained"
                                        # banner (Marek #94). The dropped-constraint surface stands.
            discrete = m.is_discrete()
            view = req.view if (req.view in VIEWS) else _recommend(m, n_states, max_branch, discrete)
            # Resilient render: a single buggy renderer must never sink the whole analysis.
            # Try the chosen view, then fall back to dependable ones; report what rendered.
            png, points = b"", []
            for cand in [view, "state_graph", "time_series"]:
                if cand in RENDERERS:
                    try:
                        png, points = _render_png(cand, prefix)
                        view = cand
                        break
                    except Exception as _re:
                        print(f"[server] render {cand} failed: {type(_re).__name__}: {_re}",
                              file=sys.stderr)
            return {
                "ok": True,
                # A model with dropped constraints is BROKEN, not a valid relation — the freed
                # variables fan the state space and any "shape" read off it is an artifact, not
                # the program's. Say so in the headline, don't describe it as relational/cyclic.
                "banner": (
                    f"⚠ Under-constrained — {dropped} dropped constraint(s); this model is "
                    f"BROKEN, not a real relation (the freed variables fan the state space)"
                    if dropped else _banner(m, max_branch, recurrent)),
                "structure": structure,
                "dropped": dropped,
                "branching": max_branch,
                "states": n_states,
                "edges": n_edges,
                "capped": capped,
                "vars": [v["name"].split(".")[-1] for v in m.interface_vars],
                "view": view,
                "views": VIEWS,
                "png": base64.b64encode(png).decode() if png else None,
                "points": points,        # interactive hover overlay (solution_space); [] otherwise
                "warnings": msg if dropped else "",
            }
        except Exception as e:
            return {"ok": False, "error": f"analysis failed: {e}", "dropped": dropped}


class SolveReq(BaseModel):
    source: str
    claim: str | None = None
    given: dict[str, str] | None = None
    enumerate: bool = False
    limit: int | None = None


_HEADER_KW = ("claim", "type", "enum", "fsm", "schema", "import", "assert")

# A pure declaration: `names ∈ Type` with NO constraining comparison. Removing one un-declares
# its variable, which silently DROPS the constraints that referenced it — flipping the claim to
# SAT and making the declaration falsely look like a core member ("remove any one makes it
# solvable" is false for `x ∈ Int`). Exclude these from the delta-debug. A chained-membership
# that carries a bound (`0 ≤ x ∈ Int ≤ 5`) does NOT match (it has `≤`), so its bound stays a
# candidate.
_PURE_DECL = re.compile(r'^[A-Za-z_][\w, ]*∈\s*[A-Za-z_]\w*(\([^)]*\))?$')


def _run_query(source, claim, given, work):
    """One `evident query --json` call → parsed {ok, satisfied, claim, bindings}."""
    import json as _json
    ev = os.path.join(work, "prog.ev")
    with open(ev, "w") as f:
        f.write(source)
    cmd = [EVIDENT, "query", ev]
    if claim:
        cmd.append(claim)
    for k, v in (given or {}).items():
        cmd += ["--given", f"{k}={v}"]
    cmd.append("--json")
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=30, cwd=ROOT)
    out = (r.stdout or "").strip()
    try:
        return _json.loads(out.splitlines()[-1]) if out else {"ok": False, "error": "no output"}
    except Exception:
        return {"ok": False, "error": (r.stderr or out).strip()[-600:] or "query failed"}


def _block_term(name, val):
    """An Evident expression true for THIS witness value — assembled into a ¬(…) blocking
    constraint so enumeration can ask for a *different* solution."""
    if isinstance(val, bool):
        return f"{name} = {'true' if val else 'false'}"
    if isinstance(val, (int, float)):
        return f"{name} = {val}"
    if isinstance(val, str):
        # an enum-variant label (Idle) compares bare; a quoted string compares quoted.
        ident = val and (val[0].isalpha() or val[0] == "_") and "(" not in val
        return f"{name} = {val}" if ident else f'{name} = "{val}"'
    if isinstance(val, list):
        terms = []
        for i, el in enumerate(val):
            t = _block_term(f"{name}[{i}]", el)
            if t is None:
                return None
            terms.append(t)
        return "(" + " ∧ ".join(terms) + ")" if terms else None
    if isinstance(val, dict):                      # a record witness (e.g. sudoku's boxes elements,
        terms = []                                 # toposort edges) — block each field by dotted name
        for fld, fv in sorted(val.items()):
            t = _block_term(f"{name}.{fld}", fv)
            if t is None:
                return None
            terms.append(t)
        return "(" + " ∧ ".join(terms) + ")" if terms else None
    return None                                    # genuinely unsupported → can't block


def _block_clause(bindings):
    terms = []
    for k, v in sorted(bindings.items()):
        t = _block_term(k, v)
        if t is None:
            return None
        terms.append(t)
    return "¬(" + " ∧ ".join(terms) + ")" if terms else None


def _enumerate(source, claim, given, limit, work):
    """Walk distinct witnesses by iterated source-level blocking: solve, append a ¬(witness)
    constraint to the claim body, re-solve, until UNSAT (complete) or the limit (≥limit)."""
    sols, blocks, resolved_claim = [], [], claim
    for _ in range(limit):
        src = source if not blocks else source.rstrip() + "\n" + "\n".join("    " + b for b in blocks) + "\n"
        r = _run_query(src, claim, given, work)
        if not r.get("ok"):
            return resolved_claim, sols, len(sols) > 0, r.get("error")  # blocking broke parse → stop
        resolved_claim = r.get("claim") or resolved_claim
        if not r.get("satisfied"):
            return resolved_claim, sols, True, None                     # exhausted → complete
        b = r.get("bindings", {})
        sols.append(b)
        clause = _block_clause(b)
        if clause is None:
            return resolved_claim, sols, False, None                    # can't block → incomplete
        blocks.append(clause)
    return resolved_claim, sols, False, None                            # hit limit → ≥limit


def _unsat_core(source, claim, work):
    """A MINIMAL unsat core by deletion-based minimization over the source's constraint lines.

    The naive "a line whose individual removal flips to SAT is in the core" is UNSOUND when
    constraints are redundant: for {x>3, x>5, y>5, y<100, x+y<10} it drops x>5 (removing it still
    leaves x>3 ⇒ SAT) yet reports a SATISFIABLE set as 'the core'. Instead: start with every
    constraint line, and drop a line ONLY when the program stays UNSAT without it. The residual is
    a genuine minimal core — every member is necessary AND the set itself is unsatisfiable.

    Header/decl/comment lines are never candidates (a pure decl's removal un-declares a var and
    cascades to drop its constraints). Line granularity; multi-line ∀ blocks may be missed."""
    lines = source.split("\n")
    cand = []
    for i, ln in enumerate(lines):
        s = ln.strip()
        if (not s or s.startswith("--") or s.split(" ", 1)[0] in _HEADER_KW
                or _PURE_DECL.match(s)):
            continue
        cand.append(i)
    cand_set = set(cand)
    keep = set(cand)
    for i in cand:
        trial_keep = keep - {i}
        trial = "\n".join(ln for j, ln in enumerate(lines)
                          if j not in cand_set or j in trial_keep)
        r = _run_query(trial, claim, None, work)
        if r.get("ok") and r.get("satisfied") is False:   # still UNSAT without line i → redundant
            keep = trial_keep
    return [{"line": i + 1, "text": lines[i].strip()} for i in sorted(keep)]


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
            return {"ok": True, "satisfied": bool(sols), "claim": claim, "solutions": sols,
                    "count": len(sols), "complete": complete, "limit": limit}
        r = _run_query(req.source, req.claim, req.given, work)
        if r.get("ok") and r.get("satisfied") is False and not req.given:
            r["core"] = _unsat_core(req.source, r.get("claim") or req.claim, work)
        return r


class InvariantReq(BaseModel):
    source: str
    var: str
    op: str
    value: str | int | float | bool


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
            return {"ok": True, **m.check_invariant(req.var, req.op, req.value, limit=REACH_LIMIT)}
        except Exception as e:
            return {"ok": False, "error": str(e)}


class TemporalReq(BaseModel):
    source: str
    var: str
    op: str
    value: str | int | float | bool
    modality: str = "eventually"          # "eventually" (◇Q) | "leads_to" (P ⤳ Q)
    p_var: str | None = None
    p_op: str | None = None
    p_value: str | int | float | bool | None = None


@app.post("/api/temporal")
def temporal(req: TemporalReq):
    """Check a LIVENESS property over the reachable graph: ◇Q (eventually) / P⤳Q (leads-to).
    Returns holds + a counterexample state and the TRACE (a run that dodges Q forever)."""
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg}
        try:
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            return {"ok": True, **m.check_temporal(
                req.var, req.op, req.value, modality=req.modality,
                p_var=req.p_var, p_op=req.p_op, p_value=req.p_value, limit=REACH_LIMIT)}
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

    html = re.sub(r'(app\.(?:js|css))(?:\?v=[^"\']*)?', stamp, html)
    return Response(html, media_type="text/html", headers=_NOCACHE)


app.mount("/static", StaticFiles(directory=STATIC), name="static")


if __name__ == "__main__":
    import uvicorn
    print(f"[server] runtime: {EVIDENT}")
    print(f"[server] views: {VIEWS}")
    uvicorn.run(app, host="0.0.0.0", port=5173, log_level="warning")
