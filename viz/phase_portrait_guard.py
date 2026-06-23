#!/usr/bin/env python3
"""phase_portrait_guard.py — divergence-guarded reads of the model's transition, for the
phase-portrait vector field.

A phase portrait grids the successor relation over a whole plane of points and follows
trajectories from many seeds. For a chaotic / continuous system (predator-prey, the
double pendulum, an under-damped spring) some of those points DIVERGE: the successor's
next z3 literal becomes a 1000+-digit integer whose `as_long()` raises (an OverflowError
on the int path, a ValueError on the digit-limit path). A single runaway grid cell would
otherwise crash the entire render.

This module wraps `m.successor()` / the successor-chain walk so a diverging point is SKIPPED
(grid) or TRUNCATES the chain (trajectory) instead of crashing — reusing the EXISTING
transition relation, never reimplementing it. It only clamps the READ.

`_DIVERGE` mirrors `time_series_ensemble._DIVERGE` so the two all-conditions views share a
divergence scale.
"""

# A successor whose any numeric leaf passes this magnitude has diverged; the next z3 literal
# risks the OverflowError / ValueError blow-up the diagram review flagged.
_DIVERGE = 1e14


def diverged(state):
    """True if any numeric leaf of `state` has blown past _DIVERGE (magnitude or NaN)
    — the chain has diverged and probing its successor risks an OverflowError."""
    for val in state.values():
        if isinstance(val, (int, float)) and not isinstance(val, bool):
            if abs(val) > _DIVERGE or val != val:    # magnitude or NaN
                return True
    return False


def safe_successor(m, state):
    """`m.successor(state)` GUARDED for the vector field: returns the next state, or None
    if this point has already diverged OR its successor literal blows up (OverflowError /
    ValueError from a giant z3 numeral). Reuses the EXISTING transition — never reimplements
    it — only clamps the read so one runaway grid cell never crashes the whole field."""
    if diverged(state):
        return None
    try:
        return m.successor(state)
    except (OverflowError, ValueError):
        return None


def safe_trajectory(m, start, steps):
    """Follow the successor chain from `start` like `m.trajectory`, but STOP at divergence
    instead of crashing on the next giant literal. Same successor relation, guarded read —
    so a runaway seed truncates honestly rather than raising. Returns the state list."""
    cur = start
    path = [cur]
    seen = {m._key(cur)}
    for _ in range(steps):
        nxt = safe_successor(m, cur)
        if nxt is None:
            break
        path.append(nxt)
        k = m._key(nxt)
        if k in seen:                                # fixed point / revisit → stop
            break
        seen.add(k)
        cur = nxt
    return path
