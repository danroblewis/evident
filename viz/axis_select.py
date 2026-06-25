"""axis_select — explicit projection-axis override for the axis-taking views (#445/#421).

The 2-axis views (phase_portrait, nullcline_field, scatter_matrix, orbit_scatter,
occupancy_heatmap) and the 1-axis cobweb each AUTO-PICK their projection axes from the
ranked vars. #445 lets the caller pass explicit `x_var`/`y_var` names (the owner's axis
selector, #421); this module is the ONE place that resolves a requested name against the
model and records what was actually used, so every renderer honors the override identically
and the response can echo the active axes + flag a rejected request.

`resolve_axes` takes the renderer's OWN default vars (so the fallback is unchanged — this is
purely additive) and swaps in a requested var only when it names a real, type-appropriate
carried leaf. `write_axes` drops a `<out>.axes.json` sidecar (mirroring write_points) that the
server reads back into the analyze response."""
import json


def _by_name(m, name):
    """The carried/derived var dict whose SHORT or full name matches `name`, else None.
    Accepts either the short leaf name (`balance`, what the UI shows) or the full dotted
    name (`state.balance`, what the schema carries)."""
    if not name:
        return None
    for v in m.carried + getattr(m, "derived", []):
        if v["name"] == name or v["name"].split(".")[-1] == name:
            return v
    return None


def resolve_axes(m, x_var, y_var, default_x, default_y, candidates=None):
    """Resolve the (x, y) axis var DICTS, honoring an explicit request and falling back to the
    renderer's auto-pick. A requested name is accepted only if it names a real carried leaf in
    `candidates` (default: any numeric carried var — the only kind that drives a continuous axis);
    an unknown / wrong-type / out-of-candidate-set name is IGNORED and the default stands.

    Returns (x, y, info) where x/y are var dicts (or None, mirroring the passed defaults) and
    `info` is the echo dict: {x, y, requested:{x,y}, fell_back:bool} with SHORT names for the UI."""
    pool = candidates if candidates is not None else \
        [v for v in m.carried if v["kind"] in ("int", "real")]
    pool_names = {v["name"] for v in pool}

    def pick(req):
        """The requested var if it's a valid candidate, else None (caller uses the default)."""
        v = _by_name(m, req)
        return v if (v is not None and v["name"] in pool_names) else None

    rx, ry = pick(x_var), pick(y_var)
    x = rx if rx is not None else default_x
    y = ry if ry is not None else default_y
    # fell_back: a non-empty request named a var the renderer could NOT honor (unknown name or
    # not a valid axis candidate) — so the auto-pick default stands and the UI should flag it.
    fell_back = bool((x_var and rx is None) or (y_var and ry is None))
    info = {
        "x": x["name"].split(".")[-1] if x else None,
        "y": y["name"].split(".")[-1] if y else None,
        "requested": {"x": x_var, "y": y_var},
        "fell_back": fell_back,
    }
    return x, y, info


def write_axes(out_path, info):
    """Write the `<out>.axes.json` sidecar the server reads back into the analyze response, so
    the UI shows the active axes and can tell when a requested var was rejected (fell_back)."""
    try:
        with open(out_path + ".axes.json", "w") as f:
            json.dump(info, f)
    except OSError:
        pass
