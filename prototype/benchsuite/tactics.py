"""Tactics: a registry (incl. parameterized tactics) + the sequence generator."""
import itertools
from dataclasses import dataclass, field
import time
import z3


@dataclass(frozen=True)
class Tactic:
    name: str
    params: tuple = ()           # ((key, value), …) — kept hashable for combos

    def make(self):
        t = z3.Tactic(self.name)
        return z3.With(t, **dict(self.params)) if self.params else t

    def __str__(self):
        ps = "".join(f"[{k}={v}]" for k, v in self.params)
        return self.name + ps


# The catalogue. `simplify+blast_select_store` is included because it is the one
# that lowers set-membership store-chains to ite (see FINDINGS.md, ~700×).
TACTICS = [
    Tactic("simplify"),
    Tactic("propagate-values"),
    Tactic("solve-eqs"),
    Tactic("elim-term-ite"),
    Tactic("propagate-ineqs"),
    Tactic("ctx-simplify"),
    Tactic("simplify", (("blast_select_store", True),)),
]


def sequences(max_len):
    """The empty baseline, then every ordered sequence (with repetition) of length
    1..max_len. max_len == len(TACTICS) is the full 'until we run out' sweep."""
    yield ()
    for k in range(1, max_len + 1):
        yield from itertools.product(TACTICS, repeat=k)


def seq_str(seq):
    return ">".join(str(t) for t in seq) if seq else "(none)"


def apply(goal, seq):
    """Apply a tactic sequence to a goal. Returns (goal', ms, error_or_None)."""
    if not seq:
        return goal, 0.0, None
    ts = [t.make() for t in seq]
    pipe = ts[0] if len(ts) == 1 else z3.Then(*ts)
    t0 = time.perf_counter()
    try:
        res = pipe(goal)
    except z3.Z3Exception:
        return None, (time.perf_counter() - t0) * 1000, "tactic_error"
    ms = (time.perf_counter() - t0) * 1000
    out = z3.Goal()
    for i in range(len(res)):
        sub = res[i]
        for j in range(len(sub)):
            out.add(sub[j])
    return out, ms, None


def count(max_len):
    return 1 + sum(len(TACTICS) ** k for k in range(1, max_len + 1))
