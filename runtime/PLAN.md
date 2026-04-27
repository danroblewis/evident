# Evident Runtime Implementation Plan

The runtime takes a parsed Evident program (AST from the parser) and evaluates it
using Z3 as the constraint solver. This document is the sequenced implementation plan.

---

## Architecture overview

```
AST
 ↓
Elaborator          -- resolves schema references, names-match composition
 ↓
Encoder             -- translates elaborated AST → Z3 assertions
 ↓
Z3 Solver / Fixedpoint
 ↓
Model Extractor     -- Z3 model → Evident evidence terms
 ↓
Evidence            -- structured derivation returned to user
```

Each layer has its own module and its own test suite. Phases are sequential —
each phase depends on the previous being correct and tested.

---

## Phase 1: Z3 infrastructure and type system

**Goal**: Create Z3 sorts for every Evident type. This is the foundation everything
else builds on.

**Files**:
- `runtime/src/sorts.py` — sort registry: `Nat → IntSort`, `String → StringSort`,
  custom types → `DeclareSort`, algebraic types → `Datatype`
- `runtime/tests/test_sorts.py`

**Type mappings**:
| Evident type | Z3 encoding |
|---|---|
| `Nat` | `IntSort()` (constrained ≥ 0) |
| `Int` | `IntSort()` |
| `Real` | `RealSort()` |
| `Bool` | `BoolSort()` |
| `String` | `StringSort()` |
| `Set T` | `ArraySort(T_sort, BoolSort())` |
| `(A, B)` | `TupleSort([A_sort, B_sort])` |
| `A × B` | Same as `(A, B)` |
| Custom `schema Task` | `DeclareSort("Task")` |
| Algebraic `Color = Red \| Green \| Blue` | `Datatype("Color", ...)` |

**Tests**: For every type, assert the sort is correct. Assert `Set Nat` encodes as
`ArraySort(IntSort(), BoolSort())`. Assert algebraic types have the right constructors.

**Agent**: Can be written independently, no dependencies.

---

## Phase 2: Variable creation and schema instantiation

**Goal**: Given a schema and a set of bindings (some variables may be concrete,
some unbound), create the corresponding Z3 variables.

**Files**:
- `runtime/src/env.py` — `Environment`: maps variable names → Z3 exprs
- `runtime/src/instantiate.py` — `instantiate_schema(schema, env)`: creates fresh
  Z3 constants for unbound variables, concrete values for bound ones
- `runtime/tests/test_instantiate.py`

**Key idea**: An unbound `x ∈ Nat` becomes a Z3 `Int("x")`. A bound `x = 5`
becomes `IntVal(5)`. The environment tracks both.

**Tests**: Instantiate a simple schema (`Task` with `id ∈ Nat, duration ∈ Nat`).
Assert that unbound variables become Z3 `Const` nodes. Assert bound variables
become `IntVal`/`StringVal` etc.

---

## Phase 3: Basic constraint translation

**Goal**: Translate arithmetic and membership constraints to Z3 assertions.

**Files**:
- `runtime/src/translate.py` — `translate_constraint(constraint, env) → z3.BoolRef`
- `runtime/tests/test_translate.py`

**Constraints handled in this phase**:
- `ArithmeticConstraint`: `a = b` → `a == b`, `a ≤ b` → `a <= b`, etc.
- `MembershipConstraint` for primitive types: `x ∈ Nat` → `x >= 0`
- `LogicConstraint`: `¬P` → `Not(P)`, `P ∧ Q` → `And(P, Q)`, `P ∨ Q` → `Or(P, Q)`
- `BindingConstraint`: `x = expr` → `x == translate_expr(expr)`

**Tests**: For each constraint form, build the Z3 assertion and call `solver.check()`.
Simple satisfiable cases (should return `sat`) and unsatisfiable cases (should return
`unsat`). E.g.:
```python
# x ∈ Nat, x > 5, x < 3 → unsat
# x ∈ Nat, x > 5, x < 10 → sat, model has x in (5, 10)
```

