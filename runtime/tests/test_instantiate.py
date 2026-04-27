"""
Tests for Phase 2: Environment and schema instantiation.
"""
import sys
import os
import pytest
import z3

# Make the runtime src/ importable
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from src.env import Environment
from src.instantiate import instantiate_schema, make_const, type_constraint
from src.ast_types import SchemaDecl, Param, Identifier


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def make_nat_param(*names: str) -> Param:
    """Return a Param that declares ``names ∈ Nat``."""
    return Param(names=list(names), set=Identifier(name="Nat"))


def make_type_param(type_name: str, *names: str) -> Param:
    """Return a Param that declares ``names ∈ type_name``."""
    return Param(names=list(names), set=Identifier(name=type_name))


def task_schema() -> SchemaDecl:
    """A simple Task schema: ``id ∈ Nat, duration ∈ Nat``."""
    return SchemaDecl(
        keyword="schema",
        name="Task",
        params=[make_nat_param("id"), make_nat_param("duration")],
        body=[],
    )


# ---------------------------------------------------------------------------
# make_const
# ---------------------------------------------------------------------------

class TestMakeConst:
    def test_returns_int_const_for_int_sort(self):
        c = make_const("x", z3.IntSort())
        assert z3.is_int(c)

    def test_name_without_prefix(self):
        c = make_const("foo", z3.IntSort())
        assert c.decl().name() == "foo"

    def test_name_with_prefix(self):
        c = make_const("bar", z3.IntSort(), prefix="task_")
        assert c.decl().name() == "task_bar"

    def test_returns_bool_const_for_bool_sort(self):
        c = make_const("flag", z3.BoolSort())
        assert z3.is_bool(c)


# ---------------------------------------------------------------------------
# type_constraint
# ---------------------------------------------------------------------------

class TestTypeConstraint:
    def test_nat_produces_ge_zero(self):
        x = z3.Int("x")
        constraints = type_constraint(x, "Nat")
        assert len(constraints) == 1
        # The constraint should be satisfied when x = 1 and violated when x = -1
        s = z3.Solver()
        s.add(constraints)
        s.add(x == 1)
        assert s.check() == z3.sat
        s2 = z3.Solver()
        s2.add(constraints)
        s2.add(x == -1)
        assert s2.check() == z3.unsat

    def test_int_produces_no_constraints(self):
        x = z3.Int("x")
        assert type_constraint(x, "Int") == []

    def test_bool_produces_no_constraints(self):
        b = z3.Bool("b")
        assert type_constraint(b, "Bool") == []

    def test_custom_type_produces_no_constraints(self):
        s = z3.DeclareSort("User")
        u = z3.Const("u", s)
        assert type_constraint(u, "User") == []


# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------

class TestEnvironment:
    def test_empty_env_lookup_returns_none(self):
        env = Environment()
        assert env.lookup("x") is None

    def test_bind_and_lookup(self):
        env = Environment()
        x = z3.Int("x")
        env2 = env.bind("x", x)
        assert z3.eq(env2.lookup("x"), x)

    def test_bind_returns_new_env(self):
        env = Environment()
        env2 = env.bind("x", z3.Int("x"))
        # Original is unchanged
        assert env.lookup("x") is None

    def test_is_bound(self):
        env = Environment().bind("x", z3.Int("x"))
        assert env.is_bound("x")
        assert not env.is_bound("y")

    def test_lookup_checks_parent(self):
        x = z3.Int("x")
        parent = Environment().bind("x", x)
        child = Environment(parent=parent)
        assert z3.eq(child.lookup("x"), x)

    def test_lookup_child_shadows_parent(self):
        x_parent = z3.Int("x_parent")
        x_child = z3.Int("x_child")
        parent = Environment().bind("x", x_parent)
        child = Environment(parent=parent).bind("x", x_child)
        assert z3.eq(child.lookup("x"), x_child)

    def test_merge_disjoint(self):
        x = z3.Int("x")
        y = z3.Int("y")
        env_a = Environment().bind("x", x)
        env_b = Environment().bind("y", y)
        merged = env_a.merge(env_b)
        assert z3.eq(merged.lookup("x"), x)
        assert z3.eq(merged.lookup("y"), y)

    def test_merge_same_binding_ok(self):
        x = z3.Int("x")
        env_a = Environment().bind("x", x)
        env_b = Environment().bind("x", x)
        merged = env_a.merge(env_b)
        assert z3.eq(merged.lookup("x"), x)

    def test_merge_conflicting_bindings_raises(self):
        env_a = Environment().bind("x", z3.IntVal(1))
        env_b = Environment().bind("x", z3.IntVal(2))
        with pytest.raises(ValueError, match="incompatible"):
            env_a.merge(env_b)

    def test_merge_shared_name_same_const(self):
        """After merge, shared names reference the same Z3 object."""
        user = z3.Const("user", z3.DeclareSort("User"))
        env_a = Environment().bind("user", user)
        env_b = Environment().bind("user", user)
        merged = env_a.merge(env_b)
        # Must be the identical Python / Z3 object
        assert merged.lookup("user") is user


