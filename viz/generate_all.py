"""generate_all — diagram EVERY schema in every example program (one file each).

Walks the example .ev corpus, lists each file's SAT schemas, and runs the generic
generator on each — a separate diagram file per (file, schema). Robust: a failing
schema is skipped, not fatal. Then builds a contact sheet of everything.
"""
import glob
import os
import sys
import traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import diagram
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

ROOT = diagram.ROOT
DIRS = ["ide/examples", "programs"]            # .ev corpora to visualise
OUT = os.path.join(ROOT, "viz", "diagrams")


def ev_files():
    fs = []
    for d in DIRS:
        fs += glob.glob(os.path.join(ROOT, d, "*.ev"))
    return sorted(fs)


def run():
    os.makedirs(OUT, exist_ok=True)
    ok = skip = 0
    for ev in ev_files():
        rel = os.path.relpath(ev, ROOT)
        try:
            src = open(ev, encoding="utf-8").read()
            schemas = diagram.list_schemas(rel)
        except Exception as e:
            print(f"  [file-skip] {rel}: {str(e)[:60]}", flush=True); continue
        for sch in schemas:
            try:
                p = diagram.generate(rel, sch, src, OUT)
                if p:
                    print(f"  [ok]   {os.path.basename(p)}", flush=True); ok += 1
                else:
                    print(f"  [none] {rel} {sch}", flush=True); skip += 1
            except Exception as e:
                print(f"  [skip] {rel} {sch}: {str(e)[:70]}", flush=True); skip += 1
    print(f"\n{ok} diagrams generated, {skip} skipped", flush=True)
    return ok


def contact_sheet():
    imgs = sorted(glob.glob(os.path.join(OUT, "*.png")))
    if not imgs:
        return
    cols = 4
    rows = (len(imgs) + cols - 1) // cols
    fig, axes = plt.subplots(rows, cols, figsize=(cols * 3.6, rows * 3.0))
    axes = axes.ravel() if rows * cols > 1 else [axes]
    for ax, img in zip(axes, imgs):
        ax.imshow(plt.imread(img)); ax.axis("off")
        ax.set_title(os.path.basename(img)[:-4], fontsize=6.5, color="#2a2c34")
    for ax in axes[len(imgs):]:
        ax.axis("off")
    fig.suptitle("Diagrams generated from every example schema", fontsize=15,
                 weight="bold", color="#2a2c34")
    fig.tight_layout(rect=(0, 0, 1, 0.985))
    out = os.path.join(ROOT, "viz", "contact_sheet.png")
    fig.savefig(out, dpi=110, facecolor="white"); plt.close(fig)
    print("wrote", os.path.relpath(out, ROOT))


if __name__ == "__main__":
    if run():
        contact_sheet()
