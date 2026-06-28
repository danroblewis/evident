"""fixedpoint_data.py — the abstract `<out>.data.json` substrate for render_fixedpoint_map.

The picture's MEANING in machine-checkable form: WHERE the system comes to rest (its attractors).
A golden test asserts on this — for a system with no equilibria (a free random walk) the correct,
expert content is "no fixed points, no limit cycles", and this data says so explicitly rather than
leaving a blank the test can't distinguish from a render failure.

Schema (`<out>.data.json`):
    {
      "view": "fixedpoint_map",
      "model": "<fsm>",
      "mode": "all-conditions" | "reachable" | "grid" | "none",  # how states were seeded
      "n_states": <int>,                       # states sampled/enumerated
      "fixed_point_count": <int>,              # states s with s ∈ successors(s) AND no other successor
      "cycle_count": <int>,                    # detected limit cycles
      "cycle_periods": [<int>, ...],           # the periods found (empty if none)
      "has_equilibria": <bool>,                # fixed_point_count>0 or cycle_count>0
      "axes": {"x": "<short>"|null, "y": "<short>"|null}
    }
"""
import json


def build(model, mode, n_states, fixed, cycles, xvar, yvar):
    periods = sorted({len(c) - 1 for c in cycles}) if cycles else []
    return {
        "view": "fixedpoint_map",
        "model": model.fsm,
        "mode": mode or "none",
        "n_states": int(n_states),
        "fixed_point_count": len(fixed or []),
        "cycle_count": len(cycles or []),
        "cycle_periods": periods,
        "has_equilibria": bool(fixed) or bool(cycles),
        "axes": {"x": xvar["name"].split(".")[-1] if xvar else None,
                 "y": yvar["name"].split(".")[-1] if yvar else None},
    }


def write(out_path, data):
    """Mirror region_data.write — never raises; a sidecar failure must not fail the render."""
    try:
        with open(out_path + ".data.json", "w") as f:
            json.dump(data, f, indent=2)
    except Exception:
        pass


def emit(out_path, model, mode, states, fixed, cycles, xvar, yvar):
    """build + write in one call (keeps the renderer's render() short)."""
    write(out_path, build(model, mode, len(states), fixed, cycles, xvar, yvar))
