"""
Range analysis for the Evident IDE.

Uses Z3 Optimize to find the minimum and maximum values for each free
numeric variable in a schema.
"""

import z3
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def compute_ranges(source: str, schema_name: str, given: dict) -> dict:
    """
    For each free variable in schema_name, compute min and max using Z3 Optimize.

    Returns:
        {
            var_name: {
                "min": int | None,
                "max": int | None,
                "type": str,
            }
            OR
            var_name: {
                "fixed": value,
                "type": str,
            }
        }
    """
    from runtime.src.runtime import EvidentRuntime
    from runtime.src.ast_types import MembershipConstraint, Identifier

    rt = EvidentRuntime()
    rt.load_source(source)
    schema = rt.schemas.get(schema_name)
    if schema is None:
        return {}

    ranges = {}

    for item in schema.body:
        if (
            isinstance(item, MembershipConstraint)
            and item.op == "∈"
            and isinstance(item.left, Identifier)
        ):
            name = item.left.name
            type_name = (
                item.right.name
                if isinstance(item.right, Identifier)
                else "unknown"
            )

            if name in given:
                ranges[name] = {"fixed": given[name], "type": type_name}
                continue

            if type_name in ("Nat", "Int"):
                lo = _optimize(source, schema_name, given, name, minimize=True)
                hi = _optimize(source, schema_name, given, name, minimize=False)
                ranges[name] = {"min": lo, "max": hi, "type": type_name}
            else:
                ranges[name] = {"min": None, "max": None, "type": type_name}

    return ranges


def _optimize(
    source: str,
    schema_name: str,
    given: dict,
    var_name: str,
    minimize: bool,
) -> int | None:
    """
    Run Z3 Optimize to find the min or max of var_name subject to the
    schema constraints and given bindings.
    """
    from runtime.src.runtime import EvidentRuntime
    from runtime.src.evaluate import EvidentSolver, _translate_body_constraint
    from runtime.src.instantiate import instantiate_schema
    from runtime.src.ast_types import EvidentBlock, PassthroughItem
    from runtime.src.env import Environment

    rt = EvidentRuntime()
    rt.load_source(source)
    schema = rt.schemas.get(schema_name)
    if schema is None:
        return None

    solver_obj = EvidentSolver()
    solver_obj.registry = rt.solver.registry
    for sname, sobj in rt.schemas.items():
        solver_obj.schemas[sname] = sobj

    # Build environment from given
    init_env = Environment()
    for vname, val in given.items():
        z3_val = solver_obj._python_to_z3_untyped(val)
        init_env = init_env.bind(vname, z3_val)

    env, type_constraints = instantiate_schema(schema, init_env, solver_obj.registry)

    opt = z3.Optimize()
    for tc in type_constraints:
        opt.add(tc)

    for item in schema.body:
        if isinstance(item, (EvidentBlock, PassthroughItem)):
            continue
        try:
            c = _translate_body_constraint(item, env, solver_obj.registry)
            opt.add(c)
        except (NotImplementedError, KeyError):
            pass

    target = env.lookup(var_name)
    if target is None:
        return None

    if minimize:
        opt.minimize(target)
    else:
        opt.maximize(target)

    if opt.check() == z3.sat:
        val = opt.model().eval(target, model_completion=True)
        try:
            return val.as_long()
        except Exception:
            return None
    return None
