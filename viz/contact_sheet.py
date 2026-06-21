#!/usr/bin/env python3
"""Generate visualization diagrams for a list of sample Evident programs and build
a markdown contact sheet grouped by program.

For each sample it runs `evident export` to produce the transition IR, then runs
EVERY `viz/render_<type>.py` renderer against it (in a worker pool), and writes
`viz/CONTACT_SHEET.md` with one section per program showing all its diagrams.

Extending:
  - add a program: append its path to SAMPLES (or pass paths as CLI args)
  - add a visualization: drop a `viz/render_<type>.py` (auto-discovered)

Usage:
  python3 viz/contact_sheet.py [program.ev ...]        # default: SAMPLES below
  python3 viz/contact_sheet.py --workers 8 a.ev b.ev
"""
import sys
import os
import glob
import json
import subprocess
import concurrent.futures
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent          # repo root
VIZ = ROOT / "viz"
EVIDENT = ROOT / "runtime" / "target" / "release" / "evident"
IR_DIR = VIZ / "ir"
GALLERY = VIZ / "gallery"
SHEET = VIZ / "CONTACT_SHEET.md"
RENDER_TIMEOUT = 240
COLS = 3              # diagrams per row in the contact sheet (override: --cols N)
IMG_WIDTH = 360       # thumbnail width in px

# Extensible list of sample programs. Each should be a single-fsm Evident program.
SAMPLES = [
    "examples/daemons/vanderpol.ev",   # numeric  — limit cycle
    "examples/daemons/dungeon.ev",     # discrete — text-adventure map
    "examples/daemons/vending.ev",     # mixed    — enum + int + bool, limit cycle
]


def renderers():
    return sorted(glob.glob(str(VIZ / "render_*.py")))


def viz_type(path):
    return os.path.basename(path)[len("render_"):-len(".py")]


def export(sample):
    """Run `evident export`. Returns (name, sample, ir or None) where ir is
    (smt2_path, schema_path)."""
    name = pathlib.Path(sample).stem
    out = IR_DIR / name
    r = subprocess.run([str(EVIDENT), "export", sample, "--out", str(out)],
                       cwd=ROOT, capture_output=True, text=True)
    if r.returncode != 0:
        return name, sample, None, r.stderr.strip()[-300:]
    return name, sample, (f"{out}.smt2", f"{out}.schema.json"), ""


def interestingness(path):
    """A crude visual-content score so degenerate / near-blank diagrams sink and
    busy, structured ones float. Background-agnostic (grayscale contrast + edge
    density), so it works across the renderers' different themes."""
    try:
        import numpy as np
        import matplotlib.image as mpimg
        img = mpimg.imread(str(path))
        a = img[..., :3] if img.ndim == 3 else img
        g = a.mean(axis=2) if a.ndim == 3 else a
        gy, gx = np.gradient(g.astype(float))
        return round(float(g.std()) + 8.0 * float(np.hypot(gx, gy).mean()), 4)
    except Exception:
        return 0.0


def render(job):
    rp, vt, name, smt2, schema = job
    out = GALLERY / f"{vt}__{name}.png"
    try:
        r = subprocess.run([sys.executable, rp, smt2, schema, str(out)],
                           cwd=ROOT, capture_output=True, text=True,
                           timeout=RENDER_TIMEOUT)
        ok = r.returncode == 0 and out.exists() and out.stat().st_size > 2000
        err = "" if ok else (r.stderr.strip()[-300:] or "no output produced")
    except subprocess.TimeoutExpired:
        ok, err = False, f"timeout >{RENDER_TIMEOUT}s"
    score = interestingness(out) if ok else 0.0
    return vt, name, out, ok, err, score


def describe(ir):
    """One-line state description from the schema JSON."""
    try:
        s = json.load(open(ir[1]))
    except Exception:
        return ""
    vs = s.get("state", [])
    kinds = {v["kind"] for v in vs}
    cat = ("discrete" if kinds <= {"bool", "enum", "string"}
           else "numeric" if kinds <= {"int", "real"} else "mixed")
    return cat + " — " + ", ".join(f"{v['name']} ({v['kind']})" for v in vs)


