"""timing_ensemble — the ALL-INITIAL-CONDITIONS timeline ensemble for timing_diagram.

`render_timing_diagram` historically followed ONE forward trajectory from ONE seed
and drew it as digital waveforms — for a deterministic FSM that is a single run, not
the program's behavior. This module roots the timing diagram on the SET of initial
conditions instead: it takes every valid carried-state assignment (the same global
root `Model.full_state_graph` gives the state_graph / basin_map / transition_matrix
views), follows each one FORWARD via the existing successor relation, and returns an
ENSEMBLE of timelines. The renderer then draws, per signal, the reachable ENVELOPE at
each tick (the band of values any initial condition can be in) — so the picture is
all-conditions behavior, not one trajectory.

It does NOT reimplement the transition: it enumerates initial states with
`full_state_graph` and steps each with the SAME `successor()` the from-init walk uses.
Reals / strings / seqs / unbounded ints aren't finitely enumerable, so the ensemble
is unavailable there (`build_ensemble` returns None) and the renderer keeps the honest
single-seed fallback.
"""

# Cap on how many initial-condition timelines we trace, so a wide (but still
# discrete) product doesn't blow the render time. We sample the global state set
# down to this many roots if it is larger; the band is still over the sample.
MAX_TIMELINES = 256


def _forward_trace(m, start, steps):
    """One forward timeline of length steps+1 from `start`, following the successor
    chain. Holds the last value once the chain hits a fixed point / dies, so every
    timeline spans the full time axis and the per-tick band is over the same width."""
    cur = start
    trace = [cur]
    seen = {m._key(cur)}
    for _ in range(steps):
        nxt = m.successor(cur)
        if nxt is None:
            break
        trace.append(nxt)
        k = m._key(nxt)
        cur = nxt
        if k in seen:                  # entered a cycle/fixed point — hold to full width
            break
        seen.add(k)
    while len(trace) < steps + 1:
        trace.append(trace[-1])
    return trace


def build_ensemble(m, steps, limit=5000):
    """The all-initial-conditions ensemble: a list of forward timelines, one per valid
    initial carried assignment (deduped, sampled to MAX_TIMELINES), or None when the
    program is not finitely enumerable (real / string / seq / unbounded int / over-cap
    / two-tick) — in which case the renderer falls back to the single-seed trace.

    Roots on `m.full_state_graph` (the SAME global graph state_graph/basin_map use) so
    the timing diagram shows behavior over ALL starting states, and steps each with the
    EXISTING successor relation, so the dynamics are identical to the from-init walk —
    only the ROOT SET differs (every state vs the single seed).

    The gate is `full_state_graph`'s OWN `info["discrete"]` (every carried var finitely
    enumerable — bool/enum/bounded-int), NOT `m.is_discrete()`: the latter is False for
    a bounded-int signal like a timer/counter, but such a model still has a finite global
    graph and an honest ensemble. Reals / strings / seqs / unbounded ints fail the flag
    and fall back."""
    states, _edges, info = m.full_state_graph(limit=limit)
    if not (info["discrete"] and not info["capped"] and states):
        return None
    # Stable sample if the global set is wider than we want to trace: take an evenly
    # spaced subset (deterministic — full_state_graph's order is a stable product).
    if len(states) > MAX_TIMELINES:
        stride = len(states) / MAX_TIMELINES
        roots = [states[int(i * stride)] for i in range(MAX_TIMELINES)]
    else:
        roots = states
    return [_forward_trace(m, s, steps) for s in roots]


def track_band(track, ensemble, n):
    """For ONE expanded track, the per-tick set of values observed across the whole
    ensemble: returns `bands`, a list of length n where bands[t] is the (ordered) list
    of distinct values the signal takes at tick t over all timelines. A crisp signal
    has len(bands[t]) == 1 at every tick; a divergent one fans out — that fan IS the
    all-conditions envelope the renderer fills."""
    get = track["get"]
    bands = []
    for t in range(n):
        vals = []
        seen = set()
        for trace in ensemble:
            v = get(trace[t])
            hv = v if not isinstance(v, list) else tuple(v)
            if hv not in seen:
                seen.add(hv)
                vals.append(v)
        bands.append(vals)
    return bands