---

## Phase 4: Set encoding

**Goal**: Encode Evident sets using Z3's Array theory. This is the core of the
set-theoretic model.

**Files**:
- `runtime/src/sets.py` — set operations as Z3 Array operations
- `runtime/tests/test_sets.py`

**Encodings**:
| Evident | Z3 |
|---|---|
| `x ∈ S` | `S[x]` (array select) |
| `x ∉ S` | `Not(S[x])` |
| `S ⊆ T` | `ForAll([x], Implies(S[x], T[x]))` |
| `S ∪ T` | `λx. Or(S[x], T[x])` |
| `S ∩ T` | `λx. And(S[x], T[x])` |
| `S \ T` | `λx. And(S[x], Not(T[x]))` |
| `{a, b, c}` | `Store(Store(EmptySet, a, True), b, True)` |
| `{}` | `K(sort, False)` (constant false array) |
| `\|S\|` | Requires finite domain — use PbEq or count trick |

**Key challenge**: Cardinality `|S|` requires finite domains or Pseudo-Boolean
constraints. For truly infinite domains (all Nat), cardinality is not directly
expressible in Z3. Handle by either: (a) bounding the domain, or (b) using
`z3.PbEq` for cardinality over enumerated elements.

**Tests**: Assert membership, subset, union, intersection. Assert `{1,2,3}` contains
1, 2, 3 and not 4. Assert `|{1,2,3}|` = 3 (with bounded domain).

---

## Phase 5: Set comprehensions and filter sugar

**Goal**: Translate `{ x ∈ S | P(x) }` and `S[condition]` to Z3.

**Files**:
- `runtime/src/comprehension.py`
- `runtime/tests/test_comprehension.py`

**Encodings**:
- `{ x ∈ S | P(x) }` → `λx. And(S[x], P(x))` — intersection of S with {x | P(x)}
- `S[.field = v]` → `{ x ∈ S | x.field = v }` — desugared to comprehension
- `S.field` → set image: `{ x.field | x ∈ S }` — requires quantifier

**Tests**: Build comprehensions, check that filtered sets contain correct elements.
Test `S[.x > 5]` over a concrete finite set.

---

## Phase 6: Quantifiers (∀ and ∃)

**Goal**: Translate bounded universal and existential quantifiers.

**Files**:
- `runtime/src/quantifiers.py`
- `runtime/tests/test_quantifiers.py`

**Encodings**:
| Evident | Z3 |
|---|---|
| `∀ x ∈ S : P(x)` | `ForAll([x], Implies(S[x], P(x)))` |
| `∃ x ∈ S : P(x)` | `Exists([x], And(S[x], P(x)))` |
| `∃! x ∈ S : P(x)` | Unique existential — encode as count = 1 |
| `¬∃ x ∈ S : P(x)` | `Not(Exists([x], And(S[x], P(x))))` |

**Note**: Z3 quantifiers are expensive. For finite, enumerated domains, unroll
the quantifier instead. The solver should detect when a domain is concrete and
finite and choose accordingly.

**Cardinality constraints** (`at_most`, `at_least`, `exactly`, `all_different`):
- Use `z3.PbLe`, `z3.PbGe`, `z3.PbEq` for pseudo-boolean constraints
- `all_different` → `Distinct(...)` (Z3 built-in)

**Tests**: `∀ x ∈ {1,2,3} : x > 0` → sat. `∀ x ∈ {1,2,3} : x > 2` → unsat.
`∃ x ∈ {1,2,3} : x > 2` → sat (x = 3). `all_different {1,2,3}` → sat.
`all_different {1,2,1}` → unsat.

---

## Phase 7: Full schema evaluation

**Goal**: Evaluate a complete schema — all variables, all constraints, produce a
model if satisfiable.

**Files**:
- `runtime/src/solver.py` — `EvidentSolver`: wraps Z3 Solver, manages context
- `runtime/src/evaluate.py` — `evaluate_schema(schema, bindings) → Result`
- `runtime/tests/test_evaluate.py`

