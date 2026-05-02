"""
Variable range finder for the Evident sampler.

For each numeric variable, finds a (min, max) pair using two stages:
  1. Z3 Optimize — exact bounds in one call, fast for linear systems.
     Times out after OPTIMIZE_TIMEOUT_MS and falls back to stage 2.
  2. Iterative tightening — repeatedly add var < current / var > current
     and re-solve. Not exact but converges quickly in practice.

Returns: { varname: { "min": lo, "max": hi }, ... }
Missing min or max means the bound is unbounded or couldn't be determined.
"""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

import z3

OPTIMIZE_TIMEOUT_MS = 500   # per direction (min or max)
TIGHTEN_ITERS       = 12    # fallback: max iterations per direction


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _to_python(val):
    """Convert a Z3 numeric value to a Python int or float."""
    if val is None:
        return None
    if z3.is_int_value(val):
        return val.as_long()
    if z3.is_rational_value(val):
        try:
            return val.numerator_as_long() / val.denominator_as_long()
        except Exception:
            return float(val.as_decimal(10).rstrip('?'))
    return None


def _is_infinite(val):
    """True if Z3 returned ±oo as an objective bound."""
    s = str(val)
    return 'oo' in s or 'inf' in s.lower()


# ---------------------------------------------------------------------------
# Stage 1: Z3 Optimize
# ---------------------------------------------------------------------------

def _optimize_bounds(assertions, z3_var, is_int):
    """
    Try to find exact min and max via z3.Optimize.
    Runs min and max as separate calls so they don't interfere.
    Returns (lo, hi) — either may be None on timeout / unknown.
    """
    lo = hi = None

    for direction in ('min', 'max'):
        opt = z3.Optimize()
        opt.set('timeout', OPTIMIZE_TIMEOUT_MS)
        for a in assertions:
            opt.add(a)

        h = opt.minimize(z3_var) if direction == 'min' else opt.maximize(z3_var)
        result = opt.check()

        if result == z3.sat:
            try:
                bound = opt.lower(h) if direction == 'min' else opt.upper(h)
                if not _is_infinite(bound):
                    py = _to_python(bound)
                    if py is not None:
                        if direction == 'min':
                            lo = int(py) if is_int else py
                        else:
                            hi = int(py) if is_int else py
            except Exception:
                pass
        # unknown = timed out; unsat = no solution (shouldn't happen since
        # we pre-check base satisfiability)

    return lo, hi


# ---------------------------------------------------------------------------
# Stage 2: Iterative tightening
# ---------------------------------------------------------------------------

def _tighten(base_solver, z3_var, seed_val, direction, is_int, max_iters):
    """
    Starting from seed_val, repeatedly add var < current (min) or
    var > current (max) and re-solve to find a tighter bound.

    Returns (bound, converged) where converged=True means we hit UNSAT
    (found the true bound) and False means we exhausted max_iters without
    hitting a wall (variable is likely unbounded in that direction).
    """
    current = seed_val
    converged = False
    base_solver.push()
    for _ in range(max_iters):
        if direction == 'min':
            base_solver.add(z3_var < (z3.IntVal(current) if is_int else z3.RealVal(current)))
        else:
            base_solver.add(z3_var > (z3.IntVal(current) if is_int else z3.RealVal(current)))

        if base_solver.check() != z3.sat:
            converged = True
            break

        val = base_solver.model().eval(z3_var, model_completion=True)
        py = _to_python(val)
        if py is None:
            break
        current = int(py) if is_int else py

    base_solver.pop()
    return current, converged


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def compute_ranges(source, schema_name, given):
    """
    Return { varname: { "min": lo, "max": hi } } for every numeric variable
    in schema_name that is not pinned in given.
    """
    from runtime.src.runtime import EvidentRuntime
    from runtime.src.evaluate import EvidentSolver, _translate_body_constraint
    from runtime.src.instantiate import instantiate_schema
    from runtime.src.ast_types import EvidentBlock, PassthroughItem
    from runtime.src.env import Environment

    # ── Build runtime and schema ──────────────────────────────────────────────
    rt = EvidentRuntime()
    rt.load_source(source)
    schema = rt.schemas.get(schema_name)
    if schema is None:
        return {}

    solver_obj = EvidentSolver()
    solver_obj.registry = rt.solver.registry
    for sname, sobj in rt.schemas.items():
        solver_obj.schemas[sname] = sobj

    init_env = Environment()
    for vname, val in given.items():
        init_env = init_env.bind(vname, solver_obj._python_to_z3_untyped(val))

    env, type_constraints = instantiate_schema(
        schema, init_env, solver_obj.registry, schemas=solver_obj.schemas
    )

    base_solver = z3.Solver()
    for tc in type_constraints:
        base_solver.add(tc)
    for item in schema.body:
        if isinstance(item, (EvidentBlock, PassthroughItem)):
            continue
        try:
            base_solver.add(_translate_body_constraint(item, env, solver_obj.registry))
        except (NotImplementedError, KeyError):
            pass

    if base_solver.check() != z3.sat:
        return {}

    seed = base_solver.model()
    result = {}

    # ── Find bounds for each numeric variable ─────────────────────────────────
    for vname, z3_var in env.bindings.items():
        if vname.startswith('__') or vname.startswith('.') or vname in given:
            continue
        if not (z3.is_int(z3_var) or z3.is_real(z3_var)):
            continue

        is_int = z3.is_int(z3_var)
        seed_val = _to_python(seed.eval(z3_var, model_completion=True))
        if seed_val is None:
            continue
        if is_int:
            seed_val = int(seed_val)

        # Stage 1: Z3 Optimize
        lo, hi = _optimize_bounds(base_solver.assertions(), z3_var, is_int)

        # Stage 2: tighten any bound that Optimize didn't find.
        # Only keep the tightened value if we actually converged (hit UNSAT),
        # meaning it's a true bound. If we exhausted iterations, the variable
        # is likely unbounded in that direction — leave it as None so the
        # sampler uses its wider fallback defaults.
        if lo is None:
            lo_val, lo_conv = _tighten(base_solver, z3_var, seed_val, 'min', is_int, TIGHTEN_ITERS)
            if lo_conv:
                lo = lo_val
        if hi is None:
            hi_val, hi_conv = _tighten(base_solver, z3_var, seed_val, 'max', is_int, TIGHTEN_ITERS)
            if hi_conv:
                hi = hi_val

        result[vname] = {'min': lo, 'max': hi}

    return result
