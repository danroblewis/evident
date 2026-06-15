"""pipeline_fixedpoint — a relatively complex transition model distilled into a
fixed-point model by Spacer (Z3's Fixedpoint engine).

The transition: a 3-stage pipeline of bounded queues (q0, q1, q2; each depth <=
CAP). Five operations:
  arrive  q0 += 1          (only while q0 < CAP)
  q0→q1   q0 -= 1, q1 += 1 (only while q0 > 0 and q1 < CAP)
  q1→q2   q1 -= 1, q2 += 1 (only while q1 > 0 and q2 < CAP)
  depart  q2 -= 1          (only while q2 > 0)
  idle    no change

That is the OPERATIONAL model — you run it step by step. Spacer takes it and the
property "no stage is ever out of bounds" and produces the FIXED-POINT model: an
inductive invariant characterizing every reachable state in closed form
(`0 <= q0,q1,q2 <= CAP`), proven for all time without unrolling. That invariant
IS the reachability fixed point of the transition — the "optimization" is turning
unbounded step-by-step execution into one closed-form description of all reachable
states.

(Tool note: this is the REACHABILITY fixed point, which Spacer extracts. The
*stable-state* fixed point `s = step(s)` from the recursion taxonomy is a
different thing we have no automated tool for. And Spacer is semi-decidable: it
nails difference-bounded invariants like these in ms, but diverges on
sum/conservation invariants — e.g. a 3-account ledger proving a+b+c==TOTAL hangs.)

Run:  python3 pipeline_fixedpoint.py        (from the prototype/ directory)
"""
import time
import z3

CAP = 5


def transition(q, q2):
    """One pipeline operation. q, q2 are 3-tuples of queue depths."""
    def upd(deltas):
        return z3.And(*[q2[k] == q[k] + deltas.get(k, 0) for k in range(3)])
    return z3.Or(
        z3.And(q[0] < CAP,           upd({0: 1})),          # arrive
        z3.And(q[0] > 0, q[1] < CAP, upd({0: -1, 1: 1})),   # q0 -> q1
        z3.And(q[1] > 0, q[2] < CAP, upd({1: -1, 2: 1})),   # q1 -> q2
        z3.And(q[2] > 0,             upd({2: -1})),          # depart
        upd({}),                                             # idle
    )


# ── execution: push some items through the pipeline (it does something) ──────
def run(ops):
    q, trace = [0, 0, 0], [(0, 0, 0)]
    for op in ops:
        if op == "arrive" and q[0] < CAP:
            q[0] += 1
        elif op == "01" and q[0] > 0 and q[1] < CAP:
            q[0] -= 1; q[1] += 1
        elif op == "12" and q[1] > 0 and q[2] < CAP:
            q[1] -= 1; q[2] += 1
        elif op == "depart" and q[2] > 0:
            q[2] -= 1
        trace.append(tuple(q))
    return trace


# ── verification: Spacer extracts the reachability fixed point ───────────────
def extract_fixed_point():
    fp = z3.Fixedpoint()
    fp.set(engine="spacer")
    Inv = z3.Function("Inv", z3.IntSort(), z3.IntSort(), z3.IntSort(), z3.BoolSort())
    fp.register_relation(Inv)
    q = list(z3.Ints("q0 q1 q2"))
    q2 = list(z3.Ints("q0_ q1_ q2_"))
    for v in q + q2:
        fp.declare_var(v)
    fp.rule(Inv(z3.IntVal(0), z3.IntVal(0), z3.IntVal(0)))          # init: all empty
    fp.rule(Inv(q2[0], q2[1], q2[2]), [Inv(*q), transition(q, q2)])
    bad = z3.Or(*[z3.Or(qi < 0, qi > CAP) for qi in q])             # any stage OOB
    t0 = time.perf_counter()
    res = fp.query(z3.And(Inv(*q), bad))
    ms = (time.perf_counter() - t0) * 1000
    return res, (fp.get_answer() if res == z3.unsat else None), ms


if __name__ == "__main__":
    print("── the transition model RUN (items flowing through the pipeline) ──")
    ops = ["arrive", "arrive", "01", "arrive", "01", "12", "arrive", "01",
           "12", "depart", "12", "depart"]
    for st in run(ops):
        pass
    print("  (q0,q1,q2):", " → ".join(str(s) for s in run(ops)))

    print("\n── Spacer distills it into the FIXED-POINT MODEL ──")
    res, inv, ms = extract_fixed_point()
    print(f"  any stage out of bounds reachable?  {res}   "
          f"({ms:.0f} ms; unsat = proven safe forever)")
    if inv is not None:
        print("  synthesized inductive invariant (the reachability fixed point):")
        print("   ", inv)
        print("    i.e. 0 ≤ q0, q1, q2 ≤ %d — every reachable state, in closed form" % CAP)
