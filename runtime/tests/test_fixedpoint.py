"""
Phase 9 tests: FixedpointSolver — forward implications via Z3 Fixedpoint.
"""

import pytest
import z3

from src.sorts import SortRegistry
from src.fixedpoint import FixedpointSolver


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

def fresh_solver(engine: str = "spacer") -> tuple[FixedpointSolver, SortRegistry]:
    registry = SortRegistry()
    fs = FixedpointSolver(registry, engine=engine)
    return fs, registry


# ---------------------------------------------------------------------------
# 1. Simple ground facts — no rules
# ---------------------------------------------------------------------------

class TestSimpleFacts:
    """Declare a relation, add ground facts, query them back."""

    def test_fact_present(self):
        fs, _ = fresh_solver()
        Int = z3.IntSort()
        fs.declare_relation("known", [Int])

        fs.add_fact("known", [z3.IntVal(42)])
        fs.add_fact("known", [z3.IntVal(99)])

        assert fs.query("known", [z3.IntVal(42)])
        assert fs.query("known", [z3.IntVal(99)])

    def test_fact_absent(self):
        fs, _ = fresh_solver()
        Int = z3.IntSort()
        fs.declare_relation("known", [Int])

        fs.add_fact("known", [z3.IntVal(42)])

        assert not fs.query("known", [z3.IntVal(7)])

    def test_binary_fact(self):
        fs, _ = fresh_solver()
        Int = z3.IntSort()
        fs.declare_relation("pair", [Int, Int])

        fs.add_fact("pair", [z3.IntVal(1), z3.IntVal(2)])

        assert fs.query("pair", [z3.IntVal(1), z3.IntVal(2)])
        assert not fs.query("pair", [z3.IntVal(2), z3.IntVal(1)])

    def test_string_fact(self):
        fs, _ = fresh_solver()
        Str = z3.StringSort()
        fs.declare_relation("tag", [Str])

        fs.add_fact("tag", [z3.StringVal("hello")])
        fs.add_fact("tag", [z3.StringVal("world")])

        assert fs.query("tag", [z3.StringVal("hello")])
        assert fs.query("tag", [z3.StringVal("world")])
        assert not fs.query("tag", [z3.StringVal("nope")])


# ---------------------------------------------------------------------------
# 2. Transitive closure — reachable
# ---------------------------------------------------------------------------

class TestTransitiveClosure:
    """
    Rules (Evident syntax):
        node n ⇒ reachable n n
        reachable a b, adjacent b c ⇒ reachable a c

    Facts: adjacent(1,2), adjacent(2,3), adjacent(3,4)
    Nodes: 1, 2, 3, 4
    """

    @pytest.fixture()
    def reachable_solver(self):
        fs, _ = fresh_solver()
        Int = z3.IntSort()

        node_rel = fs.declare_relation("node", [Int])
        adj_rel  = fs.declare_relation("adjacent", [Int, Int])
        reach_rel = fs.declare_relation("reachable", [Int, Int])

        # Facts
        for i in range(1, 5):
            fs.add_fact("node", [z3.IntVal(i)])
        fs.add_fact("adjacent", [z3.IntVal(1), z3.IntVal(2)])
        fs.add_fact("adjacent", [z3.IntVal(2), z3.IntVal(3)])
        fs.add_fact("adjacent", [z3.IntVal(3), z3.IntVal(4)])

        # Rules
        n = z3.Int("n")
        a = z3.Int("a")
        b = z3.Int("b")
        c = z3.Int("c")
        fs.add_rule("reachable", [n, n], [("node", [n])])
        fs.add_rule("reachable", [a, c], [("reachable", [a, b]), ("adjacent", [b, c])])

        return fs

    def test_direct_edge(self, reachable_solver):
        fs = reachable_solver
        assert fs.query("reachable", [z3.IntVal(1), z3.IntVal(2)])
        assert fs.query("reachable", [z3.IntVal(2), z3.IntVal(3)])
        assert fs.query("reachable", [z3.IntVal(3), z3.IntVal(4)])

    def test_transitive(self, reachable_solver):
        fs = reachable_solver
        # 1 → 2 → 3 → 4
        assert fs.query("reachable", [z3.IntVal(1), z3.IntVal(3)])
        assert fs.query("reachable", [z3.IntVal(1), z3.IntVal(4)])
        assert fs.query("reachable", [z3.IntVal(2), z3.IntVal(4)])

    def test_reflexive(self, reachable_solver):
        fs = reachable_solver
        for i in range(1, 5):
            assert fs.query("reachable", [z3.IntVal(i), z3.IntVal(i)])

    def test_not_backwards(self, reachable_solver):
        fs = reachable_solver
        assert not fs.query("reachable", [z3.IntVal(4), z3.IntVal(1)])
        assert not fs.query("reachable", [z3.IntVal(3), z3.IntVal(1)])
        assert not fs.query("reachable", [z3.IntVal(2), z3.IntVal(1)])

    def test_not_skipping_gap(self, reachable_solver):
        """Node 5 is not in the graph at all."""
        fs = reachable_solver
        assert not fs.query("reachable", [z3.IntVal(1), z3.IntVal(5)])


