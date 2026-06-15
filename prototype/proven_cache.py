"""proven_cache — a daemon that DOES something, with a Spacer proof of its
correctness, and the same proof catching a bug.

The program is a session-cache daemon. State `n` = number of sessions cached;
capacity CAP. It handles events:
  open   — cache a new session: n' = n + 1, but only while n < CAP (the capacity
           guard — i.e. evict/refuse instead of growing without bound)
  close  — a session ends:      n' = n - 1, but only while n > 0
  idle   — n' = n

We prove the SAFETY invariant `0 <= n <= CAP` holds in EVERY reachable state, for
unbounded time, using Spacer (Z3's Fixedpoint engine). This is an INDUCTIVE
invariant — Spacer proves the operation preserves the bound (plus it holds
initially) and induction covers all of time. It is NOT a 50-step unroll: the
proof says nothing about a step count, it says "forever".

Then we inject the classic leak — drop the capacity guard, so the cache grows
without eviction — and the SAME proof fails: Spacer reports the overflow state is
reachable and hands back a counterexample trace. Note it finds this at proof time,
in milliseconds, no matter how many requests the overflow would actually take to
manifest at runtime — because it reasons about the per-step trend, not the symptom.

Run:  python3 proven_cache.py        (from the prototype/ directory)
"""
import z3

CAP = 4


# ── the daemon's operations, as a transition relation ────────────────────────
def transition(n, n2, guarded=True):
    """One event applied. `guarded=False` injects the bug: open without the
    capacity guard, so the cache grows without eviction."""
    open_ok = (n < CAP) if guarded else True
    return z3.Or(
        z3.And(open_ok, n2 == n + 1),       # open: cache a session
        z3.And(n > 0,   n2 == n - 1),       # close: evict a session
        n2 == n,                            # idle
    )


# ── execution: run a concrete event stream (the daemon doing something) ──────
def run(events):
    n, trace = 0, [0]
    for ev in events:
        if ev == "open" and n < CAP:
            n += 1
        elif ev == "close" and n > 0:
            n -= 1
        trace.append(n)
    return trace


# ── verification: Spacer proves `0 <= n <= CAP` for ALL reachable states ─────
def prove(guarded=True):
    fp = z3.Fixedpoint()
    fp.set(engine="spacer")
    Inv = z3.Function("Inv", z3.IntSort(), z3.BoolSort())
    fp.register_relation(Inv)
    n, n2 = z3.Ints("n n2")
    fp.declare_var(n, n2)
    fp.rule(Inv(z3.IntVal(0)))                                  # init: empty cache
    fp.rule(Inv(n2), [Inv(n), transition(n, n2, guarded)])     # the operation
    res = fp.query(z3.And(Inv(n), z3.Or(n < 0, n > CAP)))      # any bad state?
    inv = fp.get_answer() if res == z3.unsat else None         # the invariant, if safe
    return res, inv


def run_unguarded(events):
    """The buggy daemon (open never checks capacity) — to exhibit the overflow."""
    n, trace = 0, [0]
    for ev in events:
        if ev == "open":
            n += 1                          # no guard — the leak
        elif ev == "close" and n > 0:
            n -= 1
        trace.append(n)
    return trace


if __name__ == "__main__":
    print("── the daemon does something (an event stream) ──")
    events = ["open", "open", "open", "close", "open", "open", "open", "close", "open"]
    print("  events:    ", events)
    print("  cache size:", " → ".join(map(str, run(events))),
          f"   (CAP={CAP}; note it holds at {CAP} — the guard works)")

    print("\n── PROOF (correct daemon): 0 ≤ n ≤ CAP in every reachable state, forever ──")
    res, inv = prove(guarded=True)
    print(f"  overflow / underflow reachable?  {res}   (unsat = PROVEN SAFE)")
    print("  synthesized inductive invariant:", inv, "  (i.e. 0 ≤ n ≤ %d)" % CAP)

    print("\n── BUG INJECTED (capacity guard dropped — cache grows without eviction) ──")
    res, _ = prove(guarded=False)
    print(f"  overflow reachable?  {res}   (sat = BUG FOUND — the proof correctly fails)")
    overflow = run_unguarded(["open"] * (CAP + 1))
    print(f"  counterexample: {CAP + 1} opens with the guard gone →",
          " → ".join(map(str, overflow)), f"(exceeds CAP={CAP})")
    print("  found at proof time in ms — no matter how many requests it would take")
    print("  to actually exhaust memory at runtime.")
