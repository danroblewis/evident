"""Example sub-models exercising the composition + unrolling POC.

  sum_to  — one transition (i, acc); tail-recursive accumulator. sum 1..n.
  list_max — a transition (idx, best) that COMPOSES a value sub-model `at`
             (a fixed list as a lookup relation). Iterative max over the list.
"""
import os
import z3
from .core import (Model, Transition, RecModel, section_md, rec_section_md,
                   write_report)


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


SumToRec = RecModel("sum_to", [("n", "Int"), ("acc", "Int")], "Int", _sum_to)


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


def main():
    out = os.path.join(os.path.dirname(__file__), os.pardir, "results")
    os.makedirs(out, exist_ok=True)
    s0 = rec_section_md(SumToRec, calls=[((5, 0), 15), ((3, 0), 6), ((10, 0), 55)])
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
                 [s0, s1, s2])
    print(f"sum_to   one-shot/incremental: {a_one} / {a_inc}  (expect 15)")
    print(f"list_max one-shot/incremental: {b_one} / {b_inc}  (expect {max(LIST)})")
    print(f"wrote {os.path.relpath(os.path.abspath(path))}")


if __name__ == "__main__":
    main()
