"""
Sampling strategies for the Evident IDE.

Three strategies:
  blocking_clause_sample  — find n diverse solutions via blocking clauses (most systematic)
  random_seed_sample      — add random hint constraints to nudge the solver (fast, diverse)
  grid_sample             — sweep x_var across x_range in N steps (transfer functions)
"""

import random
from dataclasses import dataclass
from typing import Any
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent))


@dataclass
class Sample:
    bindings: dict[str, Any]
    satisfied: bool


# ---------------------------------------------------------------------------
# Blocking clause sampling
# ---------------------------------------------------------------------------


def blocking_clause_sample(
    source: str,
    schema_name: str,
    given: dict,
    n: int,
) -> list[Sample]:
    """
    Find n diverse solutions via blocking clauses. Most systematic.

    Each iteration:
      1. Evaluate the schema (with previous solutions excluded via extra 'given' exclusions).
      2. If sat, record bindings.
      3. Build a blocking constraint by adding an unsatisfiable hint for each
         previous solution, then re-run. We achieve this by re-running with a
         fresh solver each time, encoding the negation of all previous solutions
         directly via Z3.
    """
    import z3
    from runtime.src.runtime import EvidentRuntime
    from runtime.src.evaluate import EvidentSolver
    from runtime.src.instantiate import instantiate_schema
    from runtime.src.ast_types import MembershipConstraint, Identifier, EvidentBlock, PassthroughItem, MultiMembershipDecl
    from runtime.src.evaluate import _translate_body_constraint

    # We'll build the constraint system incrementally in Z3, adding blocking
    # clauses after each solution.

    rt = EvidentRuntime()
    rt.load_source(source)
    schema = rt.schemas.get(schema_name)
    if schema is None:
        return []

    samples: list[Sample] = []
    previous_solutions: list[dict[str, Any]] = []

    for _ in range(n):
        solver_obj = EvidentSolver()
        solver_obj.registry = rt.solver.registry
        # Copy schema registrations
        for sname, sobj in rt.schemas.items():
            solver_obj.schemas[sname] = sobj

        # Build environment from given
        from runtime.src.env import Environment
        init_env = Environment()
        for vname, val in given.items():
            z3_val = solver_obj._python_to_z3_untyped(val)
            init_env = init_env.bind(vname, z3_val)

        env, type_constraints = instantiate_schema(schema, init_env, solver_obj.registry, schemas=solver_obj.schemas)

        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)

        for item in schema.body:
            if isinstance(item, (EvidentBlock, PassthroughItem)):
                continue
            try:
                z3_constraint = _translate_body_constraint(item, env, solver_obj.registry)
                s.add(z3_constraint)
            except (NotImplementedError, KeyError):
                pass

        # Add blocking clauses for all previous solutions
        for prev in previous_solutions:
            prev_clauses = []
            for vname, py_val in prev.items():
                if vname in given:
                    continue
                z3_var = env.lookup(vname)
                if z3_var is None:
                    continue
                try:
                    z3_val = solver_obj._python_to_z3_untyped(py_val)
                    prev_clauses.append(z3_var == z3_val)
                except Exception:
                    pass
            if prev_clauses:
                s.add(z3.Not(z3.And(*prev_clauses)))

        # Try with a random enum hint first (push/pop so UNSAT from a bad hint
        # doesn't kill the loop — it just falls back to unconstrained solving).
        enum_hints = []
        # Scan both body and params for enum-typed variables to hint.
        def _enum_candidates():
            for item in schema.body:
                if (isinstance(item, MembershipConstraint) and item.op == "∈"
                        and isinstance(item.left, Identifier)):
                    yield item.left.name, item.right.name if isinstance(item.right, Identifier) else None
            for param in schema.params:
                tname = param.set.name if isinstance(param.set, Identifier) else None
                for vname in param.names:
                    yield vname, tname

        for vname, type_name in _enum_candidates():
            if vname in given or not type_name:
                continue
            ctors = solver_obj.registry.get_constructors_for(type_name)
            if ctors:
                z3_var = env.lookup(vname)
                if z3_var is not None:
                    enum_hints.append(z3_var == random.choice(ctors))

        model = None
        if enum_hints:
            s.push()
            for h in enum_hints:
                s.add(h)
            if s.check() == z3.sat:
                model = s.model()
            s.pop()

        if model is None:
            if s.check() != z3.sat:
                break  # no more solutions at all
            model = s.model()

        bindings = solver_obj._extract_model(env, model)
        previous_solutions.append(bindings)
        samples.append(Sample(bindings=bindings, satisfied=True))

    return samples


# ---------------------------------------------------------------------------
# Random seed sampling
# ---------------------------------------------------------------------------


