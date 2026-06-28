#!/usr/bin/env python3
"""Evident Web IDE — the program runner router.

One endpoint: POST /api/run — writes the source to a temp .ev, shells
`evident effect-run` with a step cap, and returns stdout + stderr + exit code.

Mounted onto the FastAPI app in `server.py` via `app.include_router(router)`.
"""
import os
import subprocess
import tempfile

from config import EVIDENT, ROOT
from fastapi import APIRouter
from pydantic import BaseModel

router = APIRouter()

_DEFAULT_MAX_STEPS = 200
_TIMEOUT_SECS = 15


class RunReq(BaseModel):
    source: str
    args: list[str] | None = None          # extra CLI args (reserved for future use)
    max_steps: int | None = None           # step cap — defaults to _DEFAULT_MAX_STEPS


@router.post("/api/run")
def run_program(req: RunReq):
    """Run `evident effect-run` on the source. Always passes --max-steps to prevent
    an infinite FSM from hanging the server. Returns stdout, stderr, exit_code, and
    a note when the subprocess timed out."""
    max_steps = max(1, min(req.max_steps or _DEFAULT_MAX_STEPS, 10000))
    with tempfile.TemporaryDirectory() as work:
        ev = os.path.join(work, "prog.ev")
        with open(ev, "w") as f:
            f.write(req.source)
        cmd = [EVIDENT, "effect-run", ev, "--max-steps", str(max_steps)]
        if req.args:
            cmd.extend(req.args)
        timed_out = False
        try:
            r = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=_TIMEOUT_SECS,
                cwd=ROOT,
            )
            stdout = r.stdout or ""
            stderr = r.stderr or ""
            exit_code = r.returncode
        except subprocess.TimeoutExpired as e:
            stdout = (e.stdout or b"").decode("utf-8", errors="replace")
            stderr = (e.stderr or b"").decode("utf-8", errors="replace")
            exit_code = -1
            timed_out = True

        note = None
        if timed_out:
            note = f"timed out after {_TIMEOUT_SECS}s — output above is partial"
        elif exit_code == 0 and not stdout and not stderr:
            note = "program exited cleanly with no output"

        return {
            "ok": True,
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
            "timed_out": timed_out,
            "max_steps": max_steps,
            "note": note,
        }
