"""
Phase 5: Set comprehensions and filter sugar.

Translates { output | generators... }, S[condition], S.field,
and S grouped_by .field into Z3 Array expressions.
"""

from __future__ import annotations

import z3
from z3 import (
    And,
    ArrayRef,
    BoolVal,
    Exists,
    FreshConst,
    Lambda,
    Or,
    Select,
    SortRef,
    TupleSort,
)

from .env import Environment
from .sorts import SortRegistry
from .sets import empty_set, set_member


def translate_set_comprehension(
    node,  # SetComprehension
    env: Environment,
    registry: SortRegistry,
) -> z3.ArrayRef:
    """
    { output | generators... }

    Each generator is either a Binding (x ∈ S) or a bare constraint.
    The result is the set of output values where all generators hold.

    For { (v1, v2) | (i, v1) ∈ entries, (i+1, v2) ∈ entries }:
    - Introduce fresh vars for i, v1, v2
    - Result is λ(v1,v2). ∃i. entries[(i,v1)] ∧ entries[(i+1,v2)]

    Strategy:
    1. Walk generators to collect all bound names and the sets/constraints.
    2. For each Binding, introduce a fresh Z3 constant for each name.
       If the binding names form a tuple pattern (e.g. (i, v1) ∈ S), we
       introduce a single fresh tuple variable p and bind name_k → accessor_k(p).
    3. Translate the output expression in the extended env.
    4. Build the body: conjunction of all membership/constraint tests.
    5. Return λ(output_var). ∃(existential_vars). body ∧ output_var = output_expr
    """
    from .translate import translate_expr, translate_constraint
    from .ast_types import Binding, ComprehensionGenerator

    # We need to know the output sort to build the lambda.
    # We'll figure it out after translating the output expression.

    # Phase 1: extend env with fresh variables for all bindings.
    current_env = env
    # fresh_vars: list of (z3_var, set_expr) pairs for membership
    # constraints collected from generators.
    membership_constraints: list[z3.BoolRef] = []
    # all fresh variables introduced (for the existential)
    all_fresh_vars: list[z3.ExprRef] = []

    for gen in node.generators:
        if gen.binding is not None:
            binding: Binding = gen.binding
            names = binding.names
            set_expr = translate_expr(binding.set, current_env, registry)

            if len(names) == 1:
                # Simple: x ∈ S — fresh var of the element sort.
                elem_sort = set_expr.sort().domain()
                fresh_var = FreshConst(elem_sort, names[0])
                current_env = current_env.bind(names[0], fresh_var)
                all_fresh_vars.append(fresh_var)
                membership_constraints.append(Select(set_expr, fresh_var))

            else:
                # Tuple destructuring: (a, b, ...) ∈ S
                # S must be a set of tuples: Array(TupleSort, Bool).
                tuple_sort = set_expr.sort().domain()
                # Retrieve the tuple accessors from z3.TupleSort.
                # The sort name encodes the component sorts: "Tuple_X_Y_..."
                # We need to re-derive the mk/accessors.
                sort_name = tuple_sort.name()
                # Re-create the TupleSort to get accessors (idempotent in Z3).
                # We need the component sorts — extract them from the sort name
                # or from the tuple sort itself.
                # z3 TupleSort returns (sort, mk, [accessor_i])
                # We stored the sort in the registry under sort_name.
                # Recover accessors via z3's tuple API.
                # Unfortunately z3 doesn't expose accessors directly from
                # a SortRef, so we reconstruct by looking in the registry or
                # using the z3 Datatype accessors.
                accs = _get_tuple_accessors(tuple_sort, len(names))

                fresh_tuple = FreshConst(tuple_sort, "_".join(names))
                all_fresh_vars.append(fresh_tuple)
                membership_constraints.append(Select(set_expr, fresh_tuple))

                for k, name in enumerate(names):
                    proj = accs[k](fresh_tuple)
                    current_env = current_env.bind(name, proj)

            # Optional guard on the binding.
            if binding.guard is not None:
                guard_z3 = translate_expr(binding.guard, current_env, registry)
                membership_constraints.append(guard_z3)

        elif gen.constraint is not None:
            # Bare constraint — translate directly with the current env.
            c_z3 = translate_constraint(gen.constraint, current_env, registry)
            membership_constraints.append(c_z3)

    # Phase 2: translate the output expression.
    output_z3 = translate_expr(node.output, current_env, registry)
    output_sort = output_z3.sort()

    # Phase 3: build the result set.
    # result = λy. ∃(fresh_vars). body ∧ y = output_z3
    body = And(*membership_constraints) if membership_constraints else BoolVal(True)

    result_var = FreshConst(output_sort, "result")

    if all_fresh_vars:
        inner = Exists(all_fresh_vars, And(body, result_var == output_z3))
    else:
        inner = And(body, result_var == output_z3)

    return Lambda([result_var], inner)


