"""golden.py — the shared harness for the viz golden-standard suite.

A golden test encodes what a DOMAIN EXPERT expects a diagram to convey for a given model, derived
from the model's TRUE MATHEMATICS — never from the current renderer output. The harness:

  * exports an Evident source to smt2+schema and loads a Model (runtime ground truth),
  * renders ONE view to a temp PNG (axes pinnable via x_var/y_var),
  * loads the renderer's `<out>.data.json` (the abstract substrate),
  * runs a list of EXPECTATIONS — each a named predicate over (model, data) returning ok/why,

and REPORTS pass/fail per (example, view, expectation) WITHOUT failing the build. The suite is a
STANDARD, not a CI gate: a red row means "this diagram does not yet meet the expert expectation",
which is the signal that drives a fix — not a broken build.

Usage from repo root:  python3 tests/viz_golden/run.py
"""
import json
import os
import sys
import tempfile

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
for p in (os.path.join(_ROOT, "ide", "web"), os.path.join(_ROOT, "viz")):
    if p not in sys.path:
        sys.path.insert(0, p)


class Check:
    """One expert expectation: a name + a predicate (model, data) -> (ok: bool, detail: str)."""
    def __init__(self, name, fn):
        self.name, self.fn = name, fn

    def run(self, model, data):
        try:
            ok, detail = self.fn(model, data)
            return bool(ok), detail
        except Exception as e:                                 # a thrown check is a FAIL, not a crash
            return False, f"check raised: {type(e).__name__}: {e}"


def run_case(name, source, view, checks, x_var=None, y_var=None):
    """Export+load `source`, render `view` (pinning x_var/y_var), load <out>.data.json, run every
    check. Returns {example, view, axes_requested, results:[(check, ok, detail)], error}."""
    from runtime_io import _export
    from evident_viz import load as load_model

    rec = {"example": name, "view": view, "axes_requested": {"x": x_var, "y": y_var},
           "results": [], "error": None}
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, _dropped, msg = _export(source, w)
        if not ok:
            rec["error"] = f"export failed: {msg.splitlines()[0][:80]}"
            return rec
        smt2, schema = prefix + ".smt2", prefix + ".schema.json"
        out = os.path.join(w, f"{view}.png")
        # Drive the renderer through the IDE's OWN adapter (ide/web/render.py::RENDERERS), the single
        # source of truth for the DUAL CONTRACT: it normalizes EVERY renderer shape —
        # render(smt2,schema,out[,x_var,y_var]) · render(model,out) · render(model,out,all_conditions)
        # · CLI main() — to one uniform (smt2, schema, out, x_var=, y_var=) call, and threads axes only
        # to views that declare them. So the harness exercises the EXACT path the product uses, and a
        # model-taking renderer (terminal_map/transition_matrix/space_time) is loaded for it by the
        # adapter — the test never has to know the renderer's shape.
        from render import RENDERERS
        RENDERERS[view](smt2, schema, out, x_var=x_var, y_var=y_var)
        # The model for ORACLE checks is loaded SEPARATELY (the renderer got its own via the adapter)
        # — the oracle probes the transition independently of the renderer.
        model = load_model(smt2, schema)
        data_path = out + ".data.json"
        if not os.path.exists(data_path):
            rec["error"] = "renderer wrote no <out>.data.json"
            return rec
        with open(data_path) as fh:
            data = json.load(fh)
        rec["data"] = data
        for c in checks:
            okc, detail = c.run(model, data)
            rec["results"].append((c.name, okc, detail))
    return rec


def report(records):
    """Print a per-(example, view, check) pass/fail table + a summary. Returns the number of FAILED
    checks (for an informational exit code — the RUNNER still exits 0 so the suite never gates CI)."""
    failed = total = 0
    for rec in records:
        head = f"{rec['example']}  ·  {rec['view']}"
        ax = rec["axes_requested"]
        if ax["x"] or ax["y"]:
            head += f"   [axes pinned x={ax['x']} y={ax['y']}]"
        print(f"\n=== {head} ===")
        if rec["error"]:
            print(f"  ERROR: {rec['error']}")
            failed += 1
            total += 1
            continue
        for cname, ok, detail in rec["results"]:
            total += 1
            failed += (0 if ok else 1)
            mark = "PASS" if ok else "FAIL"
            print(f"  [{mark}] {cname}")
            if detail:
                print(f"         {detail}")
    print(f"\n{'-' * 60}\nGOLDEN SUMMARY: {total - failed}/{total} expectations met "
          f"({failed} unmet — informational, does NOT gate CI)")
    return failed