**`evaluate_schema` steps**:
1. Create sort for each variable type
2. Instantiate variables (bound → concrete, unbound → Z3 Const)
3. Translate each body constraint to Z3
4. Add all assertions to the solver
5. Call `solver.check()`
6. If `sat`: extract model, build result dict `{name: value}`
7. If `unsat`: return explanation (which constraints conflict)

**Tests**: Use the `.ev` test fixtures. For each valid fixture, evaluate with some
variables bound and assert the solver finds the expected values for unbound variables.
This is where the `.expected.json` files become real — compare solver output to expected.

---

## Phase 8: Schema composition (names-match, passthrough, partial application)

**Goal**: Implement the composition mechanisms that make schemas composable.

**Files**:
- `runtime/src/compose.py`
- `runtime/tests/test_compose.py`

**Names-match**: When schema A is applied inside schema B's body, variables with
matching names are identified (same Z3 variable). The composer builds a merged
environment where shared names point to the same Z3 constant.

**Pass-through** (`..schema`): All variables of the sub-schema are lifted into the
parent's environment. If a name already exists in the parent, it's identified;
otherwise it's added as a new variable.

**Partial application** (`claim editor = has_role role: "editor"`): Fix `role` to
a concrete Z3 value, leave other variables as fresh constants. The result is a
partially-evaluated schema stored for later composition.

**Chain composition** (`A · B`): Natural join — create a merged environment, add
assertions from both A and B, shared variables are identified.

**Tests**: Compose two schemas with a shared variable `user`. Assert that the merged
system correctly constrains `user` from both. Test pass-through with `..`. Test
partial application by fixing one variable and querying the other.

---

## Phase 9: Forward implications and fixpoint (Z3 Fixedpoint)

**Goal**: Implement the fixpoint computation for forward implications
(`condition ⇒ claim`) and recursive schemas (like `reachable`).

**Files**:
- `runtime/src/fixedpoint.py` — `FixedpointSolver`: wraps `z3.Fixedpoint`
- `runtime/tests/test_fixedpoint.py`

**Approach**: Z3's `Fixedpoint` engine implements Datalog / bottom-up evaluation.
Forward rules map directly:
```python
fp = z3.Fixedpoint()
fp.register_relation(reachable)
fp.add_rule(reachable(a, a), node(a))             # node n ⇒ reachable n n
fp.add_rule(reachable(a, c), [reachable(a, b), adjacent(b, c)])
```

**Integration**: The main solver uses `EvidentSolver` for most constraints and
delegates to `FixedpointSolver` for schemas defined by forward rules. The two
communicate through the evidence base: facts derived by fixpoint become available
to the main solver and vice versa.

**Tests**: The `reachable` schema from fixture 10. The graph hierarchy from example
14. Assert that the fixpoint correctly computes transitive closure.

---

## Phase 10: Evidence terms

**Goal**: When the solver finds a satisfying assignment, build a structured evidence
term — the derivation tree showing how each claim was established.

**Files**:
- `runtime/src/evidence.py` — `Evidence`, `EvidenceTree`
- `runtime/tests/test_evidence.py`

**Evidence structure**:
```python
@dataclass
class Evidence:
    claim: str
    variables: dict[str, Any]   # the variable assignments
    sub_evidence: list[Evidence] # how sub-claims were established
```

**Model extraction**: After `solver.check() == sat`, call `solver.model()` to
get the Z3 model. Walk the model to extract concrete values for each variable.
Recursively collect sub-evidence from claims that were evaluated to establish
this one.

**Tests**: Evaluate `sorted [1, 2, 3]`. Assert the evidence tree contains the
consecutive-pair witnesses. Evaluate `valid_conference` with the conference data.
Assert the evidence tree shows which slot each talk was assigned to.

---

## Phase 11: Algebraic types and pattern matching

**Goal**: Encode algebraic/sum types and translate `evident` blocks (pattern matching).

