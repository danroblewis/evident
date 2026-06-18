"""fsm_trace — run an Evident FSM forward headless and capture its state trajectory.

Replicates the executor's run loop (advance_state → state_given → query) but feeds
synthetic input instead of a real plugin, so Seq-state programs (balls, the movement
games) evolve exactly as they do live. capture(bindings) pulls the values to plot.
"""
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
from runtime.src.executor import EvidentExecutor


def trace(evfile, steps=80, input_given=None, capture=None):
    ex = EvidentExecutor()
    ex.load(os.path.join(ROOT, evfile))
    declared = ex._collect_vars("main", set())
    state_pairs = ex._detect_state_pairs(declared)
    current = {b: ex._initial_state(t) for b, (_, t) in state_pairs.items()}
    rows = []
    for step in range(steps):
        ig = input_given(step) if callable(input_given) else input_given
        given = dict(ig or {})
        for base, st in current.items():
            given.update(ex._state_given(base, st))
        r = ex.rt.query("main", given=given, cached=True)
        if not r.satisfied:
            break
        if capture:
            rows.append(capture(r.bindings))
        new = ex._advance_state(r.bindings, state_pairs)
        for base in current:
            if base in new and new[base]:
                current[base] = new[base]
    return rows


if __name__ == "__main__":   # smoke test: do the 4 balls evolve smoothly?
    def cap(b):
        return [(b[f"state.balls.{i}"]["pos_y"], b[f"state.balls.{i}"]["vy"])
                for i in range(4)]
    rows = trace("programs/balls_demo/balls.ev", steps=8,
                 input_given={"input.dt": 16}, capture=cap)
    for k, row in enumerate(rows):
        print(f"step {k}: ball0 (pos_y,vy)={row[0]}")
