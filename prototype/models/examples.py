"""Example sub-models exercising the composition + unrolling POC.

  sum_to  — one transition (i, acc); tail-recursive accumulator. sum 1..n.
  list_max — a transition (idx, best) that COMPOSES a value sub-model `at`
             (a fixed list as a lookup relation). Iterative max over the list.
"""
import os
import z3
from .core import (Model, Transition, RecModel, BoundedRec, section_md,
                   rec_section_md, bounded_section_md, write_report)


# ── sum_to: pure tail-recursion, one sub-model (the transition) ───────────────
def _sum_step(cur, nxt):
    i, acc = cur["i"], cur["acc"]
    return z3.If(i == 0,
                 z3.And(nxt["i"] == i, nxt["acc"] == acc),          # base: hold
                 z3.And(nxt["i"] == i - 1, nxt["acc"] == acc + i))  # else: accumulate


SumTo = Transition("sum_to", [("i", "Int"), ("acc", "Int")], _sum_step)


# ── sum_to, RECURSIVE: defined using itself (contrast with the transition) ────
# The first parameter `sum_to` IS the model referencing itself (the recursive
# handle). The body literally calls sum_to(n-1, acc+n) — a sum_to defined with
# a sum_to. (You pass the self-reference in because you're defining the very
# thing you're calling — the standard fixpoint pattern.)
def _sum_to(sum_to, n, acc):
    return z3.If(n == 0,
                 acc,                          # base
                 sum_to(n - 1, acc + n))       # recurse: sum_to calls sum_to


# The SAME body, two owners of the recursion:
SumToRec = RecModel("sum_to", [("n", "Int"), ("acc", "Int")], "Int", _sum_to)
#   (A) Z3 owns the unfolding — semi-decidable
SumToBounded = BoundedRec("sum_to", [("n", "Int"), ("acc", "Int")], "Int", _sum_to)
#   (B) the runtime owns the unfolding — bounded, always decidable


# ── list_max: a transition that COMPOSES a value sub-model `at` ────────────────
LIST = [3, 1, 4, 1, 5, 9, 2, 6]


def _at(idx):                              # value sub-model: LIST[idx] as a lookup
    e = z3.IntVal(LIST[-1])
    for j in range(len(LIST) - 2, -1, -1):
        e = z3.If(idx == j, z3.IntVal(LIST[j]), e)
    return e


At = Model("at", [("idx", "Int")], _at)


def _max_step(cur, nxt):
    idx, best = cur["idx"], cur["best"]
    v = At(idx)                            # <-- compose the `at` sub-model
    return z3.If(idx == len(LIST),
                 z3.And(nxt["idx"] == idx, nxt["best"] == best),    # base: hold
                 z3.And(nxt["idx"] == idx + 1,
                        nxt["best"] == z3.If(v > best, v, best)))    # else: max-scan


ListMax = Transition("list_max", [("idx", "Int"), ("best", "Int")],
                     _max_step, uses=("at",))


# ── list_sum: sum a SEQUENCE (sum_to over data, not the counter) ──────────────
# Identical shape to sum_to, but accumulates LIST[idx] instead of the index i —
# so it sums real data. Composes the same `at` lookup as list_max (same list,
# different fold: list_max -> 9, list_sum -> 31).
def _sum_seq_step(cur, nxt):
    idx, acc = cur["idx"], cur["acc"]
    v = At(idx)                            # <-- the sequence element LIST[idx]
    return z3.If(idx == len(LIST),
                 z3.And(nxt["idx"] == idx, nxt["acc"] == acc),       # base: hold
                 z3.And(nxt["idx"] == idx + 1, nxt["acc"] == acc + v))  # accumulate


ListSum = Transition("list_sum", [("idx", "Int"), ("acc", "Int")],
                     _sum_seq_step, uses=("at",))


# ── gcd: Euclid's algorithm — two interacting variables ──────────────────────
def _gcd_step(cur, nxt):
    a, b = cur["a"], cur["b"]
    return z3.If(b == 0,
                 z3.And(nxt["a"] == a, nxt["b"] == b),         # done: gcd is in `a`
                 z3.And(nxt["a"] == b, nxt["b"] == a % b))     # Euclid: (a,b)→(b, a mod b)


Gcd = Transition("gcd", [("a", "Int"), ("b", "Int")], _gcd_step)


# ── running_mean: an ONLINE average (Welford-style incremental update) ────────
def _at_real(idx):
    e = z3.RealVal(LIST[-1])
    for j in range(len(LIST) - 2, -1, -1):
        e = z3.If(idx == j, z3.RealVal(LIST[j]), e)
    return e


def _mean_step(cur, nxt):
    n, avg = cur["n"], cur["avg"]
    v = _at_real(n)
    return z3.If(n == len(LIST),
                 z3.And(nxt["n"] == n, nxt["avg"] == avg),                  # done
                 z3.And(nxt["n"] == n + 1,
                        nxt["avg"] == avg + (v - avg) / (z3.ToReal(n) + 1)))  # update


RunningMean = Transition("running_mean", [("n", "Int"), ("avg", "Real")],
                         _mean_step, uses=("at",))


# ── fibonacci: a transition that NEVER halts — flows outward forever ──────────
def _fib_step(cur, nxt):
    a, b = cur["a"], cur["b"]
    return z3.And(nxt["a"] == b, nxt["b"] == a + b)            # (a,b)→(b, a+b)


Fibonacci = Transition("fibonacci", [("a", "Int"), ("b", "Int")], _fib_step)


# ── token_bucket: a rate-limiter DAEMON (never overspend) ────────────────────
def _token_step(cur, nxt, CAP=5, QMAX=6):
    tok, q = cur["tokens"], cur["pending"]
    return z3.Or(
        z3.And(tok < CAP, nxt["tokens"] == tok + 1, nxt["pending"] == q),   # refill
        z3.And(q < QMAX, nxt["tokens"] == tok, nxt["pending"] == q + 1),    # request in
        z3.And(tok > 0, q > 0,                                              # serve: spend
               nxt["tokens"] == tok - 1, nxt["pending"] == q - 1),
        z3.And(nxt["tokens"] == tok, nxt["pending"] == q))                  # idle


TokenBucket = Transition("token_bucket",
                         [("tokens", "Int"), ("pending", "Int")], _token_step)


def main():
    out = os.path.join(os.path.dirname(__file__), os.pardir, "results")
    os.makedirs(out, exist_ok=True)
    s0 = rec_section_md(SumToRec, calls=[((5, 0), 15), ((3, 0), 6), ((10, 0), 55)])
    sB = bounded_section_md(SumToBounded, arg_vals=(3, 0), depth=5)
    s1, a_one, a_inc = section_md(
        "sum_to — same computation as a transition (tail-call eliminated)",
        SumTo, submodels=[], init={"i": 5, "acc": 0}, fuel=5,
        done=lambda v: v["i"] == 0)
    s2, b_one, b_inc = section_md(
        f"list_max — iterative max over {LIST}",
        ListMax, submodels=[At], init={"idx": 0, "best": -999},
        fuel=len(LIST), done=lambda v: v["idx"] == len(LIST))
    path = os.path.join(out, "models.md")
    write_report(path, "Sub-model composition — prettified Z3-AST report",
                 [s0, sB, s1, s2])
    print(f"sum_to   one-shot/incremental: {a_one} / {a_inc}  (expect 15)")
    print(f"list_max one-shot/incremental: {b_one} / {b_inc}  (expect {max(LIST)})")
    print(f"wrote {os.path.relpath(os.path.abspath(path))}")


if __name__ == "__main__":
    main()
