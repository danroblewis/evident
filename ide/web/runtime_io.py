"""Subprocess wrappers around the `evident` binary.

`_export` runs `evident export` (source → SMT-LIB + schema JSON) and cleans the
user-facing error text; `_run_query` runs `evident query --json` (the same
encode+solve path as `test`). Both write the source to a temp `.ev` and shell out
to the release binary at `config.EVIDENT`, from `config.ROOT`.
"""
import os
import subprocess

from config import EVIDENT, ROOT


def _export(source: str, work: str, entry: str | None = None):
    """Write source, run `evident export`. Returns (ok, prefix, dropped, message).

    `entry` names which top-level fsm/claim to render (the IDE's entry picker, #290);
    when omitted the binary defaults to the LAST-DEFINED fsm-or-claim in source order."""
    ev = os.path.join(work, "prog.ev")
    with open(ev, "w") as f:
        f.write(source)
    prefix = os.path.join(work, "prog")
    cmd = [EVIDENT, "export", ev, "--out", prefix]
    if entry:
        cmd += ["--entry", entry]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=30, cwd=ROOT)
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
