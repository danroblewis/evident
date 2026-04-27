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
    from runtime.src.ast_types import MembershipConstraint, Identifier, EvidentBlock, PassthroughItem
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

        env, type_constraints = instantiate_schema(schema, init_env, solver_obj.registry)

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
            # Block: NOT (all free vars == their previous values simultaneously)
            prev_clauses = []
            for vname, py_val in prev.items():
                if vname in given:
                    continue  # given vars are fixed; don't block on them
                z3_var = env.lookup(vname)
                if z3_var is None:
                    continue
                try:
                    z3_val = solver_obj._python_to_z3_untyped(py_val)
                    prev_clauses.append(z3_var == z3_val)
                except Exception:
                    pass
            if prev_clauses:
                # Exclude this exact assignment
                s.add(z3.Not(z3.And(*prev_clauses)))

        result = s.check()
        if result != z3.sat:
            # No more solutions
            break

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

    For each attempt:
      1. Pick random values for free variables within estimated ranges.
      2. Add those as additional 'given' hints (soft — the solver may pick
         nearby values if the exact hint is infeasible).
      3. Evaluate; if sat, record.
    """
    from runtime.src.runtime import EvidentRuntime
    from runtime.src.ast_types import MembershipConstraint, Identifier

    # First, get rough ranges from a quick analysis
    rt_probe = EvidentRuntime()
    rt_probe.load_source(source)
    schema = rt_probe.schemas.get(schema_name)
    if schema is None:
        return []

    # Collect free variable names and their types from the schema body
    free_vars: dict[str, str] = {}  # name -> type_name
    for item in schema.body:
        if (
            isinstance(item, MembershipConstraint)
            and item.op == "∈"
            and isinstance(item.left, Identifier)
        ):
            vname = item.left.name
            type_name = item.right.name if isinstance(item.right, Identifier) else "unknown"
            if vname not in given:
                free_vars[vname] = type_name

    # Default ranges for random hints
    _default_ranges: dict[str, tuple[int, int]] = {
        "Nat": (0, 100),
        "Int": (-50, 50),
    }

    samples: list[Sample] = []
    attempts = n * attempts_multiplier

    for _ in range(attempts):
        if len(samples) >= n:
            break

        # Build random hints for numeric free variables
        hints: dict[str, Any] = {}
        for vname, type_name in free_vars.items():
            if type_name in _default_ranges:
                lo, hi = _default_ranges[type_name]
                hints[vname] = random.randint(lo, hi)

        # Merge hints with given (given takes priority)
        combined_given = {**hints, **given}

        rt = EvidentRuntime()
        rt.load_source(source)
        try:
            result = rt.query(schema_name, given=combined_given)
            if result.satisfied:
                samples.append(Sample(bindings=result.bindings, satisfied=True))
            else:
                samples.append(Sample(bindings={}, satisfied=False))
        except Exception:
            pass

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
