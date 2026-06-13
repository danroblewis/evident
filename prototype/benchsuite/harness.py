"""Timing harness: wall-clock floor + Z3's deterministic work counter."""
import time
import z3

# Z3's 'rlimit count' is CUMULATIVE over the (global) context; work per build is
# deterministic, so per-solve rlimit = Δcumulative ÷ reps. Track the running total.
_CUM = 0.0


def solve(goal, reps, timeout_ms):
    """Solve a Goal `reps` times (fresh Solver each); return result + metrics."""
    global _CUM
    walls, result, stats = [], None, None
    expr = goal.as_expr()
    for _ in range(reps):
        s = z3.Solver()
        s.set("timeout", timeout_ms)
        s.add(expr)
        t0 = time.perf_counter()
        result = s.check()
        walls.append((time.perf_counter() - t0) * 1000.0)
        stats = s.statistics()

    rlimit = None
    try:
        cum = stats.get_key_value("rlimit count")
        rlimit = int((cum - _CUM) / reps) if cum >= _CUM else None
        _CUM = cum
    except Exception:
        pass

    return {"result": str(result), "min_ms": round(min(walls), 2), "rlimit": rlimit}
