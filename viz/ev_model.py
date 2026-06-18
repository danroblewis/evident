"""ev_model — adapt a real Evident FSM/transition claim to the phase-portrait engine.

An EvidentModel wraps a .ev file + a transition claim. `axes` is the list of
(current_var, next_var) pairs that form the phase plane — e.g. [("pos","pos_next"),
("v","v_next")] for a physics claim, or [("state.q0","state_next.q0"), …] for a
struct-state FSM. `given` pins any other parameters (inputs, bounds, flags) to fixed
values so the autonomous flow is well-defined. _ev_successors(point) asks the runtime
(via the blocking-clause sampler) for the set of next-states in one tick — the fan.
"""
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "ide", "backend"))
sys.path.insert(0, ROOT)
from sampler import blocking_clause_sample
from runtime.src.runtime import EvidentRuntime


class EvidentModel:
    def __init__(self, evfile, claim, axes, sort="Int", given=None, nondet=False):
        self.name = claim
        self.claim = claim
        self.axes = list(axes)                       # [(cur, nxt), ...]
        self.state = [c for c, _ in self.axes]       # plane axes = current vars
        self.sorts = {c: sort for c, _ in self.axes}
        self.given_fixed = dict(given or {})
        self.nondet = nondet
        self.source = open(os.path.join(ROOT, evfile), encoding="utf-8").read()
        # one loaded runtime, reused per grid point (no re-parse) for the fast path
        self._rt = EvidentRuntime()
        self._rt.load_file(os.path.join(ROOT, evfile))

    def _ev_successors(self, point, kmax=6):
        given = dict(self.given_fixed)
        for cur, _ in self.axes:
            given[cur] = int(round(point[cur]))
        if not self.nondet:
            # deterministic: one query against the already-loaded runtime
            r = self._rt.query(self.claim, given=given)
            if not r.satisfied:
                return []
            try:
                return [{cur: r.bindings[nx] for cur, nx in self.axes}]
            except KeyError:
                return []
        # nondeterministic: enumerate the fan via the blocking-clause sampler
        out, seen = [], set()
        for s in blocking_clause_sample(self.source, self.claim, given, max(kmax, 1)):
            if not s.satisfied:
                continue
            try:
                nxt = {cur: s.bindings[nx] for cur, nx in self.axes}
            except KeyError:
                continue
            key = tuple(nxt[c] for c, _ in self.axes)
            if key not in seen:
                seen.add(key); out.append(nxt)
        return out
