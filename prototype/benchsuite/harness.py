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


def rlimit_delta(stats):
    """Δ rlimit from the cumulative global counter (mirrors `solve`), or None.

    For solve-only encodings (Fixedpoint/RecFunction) that drive Z3 themselves
    and hand us their own `statistics()` object."""
    global _CUM
    try:
        cum = stats.get_key_value("rlimit count")
        d = int(cum - _CUM) if cum >= _CUM else None
        _CUM = cum
        return d
    except Exception:
        return None
