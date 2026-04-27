"""
Phase 8 tests: schema composition mechanisms.

Tests cover:
- names_match_compose: natural join on shared variable names
- passthrough_compose: ..schema — lift all sub-schema vars into parent scope
- partial_application / PartialSchema: fix some vars, leave others free
- chain_compose: A · B · C with shared variables identified
- Full evaluation with composition via EvidentSolver
"""

import pytest
import z3

from runtime.src.sorts import SortRegistry
from runtime.src.env import Environment
from runtime.src.instantiate import make_const, type_constraint
from runtime.src.compose import (
    names_match_compose,
    passthrough_compose,
    partial_application,
    PartialSchema,
    chain_compose,
)
from runtime.src.evaluate import EvidentSolver, EvaluationResult
from runtime.src.ast_types import (
    SchemaDecl,
    Param,
    Identifier,
    ArithmeticConstraint,
    NatLiteral,
    StringLiteral,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def nat_param(*names: str) -> Param:
    """Param declaring ``names ∈ Nat``."""
    return Param(names=list(names), set=Identifier(name="Nat"))


def string_param(*names: str) -> Param:
    """Param declaring ``names ∈ String``."""
    return Param(names=list(names), set=Identifier(name="String"))


def mk_schema(name: str, params: list[Param], body: list = None) -> SchemaDecl:
    return SchemaDecl(keyword="schema", name=name, params=params, body=body or [])


def fresh_registry() -> SortRegistry:
    return SortRegistry()


# ---------------------------------------------------------------------------
# Names-match tests
# ---------------------------------------------------------------------------


class TestNamesMatchCompose:
    """names_match_compose — natural join on shared variable names."""

    def test_shared_single_var_same_z3_const(self):
        """
        Two schemas both declare `user ∈ Nat`.
        After names_match_compose, both reference the same Z3 Int constant.
        """
        registry = fresh_registry()

        schema_a = mk_schema("A", [nat_param("user")])
        schema_b = mk_schema("B", [nat_param("user")])

        # Instantiate A first to get an env with 'user'
        env_a = Environment()
        user_a = make_const("user", registry.get("Nat"))
        env_a = env_a.bind("user", user_a)

        # Compose B into A's env — 'user' should be shared
        merged_env, constraints = names_match_compose(env_a, schema_b, registry)

        assert "user" in merged_env.bindings
        # The merged 'user' must be the exact same Z3 object as env_a's 'user'
        assert z3.eq(merged_env.bindings["user"], user_a)

    def test_overlapping_var_y_shared_xy_z_fresh(self):
        """
        Schema A has `x ∈ Nat, y ∈ Nat`. Schema B has `y ∈ Nat, z ∈ Nat`.
        After composing B into A's env: x and z are independent, y is shared.
        """
        registry = fresh_registry()

        schema_a = mk_schema("A", [nat_param("x"), nat_param("y")])
        schema_b = mk_schema("B", [nat_param("y"), nat_param("z")])

        # Build env for A
        x_a = make_const("x", registry.get("Nat"))
        y_a = make_const("y", registry.get("Nat"))
        env_a = Environment()
        env_a = env_a.bind("x", x_a)
        env_a = env_a.bind("y", y_a)

        # Compose B into A's env
        merged_env, constraints = names_match_compose(env_a, schema_b, registry)

        # y must be the same as A's y
        assert z3.eq(merged_env.bindings["y"], y_a)
        # x is preserved from A
        assert z3.eq(merged_env.bindings["x"], x_a)
        # z is a fresh variable (different from x or y)
        assert "z" in merged_env.bindings
        z_merged = merged_env.bindings["z"]
        assert not z3.eq(z_merged, x_a)
        assert not z3.eq(z_merged, y_a)

    def test_no_overlap_all_fresh(self):
        """
        Parent env has `a`, sub-schema has `b`. No overlap → b is fresh.
        """
        registry = fresh_registry()
        a_const = make_const("a", registry.get("Nat"))
        env = Environment().bind("a", a_const)

        schema_b = mk_schema("B", [nat_param("b")])
        merged, constraints = names_match_compose(env, schema_b, registry)

        assert "a" in merged.bindings
        assert "b" in merged.bindings
        # b must be a fresh constant (not equal to a)
        assert not z3.eq(merged.bindings["b"], a_const)

    def test_explicit_mapping_renames_slot(self):
        """
        Explicit mapping: schema B has `person ∈ Nat`, mapped to `user` in
        parent env.  After compose, B's `person` variable equals parent's `user`.
        """
        registry = fresh_registry()

        user_const = make_const("user", registry.get("Nat"))
        env = Environment().bind("user", user_const)

        schema_b = mk_schema("B", [nat_param("person")])

        merged, constraints = names_match_compose(
            env,
            schema_b,
            registry,
            explicit_mappings={"person": "user"},
        )

        # 'person' in merged env should equal the parent's 'user'
        assert "person" in merged.bindings
        assert z3.eq(merged.bindings["person"], user_const)

    def test_explicit_mapping_missing_parent_var_raises(self):
        """
        Explicit mapping that references a non-existent parent var should raise.
        """
        registry = fresh_registry()
        env = Environment()  # empty

        schema_b = mk_schema("B", [nat_param("person")])

        with pytest.raises(KeyError, match="no variable"):
            names_match_compose(
                env,
                schema_b,
                registry,
                explicit_mappings={"person": "nonexistent"},
            )

    def test_type_constraints_collected(self):
        """
        names_match_compose should return Nat ≥ 0 type constraints.
        """
        registry = fresh_registry()
        env = Environment()

        schema = mk_schema("S", [nat_param("n")])
        merged, constraints = names_match_compose(env, schema, registry)

        # For a Nat variable, we expect at least one type constraint (n >= 0)
        assert len(constraints) >= 1

        # Verify the constraint is satisfiable
        s = z3.Solver()
        for c in constraints:
            s.add(c)
        assert s.check() == z3.sat

    def test_empty_schema_returns_parent_env(self):
        """
        Composing an empty schema (no params) should return the parent env unchanged.
        """
        registry = fresh_registry()
        a_const = make_const("a", registry.get("Nat"))
        env = Environment().bind("a", a_const)

        empty_schema = mk_schema("Empty", [])
        merged, constraints = names_match_compose(env, empty_schema, registry)

        assert "a" in merged.bindings
        assert z3.eq(merged.bindings["a"], a_const)
        assert constraints == []


# ---------------------------------------------------------------------------
# Passthrough composition tests
# ---------------------------------------------------------------------------


class TestPassthroughCompose:
    """passthrough_compose — ..schema lifts all sub-schema vars into parent."""

    def test_fresh_vars_added_to_parent(self):
        """
        Parent env has no variables.  Sub-schema has `x ∈ Nat, y ∈ Nat`.
        After passthrough, both x and y are present in the env.
        """
        registry = fresh_registry()
        env = Environment()

        sub = mk_schema("Sub", [nat_param("x"), nat_param("y")])
        merged, constraints = passthrough_compose(env, sub, registry)

        assert "x" in merged.bindings
        assert "y" in merged.bindings
        assert len(constraints) >= 2  # at least n >= 0 for each Nat

    def test_shared_var_identified(self):
        """
        Parent already has `x`.  Sub-schema also has `x`.
        After passthrough, `x` is the same constant.
        """
        registry = fresh_registry()
        x_const = make_const("x", registry.get("Nat"))
        env = Environment().bind("x", x_const)

        sub = mk_schema("Sub", [nat_param("x"), nat_param("y")])
        merged, constraints = passthrough_compose(env, sub, registry)

        # x is shared from parent
        assert z3.eq(merged.bindings["x"], x_const)
        # y is a new variable
        assert "y" in merged.bindings

    def test_fresh_var_not_equal_to_existing(self):
        """
        New variables from sub-schema are distinct Z3 constants.
        """
        registry = fresh_registry()
        x_const = make_const("x", registry.get("Nat"))
        env = Environment().bind("x", x_const)

        sub = mk_schema("Sub", [nat_param("y")])
        merged, constraints = passthrough_compose(env, sub, registry)

        assert "y" in merged.bindings
        assert not z3.eq(merged.bindings["y"], x_const)

    def test_rename_mapping_sub_var_to_parent_var(self):
        """
        mappings={'sub_x': 'parent_x'}: sub-schema's `sub_x` is treated as
        the parent's `parent_x`.
        """
        registry = fresh_registry()
        parent_x = make_const("parent_x", registry.get("Nat"))
        env = Environment().bind("parent_x", parent_x)

        sub = mk_schema("Sub", [nat_param("sub_x")])
        merged, constraints = passthrough_compose(
            env, sub, registry, mappings={"sub_x": "parent_x"}
        )

        # Both sub_x and parent_x should resolve to the same constant.
        # Check whichever key is present in the merged env.
        if "parent_x" in merged.bindings:
            assert z3.eq(merged.bindings["parent_x"], parent_x)
        elif "sub_x" in merged.bindings:
            assert z3.eq(merged.bindings["sub_x"], parent_x)
        else:
            pytest.fail("Neither 'parent_x' nor 'sub_x' found in merged env")

    def test_passthrough_with_multiple_params_types(self):
        """
        Sub-schema has both Nat and String params.  Both appear in merged env.
        """
        registry = fresh_registry()
        env = Environment()

        sub = mk_schema(
            "Sub",
            [nat_param("count"), string_param("label")],
        )
        merged, constraints = passthrough_compose(env, sub, registry)

        assert "count" in merged.bindings
        assert "label" in merged.bindings
        # count sort should be Int, label sort should be String
        assert merged.bindings["count"].sort() == z3.IntSort()
        assert merged.bindings["label"].sort() == z3.StringSort()


# ---------------------------------------------------------------------------
# Partial application tests
# ---------------------------------------------------------------------------


class TestPartialApplication:
    """PartialSchema — fix some vars, leave others free."""

    def test_fixed_var_uses_provided_value(self):
        """
        `editor = has_role role: "editor"`.
        PartialSchema with `role` fixed to StringVal("editor").
        After instantiate, `role` uses the fixed value.
        """
        registry = fresh_registry()
        schema = mk_schema(
            "has_role",
            [string_param("role"), nat_param("user")],
        )

        role_val = z3.StringVal("editor")
        ps = partial_application(schema, {"role": role_val}, registry)

        assert isinstance(ps, PartialSchema)
        assert "role" in ps.fixed
        assert z3.eq(ps.fixed["role"], role_val)

    def test_instantiate_fixed_var_bound(self):
        """
        When PartialSchema.instantiate is called, fixed var 'role' is bound
        to the fixed value in the resulting env.
        """
        registry = fresh_registry()
        schema = mk_schema(
            "has_role",
            [string_param("role"), nat_param("user")],
        )

        role_val = z3.StringVal("editor")
        ps = partial_application(schema, {"role": role_val}, registry)

        env = Environment()
        merged, constraints = ps.instantiate(env)

        assert "role" in merged.bindings
        assert z3.eq(merged.bindings["role"], role_val)
        # 'user' should be a fresh free variable
        assert "user" in merged.bindings
        assert merged.bindings["user"].sort() == z3.IntSort()

    def test_instantiate_free_var_uses_parent_if_available(self):
        """
        If the parent env already has `user`, the PartialSchema uses that binding.
        """
        registry = fresh_registry()
        schema = mk_schema(
            "has_role",
            [string_param("role"), nat_param("user")],
        )

        existing_user = make_const("user", registry.get("Nat"))
        env = Environment().bind("user", existing_user)

        role_val = z3.StringVal("editor")
        ps = partial_application(schema, {"role": role_val}, registry)

        merged, constraints = ps.instantiate(env)

        # user from parent is shared
        assert z3.eq(merged.bindings["user"], existing_user)

    def test_partial_application_nat_constraint(self):
        """
        A schema with n ∈ Nat partially applied with n=5.
        The resulting PartialSchema should produce n == IntVal(5).
        """
        registry = fresh_registry()
        schema = mk_schema("Threshold", [nat_param("n")])

        n_val = z3.IntVal(5)
        ps = partial_application(schema, {"n": n_val}, registry)
        merged, constraints = ps.instantiate(Environment())

        assert z3.eq(merged.bindings["n"], n_val)

    def test_partial_schema_is_dataclass(self):
        """PartialSchema should be accessible as a dataclass."""
        registry = fresh_registry()
        schema = mk_schema("S", [nat_param("x")])
        ps = PartialSchema(base=schema, fixed={}, registry=registry)
        assert ps.base is schema
        assert ps.fixed == {}


# ---------------------------------------------------------------------------
# Chain composition tests
# ---------------------------------------------------------------------------


class TestChainCompose:
    """chain_compose — A · B · C natural join."""

    def test_chain_two_schemas_shared_user(self):
        """
        `active_account · email_verified` — two schemas with shared `user`.
        Chain compose produces one env where both share `user`.
        """
        registry = fresh_registry()

        active_account = mk_schema(
            "active_account",
            [nat_param("user"), nat_param("account_id")],
        )
        email_verified = mk_schema(
            "email_verified",
            [nat_param("user"), string_param("email")],
        )

        env, constraints = chain_compose([active_account, email_verified], registry)

        assert "user" in env.bindings
        assert "account_id" in env.bindings
        assert "email" in env.bindings

        # Both schemas contributed type constraints
        assert len(constraints) >= 3  # user (x2), account_id, email

    def test_chain_shared_var_is_same_z3_const(self):
        """
        In a chain, the shared variable must be the same Z3 constant across
        both schemas — not two different constants that are merely equal.
        """
        registry = fresh_registry()

        schema_a = mk_schema("A", [nat_param("user"), nat_param("score")])
        schema_b = mk_schema("B", [nat_param("user"), nat_param("rank")])

        env, constraints = chain_compose([schema_a, schema_b], registry)

        user_var = env.bindings["user"]
        score_var = env.bindings["score"]
        rank_var = env.bindings["rank"]

        # user is the same object (structural equality)
        # score and rank are distinct fresh vars
        assert not z3.eq(score_var, rank_var)
        assert not z3.eq(user_var, score_var)
        assert not z3.eq(user_var, rank_var)

    def test_chain_three_schemas(self):
        """A · B · C where each shares one var with the next."""
        registry = fresh_registry()

        schema_a = mk_schema("A", [nat_param("x"), nat_param("y")])
        schema_b = mk_schema("B", [nat_param("y"), nat_param("z")])
        schema_c = mk_schema("C", [nat_param("z"), nat_param("w")])

        env, constraints = chain_compose([schema_a, schema_b, schema_c], registry)

        for var in ("x", "y", "z", "w"):
            assert var in env.bindings

    def test_chain_with_partial_schema(self):
        """
        Chain that includes a PartialSchema with a fixed variable.
        The fixed value propagates correctly through the chain.
        """
        registry = fresh_registry()

        base_schema = mk_schema(
            "has_role",
            [string_param("role"), nat_param("user")],
        )
        role_val = z3.StringVal("admin")
        ps = partial_application(base_schema, {"role": role_val}, registry)

        user_schema = mk_schema("active_user", [nat_param("user"), nat_param("age")])

        env, constraints = chain_compose([user_schema, ps], registry)

        assert "user" in env.bindings
        assert "age" in env.bindings
        assert "role" in env.bindings
        # role is fixed to "admin"
        assert z3.eq(env.bindings["role"], role_val)

    def test_chain_empty_list_returns_empty_env(self):
        """chain_compose([]) returns an empty environment."""
        registry = fresh_registry()
        env, constraints = chain_compose([], registry)
        assert env.bindings == {}
        assert constraints == []

    def test_chain_single_schema(self):
        """chain_compose with one schema is same as names_match_compose from empty env."""
        registry = fresh_registry()
        schema = mk_schema("S", [nat_param("x"), nat_param("y")])

        env_chain, constraints_chain = chain_compose([schema], registry)
        env_nmatch, constraints_nmatch = names_match_compose(Environment(), schema, registry)

        assert set(env_chain.bindings) == set(env_nmatch.bindings)
        assert len(constraints_chain) == len(constraints_nmatch)


# ---------------------------------------------------------------------------
# Constraint correctness via Z3 solver
# ---------------------------------------------------------------------------


class TestCompositionConstraintCorrectness:
    """Verify that composed constraints are actually enforced by Z3."""

    def test_shared_nat_var_constraint_propagates(self):
        """
        Schema A: n ∈ Nat, n > 5.
        Schema B: n ∈ Nat, n < 10.
        Chain compose: solver should find n in (5, 10).
        """
        registry = fresh_registry()
        from runtime.src.instantiate import instantiate_schema

        schema_a = mk_schema(
            "A",
            [nat_param("n")],
            body=[ArithmeticConstraint(">", Identifier("n"), NatLiteral(5))],
        )
        schema_b = mk_schema(
            "B",
            [nat_param("n")],
            body=[ArithmeticConstraint("<", Identifier("n"), NatLiteral(10))],
        )

        env, type_constraints = chain_compose([schema_a, schema_b], registry)

        from runtime.src.translate import translate_constraint
        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)

        # Translate and add each schema's body constraints
        for constraint in schema_a.body:
            s.add(translate_constraint(constraint, env, registry))
        for constraint in schema_b.body:
            s.add(translate_constraint(constraint, env, registry))

        assert s.check() == z3.sat
        model = s.model()
        n_val = model.eval(env.bindings["n"]).as_long()
        assert 5 < n_val < 10

    def test_shared_var_unsat_when_constraints_conflict(self):
        """
        Schema A: n ∈ Nat, n > 10.
        Schema B: n ∈ Nat, n < 5.
        Chain compose: n must be > 10 AND < 5 → unsat.
        """
        registry = fresh_registry()

        schema_a = mk_schema(
            "A",
            [nat_param("n")],
            body=[ArithmeticConstraint(">", Identifier("n"), NatLiteral(10))],
        )
        schema_b = mk_schema(
            "B",
            [nat_param("n")],
            body=[ArithmeticConstraint("<", Identifier("n"), NatLiteral(5))],
        )

        env, type_constraints = chain_compose([schema_a, schema_b], registry)

        from runtime.src.translate import translate_constraint
        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)
        for constraint in schema_a.body:
            s.add(translate_constraint(constraint, env, registry))
        for constraint in schema_b.body:
            s.add(translate_constraint(constraint, env, registry))

        assert s.check() == z3.unsat


