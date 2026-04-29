"""
Range analysis for the Evident IDE.

Uses a fresh Z3 Solver per check (no push/pop, no Optimize) to find the
minimum value for each free numeric variable. Avoids all Z3 global state
issues that crash the server.
"""

import z3
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def _schema_constraints(source: str, schema_name: str, given: dict):
    """Build Z3 constraints for a schema. Returns (constraints_list, env) or (None, None)."""
    from runtime.src.runtime import EvidentRuntime
    from runtime.src.evaluate import EvidentSolver, _translate_body_constraint
    from runtime.src.instantiate import instantiate_schema
    from runtime.src.ast_types import EvidentBlock, PassthroughItem
    from runtime.src.env import Environment

    rt = EvidentRuntime()
    rt.load_source(source)
    schema = rt.schemas.get(schema_name)
    if schema is None:
        return None, None

    solver_obj = EvidentSolver()
    solver_obj.registry = rt.solver.registry
    for sname, sobj in rt.schemas.items():
        solver_obj.schemas[sname] = sobj

    init_env = Environment()
    for vname, val in given.items():
        z3_val = solver_obj._python_to_z3_untyped(val)
        init_env = init_env.bind(vname, z3_val)

    env, type_constraints = instantiate_schema(schema, init_env, solver_obj.registry, schemas=solver_obj.schemas)

    body_constraints = list(type_constraints)
    for item in schema.body:
        if isinstance(item, (EvidentBlock, PassthroughItem)):
            continue
        try:
            c = _translate_body_constraint(item, env, solver_obj.registry)
            body_constraints.append(c)
        except (NotImplementedError, KeyError):
            pass

    return body_constraints, env


def _sat_with_bound(constraints, var, upper_bound: int) -> bool:
    """Check if constraints + var <= upper_bound is satisfiable using a fresh solver."""
    s = z3.Solver()
    s.set("timeout", 2000)
    for c in constraints:
        s.add(c)
    s.add(var <= upper_bound)
    return s.check() == z3.sat


def _find_min(constraints, var, lo: int = 0, hi: int = 500) -> int | None:
    """Binary search for minimum satisfying value using fresh solvers per check."""
    # First verify the problem is satisfiable at all
    s = z3.Solver()
    s.set("timeout", 2000)
    for c in constraints:
        s.add(c)
    if s.check() != z3.sat:
        return None

    result = None
    for _ in range(12):  # log2(500) ≈ 9 iterations
        if lo > hi:
            break
        mid = (lo + hi) // 2
        if _sat_with_bound(constraints, var, mid):
            result = mid
            hi = mid - 1
        else:
            lo = mid + 1
    return result


def compute_ranges(source: str, schema_name: str, given: dict) -> dict:
    """
    For each free Nat/Int variable, find its minimum via binary search.
    Returns {var_name: {"min": int|None, "type": str}} for free variables,
    {var_name: {"fixed": value, "type": str}} for given variables.
    """
    from runtime.src.ast_types import MembershipConstraint, Identifier

    try:
        from runtime.src.runtime import EvidentRuntime
        rt = EvidentRuntime()
        rt.load_source(source)
        schema = rt.schemas.get(schema_name)
        if not schema:
            return {}
    except Exception:
        return {}

    ranges: dict = {}

    for item in schema.body:
        if not (isinstance(item, MembershipConstraint)
                and item.op == "∈"
                and isinstance(item.left, Identifier)):
            continue
        name = item.left.name
        type_name = item.right.name if isinstance(item.right, Identifier) else "unknown"

        if name in given:
            ranges[name] = {"fixed": given[name], "type": type_name}
            continue

        if type_name in ("Nat", "Int"):
            try:
                constraints, env = _schema_constraints(source, schema_name, given)
                if constraints is None or env is None:
                    ranges[name] = {"min": None, "type": type_name}
                    continue
                target = env.lookup(name)
                if target is None:
                    ranges[name] = {"min": None, "type": type_name}
                    continue
                lo_start = 0 if type_name == "Nat" else -200
                lo = _find_min(constraints, target, lo_start, 500)
                ranges[name] = {"min": lo, "type": type_name}
            except Exception:
                ranges[name] = {"min": None, "type": type_name}
        else:
            ranges[name] = {"min": None, "type": type_name}

    return ranges
