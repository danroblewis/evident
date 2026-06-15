"""verified_daemon — wrap a constraint system in a constraint system: one
transition, RUN and PROVEN.

The idea (yours): the per-step iteration is one constraint system; the daemon's
correctness is another constraint system *over* it. They share the SAME
transition relation:

  • Layer 1 — EXECUTION: the transition run on the incremental loop
    (`run_persistent`). One reused constraint, constant memory — the daemon
    actually ticking, and the runtime provably won't grow (held assertions == 1).
  • Layer 2 — VERIFICATION: the same transition handed to Spacer (Z3's Fixedpoint
    engine) with a safety property. Spacer proves the property for ALL reachable
    states over unbounded time, by synthesizing an inductive invariant — without
    running the daemon. This is "prove it won't crash / won't overrun."

The daemon here is a bounded request queue of depth `q` (cap CAP). Each tick it
does exactly one of enqueue (only when not full) or dequeue (only when non-empty),
so `q` moves every tick. Safety we prove: 0 <= q <= CAP, always.

Run:  python3 verified_daemon.py        (from the prototype/ directory)
"""
import z3
from models.core import Transition, run_persistent

CAP = 3


def _step(cur, nxt):
    """The shared transition: q' = q + enq - deq, exactly one move per tick,
    guarded so q stays in range by construction-intent (which Layer 2 *proves*)."""
    q, q2 = cur["q"], nxt["q"]
    enq, deq = z3.Int("enq"), z3.Int("deq")
    return z3.And(z3.Or(enq == 0, enq == 1), z3.Or(deq == 0, deq == 1),
                  enq + deq == 1,                    # exactly one move each tick
                  z3.Implies(enq == 1, q < CAP),     # guard: no enqueue when full
                  z3.Implies(deq == 1, q > 0),       # guard: no dequeue when empty
                  q2 == q + enq - deq)


Daemon = Transition("daemon", [("q", "Int")], _step)


def execute():
    print("LAYER 1 — EXECUTION (incremental loop, constant memory)")
    final, trace, held = run_persistent(Daemon, {"q": 0}, 10)
    print("  trace:", " → ".join(f"{t['q']}" for t in trace))
    print(f"  held assertions across ALL ticks: {held}  (constant — runtime won't grow)")


def verify():
    print("\nLAYER 2 — VERIFICATION (Spacer proves safety for ALL reachable states)")
    fp = z3.Fixedpoint()
    fp.set(engine="spacer")
    Inv = z3.Function("Inv", z3.IntSort(), z3.BoolSort())
    fp.register_relation(Inv)
    q, q2, enq, deq = z3.Ints("q q2 enq deq")
    fp.declare_var(q, q2, enq, deq)
    fp.rule(Inv(z3.IntVal(0)))                        # init: q starts at 0
    fp.rule(Inv(q2), [Inv(q),                         # the SAME transition, as a rule
                      z3.Or(enq == 0, enq == 1), z3.Or(deq == 0, deq == 1),
                      enq + deq == 1,
                      z3.Implies(enq == 1, q < CAP), z3.Implies(deq == 1, q > 0),
                      q2 == q + enq - deq])
    res = fp.query(z3.And(Inv(q), z3.Or(q < 0, q > CAP)))   # any bad state reachable?
    print(f"  bad state (q < 0 or q > {CAP}) reachable?  {res}   (unsat = PROVEN SAFE)")
    if res == z3.unsat:
        print("  synthesized inductive invariant:", fp.get_answer())


if __name__ == "__main__":
    execute()
    verify()
