"""timeseries_data.py — the ABSTRACT `<out>.data.json` substrate for render_time_series.

The PNG is an ensemble-of-trajectories picture; this is its MEANING in machine-checkable form. A
golden test asserts on THIS, not on pixels: for a stochastic system the time-series' whole content
is the SPREAD of reachable values at each tick (the ensemble envelope), so the data captures, per
NUMERIC var, the min/max band at every tick across the plotted trajectories — plus whether a true
ENSEMBLE (>1 trajectory) was drawn or the renderer fell back to a single run.

Schema (`<out>.data.json`):

    {
      "view": "time_series",
      "model": "<fsm name>",
      "mode": "ensemble" | "single_run",     # single_run = the unbounded-init fallback (no ensemble box)
      "note": "<the render's own note>",
      "n_trajectories": int,                  # how many runs were plotted (1 ⇒ no spread possible)
      "n_ticks": int,
      "numeric_vars": ["<short>", ...],       # the numeric tracks (the ones spread is meaningful for)
      "spread": {                             # per numeric var, per-tick min/max across trajectories
        "<short>": {"lo": [..per tick..], "hi": [..], "range": [hi-lo per tick]}
      }
    }

`spread[var]["range"]` is the headline: for a random walk it should be NON-ZERO and GROW with tick
(diffusion). A single deterministic line has range == 0 at every tick — the data says so honestly.
Built from the SAME trajectories the renderer plots, so the data can never disagree with the picture.
"""
import math

from render_common import short
from region_data import write   # reuse the never-raises writer (mirrors the sidecar convention)


def _numeric_spread(numeric_vars, trajs, nticks):
    """Per numeric var, the per-tick [lo, hi] band + range across all trajectories (None where no
    trajectory reaches that tick). `numeric_vars` is a list of var dicts; `trajs` a list of
    per-tick state dicts."""
    out = {}
    for v in numeric_vars:
        name = v["name"]
        lo, hi, rng = [], [], []
        for t in range(nticks):
            vals = [tr[t][name] for tr in trajs
                    if t < len(tr) and isinstance(tr[t].get(name), (int, float))
                    and not (isinstance(tr[t][name], float) and math.isnan(tr[t][name]))]
            if vals:
                lo.append(min(vals)); hi.append(max(vals)); rng.append(max(vals) - min(vals))
            else:
                lo.append(None); hi.append(None); rng.append(None)
        out[short(name)] = {"lo": lo, "hi": hi, "range": rng}
    return out


def build(model, trajs, numeric_vars, mode, note):
    """Assemble the time_series `.data.json` from the plotted trajectories. `trajs` is the list of
    per-tick state dicts actually drawn (the ensemble, or a 1-element list for the single-run
    fallback); `numeric_vars` the numeric var dicts whose spread is meaningful."""
    nticks = max((len(tr) for tr in trajs), default=0)
    return {
        "view": "time_series",
        "model": model.fsm,
        "mode": mode,
        "note": note,
        "n_trajectories": len(trajs),
        "n_ticks": nticks,
        "numeric_vars": [short(v["name"]) for v in numeric_vars],
        "spread": _numeric_spread(numeric_vars, trajs, nticks),
    }


def emit(out_path, model, trajs, numeric_vars, mode, note):
    """Build + write `<out>.data.json`; never raises (a sidecar failure must not fail the render)."""
    try:
        write(out_path, build(model, trajs, numeric_vars, mode, note))
    except Exception:
        pass