# ---------------------------------------------------------------------------
# 3. Ancestor — recursive family tree
# ---------------------------------------------------------------------------

class TestAncestor:
    """
    Rules:
        parent X Y ⇒ ancestor X Y
        ancestor X Y, parent Y Z ⇒ ancestor X Z

    Facts: parent(alice=1, bob=2), parent(bob=2, carol=3)

    Integer IDs are used because Z3's spacer engine has known issues with
    recursive Datalog rules over string sorts.  Encoding names as integer
    IDs is the standard Datalog idiom and exercises the same logic.
    """

    # Person ID constants
    ALICE = z3.IntVal(1)
    BOB   = z3.IntVal(2)
    CAROL = z3.IntVal(3)

    @pytest.fixture()
    def ancestor_solver(self):
        fs, _ = fresh_solver()
        Int = z3.IntSort()

        fs.declare_relation("parent",   [Int, Int])
        fs.declare_relation("ancestor", [Int, Int])

        alice, bob, carol = self.ALICE, self.BOB, self.CAROL

        fs.add_fact("parent", [alice, bob])
        fs.add_fact("parent", [bob, carol])

        X = z3.Int("anc_X")
        Y = z3.Int("anc_Y")
        Z = z3.Int("anc_Z")

        # parent(X, Y) ⇒ ancestor(X, Y)
        fs.add_rule("ancestor", [X, Y], [("parent", [X, Y])])
        # ancestor(X, Y), parent(Y, Z) ⇒ ancestor(X, Z)
        fs.add_rule("ancestor", [X, Z], [("ancestor", [X, Y]), ("parent", [Y, Z])])

        return fs

    def test_direct_parent(self, ancestor_solver):
        fs = ancestor_solver
        assert fs.query("ancestor", [self.ALICE, self.BOB])
        assert fs.query("ancestor", [self.BOB, self.CAROL])

    def test_transitive_ancestor(self, ancestor_solver):
        fs = ancestor_solver
        assert fs.query("ancestor", [self.ALICE, self.CAROL])

    def test_not_reverse(self, ancestor_solver):
        fs = ancestor_solver
        assert not fs.query("ancestor", [self.CAROL, self.ALICE])
        assert not fs.query("ancestor", [self.BOB,   self.ALICE])
        assert not fs.query("ancestor", [self.CAROL, self.BOB])

    def test_not_self(self, ancestor_solver):
        """No self-ancestor rule was added — should be False."""
        fs = ancestor_solver
        assert not fs.query("ancestor", [self.ALICE, self.ALICE])
        assert not fs.query("ancestor", [self.BOB,   self.BOB])


# ---------------------------------------------------------------------------
# 4. Multiple rules for the same relation (union semantics)
# ---------------------------------------------------------------------------

class TestMultipleRules:
    """Two separate rules both derive facts for the same relation."""

    def test_union(self):
        fs, _ = fresh_solver()
        Int = z3.IntSort()
        fs.declare_relation("even", [Int])
        fs.declare_relation("odd",  [Int])
        fs.declare_relation("interesting", [Int])

        # even(2), even(4), odd(3)
        fs.add_fact("even", [z3.IntVal(2)])
        fs.add_fact("even", [z3.IntVal(4)])
        fs.add_fact("odd",  [z3.IntVal(3)])

        n = z3.Int("n")
        # interesting(n) :- even(n)
        fs.add_rule("interesting", [n], [("even", [n])])
        # interesting(n) :- odd(n)
        fs.add_rule("interesting", [n], [("odd", [n])])

        assert fs.query("interesting", [z3.IntVal(2)])
        assert fs.query("interesting", [z3.IntVal(3)])
        assert fs.query("interesting", [z3.IntVal(4)])
        assert not fs.query("interesting", [z3.IntVal(5)])


# ---------------------------------------------------------------------------
# 5. Declare relation returns the FuncDeclRef
# ---------------------------------------------------------------------------

class TestDeclareRelation:
    def test_returns_func_decl(self):
        fs, _ = fresh_solver()
        Int = z3.IntSort()
        rel = fs.declare_relation("foo", [Int, Int])
        assert rel is not None
        assert rel is fs.relations["foo"]

    def test_idempotent_via_separate_instance(self):
        """Each FixedpointSolver is independent; two declarations in the same
        solver would be an error at the z3 level if sorts mismatch."""
        fs, _ = fresh_solver()
        Int = z3.IntSort()
        rel1 = fs.declare_relation("bar", [Int])
        # Calling add_fact should work without error.
        fs.add_fact("bar", [z3.IntVal(1)])
        assert fs.query("bar", [z3.IntVal(1)])
