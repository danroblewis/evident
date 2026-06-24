#!/usr/bin/env python3
"""Evident Web IDE — M0 backend.

Wraps the Rust runtime (`evident export`) + the viz model-semantics layer
(`viz/evident_viz.py`) + the renderers, and serves the single-page front end. The one
endpoint that matters is POST /api/analyze: source text in, and out comes the
model-shape banner, the dropped-constraint honesty count, the reachable-set stats, and
the recommended view rendered to a PNG — i.e. everything the live write→see loop needs.

This module is the FastAPI wiring only — the `app`, middleware, and index/static serving —
plus `app.include_router(...)` for the two endpoint routers:

  * `figure_router` — the render/export half: /api/analyze, /api/figure, /api/diff, /api/smtlib
  * `solve_router`  — the interrogate half: /api/solve, /api/optimize, /api/invariant,
                      /api/temporal, /api/query, /api/explore

Each handler is a thin wrapper over an extracted helper; the work lives in the sibling
modules `runtime_io` (`evident` subprocess calls), `render` (renderer registry + claim view),
`analysis` (banner/recommend + dropped-locs), `solve` (witness enumeration + unsat core),
`smtlib_tools` (SMT-LIB export + query parse), and `config` (shared paths + serialization lock).

Run:  python3 -m uvicorn ide.web.server:app --host 0.0.0.0 --port 5173
(or:  python3 ide/web/server.py)
"""
import os
import re
import sys

from config import EVIDENT, STATIC, VIZ

sys.path.insert(0, VIZ)

import matplotlib  # noqa: E402
matplotlib.use("Agg")

from fastapi import FastAPI  # noqa: E402
from fastapi.responses import Response  # noqa: E402
from fastapi.staticfiles import StaticFiles  # noqa: E402

from render import VIEWS  # noqa: E402

import figure_router  # noqa: E402
import solve_router  # noqa: E402
# Re-export for the direct-call unit test (ide/test_all_conditions_stats.py imports these).
from figure_router import Source, analyze  # noqa: E402,F401

app = FastAPI(title="Evident IDE")


@app.middleware("http")
async def _no_cache(request, call_next):
    # This is a live-iterated dev tool: never let a browser serve a stale app.js/css, or a
    # reviewer ends up auditing an old build. Force revalidation on every response.
    resp = await call_next(request)
    resp.headers["Cache-Control"] = "no-store, must-revalidate"
    return resp


app.include_router(figure_router.router)
app.include_router(solve_router.router)


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
