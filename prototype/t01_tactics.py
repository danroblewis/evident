"""Tactic experiment — can Z3's symbolic simplification do the 'lowering' for us?

Question: the set-of-tuples encoding is ~1000x slower than ite (b01). Is that a
MODELING choice Z3 can't undo, or can a Z3 TACTIC rewrite it into the fast form?
We apply tactics to each encoding's goal and measure (a) does solve get faster,
(b) how does the formula size change. If a tactic closes the gap, the 'lowering'
is a solver-layer symbolic op, not a bespoke compiler pass.

Run: python3 prototype/t01_tactics.py
"""
import time
import z3
from b01_dispatch import b_set, b_ite, make

TACTICS = [
    "none",
    "simplify",
    "propagate-values",
    "solve-eqs",
    "ctx-solver-simplify",
    "PIPE",  # Then(simplify, propagate-values, solve-eqs, simplify)
]


def goal_of(build):
    s = build()
    g = z3.Goal()
    for a in s.assertions():
        g.add(a)
    return g


def sexpr_len(goal_or_expr):
    try:
        return len(goal_or_expr.sexpr())
    except Exception:
        return -1


def apply_tactic(g, name):
    if name == "none":
        return g, 0.0
    t = (z3.Then("simplify", "propagate-values", "solve-eqs", "simplify")
         if name == "PIPE" else z3.Tactic(name))
    t0 = time.perf_counter()
    res = t(g)
    dt = (time.perf_counter() - t0) * 1000
    # collect subgoals into one goal
    out = z3.Goal()
    for i in range(len(res)):
        sub = res[i]
        for j in range(len(sub)):
            out.add(sub[j])
    return out, dt


def solve_ms(goal, timeout_ms=20_000, reps=3):
    walls, res = [], None
    for _ in range(reps):
        s = z3.Solver()
        s.set("timeout", timeout_ms)
        s.add(goal.as_expr())
        t0 = time.perf_counter()
        res = s.check()
        walls.append((time.perf_counter() - t0) * 1000)
    return str(res), min(walls)


def run(label, build, N):
    base = goal_of(build)
    base_size = sexpr_len(base)
    print(f"\n=== {label}  N={N}   (orig formula size {base_size} chars) ===")
    print(f"{'tactic':22} {'simp_ms':>8} {'solve_ms':>9} {'total_ms':>9} "
          f"{'size→':>8} {'result':>8}")
    for name in TACTICS:
        g, simp_ms = apply_tactic(base, name)
        res, sms = solve_ms(g)
        size = sexpr_len(g)
        print(f"{name:22} {simp_ms:8.1f} {sms:9.1f} {simp_ms+sms:9.1f} "
              f"{size:8} {res:>8}")
    return base


if __name__ == "__main__":
    print("z3", z3.get_version_string())
    for N in (200, 1000):
        vals, target = make(N)
        run("set (tuple membership)", lambda v=vals, t=target, N=N: b_set(N, v, t), N)
        run("ite (ternary spine)", lambda v=vals, t=target, N=N: b_ite(N, v, t), N)

    # export set N=200 before/after simplify, for inspection
    vals, target = make(200)
    g = goal_of(lambda: b_set(200, vals, target))
    open("/tmp/set200_before.smt2", "w").write(g.sexpr())
    simp, _ = apply_tactic(g, "PIPE")
    open("/tmp/set200_after.smt2", "w").write(simp.sexpr())
    print("\nexported /tmp/set200_{before,after}.smt2  "
          f"({sexpr_len(g)} → {sexpr_len(simp)} chars)")
