#!/usr/bin/env python3
"""render_function_common.py — shared shapes for the functionizer diagram family.

The six `render_function_*.py` views (graph, residual, guards, behavior, complexity, …) each open
the same way — load the model, extract the per-variable functions — and each used to carry its own
copy of a `_placeholder`, a `__main__` arg-check, and the deps→input-variable derivation. Those are
lifted here so a renderer states only what is unique to its picture.

Nothing here draws a real diagram; it's load boilerplate, the empty-state placeholder, the dep
plumbing, and the CLI shim. Matplotlib's Agg backend selection stays in each renderer (it must run
before that module imports pyplot).
"""
import sys

sys.path.insert(0, "viz")
from evident_viz import load
from functionize import extract_functions


def load_functions(smt2, schema):
    """The opening every function renderer shares: (model, extracted-functions)."""
    m = load(smt2, schema)
    return m, extract_functions(m)


def step_dep_vars(step):
    """The carried-input deps a step reads, deduped + sorted — guarded steps gather across branches."""
    return sorted({d for b in step.get("branches", []) for d in b["deps"]} | set(step.get("deps", [])))


def step_inputs(step, prev_to_var):
    """The carried VARIABLES a step reads (each prev-dep mapped back to its source var)."""
    return [prev_to_var[d] for d in step_dep_vars(step) if d in prev_to_var]


def placeholder(out_path, fsm, title, msg, dark=False):
    """The empty-state card every function renderer falls back to when there's nothing to draw.
    `dark` matches the dark-canvas renderers (guards); the rest use the default light card."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    fig, ax = plt.subplots(figsize=(8, 6))
    if dark:
        fig.patch.set_facecolor("#0f1419")
        ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13, color="#c9d1d9")
        ax.set_axis_off(); ax.set_title(f"{fsm}  —  {title}", color="#c9d1d9")
        fig.savefig(out_path, dpi=120, bbox_inches="tight", facecolor="#0f1419")
    else:
        ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13)
        ax.set_axis_off(); ax.set_title(f"{fsm}  —  {title}")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def cli_main(render_fn, argv, script):
    """The `if __name__ == '__main__'` shim shared by every function renderer:
    <script> <smt2> <schema> <out>, else usage to stderr + exit 2."""
    if len(argv) != 4:
        print(f"usage: {script} <smt2> <schema> <out>", file=sys.stderr)
        sys.exit(2)
    render_fn(argv[1], argv[2], argv[3])
