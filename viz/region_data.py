"""region_data.py — the ABSTRACT `<out>.data.json` substrate for render_reachable_region.

The PNG is a picture; this is the picture's MEANING in machine-checkable form. A golden
test asserts on THIS, not on pixels: it captures what the reachable-region analysis proved
about WHERE the FSM can ever be, as data a domain expert can check against the model's true
mathematics.

Schema (`<out>.data.json`):

    {
      "view":     "reachable_region",
      "model":    "<fsm name>",
      "verdict":  "bounded" | "unbounded" | "indeterminate" | "unknown",
      "inductive": bool,            # proven closed under the transition (a real invariant)
      "axes":     {"x": "<short>", "y": "<short>"|null},   # the plotted axes (pinned or auto)
      "numeric_vars": ["<short>", ...],                    # all numeric carried vars
      "box":      {"<short>": [lo, hi], ...},  # the per-variable proven box (a hyper-rectangle)
      "center":   {"<short>": v, ...}|null,    # the initial state (the box should be ~centered here)
      "unbounded_vars":   ["<short>", ...],    # vars proven to grow without bound
      "note":     "<the analysis's own explanation>"|null
    }

Everything here is read off the SAME `bounding_box(model)` the renderer draws, plus the model's
initial_state — no second analysis, so the data can never disagree with the picture. The geometry
the box encodes is a hyper-rectangle (per-axis independent bounds); a test that wants the EXACT
reachable SHAPE (e.g. is it an L∞ square vs an L1 diamond) probes the transition itself — see
tests/viz_golden/region_oracle.py — and compares against this box.
"""
import json

from render_common import short


def _short_box(box):
    return {short(k): [v[0], v[1]] for k, v in box.items()}


def build(model, bounding, axes):
    """Assemble the `.data.json` dict from a loaded model + a `bounding_box(model)` result + the
    (x, y) axis short-names actually plotted (y may be None for a 1-D model)."""
    numeric = [v for v in model.carried if v["kind"] in ("int", "real")]
    init = model.initial_state() or {}
    center = ({short(v["name"]): init[v["name"]] for v in numeric if v["name"] in init}
              or None)
    return {
        "view": "reachable_region",
        "model": model.fsm,
        "verdict": bounding["verdict"],
        "inductive": bool(bounding.get("inductive")),
        "axes": {"x": axes[0], "y": axes[1]},
        "numeric_vars": [short(v["name"]) for v in numeric],
        "box": _short_box(bounding.get("box") or {}),
        "center": center,
        "unbounded_vars": [short(n) for n in bounding.get("unbounded") or []],
        "note": bounding.get("note"),
    }


def write(out_path, data):
    """Write `<out>.data.json`. Mirrors overlay_points.write_points: never raises — a sidecar
    failure must not fail the render."""
    try:
        with open(out_path + ".data.json", "w") as f:
            json.dump(data, f, indent=2)
    except Exception:
        pass
