"""ev_model — adapt a real Evident FSM to the phase-portrait engine.

An EvidentModel wraps a .ev file + an FSM claim (state/state_next over numeric
fields). Its _ev_successors(point) asks the runtime, via the blocking-clause
sampler, for the set of next-states reachable in one tick from a concrete state —
the transition fan. The engine then draws the flow, fixed points, and box.
"""
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "ide", "backend"))
from sampler import blocking_clause_sample


class EvidentModel:
    def __init__(self, evfile, claim, state, sort="Int"):
        self.name = claim
        self.state = list(state)
        self.sorts = {v: sort for v in self.state}
        self.claim = claim
        self.source = open(os.path.join(ROOT, evfile), encoding="utf-8").read()

    def _ev_successors(self, point, kmax=6):
        given = {f"state.{k}": int(round(point[k])) for k in self.state}
        out, seen = [], set()
        for s in blocking_clause_sample(self.source, self.claim, given, max(kmax, 1)):
            if not s.satisfied:
                continue
            nxt = {k: s.bindings[f"state_next.{k}"] for k in self.state}
            key = tuple(nxt[k] for k in self.state)
            if key not in seen:
                seen.add(key); out.append(nxt)
        return out
