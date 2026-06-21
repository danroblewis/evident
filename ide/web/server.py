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
import os
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

from evident_viz import load as load_model  # noqa: E402

from fastapi import FastAPI  # noqa: E402
from fastapi.responses import FileResponse, Response  # noqa: E402
from fastapi.staticfiles import StaticFiles  # noqa: E402
from pydantic import BaseModel  # noqa: E402

# --- renderers: import once, call in-process (matplotlib stays warm) ----------------
RENDERERS = {}            # view name -> module.render(smt2, schema, out_path)
for _v in ("time_series", "state_graph", "phase_portrait", "morse_graph",
           "occupancy_heatmap", "reachability_tree"):
    try:
        _m = __import__(f"render_{_v}")
        if hasattr(_m, "render"):
            RENDERERS[_v] = _m.render
    except Exception as e:                     # a renderer that won't import just isn't offered
        print(f"[server] render_{_v} unavailable: {e}", file=sys.stderr)

VIEWS = [v for v in ("time_series", "state_graph", "phase_portrait",
                     "reachability_tree", "morse_graph", "occupancy_heatmap")
         if v in RENDERERS]
REACH_LIMIT = 400                              # bounded exploration cap for the live stats
_LOCK = threading.Lock()                       # matplotlib + z3 are not thread-safe; serialize

app = FastAPI(title="Evident IDE")


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
                       capture_output=True, text=True, timeout=30)
    err = (r.stderr or "") + (r.stdout or "")
    dropped = sum(1 for ln in err.splitlines() if "dropped" in ln.lower())
    if r.returncode != 0 or not os.path.exists(prefix + ".smt2"):
        return False, prefix, dropped, err.strip()[-1200:] or "export failed"
    return True, prefix, dropped, err.strip()


def _banner(m, max_branch=1):
    """The model-shape line, from the functional-dependency analysis. Branching in the
    *reachable* relation wins: a state with multiple successors is nondeterministic no
    matter what the dependency verdict says — so the banner can't call it a pipeline."""
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
        deps = [short(d) for d in ind.get("dependents", [])[:4]]
        if deps:
            return (f"Driven pipeline — independent variable: {short(ind['driver'])}"
                    f" — computed from it: {', '.join(deps)}")
        return (f"Driven — {short(ind['driver'])} advances on its own clock "
                f"(a deterministic recurrence)")
    if ind["verdict"] == "nondeterministic":
        return "Nondeterministic — the free choice is the input, not a state variable"
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
      - otherwise the time series: a deterministic numeric ramp/trajectory reads as a clean
        line, faithful and fast for almost everything."""
    if "state_graph" in VIEWS and discrete and n_states <= 30:
        return "state_graph"
    if "reachability_tree" in VIEWS and max_branch >= 2:
        return "reachability_tree"
    return "time_series" if "time_series" in VIEWS else (VIEWS[0] if VIEWS else None)


def _render_png(view, prefix):
    out = prefix + f".{view}.png"
    RENDERERS[view](prefix + ".smt2", prefix + ".schema.json", out)
    with open(out, "rb") as f:
        return f.read()


@app.post("/api/analyze")
def analyze(req: Source):
    with _LOCK, tempfile.TemporaryDirectory() as work:
        ok, prefix, dropped, msg = _export(req.source, work)
        if not ok:
            return {"ok": False, "error": msg, "dropped": dropped}
        try:
            m = load_model(prefix + ".smt2", prefix + ".schema.json")
            states, edges = m.reachable(limit=REACH_LIMIT)
            n_states, n_edges = len(states), len(edges)
            out_deg = Counter(src for src, _ in edges)
            max_branch = max(out_deg.values()) if out_deg else 1
            capped = n_states >= REACH_LIMIT      # the reachable set didn't fit the cap
            discrete = m.is_discrete()
            view = req.view if (req.view in VIEWS) else _recommend(m, n_states, max_branch, discrete)
            png = _render_png(view, prefix) if view else b""
            return {
                "ok": True,
                "banner": _banner(m, max_branch),
                "dropped": dropped,
                "branching": max_branch,
                "states": n_states,
                "edges": n_edges,
                "capped": capped,
                "vars": [v["name"].split(".")[-1] for v in m.interface_vars],
                "view": view,
                "views": VIEWS,
                "png": base64.b64encode(png).decode() if png else None,
                "warnings": msg if dropped else "",
            }
        except Exception as e:
            return {"ok": False, "error": f"analysis failed: {e}", "dropped": dropped}


@app.get("/")
def index():
    return FileResponse(os.path.join(STATIC, "index.html"))


app.mount("/static", StaticFiles(directory=STATIC), name="static")


if __name__ == "__main__":
    import uvicorn
    print(f"[server] runtime: {EVIDENT}")
    print(f"[server] views: {VIEWS}")
    uvicorn.run(app, host="0.0.0.0", port=5173, log_level="warning")
