"""recfun_calibrate2 — second calibration pass. The first showed all FORWARD
shapes are linear/cheap (depth alone never costs) and only BACKWARD synthesis
blows up. This pass:
  (1) pushes forward linear recursion to huge depth and splits BUILD vs SOLVE
      time — to learn whether deep forward recursion can reach 10 s at all, or
      whether Python AST construction dominates first;
  (2) ladders the SEARCH shapes that actually cost — backward branching (find n
      with fib(n)=target), and a WIDE model (M independent recfun instances that
      must agree) — to find clean ~10 s base cases.

Run from prototype/:  python3 recfun_calibrate2.py
"""
import time
import z3

SOLVE_TO = 40_000

_uid = [0]
def uid():
    _uid[0] += 1
    return _uid[0]


def measure(build):
    t0 = time.perf_counter()
    g = build()
    build_ms = (time.perf_counter() - t0) * 1000
    s = z3.Solver(); s.set("timeout", SOLVE_TO)
    s.add(g.as_expr())
    t0 = time.perf_counter()
    r = s.check()
    solve_ms = (time.perf_counter() - t0) * 1000
    return str(r), build_ms, solve_ms


def sum_fwd(K):
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"sum{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
        r = z3.Int(f"r{u}")
        g = z3.Goal(); g.add(f(z3.IntVal(K)) == r); return g
    return build


def fib_bwd(T):
    """find n>=0 with fib(n) == T (branching recursion, run backward)."""
    def build():
        u = uid(); n = z3.Int(f"n{u}")
        f = z3.RecFunction(f"fibb{u}", z3.IntSort(), z3.IntSort())
        z3.RecAddDefinition(f, [n], z3.If(n < 2, n, f(n - 1) + f(n - 2)))
        x = z3.Int(f"x{u}")
        g = z3.Goal(); g.add(f(x) == T, x >= 0, x <= 60); return g
    return build


def wide_sum(M, K):
    """M independent sum_to recfun symbols, each pinned to sum_to(K). Tests how
    Z3 scales with the NUMBER of distinct recursive functions in one model."""
    def build():
        g = z3.Goal()
        target = K * (K + 1) // 2
        for _ in range(M):
            u = uid(); n = z3.Int(f"n{u}")
            f = z3.RecFunction(f"sw{u}", z3.IntSort(), z3.IntSort())
            z3.RecAddDefinition(f, [n], z3.If(n <= 0, 0, n + f(n - 1)))
            x = z3.Int(f"x{u}")
            g.add(f(x) == target, x >= 0, x <= K + 5)   # each solved backward
        return g
    return build


def main():
    print(f"calibrate2 (solve cap {SOLVE_TO} ms); BUILD vs SOLVE split\n")

    print("sum_fwd — deep forward (does depth ever reach 10 s? or build dominates?)")
    for K in [100_000, 300_000, 1_000_000, 3_000_000]:
        res, b, s = measure(sum_fwd(K))
        print(f"  K={K:>9}  {res:<7} build {b:8.1f} ms  solve {s:9.1f} ms", flush=True)
        if res == "unknown" or s >= 10_000 or b >= 30_000:
            break
    print()

    print("fib_bwd — branching recursion run BACKWARD (find n with fib(n)=T)")
    # fib: 0 1 1 2 3 5 8 13 21 34 55 89 144 233 377 610 987 1597 2584 4181 6765 ...
    for T in [55, 610, 6765, 75025, 832040, 9227465]:
        res, b, s = measure(fib_bwd(T))
        flag = "  <-- >=10s" if s >= 10_000 else ""
        print(f"  T={T:>9}  {res:<7} build {b:6.1f} ms  solve {s:9.1f} ms{flag}", flush=True)
        if res == "unknown" or s >= 10_000:
            break
    print()

    print("wide_sum — M independent recfun symbols, each solved backward (K=40)")
    for M in [1, 5, 10, 20, 40, 80]:
        res, b, s = measure(wide_sum(M, 40))
        flag = "  <-- >=10s" if s >= 10_000 else ""
        print(f"  M={M:>4}  {res:<7} build {b:6.1f} ms  solve {s:9.1f} ms{flag}", flush=True)
        if res == "unknown" or s >= 10_000:
            break
    print()


if __name__ == "__main__":
    main()
