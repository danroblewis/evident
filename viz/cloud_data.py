"""cloud_data.py — the ABSTRACT `<out>.data.json` substrate for the (x,y)-CLOUD views
(phase_portrait, scatter_matrix): the diagram MEANS "where the reachable states lie in the
(x,y) plane", so the machine-checkable substrate is that point cloud + its geometry.

A golden test asserts on THIS, not pixels. Two clouds are distinguished honestly:

  * "rendered"  — the points the renderer ACTUALLY plotted (its own sample/field). For a phase
                  portrait that follows one seed chain, or a scatter matrix that walks one
                  trajectory, this is a single drifting run — NOT the reachable set. Recording it
                  lets a test catch "the picture shows one run, not all initial conditions".
  * "reachable" — the relational reachable fan m.reachable() projects onto (x,y): the set the
                  diagram SHOULD convey (every state the FSM can occupy). The golden expectation
                  (symmetric, origin-centred, fills the square) is asserted against the picture's
                  fidelity to THIS.

Schema (`<out>.data.json`):

    {
      "view":   "phase_portrait" | "scatter_matrix",
      "model":  "<fsm name>",
      "axes":   {"x": "<short>", "y": "<short>"|null},
      "center": {"x": v, "y": v}|null,           # the initial state on the plane
      "regime": "<renderer's regime/path>"|null, # phase_portrait: numeric/discrete/degenerate/na
      "rendered_na": bool,                       # the renderer drew an N/A / placeholder card
      "rendered": {                              # the cloud the renderer actually drew (may be a
        "n": int, "x": [lo,hi]|null, "y": [lo,hi]|null, "symmetric_x": bool, "symmetric_y": bool
      },
      "reachable": {                             # the relational reachable fan projected to (x,y)
        "n": int, "x": [lo,hi]|null, "y": [lo,hi]|null, "symmetric_x": bool, "symmetric_y": bool,
        "fills_corners": bool                    # the (±k,±k) diagonal extent is occupied (L∞, not L1)
      }
    }
"""
import json

from render_common import short


def _span(vals):
    # Keep only finite numerics; NaN-guard (v == v) AND non-numeric guard (str, list, …) for
    # models whose carried state is an enum/string/seq — those raw values reach _cloud via
    # trajectory() and reachable_cloud() and must not be compared with >/< against 0.
    vals = [v for v in vals if isinstance(v, (int, float)) and v == v]
    return [min(vals), max(vals)] if vals else None


def _symmetric_about_zero(vals, tol_frac=0.25):
    """True iff the value set is ~symmetric about 0: |min| and |max| within tol_frac, AND it
    straddles the origin. A one-sided drift (a single trajectory) fails this."""
    sp = _span(vals)
    if sp is None:
        return False
    lo, hi = sp
    if lo > 0 or hi < 0:                      # doesn't straddle origin
        return False
    a, b = abs(lo), abs(hi)
    big = max(a, b)
    return big > 0 and abs(a - b) <= tol_frac * big


def _cloud(points, xn, yn):
    xs = [p[xn] for p in points]
    ys = [p[yn] for p in points] if yn else []
    return {
        "n": len(points),
        "x": _span(xs),
        "y": _span(ys) if yn else None,
        "symmetric_x": _symmetric_about_zero(xs),
        "symmetric_y": _symmetric_about_zero(ys) if yn else False,
    }


def _fills_corners(points, xn, yn):
    """The L∞-square signature: the diagonal extent is occupied — some plotted point has BOTH
    |x| and |y| near the max radius. An L1-diamond / axis-only cloud never reaches a corner.
    Returns False for non-numeric axes (enum/string/seq models whose values aren't comparable)."""
    if not yn:
        return False
    num_points = [p for p in points
                  if isinstance(p.get(xn), (int, float)) and isinstance(p.get(yn), (int, float))]
    if not num_points:
        return False
    xs = [abs(p[xn]) for p in num_points]
    ys = [abs(p[yn]) for p in num_points]
    rx, ry = max(xs), max(ys)
    if rx <= 0 or ry <= 0:
        return False
    return any(abs(p[xn]) >= 0.6 * rx and abs(p[yn]) >= 0.6 * ry for p in num_points)


