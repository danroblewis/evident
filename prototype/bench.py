"""Tiny Z3 benchmark harness.

Two metrics per case:
  - rlimit : Z3's DETERMINISTIC work counter (machine-independent, noise-free) —
             the right number for comparing encodings.
  - min_ms : wall-clock floor over `reps` runs (min cuts contention noise).

A `build()` returns a *fresh* Solver with all constraints added; we rebuild per
rep because check() caches on a solver.
"""
import time
import statistics as _stat
import z3

# Z3's 'rlimit count' in statistics() is CUMULATIVE over the (global) context, not
# per-solve. Work is deterministic per build, so per-solve rlimit = (Δcumulative /
# reps). We track the running cumulative here.
_CUM = 0.0


def bench(build, reps=3, timeout_ms=120_000):
    global _CUM
    walls, res, st = [], None, None
    for _ in range(reps):
        s = build()
        s.set("timeout", timeout_ms)
        t = time.perf_counter()
        res = s.check()
        walls.append((time.perf_counter() - t) * 1000.0)
        st = s.statistics()

    cum = None
    try:
        cum = st.get_key_value("rlimit count")
    except Exception:
        pass
    rl = None
    if cum is not None:
        rl = int((cum - _CUM) / reps) if cum >= _CUM else None
        _CUM = cum

    return {
        "result": str(res),
        "min_ms": min(walls),
        "med_ms": _stat.median(walls),
        "rlimit": rl,   # per-solve deterministic work
    }


def table(rows, cols=("label", "N", "result", "rlimit", "min_ms")):
    """rows: list of dicts. Prints an aligned table; ms to 1dp."""
    def fmt(v):
        return f"{v:.1f}" if isinstance(v, float) else ("" if v is None else str(v))
    widths = {c: max(len(c), *(len(fmt(r.get(c))) for r in rows)) for c in cols}
    line = "  ".join(c.ljust(widths[c]) for c in cols)
    print(line)
    print("  ".join("-" * widths[c] for c in cols))
    for r in rows:
        print("  ".join(fmt(r.get(c)).ljust(widths[c]) for c in cols))
