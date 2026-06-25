"""z3_budget.py — bounded Z3 solving for the claim/structure analysis.

Two safety helpers shared by the claim analyzers, so a nonlinear or otherwise undecidable claim can
never hang the analysis (and the server):

  * _tsolver()        — a Solver with the shared per-solve timeout (SOLVE_TIMEOUT_MS); a timed-out
                        check returns `unknown` instead of spinning forever.
  * _nonlinear(exprs) — detects NIA (`area = h * w`): a MUL of two non-constant terms. Z3's nonlinear
                        integer arithmetic is undecidable and routinely hangs, and the affine-relation
                        analysis can't surface relations in a product, so callers skip it.
"""
import z3

from model_const import SOLVE_TIMEOUT_MS


def _tsolver():
    """A Z3 solver with the shared per-solve timeout (SOLVE_TIMEOUT_MS) — a timed-out check returns
    `unknown` in bounded time instead of hanging the analysis (and the server)."""
    s = z3.Solver()
    s.set("timeout", SOLVE_TIMEOUT_MS)
    return s


def _nonlinear(exprs):
    """True if any constraint multiplies two NON-constant terms (NIA — `area = h * w`). Walks the Z3
    AST once for a MUL with >= 2 non-numeral operands. Linear-relation analysis can't surface relations
    in a product, and the solves hang, so callers skip the structure analysis for such claims."""
    seen = set()
    def walk(e):
        eid = e.get_id()
        if eid in seen:
            return False
        seen.add(eid)
        if z3.is_app(e):
            if e.decl().kind() == z3.Z3_OP_MUL:
                nonconst = sum(1 for i in range(e.num_args())
                               if not (z3.is_int_value(e.arg(i)) or z3.is_rational_value(e.arg(i))))
                if nonconst >= 2:
                    return True
            return any(walk(e.arg(i)) for i in range(e.num_args()))
        return False
    items = exprs if isinstance(exprs, (list, tuple)) else [exprs]
    return any(walk(c) for c in items)