def random_seed_sample(
    source: str,
    schema_name: str,
    given: dict,
    n: int,
    attempts_multiplier: int = 3,
) -> list[Sample]:
    """
    Sample by adding random hint constraints. Fast and diverse.

    Builds the Z3 constraint system ONCE, then uses push/pop to test random
    hint assignments. This avoids the ~100ms load_source overhead per attempt.
    Safe here because this always runs inside an isolated subprocess.
    """
    import z3
    from runtime.src.runtime import EvidentRuntime
    from runtime.src.evaluate import EvidentSolver, _translate_body_constraint
    from runtime.src.instantiate import instantiate_schema
    from runtime.src.ast_types import MembershipConstraint, Identifier, EvidentBlock, PassthroughItem, MultiMembershipDecl
    from runtime.src.env import Environment

    rt = EvidentRuntime()
    rt.load_source(source)
    schema = rt.schemas.get(schema_name)
    if schema is None:
        return []

    # Collect free variable names and types from body, multi-name decls, and params.
    free_vars: dict[str, str] = {}
    for item in schema.body:
        if (isinstance(item, MembershipConstraint) and item.op == "∈"
                and isinstance(item.left, Identifier)):
            vname = item.left.name
            type_name = item.right.name if isinstance(item.right, Identifier) else "unknown"
            if vname not in given:
                free_vars[vname] = type_name
        elif isinstance(item, MultiMembershipDecl):
            type_name = item.set.name if isinstance(item.set, Identifier) else "unknown"
            for vname in item.names:
                if vname not in given and vname not in free_vars:
                    free_vars[vname] = type_name
    for param in schema.params:
        type_name = param.set.name if isinstance(param.set, Identifier) else "unknown"
        for vname in param.names:
            if vname not in given and vname not in free_vars:
                free_vars[vname] = type_name

    # Build the Z3 solver and environment ONCE
    solver_obj = EvidentSolver()
    solver_obj.registry = rt.solver.registry
    for sname, sobj in rt.schemas.items():
        solver_obj.schemas[sname] = sobj

    init_env = Environment()
    for vname, val in given.items():
        z3_val = solver_obj._python_to_z3_untyped(val)
        init_env = init_env.bind(vname, z3_val)

    env, type_constraints = instantiate_schema(schema, init_env, solver_obj.registry, schemas=solver_obj.schemas)

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

    # Short-circuit if base constraints are already unsatisfiable
    if base_solver.check() != z3.sat:
        return []

    # Compute hint ranges once (binary search, also cheap since we're in a subprocess)
    computed_ranges: dict = {}
    try:
        from ranges import compute_ranges
        computed_ranges = compute_ranges(source, schema_name, given)
    except Exception:
        pass

    def _hint_range(vname: str, type_name: str) -> tuple:
        rng = computed_ranges.get(vname, {})
        lo = rng.get("min")
        if lo is not None:
            return (lo, lo + 50)
        if type_name == "Nat":          return (0,    50)
        if type_name == "Int":          return (-50,  50)
        if type_name == "Real":         return (-5.0, 5.0)
        return (0, 50)

    samples: list[Sample] = []
    seen: set = set()
    # More attempts compensate for schemas where even the tight window
    # has a low hit rate. Cap at 5000 so n=500 doesn't take forever.
    attempts = min(n * 50, 5000)

    for _ in range(attempts):
        if len(samples) >= n:
            break

        # Push a scope, add one random assignment per free integer variable,
        # check feasibility, then pop — reusing all base constraints.
        base_solver.push()
        for vname, type_name in free_vars.items():
            z3_var = env.lookup(vname)
            if z3_var is None:
                continue
            if type_name in ("Nat", "Int"):
                lo, hi = _hint_range(vname, type_name)
                base_solver.add(z3_var == random.randint(int(lo), int(hi)))
            elif type_name == "Real":
                lo, hi = _hint_range(vname, type_name)
                hint = random.uniform(lo, hi)
                base_solver.add(z3_var == z3.RealVal(hint))
            else:
                # Enum or other algebraic type — pick a random constructor
                ctors = solver_obj.registry.get_constructors_for(type_name)
                if ctors:
                    base_solver.add(z3_var == random.choice(ctors))

        # Also hint sub-schema fields (task.duration, task.deadline, …).
        # These appear as dotted names in the env but not in free_vars.
        for full_name, z3_var in env.bindings.items():
            if '.' not in full_name or full_name in given:
                continue
            try:
                if z3.is_int(z3_var):
                    lo, hi = _hint_range(full_name, 'Nat')
                    base_solver.add(z3_var == random.randint(lo, hi))
            except Exception:
                pass

        if base_solver.check() == z3.sat:
            model = base_solver.model()
            bindings = solver_obj._extract_model(env, model)
            key = tuple(sorted(bindings.items()))
            if key not in seen:
                seen.add(key)
                samples.append(Sample(bindings=bindings, satisfied=True))

        base_solver.pop()

    return samples


# ---------------------------------------------------------------------------
# Grid sampling (transfer function sweep)
# ---------------------------------------------------------------------------


def grid_sample(
    source: str,
    schema_name: str,
    given: dict,
    x_var: str,
    x_range: tuple,
    steps: int,
) -> list[Sample]:
    """
    Sweep x_var across x_range in `steps` steps. For transfer functions.

    Returns one Sample per step, with satisfied=False for infeasible points.
    """
    from runtime.src.runtime import EvidentRuntime

    x_min, x_max = x_range
    samples: list[Sample] = []

    for i in range(steps):
        x_val = x_min + (x_max - x_min) * i / max(steps - 1, 1)
        x_int = int(round(x_val))

        rt = EvidentRuntime()
        rt.load_source(source)
        sweep_given = {**given, x_var: x_int}
        try:
            result = rt.query(schema_name, given=sweep_given)
            samples.append(Sample(bindings=result.bindings if result.satisfied else {}, satisfied=result.satisfied))
        except Exception:
            samples.append(Sample(bindings={}, satisfied=False))

    return samples