**Files**:
- `runtime/src/algebraic.py`
- `runtime/tests/test_algebraic.py`

**Z3 Datatype encoding**:
```python
Color = Datatype("Color")
Color.declare("Red")
Color.declare("Green")
Color.declare("Blue")
Color = Color.create()
```

**Pattern matching** (`evident` blocks): Each `evident` block becomes a conditional
constraint — enabled when the pattern matches. For list patterns (`[x | rest]`),
use Z3 datatypes for the list structure.

---

## Phase 12: Full integration and query evaluation

**Goal**: Connect parser → elaborator → encoder → solver → evidence. Handle `?`
queries, `assert` statements, and the evidence base lifecycle.

**Files**:
- `runtime/src/runtime.py` — `EvidentRuntime`: the top-level interface
- `runtime/src/session.py` — session state (evidence base, asserted facts)
- `runtime/tests/test_integration.py`

**`EvidentRuntime` API**:
```python
rt = EvidentRuntime()
rt.load("schema sorted ...")           # load schema definitions
rt.assert_fact("x = 5")               # assert ground facts
result = rt.query("? sorted [3,1,2]") # evaluate a query
print(result.satisfied)                # True/False
print(result.evidence)                 # the derivation tree
print(result.bindings)                 # { "list": [1,2,3] }
```

**Integration tests**: Run all 20 valid fixtures through the full pipeline. For
each, assert the solver finds a valid result. For fixtures with `.expected.json`,
assert the evidence matches expectations.

---

## Phase 13: Performance and robustness

**Goal**: Make the solver fast enough to be useful and robust against edge cases.

**Concerns**:
- Quantifiers over large domains are expensive — add domain bounding
- Cardinality of infinite sets — require explicit domain declarations
- Recursive schemas without tabling can loop — add memoization
- Z3 timeouts — add configurable timeout with graceful `unknown` result

**Files**:
- `runtime/src/optimize.py` — domain inference, quantifier unrolling
- `runtime/tests/test_performance.py`

---

## Parallelization opportunities

Some phases can run in parallel with agents:

| Parallel group | Phases |
|---|---|
| A | Phase 1 (sorts) + Phase 2 (instantiation) |
| B (after A) | Phase 3 (basic constraints) + Phase 4 (sets) + Phase 11 (algebraic) |
| C (after B) | Phase 5 (comprehensions) + Phase 6 (quantifiers) + Phase 9 (fixedpoint) |
| D (after C) | Phase 7 (full schema) — needs all of the above |
| E (after D) | Phase 8 (composition) + Phase 10 (evidence) |
| F (after E) | Phase 12 (integration) |
| G (after F) | Phase 13 (performance) |

Within each group, phases are independent and can be assigned to separate agents.

---

## Directory structure

```
runtime/
  src/
    __init__.py
    sorts.py          Phase 1
    env.py            Phase 2
    instantiate.py    Phase 2
    translate.py      Phase 3
    sets.py           Phase 4
    comprehension.py  Phase 5
    quantifiers.py    Phase 6
    evaluate.py       Phase 7
    solver.py         Phase 7
    compose.py        Phase 8
    fixedpoint.py     Phase 9
    evidence.py       Phase 10
    algebraic.py      Phase 11
    runtime.py        Phase 12
    session.py        Phase 12
    optimize.py       Phase 13
  tests/
    test_sorts.py
    test_instantiate.py
    test_translate.py
    test_sets.py
    test_comprehension.py
    test_quantifiers.py
    test_evaluate.py
    test_compose.py
    test_fixedpoint.py
    test_evidence.py
    test_algebraic.py
    test_integration.py
    test_performance.py
```

---

## Definition of done

Each phase is complete when:
1. All tests in its test file pass
2. All previously passing tests still pass (no regression)
3. The integration test suite for that phase's fixtures passes
4. The code has been reviewed for correctness against the spec

The runtime is complete when all 20 valid fixtures can be evaluated end-to-end
and the solver produces correct results that match the expected outputs.
