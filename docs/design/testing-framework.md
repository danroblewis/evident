# The Evident Testing Framework

## The Origin Question

Evident started from "what if we only wrote tests?" Constraint systems
are a reasonable answer: a constraint schema *is* a test — it specifies
what valid configurations look like, and the solver either finds one or
proves none exist. The sampler generates test vectors automatically.
You never write test cases by hand.

If that's true, then testing Evident programs raises an uncomfortable
question: **what does it mean to test a program when the program IS a
constraint system?**

---

## The Inversion Problem

In conventional software:

```
write program → write tests → run tests → pass/fail
```

In Evident, the constraint schema *is already the specification*. The
solver guarantees every sample is valid. Every output of `ev-nl` that
the executor produces already satisfies the step constraints by
construction. So what is there to test?

Three answers, at different levels:

**Test that the schema itself is right.**
Did you write the constraint model you intended? The schema might be
satisfiable when it should be UNSAT, or too loose (allows things you
didn't mean to allow), or too tight (rules out things you wanted to
allow). This is the meta-test: testing the specification, not the
implementation.

**Test that the program behaves correctly end-to-end.**
The streaming executor, the batch executor, the shims — these are
implementation. They could have bugs that let an invalid output slip
through. You test by feeding a program's output back into the
specification schema and checking it satisfies it. The constraint
system is the oracle.

**Test statistical properties of the solution space.**
The schema might be satisfiable but badly shaped — most solutions
cluster near one boundary, certain enum values almost never appear,
variables that should be independent are correlated. These are real
problems that constraint tests don't catch. You need sampling.

---

## Four Layers of Testing

### Layer 1 — Constraint Tests (deterministic, one solver call)

Assert that a schema is SAT or UNSAT. Assert specific binding values.
These are what the conformance suite does now. Fast, exact, run on
every commit.

```evident
schema ValidSchedule
    -- @test sat
    -- @test given slot=5 budget=10: sat
    -- @test given slot=20 budget=5: unsat
    slot   ∈ Nat
    budget ∈ Nat
    slot > 0
    slot ≤ budget
```

What they catch: outright specification errors. A schema that should
be UNSAT but isn't. Specific cases that must hold.

What they don't catch: the shape of the solution space.

---

### Layer 2 — Property Tests (statistical, N sampler calls)

Sample N solutions and verify a property holds for some fraction of
them. The property is itself an Evident constraint evaluated against
each sample.

```evident
schema ValidSchedule
    -- @property slot > 0              -- universal: must hold for all samples
    -- @property budget ≥ 10           p=0.9   -- must hold for ≥90% of samples
    -- @property task.duration < 5     p=0.7   -- must hold for ≥70% of samples
    slot           ∈ Nat
    budget         ∈ Nat
    task           ∈ Task
    slot > 0
    slot + task.duration ≤ budget
```

Two modes:

**Universal properties** (`p=1.0`, default): must hold for every
sample. These are like type invariants — if a single sample violates
them, the schema is wrong.

**Statistical properties** (`p=k`): must hold for at least k fraction
of samples. These express preferences, typical ranges, expected
coverage. A budget that's usually above 10 but occasionally isn't.

Mechanically this is a one-sided binomial test. Given N samples where
k satisfy the property, we estimate P̂ = k/N and compare to the
threshold. With enough samples this gives a confidence interval, not
just a pass/fail.

What they catch: schemas that are satisfiable but have the wrong
distribution. Too many solutions near a boundary. Bias toward certain
values. Properties that hold most of the time but not always.

---

### Layer 3 — Distribution Tests (statistical, N calls + statistics)

Instead of testing individual samples, test statistical properties of
the distribution itself. The IDE's scatter plot already collects this
data — distribution tests make it machine-checkable.

```evident
schema ValidSchedule
    -- @distribution E[budget] ≈ 50 ± 20    -- mean of budget
    -- @distribution P(slot > budget/2) ≥ 0.4  -- tail probability
    -- @distribution cov(slot, budget) > 0.5   -- positively correlated
    -- @distribution coverage(task.id)          -- all values appear
    slot   ∈ Nat
    budget ∈ Nat
    task   ∈ Task
```

Available statistics:

| Annotation | Meaning |
|---|---|
| `E[x] ≈ v ± d` | Mean of x is within d of v |
| `P(x > k) ≥ p` | Tail probability |
| `cov(x, y) ≈ r` | Correlation coefficient |
| `coverage(x)` | Every declared value of x appears |
| `stdev(x) ≤ d` | Standard deviation bounded |

