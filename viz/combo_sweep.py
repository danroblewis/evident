#!/usr/bin/env python3
"""Selector-evaluation sweep.

For each variable-rich sample program, render EVERY pair of interface variables
(forced onto the renderer via the EVIDENT_VIZ_VARS override) for a set of
selection-using renderers, and record which pair the SELECTOR itself chose
(state_vars[:2]). A fleet of reviewers then judges, per program, whether the
selector's pick is the most informative pair or whether it "hallucinated" — picked
a worse / degenerate combination than one of the alternatives.

    python3 viz/combo_sweep.py [program ...]      # default: all programs with >=3 interface vars

Output: viz/combo/<program>/<renderer>__<a>__<b>.png  + viz/combo/manifest.json
"""
import sys
import os
import json
import glob
import itertools
import subprocess
import pathlib
import concurrent.futures

ROOT = pathlib.Path(__file__).resolve().parent.parent
VIZ = ROOT / "viz"
IR = VIZ / "ir"
COMBO = VIZ / "combo"
# 2-axis, selection-using renderer (the axis pair is the thing under test). One
# faithful renderer keeps the review tractable and isolates the selector's choice.
RENDERERS = ["orbit_scatter"]
MAX_VARS = 8          # skip programs with too many vars (unreviewably many pairs)
WORKERS = max(4, (os.cpu_count() or 4) - 2)
TIMEOUT = 180


def interface_vars(name):
    s = json.load(open(IR / f"{name}.schema.json"))
    return [v["name"] for v in s["state"] if v.get("role", "interface") == "interface"]


def selector_pick(name):
    """The pair the selector itself chooses (state_vars[:2]), no override."""
    code = (
        "import sys; sys.path.insert(0, 'viz'); from evident_viz import load; "
        f"m = load('{IR}/{name}.smt2', '{IR}/{name}.schema.json'); "
        "print(','.join(v['name'] for v in m.state_vars[:2]))"
    )
    env = {k: v for k, v in os.environ.items() if k != "EVIDENT_VIZ_VARS"}
    r = subprocess.run([sys.executable, "-c", code], cwd=ROOT,
                       capture_output=True, text=True, env=env)
    return r.stdout.strip()


def render(job):
    name, rend, a, b = job
    safe = lambda s: s.replace(".", "-")
    out = COMBO / name / f"{rend}__{safe(a)}__{safe(b)}.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env["EVIDENT_VIZ_VARS"] = f"{a},{b}"
    try:
        r = subprocess.run([sys.executable, str(VIZ / f"render_{rend}.py"),
                            str(IR / f"{name}.smt2"), str(IR / f"{name}.schema.json"), str(out)],
                           cwd=ROOT, capture_output=True, text=True, timeout=TIMEOUT, env=env)
        ok = r.returncode == 0 and out.exists() and out.stat().st_size > 2000
    except subprocess.TimeoutExpired:
        ok = False
    return name, rend, a, b, ok


def main():
    progs = sys.argv[1:] or sorted(p.stem for p in IR.glob("*.smt2"))
    jobs, manifest = [], {}
    for name in progs:
        try:
            vs = interface_vars(name)
        except FileNotFoundError:
            continue
        if not (3 <= len(vs) <= MAX_VARS):    # >=3 for a meaningful pair; <=MAX to stay reviewable
            continue
        manifest[name] = {"vars": vs, "selector_pick": selector_pick(name),
                          "n_pairs": len(list(itertools.combinations(vs, 2)))}
        for rend in RENDERERS:
            for a, b in itertools.combinations(vs, 2):
                jobs.append((name, rend, a, b))

    print(f"{len(manifest)} programs, {len(jobs)} renders, {WORKERS} workers")
    for n, info in manifest.items():
        print(f"  {n}: {len(info['vars'])} vars, selector picks [{info['selector_pick']}]")

    COMBO.mkdir(parents=True, exist_ok=True)
    nok = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=WORKERS) as ex:
        for name, rend, a, b, ok in ex.map(render, jobs):
            nok += ok
            if not ok:
                print(f"  FAIL {name} {rend} {a},{b}")
    json.dump(manifest, open(COMBO / "manifest.json", "w"), indent=2)
    print(f"\n{nok}/{len(jobs)} renders ok. manifest -> {COMBO.relative_to(ROOT)}/manifest.json")


if __name__ == "__main__":
    sys.exit(main())
