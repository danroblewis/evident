"""time_series_ensemble — initial-condition ENSEMBLE for the time-series renderer.

render_time_series used to follow ONE chain from `initial_state()` — for a deterministic
FSM that shows only the basin the seed falls into (a bistable seeded at x=1 looks like it
ALWAYS decays to 0, hiding the second attractor at 6). This module produces the SET of
initial conditions the all-conditions view demands, then forward-simulates each with the
model's EXISTING successor relation — never reimplementing the transition:

  * DISCRETE / bounded → the inits are `full_state_graph()`'s enumerated state set (every
    valid carried assignment, ignoring is_first_tick), sampled down if huge.
  * CONTINUOUS / Real → a modest product grid over each carried var's PROVEN bounded range
    (`Model.proven_range`, the same z3-Optimize box model_global enumerates ints from).

If ANY carried var has no finite range (unbounded int / unbounded real), there is no honest
ensemble box to seed, so `ensemble_inits` returns `(None, reason)` and the caller falls back
to the single-run trajectory with an honest note.

The forward walk CLAMPS divergence: a chaotic / continuous model can overflow to 1e300 and
raise OverflowError on the next z3 literal. `step_trajectory` stops a chain the moment a value
leaves a sane magnitude, so the envelope never crashes on a blow-up.
"""
import itertools

# A trajectory is cut the moment any numeric value exceeds this magnitude — past here the
# dynamics have diverged and the next z3 literal would risk an OverflowError. The envelope
# is built from the in-bounds prefix, honestly flagged as truncated.
_DIVERGE = 1e14

# Cap on how many initial conditions we forward-simulate. A 7-state bistable uses 7; a wide
# discrete product or a fine continuous grid is sampled down to this many (deterministically).
_MAX_INITS = 64

# Per-axis grid resolution for a continuous (Real) carried var. The product across axes is
# then sampled down to _MAX_INITS, so this stays modest.
_GRID_PER_AXIS = 6


def _sample_down(items, cap):
    """At most `cap` items, evenly strided across `items` (deterministic — first, last, and a
    regular spread between), so a huge init set still spans its extent rather than clustering."""
    n = len(items)
    if n <= cap:
        return list(items)
    step = n / cap
    return [items[int(i * step)] for i in range(cap)]


def _continuous_grid(m):
    """Product grid of initial-condition dicts over the carried vars' PROVEN ranges, or
    (None, reason) if any carried var is unbounded (no honest box to grid). Bool/enum carried
    vars (a mixed model) take their full small domain; int/real take a linspace over
    proven_range. The product is sampled down to _MAX_INITS by the caller."""
    axes = []                                  # [(name, [values])] in carried order
    for v in m.carried:
        kind = v["kind"]
        if kind == "bool":
            axes.append((v["name"], [False, True]))
        elif kind == "enum":
            axes.append((v["name"], list(m.enum_variants.get(v["name"], []))))
        elif kind in ("int", "real"):
            rng = m.proven_range(v)
            if rng is None:
                return None, f"{v['name']} is unbounded (no finite range to seed)"
            lo, hi = rng
            if kind == "int":
                span = int(hi) - int(lo) + 1
                n = min(_GRID_PER_AXIS, span)
                if span <= _GRID_PER_AXIS:
                    vals = list(range(int(lo), int(hi) + 1))
                else:
                    vals = [int(round(lo + (hi - lo) * i / (n - 1))) for i in range(n)]
                axes.append((v["name"], sorted(set(vals))))
            else:
                if hi <= lo:
                    vals = [float(lo)]
                else:
                    n = _GRID_PER_AXIS
                    vals = [lo + (hi - lo) * i / (n - 1) for i in range(n)]
                axes.append((v["name"], vals))
        else:                                  # string / seq carried var: not griddable
            return None, f"{v['name']} ({kind}) has no continuous grid"
    if not axes:
        return None, "no carried vars to grid"
    names = [n for n, _ in axes]
    inits = [dict(zip(names, combo))
             for combo in itertools.product(*[vals for _, vals in axes])]
    return inits, None


def ensemble_inits(m):
    """The SET of initial-condition state dicts for the ensemble, plus a `(kind, note)` tag:

      ("discrete", note)   — inits from full_state_graph() (every valid carried assignment).
      ("continuous", note) — inits from a proven-bounds product grid over the carried vars.
      (None, reason)       — no honest ensemble (some carried var unbounded); caller falls
                             back to the single from-init run and surfaces `reason`.

    Returns (inits, kind, note). `inits` is a list of partial state dicts (carried leaves only)
    suitable to seed `step_trajectory` — they pin `_prev` via the existing successor()."""
    states, _edges, info = m.full_state_graph(limit=5000)
    if info["discrete"] and states and not info["capped"]:
        inits = _sample_down(states, _MAX_INITS)
        note = (f"ensemble over all {len(states)} initial conditions"
                if len(inits) == len(states)
                else f"ensemble over {len(inits)} of {len(states)} initial conditions (sampled)")
        return inits, "discrete", note
    if info["capped"] and states:
        inits = _sample_down(states, _MAX_INITS)
        return inits, "discrete", (f"ensemble over {len(inits)} sampled initial conditions "
                                   f"(discrete product capped)")
    # Not finitely enumerable (real / unbounded) → try a continuous proven-bounds grid.
    grid, reason = _continuous_grid(m)
    if grid is None:
        return None, None, reason
    inits = _sample_down(grid, _MAX_INITS)
    note = (f"ensemble over a {len(grid)}-point grid of initial conditions"
            if len(inits) == len(grid)
            else f"ensemble over {len(inits)} of {len(grid)} grid initial conditions (sampled)")
    return inits, "continuous", note


def _diverged(state):
    """True if any numeric leaf has blown past _DIVERGE — the chain has diverged and the next
    z3 literal would risk an OverflowError. Guards the chaotic / continuous blow-up case."""
    for val in state.values():
        if isinstance(val, (int, float)) and not isinstance(val, bool):
            if abs(val) > _DIVERGE or val != val:    # magnitude or NaN
                return True
    return False


def step_trajectory(m, init, steps, prefer_change):
    """Forward-simulate ONE trajectory from `init` using the model's EXISTING successor
    relation — the same `_advance` discipline render_time_series.walk uses for a single run.
    Stops at a fixed point / revisit / divergence (clamped, never crashing). Returns the list
    of state dicts (length ≤ steps+1). `init` may be a partial carried-leaf dict; the first
    successor() pins it as `_prev` and z3 fills the rest."""
    from time_series_walk import _advance                # reuse the single-run step exactly
    cur = init
    path = [cur]
    seen = {m._key(cur)}
    for _ in range(steps):
        if _diverged(cur):
            break                                        # diverged — stop before the next z3 literal
        try:
            nxt = _advance(m, cur, prefer_change, seen)
        except (OverflowError, ValueError):
            break                                        # a literal blew up — truncate honestly
        if nxt is None:
            break
        path.append(nxt)
        k = m._key(nxt)
        if k in seen:                                    # fixed point / revisit → stop
            break
        seen.add(k)
        cur = nxt
    return path