What they catch: the solution space has the right shape. Variables that
should be independent aren't (the schema is over-constrained in a
subtle way). Enum coverage is correct. The expected value of a quantity
is in the right range.

This is where Evident testing becomes genuinely novel — most testing
frameworks don't have a concept of distribution shape as a test.

---

### Layer 4 — Execution Tests (for programs, finite automata traces)

Test streaming programs as state machines. An execution test is a
sequence of input steps and expected output steps.

```evident
schema main
    -- @test input="hello\nworld\n" output="1\thello\n2\tworld\n"
    -- @test input="" output=""
    -- @test input="only" output="1\tonly\n"
    -- @property satisfies NumberedDocument
    src ∈ StdinLines
    dst ∈ StdoutLines
    nd  ∈ NumberedDocument
    nd.contents = src.lines
    nd.lines    = dst.lines
```

Two forms:

**Exact trace**: given this exact stdin, expect this exact stdout.
These are the conventional integration tests — fast, deterministic,
easy to write.

**Contract property**: all outputs must satisfy some schema. The
`-- @property satisfies NumberedDocument` annotation says: for any
valid input, the program's output must be a member of `NumberedDocument`.
The test runner generates random inputs, runs the program, and feeds
the output into `NumberedDocument` as `given`. If that query is SAT,
the test passes.

This is **contract testing expressed in the same language as the
program**. You're using one constraint system to verify another. No
hardcoded expected values needed.

---

## The "We Only Write Tests" Insight

If the original insight was "what if we only wrote tests", then Evident
constraint schemas *are* the tests. A schema is the most precise test
you can write — it defines exactly what's valid, the solver verifies it,
and the sampler generates examples automatically.

This changes the testing hierarchy:

**Conventional stack:**
```
Unit tests
Integration tests
End-to-end tests
```

**Evident stack:**
```
Constraint schemas (= specification = "unit tests" at the spec level)
Property tests     (= does the schema have the right distribution?)
Execution tests    (= does the program satisfy the schema?)
Distribution tests (= is the solution space shaped correctly?)
```

The inversion: the "unit test" level is the constraint schema itself.
Everything above it is testing the schema's shape and the program's
adherence to it.

**What you probably don't need:**
- Hand-written unit tests for individual functions — the solver is
  correct by construction
- Mock objects — the constraint system abstracts over values
- Test fixtures with hardcoded data — the sampler generates them

**What you do need:**
- Layer 1: correctness of the schema (SAT/UNSAT checks)
- Layer 3: shape of the solution space (distribution tests)
- Layer 4: the executor correctly implements the schema (execution tests)
- Statistical rigor for Layers 2 and 3

**The level to test at:**

Property tests and end-to-end execution tests are the sweet spot. Unit
tests (in the conventional sense) would be testing the solver, which
is Z3's job. Integration tests are execution tests. The unique thing
Evident adds is Layer 2 and 3 — testing the *distribution*, not just
individual samples. That's the place to invest.

---

## IDE Integration

Tests live as annotations in the source file. They travel with the
schema. The IDE can surface them in several ways:

**Test panel**: shows pass/fail for all annotations, updates live as
samples arrive.

**Scatter plot overlay**: points that violate a `-- @property`
annotation are highlighted red. The scatter plot becomes a visual
property test.

**Distribution overlay**: `-- @distribution` annotations appear as
reference lines or bands on the scatter plot — expected mean with ±
range, correlation indicator.

**Coverage indicator**: for `-- @distribution coverage(x)`, the IDE
shows which values have appeared and which haven't.

This means the IDE's existing visualisation is already almost a
testing framework. The annotations just make implicit expectations
explicit and machine-checkable.

---

## Implementation Order

1. **Layer 1** (`-- @test sat/unsat`, `-- @test given k=v: sat`) —
   fully deterministic, no sampling needed, easiest to implement

2. **Layer 4 exact traces** (`-- @test input=... output=...`) —
   deterministic, tests the executor against hardcoded cases

3. **Layer 2 universal properties** (`-- @property expr`) —
   requires the sampler, but just checks 100% coverage of a constraint

4. **Layer 4 contract properties** (`-- @property satisfies Schema`) —
   requires the sampler + a second schema query per sample

5. **Layer 2 statistical** (`-- @property expr p=0.7`) —
   requires binomial hypothesis testing, configurable sample size

6. **Layer 3 distribution** (`-- @distribution E[x] ≈ v`) —
   requires statistics library, most novel, most IDE-integrated