def build(model, view, xn, yn, rendered_points, regime=None, rendered_na=False,
          reachable_points=None):
    """Assemble the cloud `.data.json`. `xn`/`yn` are the SHORT plotted-axis names (yn may be
    None). `rendered_points` is what the renderer drew; `reachable_points` is the relational
    reachable fan (the SHOULD-show set) — when None it's omitted."""
    init = model.initial_state() or {}
    center = None
    if xn in init or (yn and yn in init):
        center = {"x": init.get(xn), "y": init.get(yn) if yn else None}
    out = {
        "view": view,
        "model": model.fsm,
        "axes": {"x": xn, "y": yn},
        "center": center,
        "regime": regime,
        "rendered_na": bool(rendered_na),
        "rendered": _cloud(rendered_points, xn, yn),
    }
    if reachable_points is not None:
        rc = _cloud(reachable_points, xn, yn)
        rc["fills_corners"] = _fills_corners(reachable_points, xn, yn)
        out["reachable"] = rc
    return out


def reachable_cloud(model, xn, yn, limit=3000):
    """The relational reachable fan m.reachable() projected onto (xn, yn) as a list of {xn:.., yn:..}
    dicts — the set the (x,y) views SHOULD convey (every state the FSM can occupy), independent of
    whatever single run the renderer happened to sample."""
    try:
        states, _ = model.reachable(limit=limit)
    except Exception:
        return []
    pts = []
    for s in states:
        if xn in s and (yn is None or yn in s):
            pts.append({xn: s[xn], **({yn: s[yn]} if yn else {})})
    return pts


def emit_scatter(out_path, model, vars_, states):
    """scatter_matrix's data emission (moved off the renderer to keep its free-function count down):
    the (x,y) cell's plotted cloud + the relational reachable fan + the matrix's pairwise axes."""
    from render_common import short
    nums = model.numeric_vars or [v for v in vars_ if v["kind"] in ("int", "real")]
    if len(nums) < 2:
        return
    xn, yn = short(nums[0]["name"]), short(nums[1]["name"])
    rendered = [{xn: s[nums[0]["name"]], yn: s[nums[1]["name"]]}
                for s in states if nums[0]["name"] in s and nums[1]["name"] in s]
    data = build(model, "scatter_matrix", xn, yn, rendered, regime="reachable cloud + trajectory",
                 reachable_points=reachable_cloud(model, xn, yn))
    data["pairwise_vars"] = [short(v["name"]) for v in vars_]
    write(out_path, data)


def emit_degenerate(out_path, model):
    """phase_portrait's <2-axis N/A case: still emit data so the test sees the N/A honestly. Uses the
    two most-expressive vars (any kind) as nominal axes; the reachable cloud may be 1-D (yn absent)."""
    from render_common import short
    ax_dicts = ((model.numeric_vars or model.state_vars) + model.state_vars)[:2]
    xd = ax_dicts[0] if ax_dicts else None
    yd = ax_dicts[1] if len(ax_dicts) >= 2 else None
    xn = short(xd["name"]) if xd else None
    yn = short(yd["name"]) if yd else None
    write(out_path, build(model, "phase_portrait", xn, yn, [], regime="degenerate", rendered_na=True,
                          reachable_points=reachable_cloud(model, xn, yn) if xd else []))


def emit_phase(out_path, model, axx, axy, regime, rendered_na):
    """phase_portrait's data emission (moved off the renderer): the from-init dwell the picture draws
    (one seed chain, via m.trajectory) + the relational reachable fan it SHOULD convey."""
    from render_common import short
    xn, yn = short(axx["name"]), short(axy["name"])
    try:
        rendered = [{xn: s[xn], yn: s[yn]} for s in (model.trajectory(steps=200) or [])
                    if xn in s and yn in s]
    except Exception:
        rendered = []
    emit(out_path, model, "phase_portrait", xn, yn, rendered, regime=regime, rendered_na=rendered_na)


def emit(out_path, model, view, xn, yn, rendered_points, regime=None, rendered_na=False):
    """build (with the relational reachable_cloud) + write in one call — keeps each renderer's
    free-function count + render() length down (the data-emission seam lives HERE, not per renderer)."""
    write(out_path, build(model, view, xn, yn, rendered_points, regime=regime,
                          rendered_na=rendered_na,
                          reachable_points=reachable_cloud(model, xn, yn) if xn else []))


def write(out_path, data):
    """Write `<out>.data.json`. Mirrors region_data.write / overlay_points.write_points: NEVER
    raises — a sidecar failure must not fail the render."""
    try:
        with open(out_path + ".data.json", "w") as f:
            json.dump(data, f, indent=2)
    except Exception:
        pass