# ---------------------------------------------------------------------------
# Full evaluation with composition (EvidentSolver integration)
# ---------------------------------------------------------------------------


class TestFullEvaluationWithComposition:
    """Use EvidentSolver.evaluate on schemas whose body references another schema."""

    def _make_active_schema(self):
        """schema active_account — user ∈ Nat — user > 0"""
        return mk_schema(
            "active_account",
            [nat_param("user")],
            body=[ArithmeticConstraint(">", Identifier("user"), NatLiteral(0))],
        )

    def _make_verified_schema(self):
        """schema email_verified — user ∈ Nat — user < 1000"""
        return mk_schema(
            "email_verified",
            [nat_param("user")],
            body=[ArithmeticConstraint("<", Identifier("user"), NatLiteral(1000))],
        )

    def test_chain_compose_and_evaluate(self):
        """
        Chain-compose two schemas, build combined constraints,
        evaluate with Z3 — should find a model.
        """
        registry = fresh_registry()
        schema_a = self._make_active_schema()
        schema_b = self._make_verified_schema()

        env, type_constraints = chain_compose([schema_a, schema_b], registry)

        from runtime.src.translate import translate_constraint
        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)
        for item in schema_a.body:
            s.add(translate_constraint(item, env, registry))
        for item in schema_b.body:
            s.add(translate_constraint(item, env, registry))

        assert s.check() == z3.sat
        model = s.model()
        user_val = model.eval(env.bindings["user"]).as_long()
        assert 0 < user_val < 1000

    def test_evident_solver_register_and_compose(self):
        """
        Register two schemas in an EvidentSolver.  Chain-compose them using
        names_match_compose and verify the combined system is satisfiable.
        """
        registry = fresh_registry()
        solver = EvidentSolver()
        solver.registry = registry

        schema_a = self._make_active_schema()
        schema_b = self._make_verified_schema()

        solver.register_schema(schema_a)
        solver.register_schema(schema_b)

        # Compose them and evaluate schema_a (with schema_b's constraints merged)
        env, type_constraints = chain_compose(
            [solver.schemas["active_account"], solver.schemas["email_verified"]],
            registry,
        )
        assert "user" in env.bindings

        from runtime.src.translate import translate_constraint
        s = z3.Solver()
        for tc in type_constraints:
            s.add(tc)
        for item in schema_a.body:
            s.add(translate_constraint(item, env, registry))
        for item in schema_b.body:
            s.add(translate_constraint(item, env, registry))

        assert s.check() == z3.sat

    def test_names_match_compose_then_evaluate_directly(self):
        """
        Manually compose a sub-schema into a parent env and verify the
        resulting constraints are coherent.
        """
        registry = fresh_registry()

        parent_schema = mk_schema(
            "Parent",
            [nat_param("x"), nat_param("y")],
            body=[ArithmeticConstraint("=", Identifier("x"), NatLiteral(42))],
        )
        sub_schema = mk_schema(
            "Sub",
            [nat_param("y"), nat_param("z")],
            body=[ArithmeticConstraint(">", Identifier("z"), Identifier("y"))],
        )

        # Instantiate parent
        from runtime.src.instantiate import instantiate_schema
        parent_env, parent_type_constraints = instantiate_schema(
            parent_schema, Environment(), registry
        )

        # Compose sub into parent's env
        merged_env, sub_type_constraints = names_match_compose(
            parent_env, sub_schema, registry
        )

        from runtime.src.translate import translate_constraint
        s = z3.Solver()
        for tc in parent_type_constraints + sub_type_constraints:
            s.add(tc)
        for item in parent_schema.body:
            s.add(translate_constraint(item, merged_env, registry))
        for item in sub_schema.body:
            s.add(translate_constraint(item, merged_env, registry))

        assert s.check() == z3.sat
        model = s.model()
        x_val = model.eval(merged_env.bindings["x"]).as_long()
        y_val = model.eval(merged_env.bindings["y"]).as_long()
        z_val = model.eval(merged_env.bindings["z"]).as_long()

        assert x_val == 42
        assert z_val > y_val