def _get_tuple_accessors(tuple_sort: z3.SortRef, arity: int):
    """
    Return the list of field accessor functions for a Z3 tuple sort.

    Z3 encodes tuple sorts as Datatypes with a single constructor.
    The accessors are the recognizer's fields named field0, field1, ...
    """
    # In z3, the TupleSort is a Datatype with one constructor.
    # The constructor has 'arity' fields.
    # We can retrieve them via the DatatypeRef API.
    # z3.TupleSort(name, sorts) returns (sort, mk, [acc_0, ..., acc_{n-1}])
    # But we only have the sort here. We reconstruct via the sort name.
    #
    # The most reliable approach: use z3's constructor / accessor lookup.
    # tuple_sort is a DatatypeSortRef. Its constructor 0 has arity fields.
    constructor = tuple_sort.constructor(0)  # the single mk_ constructor
    # The accessors are numbered 0..arity-1.
    accs = [tuple_sort.accessor(0, k) for k in range(arity)]
    return accs


def translate_filter(
    set_expr: z3.ArrayRef,
    condition,  # Expr or Constraint — may reference '.' for the current element
    element_sort: z3.SortRef,
    env: Environment,
    registry: SortRegistry,
) -> z3.ArrayRef:
    """
    S[condition] — filter where '.' refers to the current element.
    Result: λx. S[x] ∧ condition(x)

    The condition is evaluated with '.' bound to x (the current element).
    Accepts both Expr nodes (via translate_expr) and Constraint nodes
    (via translate_constraint), dispatching based on the node type.
    """
    from .translate import translate_expr, translate_constraint
    from .ast_types import (
        ArithmeticConstraint,
        MembershipConstraint,
        LogicConstraint,
        BindingConstraint,
        SetEqualityConstraint,
    )

    _constraint_types = (
        ArithmeticConstraint,
        MembershipConstraint,
        LogicConstraint,
        BindingConstraint,
        SetEqualityConstraint,
    )

    x = FreshConst(element_sort, "elem")
    inner_env = env.bind(".", x)

    # Also bind ".<field>" → Select(field_array, x) for any field arrays in env.
    # This lets FieldAccess(Identifier('.'), field) resolve correctly via the
    # key ".<field>" (which translate_expr builds as f"{obj_name}.{field}").
    bool_sort = z3.BoolSort()
    for key, val in env.bindings.items():
        try:
            val_sort = val.sort()
        except Exception:
            continue
        if (
            isinstance(val_sort, z3.ArraySortRef)
            and val_sort.domain() == element_sort
            and val_sort.range() != bool_sort
        ):
            # This looks like a field array: Array(element_sort, field_sort).
            # translate_expr builds field-access keys as f"{obj_name}.{field}",
            # so when obj_name is "." we need to bind ".<key>" which becomes
            # "." + "." + field = "..<field>".  E.g. ".val" → key "..val".
            dotted_key = f"..{key}"  # = f"{'.'}.{key}"
            inner_env = inner_env.bind(dotted_key, z3.Select(val, x))

    if isinstance(condition, _constraint_types):
        cond_z3 = translate_constraint(condition, inner_env, registry)
    else:
        cond_z3 = translate_expr(condition, inner_env, registry)

    return Lambda([x], And(Select(set_expr, x), cond_z3))


def translate_field_projection(
    set_expr: z3.ArrayRef,
    field: str,
    element_sort: z3.SortRef,
    field_sort: z3.SortRef,
    env: Environment,
    registry: SortRegistry,
) -> z3.ArrayRef:
    """
    S.field — project one field across a set.
    Result: λy. ∃x. S[x] ∧ x.field = y

    The field mapping is expected to be stored in env as a Z3 Array
    under the key '<field>' or retrieved via a field accessor.

    Two conventions are supported:
    1. env has a key 'field' that is an Array(element_sort, field_sort).
       Then x.field = field_array[x].
    2. element_sort is a TupleSort and 'field' identifies one of its accessors.
       (Not yet implemented — requires knowing which slot 'field' maps to.)
    """
    x = FreshConst(element_sort, "x")
    y = FreshConst(field_sort, "y")

    # Look up the field as an array in the environment.
    field_array = env.lookup(field)
    if field_array is not None:
        # field_array : Array(element_sort, field_sort)
        field_val = z3.Select(field_array, x)
    else:
        raise KeyError(
            f"translate_field_projection: field {field!r} not found in environment. "
            f"Bound names: {list(env.bindings.keys())}"
        )

    return Lambda([y], Exists([x], And(Select(set_expr, x), field_val == y)))


def translate_grouped_by(
    set_expr: z3.ArrayRef,
    field: str,
    element_sort: z3.SortRef,
    key_sort: z3.SortRef,
    env: Environment,
    registry: SortRegistry,
) -> z3.ArrayRef:
    """
    S grouped_by .field
    Result: a set of sets, one per distinct field value.
    Encoded as: Array(key_sort, Array(element_sort, Bool))
    group[k] = { x ∈ S | x.field = k }

    Returns a Lambda over k that itself is a Lambda over x.
    """
    # Look up the field array in env.
    field_array = env.lookup(field)
    if field_array is None:
        raise KeyError(
            f"translate_grouped_by: field {field!r} not found in environment. "
            f"Bound names: {list(env.bindings.keys())}"
        )

    k = FreshConst(key_sort, "key")
    x = FreshConst(element_sort, "x_grp")

    field_val = z3.Select(field_array, x)

    # Inner set for key k: λx. S[x] ∧ field_array[x] = k
    inner_set = Lambda([x], And(Select(set_expr, x), field_val == k))

    # Outer map: λk. inner_set(k)  — but inner_set already depends on k,
    # so we embed k directly.
    return Lambda([k], inner_set)
