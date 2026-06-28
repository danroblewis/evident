"""Golden standard: (random_walk, terminal_map).

EXPERT MATHEMATICS. terminal_map answers "where can the FSM come to REST?" — its terminal/absorbing
set, solved abstractly from the one-step relation. A free 2-D random walk NEVER comes to rest: every
state has 9 successors (it always moves on, and even a stay-put step does not make the state
absorbing because other successors exist), and the walk is unbounded and recurrent-free. So the
correct verdict is DAEMON — no terminal state, the system runs indefinitely; its meaning is in the
ongoing behaviour, not an end state.

EXPERT EXPECTATION: verdict = 'daemon', terminal_count = 0. A 'terminates' verdict, or any nonzero
terminal_count, would be WRONG for a random walk and is the regression this test exposes.
"""
from golden import Check, run_case

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _verdict_daemon(model, data):
    ok = data["verdict"] == "daemon"
    return ok, f"verdict={data['verdict']!r} (a free random walk never rests → expect 'daemon')"


def _no_terminal_states(model, data):
    ok = data["terminal_count"] == 0
    return ok, f"terminal_count={data['terminal_count']} (no absorbing/rest state exists)"


CHECKS = [
    Check("verdict is DAEMON (runs indefinitely, no rest)", _verdict_daemon),
    Check("no terminal states", _no_terminal_states),
]


def case():
    return run_case("random_walk", SOURCE, "terminal_map", CHECKS)
