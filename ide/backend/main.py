"""
FastAPI backend for the Evident IDE.

Endpoints:
  POST /parse      — parse source, return schema names + errors
  POST /evaluate   — evaluate a schema with given bindings
  POST /ranges     — compute min/max for each free variable via Z3 Optimize
  POST /sample     — sample valid assignments (blocking or random strategy)
  POST /transfer   — sweep x_var across a range, solve for y_var at each step
"""

import sys
import json
import hashlib
import subprocess
from collections import OrderedDict
from pathlib import Path

# Ensure the project root is on sys.path so `runtime` and `parser` are importable.
_project_root = str(Path(__file__).parent.parent.parent)
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

# Also add ide/backend to sys.path so sibling modules (sampler, ranges) are
# importable without relative-import syntax when run as __main__.
_backend_dir = str(Path(__file__).parent)
if _backend_dir not in sys.path:
    sys.path.insert(0, _backend_dir)

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel
from typing import Any

_worker_script = str(Path(__file__).parent / "z3_worker.py")

# ---------------------------------------------------------------------------
# Request cache — keyed by sha256(json(payload)), LRU eviction at 128 entries
# Toggle with POST /cache/enable or POST /cache/disable
# ---------------------------------------------------------------------------

_CACHE_MAX = 128
_cache: OrderedDict = OrderedDict()
_cache_enabled: bool = True


def _cache_key(command: str, payload: dict) -> str:
    raw = json.dumps({"cmd": command, **payload}, sort_keys=True)
    return hashlib.sha256(raw.encode()).hexdigest()


def _cache_get(key: str):
    if not _cache_enabled:
        return None
    if key in _cache:
        _cache.move_to_end(key)
        return _cache[key]
    return None


def _cache_put(key: str, value: dict):
    if not _cache_enabled:
        return
    _cache[key] = value
    _cache.move_to_end(key)
    while len(_cache) > _CACHE_MAX:
        _cache.popitem(last=False)


def _call_worker(command: str, payload: dict, timeout: int = 60) -> dict:
    """Run a Z3 computation in an isolated subprocess. Returns parsed JSON result."""
    try:
        proc = subprocess.run(
            [sys.executable, _worker_script, command],
            input=json.dumps(payload),
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        if proc.stdout.strip():
            return json.loads(proc.stdout)
        return {"error": proc.stderr.strip() or "worker produced no output"}
    except subprocess.TimeoutExpired:
        return {"error": "computation timed out"}
    except Exception as e:
        return {"error": str(e)}

app = FastAPI(title="Evident IDE")
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# Serve frontend static files at /app (not /) so API routes at / are not shadowed.
# StaticFiles mounted at "/" with html=True would intercept all POST routes with 405.
_frontend_dir = Path(__file__).parent.parent / "frontend"
if _frontend_dir.exists():
    app.mount(
        "/app",
        StaticFiles(directory=str(_frontend_dir), html=True),
        name="frontend",
    )


# ---------------------------------------------------------------------------
# Request models
# ---------------------------------------------------------------------------


class ParseRequest(BaseModel):
    source: str


class EvaluateRequest(BaseModel):
    source: str
    schema: str
    given: dict[str, Any] = {}


class SampleRequest(BaseModel):
    source: str
    schema: str
    given: dict[str, Any] = {}
    n: int = 10
    strategy: str = "blocking"  # "blocking" | "random" | "grid"


class RangesRequest(BaseModel):
    source: str
    schema: str
    given: dict[str, Any] = {}


class TransferRequest(BaseModel):
    source: str
    schema: str
    given: dict[str, Any] = {}
    x_var: str
    y_var: str
    x_min: float
    x_max: float
    steps: int = 50


# ---------------------------------------------------------------------------
# Endpoints
# ---------------------------------------------------------------------------


@app.post("/parse")
def parse_source(req: ParseRequest):
    """Parse Evident source and return schema names + errors."""
    try:
        from parser.src.parser import parse
        from runtime.src.runtime import EvidentRuntime

        program = parse(req.source)
        rt = EvidentRuntime()
        rt.load_program(program)
        schemas = list(rt.schemas.keys())
        return {"schemas": schemas, "errors": []}
    except Exception as e:
        line = getattr(e, "line", None)
        col = getattr(e, "column", None)
        return {
            "schemas": [],
            "errors": [{"line": line, "col": col, "message": str(e)}],
        }


@app.post("/evaluate")
def evaluate_schema(req: EvaluateRequest):
    """Evaluate a schema with optional given bindings and return the result."""
    try:
        from runtime.src.runtime import EvidentRuntime

        rt = EvidentRuntime()
        rt.load_source(req.source)
        result = rt.query(req.schema, given=req.given)
        return {
            "satisfied": result.satisfied,
            "bindings": result.bindings,
            "evidence": result.evidence.to_dict() if result.evidence else None,
        }
    except KeyError as e:
        raise HTTPException(status_code=404, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=400, detail=str(e))


@app.post("/ranges")
def get_ranges(req: RangesRequest):
    """Compute valid ranges for each variable in an isolated subprocess."""
    payload = {"source": req.source, "schema": req.schema, "given": req.given}
    key = _cache_key("ranges", payload)
    cached = _cache_get(key)
    if cached is not None:
        return cached
    result = _call_worker("ranges", payload)
    if "error" in result and "ranges" not in result:
        return {"ranges": {}, "error": result["error"]}
    _cache_put(key, result)
    return result


@app.post("/sample")
def sample_schema(req: SampleRequest):
    """Sample valid assignments in an isolated subprocess."""
    payload = {"source": req.source, "schema": req.schema, "given": req.given,
               "n": req.n, "strategy": req.strategy}
    key = _cache_key("sample", payload)
    cached = _cache_get(key)
    if cached is not None:
        return cached
    result = _call_worker("sample", payload, timeout=120)
    if "error" in result and "samples" not in result:
        raise HTTPException(status_code=400, detail=result["error"])
    _cache_put(key, result)
    return result


@app.post("/transfer")
def transfer_function(req: TransferRequest):
    """Sweep x_var across [x_min, x_max] in steps, solving for y_var at each point."""
    try:
        from runtime.src.runtime import EvidentRuntime

        points = []
        for i in range(req.steps):
            x_val = req.x_min + (req.x_max - req.x_min) * i / max(req.steps - 1, 1)
            x_int = int(round(x_val))

            rt = EvidentRuntime()
            rt.load_source(req.source)
            sweep_given = {**req.given, req.x_var: x_int}
            result = rt.query(req.schema, given=sweep_given)
            points.append(
                {
                    "x": x_val,
                    "y": result.bindings.get(req.y_var) if result.satisfied else None,
                    "feasible": result.satisfied,
                }
            )

        return {"points": points}
    except Exception as e:
        raise HTTPException(status_code=400, detail=str(e))


# ---------------------------------------------------------------------------
# Cache control
# ---------------------------------------------------------------------------


@app.post("/cache/enable")
def cache_enable():
    global _cache_enabled
    _cache_enabled = True
    return {"cache": "enabled", "entries": len(_cache)}


@app.post("/cache/disable")
def cache_disable():
    global _cache_enabled
    _cache_enabled = False
    _cache.clear()
    return {"cache": "disabled"}


@app.get("/cache/status")
def cache_status():
    return {"cache": "enabled" if _cache_enabled else "disabled", "entries": len(_cache)}


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000, reload=False)
