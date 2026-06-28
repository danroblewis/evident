"""valueheat_data.py — the ABSTRACT `<out>.data.json` substrate for render_value_heatmap.

The PNG packs every carried variable's value-over-time into a raster (one row per leaf, one column
per tick). This captures that raster's MEANING as machine-checkable data, built from the SAME `rows`
and `traj` the renderer draws — so the data can never disagree with the picture.

Schema (`<out>.data.json`):

    {
      "view":   "value_heatmap",
      "model":  "<fsm name>",
      "rows":   ["<short var name>", ...],   # one entry per rastered leaf (the y-axis)
      "ticks":  <int>,                       # number of columns actually rastered (the trajectory length)
      "nondeterministic": bool,              # True ⇒ this is ONE sampled run of many (a free-input FSM)
      "halted": bool,                        # True ⇒ the walk stopped (fixed point / revisit) before the cap
      "max_ticks": <int>,                    # the tick cap the renderer would walk to (so ticks ≪ cap = a dead run)
      "series": {                            # per-row: the literal value sequence the raster colors
         "<short>": {"values": [...], "min": v, "max": v, "n_distinct": <int>}
      },
      "note":   "<the renderer's N/A reason>"|null   # set only on the N/A panel path
    }

`n_distinct == 1` for a row is a FLAT band — a variable the raster shows as never changing. For a
random walk that is a regression: the var should wander. `ticks ≪ max_ticks` means the sampled walk
died early (the king-move walk revisits the origin and the dedup'd walker calls it a fixed point).
"""
import json

from render_common import short


def build(model, rows, traj, nondet, halted, max_ticks, note=None):
    """Assemble the value_heatmap data dict from the renderer's OWN rows + sampled trajectory."""
    series = {}
    for var in rows:
        vals = [s.get(var["name"]) for s in traj]
        nums = [v for v in vals if isinstance(v, (int, float)) and not isinstance(v, bool)]
        series[short(var["name"])] = {
            "values": vals,
            "min": (min(nums) if nums else None),
            "max": (max(nums) if nums else None),
            "n_distinct": len({v for v in vals if v is not None}),
        }
    return {
        "view": "value_heatmap",
        "model": model.fsm,
        "rows": [short(v["name"]) for v in rows],
        "ticks": len(traj),
        "nondeterministic": bool(nondet),
        "halted": bool(halted),
        "max_ticks": max_ticks,
        "series": series,
        "note": note,
    }


def write(out_path, data):
    """Write `<out>.data.json`. Mirrors region_data.write: never raises — a sidecar failure must not
    fail the render."""
    try:
        with open(out_path + ".data.json", "w") as f:
            json.dump(data, f, indent=2)
    except Exception:
        pass
