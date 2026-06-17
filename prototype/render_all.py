"""render_all — regenerate EVERY example image from the current renderer.

One command to re-render the whole set, so a change to phaseportrait.render (or any
model) propagates everywhere at once — no hand-tweaking individual images. Then it
builds results/contact_sheet.png, a single montage of all per-model portraits, so
you can see the whole gallery change together.

Run from prototype/:  python3 render_all.py
"""
import glob
import importlib
import os
import traceback

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HERE = os.path.dirname(os.path.abspath(__file__))

# every script that emits images, in dependency order (the gallery first)
SCRIPTS = ["model_gallery", "phase_portrait", "phaseportrait", "diagram_zoo",
           "utility_programs", "phase_semantics", "highdim_demo",
           "fibonacci_honest"]


def run_all():
    for name in SCRIPTS:
        try:
            mod = importlib.import_module(name)
            importlib.reload(mod)              # pick up edits if re-run in one session
            mod.main()
            plt.close("all")
            print(f"  [ok]   {name}", flush=True)
        except Exception as e:                 # one bad script shouldn't stop the rest
            print(f"  [FAIL] {name}: {e}", flush=True)
            traceback.print_exc()
            plt.close("all")


def contact_sheet():
    imgs = sorted(glob.glob(os.path.join(HERE, "results", "models", "*.png")))
    if not imgs:
        print("  (no model images to montage)"); return
    cols = 2
    rows = (len(imgs) + cols - 1) // cols
    fig, axes = plt.subplots(rows, cols, figsize=(cols * 8.0, rows * 3.0))
    axes = axes.ravel()
    for ax, img in zip(axes, imgs):
        ax.imshow(plt.imread(img))
        ax.set_title(os.path.basename(img)[:-4], fontsize=9, color="#2a2c34")
        ax.axis("off")
    for ax in axes[len(imgs):]:
        ax.axis("off")
    fig.suptitle("All example models — contact sheet", fontsize=15, weight="bold",
                 color="#2a2c34")
    fig.tight_layout(rect=(0, 0, 1, 0.985))
    out = os.path.join(HERE, "results", "contact_sheet.png")
    fig.savefig(out, dpi=110, facecolor="white"); plt.close(fig)
    print("wrote", os.path.relpath(out), flush=True)


def main():
    print("re-rendering every example...", flush=True)
    run_all()
    print("building contact sheet...", flush=True)
    contact_sheet()


if __name__ == "__main__":
    main()