# ---------------------------------------------------------------------------
# instantiate_schema — no prior bindings
# ---------------------------------------------------------------------------

class TestInstantiateSchemaFresh:
    def setup_method(self):
        try:
            from src.sorts import SortRegistry
        except ImportError:
            from src.instantiate import SortRegistry
        self.registry = SortRegistry()

    def test_creates_int_constants_for_nat_params(self):
        schema = task_schema()
        env, constraints = instantiate_schema(schema, Environment(), self.registry)

        id_var = env.lookup("id")
        dur_var = env.lookup("duration")

        assert id_var is not None, "id should be bound"
        assert dur_var is not None, "duration should be bound"
        assert z3.is_int(id_var), "id should be Z3 Int"
        assert z3.is_int(dur_var), "duration should be Z3 Int"

    def test_variables_are_named_correctly(self):
        schema = task_schema()
        env, _ = instantiate_schema(schema, Environment(), self.registry)

        assert env.lookup("id").decl().name() == "id"
        assert env.lookup("duration").decl().name() == "duration"

    def test_nat_type_constraints_returned(self):
        schema = task_schema()
        _, constraints = instantiate_schema(schema, Environment(), self.registry)

        # Two Nat params → two >= 0 constraints
        assert len(constraints) == 2

    def test_nat_constraints_are_satisfiable(self):
        schema = task_schema()
        env, constraints = instantiate_schema(schema, Environment(), self.registry)

        s = z3.Solver()
        s.add(constraints)
        assert s.check() == z3.sat

    def test_nat_constraints_reject_negatives(self):
        schema = task_schema()
        env, constraints = instantiate_schema(schema, Environment(), self.registry)

        s = z3.Solver()
        s.add(constraints)
        s.add(env.lookup("id") == -1)
        assert s.check() == z3.unsat

    def test_prefix_applied_to_const_names(self):
        schema = task_schema()
        env, _ = instantiate_schema(
            schema, Environment(), self.registry, prefix="task1_"
        )
        assert env.lookup("id").decl().name() == "task1_id"
        assert env.lookup("duration").decl().name() == "task1_duration"


# ---------------------------------------------------------------------------
# instantiate_schema — with pre-bound variables
# ---------------------------------------------------------------------------