def main():
    args = sys.argv[1:]
    workers = max(4, (os.cpu_count() or 4) - 2)
    if "--workers" in args:
        i = args.index("--workers")
        workers = int(args[i + 1])
        del args[i:i + 2]
    cols = COLS
    if "--cols" in args:
        i = args.index("--cols")
        cols = int(args[i + 1])
        del args[i:i + 2]
    samples = args or SAMPLES

    IR_DIR.mkdir(parents=True, exist_ok=True)
    GALLERY.mkdir(parents=True, exist_ok=True)
    rs = renderers()
    if not rs:
        print("no viz/render_*.py renderers found", file=sys.stderr)
        return 1
    print(f"{len(samples)} programs x {len(rs)} renderers, {workers} workers\n")

    # Phase 1 — export every sample's transition IR (parallel).
    exported = {}      # name -> ir | None
    order = []         # [(name, sample, err)]
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as ex:
        for name, sample, ir, err in ex.map(export, samples):
            exported[name] = ir
            order.append((name, sample, err))
            print(("export ok  " if ir else "export FAIL ") + name + (f"  {err}" if err else ""))

    # Phase 2 — render every (sample x renderer) in the worker pool.
    jobs = [(rp, viz_type(rp), name, ir[0], ir[1])
            for name, _, _ in order if (ir := exported[name])
            for rp in rs]
    results = {}       # (vt, name) -> (out, ok, err)
    print()
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as ex:
        for vt, name, out, ok, err, score in ex.map(render, jobs):
            results[(vt, name)] = (out, ok, err, score)
            print(("  ok   " if ok else "  FAIL ") + f"{vt} / {name}"
                  + (f"  [{score:.2f}]" if ok else f"  {err}"))

    # Phase 3 — markdown contact sheet, grouped by program.
    write_sheet(order, exported, rs, results, cols)
    n_ok = sum(1 for v in results.values() if v[1])
    print(f"\nwrote {SHEET.relative_to(ROOT)}  ({n_ok}/{len(jobs)} diagrams ok)")
    return 0


def write_sheet(order, exported, rs, results, cols=COLS):
    """Markdown grouped by program. Each row-group of `cols` diagrams is its own
    small table: the HEADER row holds the viz-type labels and the single DATA row
    holds the native-markdown images — so labels sit above images using ONLY
    standard markdown (no raw HTML)."""
    types = [viz_type(rp) for rp in rs]
    out = ["# Evident visualization contact sheet", "",
           f"_{len(order)} programs x {len(rs)} visualization types — generated by "
           f"`viz/contact_sheet.py` ({cols} per row). Grouped by program._", "",
           "Jump to: " + " · ".join(f"[{n}](#{n})" for n, _, _ in order), ""]
    for name, sample, err in order:
        out += [f"## {name}", "", f"`{sample}`"]
        ir = exported[name]
        if not ir:
            out += ["", f"**export failed:** {err}", ""]
            continue
        out += ["", f"_{describe(ir)}_   _(diagrams sorted by interestingness)_", ""]
        ranked = sorted(types, key=lambda vt: -results.get((vt, name), (None, False, "", 0.0))[3])
        for r0 in range(0, len(ranked), cols):
            group = ranked[r0:r0 + cols]
            labels, imgs = [], []
            for vt in group:
                o, ok, rerr, score = results.get((vt, name), (None, False, "missing", 0.0))
                labels.append(f"{vt} · {score:.2f}" if ok else vt)
                if ok:
                    imgs.append(f"![{vt}]({os.path.relpath(o, VIZ)})")
                else:
                    imgs.append("_failed: " + rerr.replace("|", "/").replace("\n", " ") + "_")
            out.append("| " + " | ".join(labels) + " |")     # labels + score (header row)
            out.append("|" + "---|" * len(group))
            out.append("| " + " | ".join(imgs) + " |")        # images (data row)
            out.append("")
    SHEET.write_text("\n".join(out) + "\n")


if __name__ == "__main__":
    sys.exit(main())
