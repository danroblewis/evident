# The Evident Testing Framework

## The Origin

Evident started from "what if we only wrote tests?" Constraint schemas
are a reasonable answer: a schema *is* a test — it specifies what valid
configurations look like, and the solver either finds one or proves none
exist.

The question for the testing framework is narrower: how do you test an
Evident *program*? Not the solver (that's Z3's job), but the models and
claims you write.

---

## Keyword Conventions

`schema`, `claim`, and `type` are syntactically identical — all produce
a `SchemaDecl`, all queryable, all usable as membership targets. The
distinction is a convention for the reader:

- `type` — algebraic or enum declarations
  ```evident
  type Direction = North | South | East | West
  ```

- `schema` — structural models and programs; the "what does a valid X
  look like" question
  ```evident
  schema ValidSchedule
      slot   ∈ Nat
      budget ∈ Nat
      slot > 0
      slot ≤ budget
  ```

- `claim` — assertions and properties; the "is this possible or
  impossible" question
  ```evident
  claim no_slot_exceeds_budget
      s ∈ ValidSchedule
      s.slot > s.budget
  ```

These conventions aren't enforced by the runtime — they're a shared
reading contract.

---

## Test Files

Tests live in separate `.ev` files, not embedded in the model. A test
file imports the schemas it tests and makes claims about them.

Discovery convention: `evident test` looks for files matching
`test_*.ev` in the current directory and any `tests/` subdirectory.
You can also pass a path explicitly.

---

## sat\_ and unsat\_

A test file is a collection of claims. Each claim has a naming prefix
that declares the expected outcome:

- `sat_*` — the solver must find at least one satisfying assignment
- `unsat_*` — the solver must find no satisfying assignment

```evident
import "../schedule.ev"

claim sat_valid_schedule_exists
    s ∈ ValidSchedule
    s.slot = 5

claim unsat_slot_exceeds_budget
    s ∈ ValidSchedule
    s.slot > s.budget
```

The runner queries each claim and reports pass/fail against the expected
outcome.

### Why both are needed

`unsat_` is not just sugar for `sat_` with a negated body. They're
different ways of writing the claim.

The `unsat_` form lets you describe a scenario directly — you write the
bad state and assert it can't exist:

```evident
claim unsat_dungeon_reachable_without_torch
    s0 ∈ GameState
    s0.location  = Entrance
    s0.inventory = ⟨⟩

    t1 ∈ GameTransition
    t1.state = t1.next

    t2 ∈ GameTransition
    t2.state = t1.next

    t3 ∈ GameTransition
    t3.state = t2.next

    t3.next.location = Dungeon
```

The equivalent `sat_` form requires wrapping the entire scenario in a
negated existential — every variable moves into a `¬∃` binder list,
every constraint becomes a conjunct. Adding a step means editing two
places. At ten steps the expression has to be held in your head all at
once instead of read line by line.

The `unsat_` form reads like a scenario description. The `sat_` form
reads like a formula. Both are valid; they express the same fact
differently. The naming prefix tells the runner which outcome means
"pass."

---

## What `evident test` Does

```
evident test [path]
```

1. Discover all `test_*.ev` files at the given path (default: current
   directory and `tests/`)
2. Load each file
3. For each schema whose name starts with `sat_`: query it, expect SAT
4. For each schema whose name starts with `unsat_`: query it, expect UNSAT
5. Report pass/fail per claim, with a summary at the end

Each claim is an independent Z3 call. No state is shared between claims.
Adding more claims to a test file does not affect the solving time of
any individual claim.

---

## What This Does Not Cover (Yet)

**Sampling / property tests**: `evident sample` already generates
diverse solutions. Integrating sampling into the test runner — "for
N samples from this schema, verify this property holds" — is a natural
extension but not in scope for the first version.

**Distribution tests**: Verifying the *shape* of the solution space
(mean, coverage, correlation). Requires a statistics layer. Deferred.

**Execution traces**: Testing streaming programs (`schema main` with
`..LineReader`/`..LineWriter`) by feeding them a sequence of inputs and
checking outputs. The design direction is to express traces as Evident
schemas over `Seq(State) × Seq(Output)`, using the constraint system
as the oracle. Not yet implemented.

---

## What You Don't Need

- Hand-written unit tests for individual functions — the solver is
  correct by construction
- Mock objects — constraints abstract over values
- Test fixtures with hardcoded data — write a `sat_` claim with the
  specific values you want to check

The schema *is* the specification. The `sat_`/`unsat_` claims verify
that the specification is shaped the way you intended.