class TestInstantiateSchemaWithGiven:
    def setup_method(self):
        from src.instantiate import SortRegistry
        self.registry = SortRegistry()

    def test_pre_bound_id_is_used(self):
        schema = task_schema()
        given = Environment().bind("id", z3.IntVal(5))
        env, constraints = instantiate_schema(schema, given, self.registry)

        id_var = env.lookup("id")
        assert z3.is_int(id_var)
        # IntVal(5) is a concrete value — simplify should give 5
        assert z3.simplify(id_var).as_long() == 5

    def test_unbound_duration_is_fresh_const(self):
        schema = task_schema()
        given = Environment().bind("id", z3.IntVal(5))
        env, _ = instantiate_schema(schema, given, self.registry)

        dur = env.lookup("duration")
        assert dur is not None
        assert z3.is_int(dur)
        # Should be a symbolic constant, not a concrete value
        assert z3.is_const(dur)

    def test_nat_constraint_on_pre_bound_id(self):
        """A pre-bound id=5 still gets a x>=0 constraint (5 >= 0 is trivially true)."""
        schema = task_schema()
        given = Environment().bind("id", z3.IntVal(5))
        env, constraints = instantiate_schema(schema, given, self.registry)

        # Two constraints expected: one for id (IntVal(5) >= 0) and one for duration
        assert len(constraints) == 2
        s = z3.Solver()
        s.add(constraints)
        assert s.check() == z3.sat

    def test_both_vars_pre_bound(self):
        schema = task_schema()
        given = (
            Environment()
            .bind("id", z3.IntVal(3))
            .bind("duration", z3.IntVal(7))
        )
        env, constraints = instantiate_schema(schema, given, self.registry)
        assert z3.simplify(env.lookup("id")).as_long() == 3
        assert z3.simplify(env.lookup("duration")).as_long() == 7
        assert len(constraints) == 2


# ---------------------------------------------------------------------------
# Shared variable unification across two schemas
# ---------------------------------------------------------------------------

class TestSharedVariableMerge:
    """Two schemas that share a variable name should reference the same Z3 const."""

    def setup_method(self):
        from src.instantiate import SortRegistry
        self.registry = SortRegistry()

    def _user_schema(self, name: str) -> SchemaDecl:
        return SchemaDecl(
            keyword="schema",
            name=name,
            params=[make_type_param("User", "user")],
            body=[],
        )

    def test_shared_user_variable_after_merge(self):
        schema_a = self._user_schema("SchemaA")
        schema_b = self._user_schema("SchemaB")

        env_a, _ = instantiate_schema(schema_a, Environment(), self.registry)
        # Pass env_a as the given bindings for schema_b so the shared name unifies
        env_b, _ = instantiate_schema(schema_b, env_a, self.registry)

        # Both should reference the same Z3 constant (same object or eq)
        user_a = env_a.lookup("user")
        user_b = env_b.lookup("user")
        assert z3.eq(user_a, user_b), (
            f"Expected user to be the same Z3 constant; got {user_a} and {user_b}"
        )

    def test_merge_after_separate_instantiation_raises_on_conflict(self):
        """If two schemas produce different constants for 'user', merging raises."""
        schema_a = self._user_schema("SchemaA")
        schema_b = self._user_schema("SchemaB")

        # Instantiate independently — each gets its own fresh constant
        env_a, _ = instantiate_schema(
            schema_a, Environment(), self.registry, prefix="a_"
        )
        env_b, _ = instantiate_schema(
            schema_b, Environment(), self.registry, prefix="b_"
        )

        # The two 'user' constants have different names so they are distinct
        user_a = env_a.lookup("user")
        user_b = env_b.lookup("user")
        assert not z3.eq(user_a, user_b), (
            "Sanity check: independently created constants should be distinct"
        )
        with pytest.raises(ValueError, match="incompatible"):
            env_a.merge(env_b)

    def test_merge_after_separate_instantiation_succeeds_with_same_const(self):
        """Merging is fine when both environments hold the identical Z3 object."""
        user = z3.Const("user", z3.DeclareSort("User"))

        env_a = Environment().bind("user", user).bind("a_field", z3.Int("a_field"))
        env_b = Environment().bind("user", user).bind("b_field", z3.Int("b_field"))

        merged = env_a.merge(env_b)
        assert merged.lookup("user") is user
        assert z3.is_int(merged.lookup("a_field"))
        assert z3.is_int(merged.lookup("b_field"))
