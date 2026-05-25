# The selection-policy axis + the residual functionizer

> A constraint model is a **relation**, not a function. "Functionizing"
> a model **selects a function** out of that relation. *Which* function
> it selects is governed by a **selection policy** — and the policy is a
> first-class design axis, orthogonal to the lowering mechanism (native
> code, GLSL, symbolic regression, …). This doc names that axis,
> classifies the five existing functionizers on it, and specifies the
> one policy with zero implementations today: **defer** — the
> *residual* (partial) functionizer.
>
> See also:
> [`../satisfier-functionizer.md`](../satisfier-functionizer.md) — the
> **witness** policy as currently shipped (the SatisfierFunctionizer);
> [`compile-claims-to-functions.md`](compile-claims-to-functions.md) —
> the function-izer pipeline and the allegoric "factor a relation into a
> map + a residual" framing this doc operationalizes;
> [`cegar-scaffolding.md`](cegar-scaffolding.md) — the audit/refine loop
> that the same `Functionizer` trait supports;
> [`nested-fsm-strategies.md`](nested-fsm-strategies.md) — the **sibling
> chooser one level up**: where this doc's selection-policy axis governs
> *which function a functionizer selects from a relation*, that doc's
> strategy selector governs *which execution strategy runs a nested FSM*
> — and it mirrors the same try-fast-fall-through-to-an-always-correct-
> baseline shape (`query.rs`) this axis lives inside.

---

## § 1 — Models are relations; functionizing selects a function

An Evident claim is a set of constraints over its variables. Pin a
subset of those variables (the `given`) and the claim defines a
**relation**

```
R ⊆ Inputs × Outputs
```

— for each input assignment, the *set* of output assignments that
satisfy the body. A full Z3 solve picks one element of that set on
demand. Querying asks "is `R(given, ·)` non-empty?" and hands back a
witness.

**Functionizing selects a function from `R`.** A `CompiledFunction`
is, by type, a map `Inputs → Outputs` (`call(given) -> Option<HashMap>`).
Turning the relation `R` into that map is a *choice*: for every input,
which of the valid outputs does the function return? That choice is the
content of this document.

The vocabulary is standard, and worth pinning precisely:

- A **witness** for a free variable is any value that satisfies the
  constraints on it. `∃x. P(x)` asserts a witness exists.
- A function that produces a witness is a **Skolem function**: the
  `f` in `∀g. ∃x. P(g, x)` ⟺ `∀g. P(g, f(g))`. Functionizing a claim
  with genuinely-free outputs *is* Skolem-function extraction — we are
  building the `f`.
- The same idea in hardware synthesis is a **don't-care** output: the
  spec leaves the value unconstrained on some inputs, and the
  synthesizer is free to choose whatever is cheapest. A witnessed
  output is a resolved don't-care.

When `R` happens to be **functional** on the chosen partition (exactly
one output per input — the 2-copy uniqueness check in
[`compile-claims-to-functions.md`](compile-claims-to-functions.md)
returns UNSAT), the selection is forced: there is only one function to
pick. Functionizing such a claim is unambiguous. The interesting cases
are the ones where `R` is *not* a function — where multiple outputs are
valid and the functionizer must *choose*. That is where the policy axis
lives.

### The two policies in the codebase today

There are five `Functionizer` implementations
(`runtime/src/functionize/`). They differ in *lowering mechanism* —
how they realize the chosen function — but collapse to just **two**
selection policies:

| Impl | Lowering mechanism | Selection policy |
|---|---|---|
| `CraneliftFunctionizer` | translate `Z3Program` ASTs → native code (JIT) | **determine** |
| `SymbolicFunctionizer` | genetic-program a closed-form that fits input→output exactly | **determine** |
| `LlmFunctionizer` | generate source from the program, verify | **determine** |
| `GlslFunctionizer` | emit a fragment shader (macOS, headless CGL) | **determine** |
| `SatisfierFunctionizer` | seeded PRNG draw + delegate the remainder to Cranelift | **witness** |

The four code-emitting strategies are all the **determine** policy:
they require every output to be *uniquely fixed* by the body (Cranelift
needs an equation per output; symbolic regression rejects any residual
`checks`/`predicates` because "a total closed-form can't honor" a
conditional; etc.). They differ only in *what they emit*, not in *what
they do with a free variable* — which is: refuse it (`compile → None`),
falling through to the slow solve.

`SatisfierFunctionizer` is the only one that does something *other* than
refuse a free variable: it **witnesses** it — draws a value from the
variable's finite domain. That is a different point on the selection
axis, not a different lowering target.

The third policy — **defer** — has no implementation. This doc designs
it.

---

## § 2 — The three selection policies

| Policy | What it does with a free variable | Functionizer | Status |
|---|---|---|---|
| **determine** | requires the output to be *uniquely fixed*; refuses otherwise | Cranelift / Symbolic / Llm / Glsl | landed |
| **witness** | *picks* any valid value (seeded sample from a finite domain) | Satisfier (W) | landed |
| **defer** | keeps it *symbolic*; returns the determined outputs **plus the residual constraints**, hands them up | *residual* | **missing — designed here** |

### determine — "the output is forced; compute it"

- **When it applies.** The output has a unique value given the inputs:
  an equation `y = expr(given, earlier-outputs)`, or a cluster of
  equalities that algebraically isolate it. `R` restricted to the
  partition is a *map*.
- **What it produces.** A total `Inputs → Outputs` function. No choice,
  no randomness, no residual.
- **What it costs.** Nothing at call time beyond the arithmetic itself
  (Cranelift: a handful of native instructions). The cost is the
  compile-time gate (the 2-copy uniqueness check) and the refusal rate:
  any non-functional output sends the whole component to the slow solve.

### witness — "the output is free; choose one, now"

- **When it applies.** The output is unbound but its domain is finite
  and cheap to sample: `lo ≤ x ≤ hi`, `c ∈ EnumType`, `x ∈ {a, b, c}`
  (see [`../satisfier-functionizer.md`](../satisfier-functionizer.md)).
  `R` is non-functional but its output set is enumerable.
- **What it produces.** A total function still — but the value it
  returns is a *chosen* element of the valid set, made a pure function
  of the inputs by seeding the PRNG on them (see § 5).
- **What it costs.** A PRNG draw (≈ 5 native instructions) instead of a
  solve cycle (≈ ms). The real cost is *soundness scope* (§ 4): the
  choice is only legitimate when the variable is **globally** free.

### defer — "the output is free; don't choose — hand the constraint up"

- **When it applies.** The output is free in *this* sub-model, but the
  sub-model is an **intermediate** that a larger composition will
  resolve. Choosing now would either be premature (a sibling constraint
  may force a different value) or wasteful (the caller is going to solve
  anyway).
- **What it produces.** A **partial** result: the determined bindings
  *plus a representation of the residual constraints* on the still-free
  variables — enough for the caller to fold into its own solve.
- **What it costs.** The determined dimension still runs at native
  speed; only the genuinely-free dimension is carried symbolically to
  the caller. The cost is interface complexity (the return type is no
  longer a flat `HashMap`) and the caller's obligation to resolve the
  residual.

### The distinguishing insight: leaf vs intermediate

Witness and defer both confront a *free* variable. They differ on one
question — **is this the place where the value should be decided?**

> *If you needed a complete solution right here, you'd commit a value
> (witness). If this value is going to be composed into a larger
> function, you don't pick it — you defer, and let the context that can
> see all of its constraints resolve it.*

A **leaf** sub-model — the final consumer of a value, with no further
composition above it — wants **witness**: produce a usable answer now.
An **intermediate** sub-model — whose outputs feed a larger model that
adds more constraints on them — wants **defer**: a premature choice
here is, at best, a wasted solve and, at worst, a wrong answer the
larger model can no longer correct (§ 4).

The determine policy never faces this question, because a determined
output has nothing to choose: leaf and intermediate agree on the one
value the equation forces.

---

## § 3 — The residual functionizer (the new strategy)

### Input

A `Z3Program` (or, in the decomposed pipeline, a single component's
assertions — `compile_one_component` in `runtime/src/runtime/query.rs`)
with **some outputs determined** (defined by equations → `Z3Step`s) and
**some variables free** (bounded or relationally constrained, but not
uniquely fixed).

This is exactly the shape that makes Cranelift and the satisfier both
refuse today:

- Cranelift refuses because not every output has a defining equation.
- The satisfier refuses because, after stripping the samplers it
  recognizes, a *residual* constraint survives in `program.checks` /
  `program.predicates` (its `compile` bails: "checks + predicates
  remain after sampling").

The residual functionizer is precisely the strategy that *keeps* that
residual instead of refusing on it.

### Output: not a total `HashMap`

The current contract is total:

```rust
fn call(&self, given: &HashMap<String, Value>)
    -> Option<HashMap<String, Value>>;   // every output bound, or None
```

A residual result is **partial** — it binds the determined outputs and
describes what is left:

```rust
pub enum CallResult {
    /// Every output bound — today's behavior, unchanged.
    Total(HashMap<String, Value>),
    /// Determined outputs bound; the rest deferred as residual
    /// constraints for the caller to resolve.
    Partial { determined: HashMap<String, Value>, residual: Residual },
}
```

### The residual representation

Three candidates were on the table:

**(a) The leftover Z3 `Bool` assertions referencing the free vars.**
The residual *is* a conjunction of Bool constraints on the free
variables, with the determined values substituted in. This is the form
the data is already in: the extractor's `Z3Program.checks` /
`Z3Program.predicates` hold exactly these leftover assertions, and the
decomposition pass in `query.rs` already produces per-component
`Vec<Bool<'static>>` assertion lists. A composing caller folds them
into a larger solve by simple conjunction (assert them into its
solver). Full fidelity — any constraint Z3 can express survives.

**(b) A reduced `Z3Program` for just the free dimension.** Wrong shape.
A `Z3Program`'s `steps` are *topologically-ordered defining
assignments* — the IR of the **function-shaped** part. The residual is
by definition the part that *isn't* function-shaped (no ordering, no
defining equation); forcing it into `steps` misrepresents it. The
`Z3Program` is the right container for the *determined* half, not the
free half.

**(c) A typed "hole" descriptor (var + its bound/domain).** Too lossy.
A hole descriptor can capture box constraints (`b ∈ [a, 100]`) but not
*relational* residuals — `b ≥ a` where `a` is itself a determined
output, or `b ≠ c` between two free vars, or anything Z3 can express
beyond an interval. Reducing every residual to a domain box silently
drops the constraints that don't fit, which in Evident is the
worst-class bug (a silent SAT-but-wrong, § 4).

**Recommendation: (a), indexed by a thin slice of (c).** The residual
carries the leftover `Bool` assertions (full fidelity, trivially
composable) *plus* an explicit list of the free-variable "holes" (name
+ sort / `Var` entry) so the caller doesn't have to re-scan the Bools to
learn what's free and how to extract it from a model:

```rust
pub struct Residual {
    /// Still-free variables: name + the env `Var` (sort + extraction
    /// shape). The caller's solve must bind these.
    holes: Vec<(String, Var)>,
    /// Leftover Bool assertions on the holes, with the determined
    /// outputs already substituted in. Conjunction = the residual
    /// relation. Live in a 'static-leaked context (see below).
    constraints: Vec<Bool<'static>>,
}
```

**Lifetime discipline.** Z3 `Bool<'ctx>` ASTs are context-bound, and
`CompiledFunction` otherwise hands back owned `Value`s with no Z3
lifetime. Carrying live ASTs ties the artifact to a Z3 context — so the
residual must use the **same `'static`-leaked-context discipline that
`SlowPart` already uses** in `query.rs` (the parallel slow path
translates a component's assertions into a private leaked `'static`
context so it can `check()` independently). A `Residual` is, in fact,
morally a *pre-`SlowPart`*: holes + constraints + env, minus the solver.
This is a feature — it means the consuming caller (below) is a
near-trivial adapter onto machinery that already exists.

### How it slots into the `Functionizer` trait

The current `compile → CompiledFunction → call` chain assumes a total
result. Two ways to admit a partial one:

1. **Mutate `CompiledFunction::call`** to return `CallResult`. Rejected:
   it breaks all five existing impls, and it adds an enum match +
   allocation to the *hot total path* that 99% of calls take.
2. **A sibling trait pair.** Recommended:

```rust
pub trait PartialFunctionizer {
    fn compile_partial(&self, program: &Z3Program,
                       enums: &EnumRegistry, datatypes: &DatatypeRegistry)
        -> Option<Rc<dyn PartialCompiledFunction>>;
}
pub trait PartialCompiledFunction {
    fn call_partial(&self, given: &HashMap<String, Value>) -> Option<CallResult>;
}
```

**Back-compat with the five existing impls.** They are untouched. A
blanket adapter lifts any `CompiledFunction` into a
`PartialCompiledFunction` that always returns `CallResult::Total`:

```rust
impl<C: CompiledFunction> PartialCompiledFunction for Total<C> {
    fn call_partial(&self, g: &HashMap<String, Value>) -> Option<CallResult> {
        self.0.call(g).map(CallResult::Total)
    }
}
```

So Cranelift / Symbolic / Llm / Glsl / Satisfier all participate in a
partial-aware pipeline with *zero edits* — they simply never produce a
`Partial`. Only `ResidualFunctionizer` implements
`PartialCompiledFunction` natively. The runtime opts into the partial
path only at call sites that can *consume* a residual (§ 7); everywhere
else keeps calling `call` and getting a `HashMap`. This keeps the trait
change additive and the common path's cost unchanged.

### Worked example

```evident
-- sub-model, queried with x given
a ∈ Int = x + 1
b ∈ Int
a ≤ b            -- b ≥ a
b ≤ 100
```

`a` is determined (`a = x + 1`). `b` is free, constrained to the range
`[a, 100]`. With `x = 10` given (so `a = 11`):

| Policy | Result | Why |
|---|---|---|
| **determine** (Cranelift) | refuses → slow Z3 solve | `b` has no defining equation; the 2-copy check is SAT (`b = 11` and `b = 12` both satisfy) → not functional. |
| **witness** (Satisfier) | `{a: 11, b: 47}` (some drawn value in `[11, 100]`) | The range `11 ≤ b ≤ 100` is a `SampleRange`; draw deterministically from it. The draw is final. |
| **defer** (residual) | `Partial { determined: {a: 11}, residual: { holes: [b: Int], constraints: [11 ≤ b, b ≤ 100] } }` | `a` computed natively; `b` left symbolic with its bounds (with `a`'s value substituted), for the caller to resolve in context. |

**How a composing caller consumes the residual.** Suppose this
sub-model is an intermediate inside a larger model that adds
`b = 2 * x + 30`. The caller:

1. Takes the determined bindings (`a = 11`) directly — native, done.
2. Builds a scoped solve over the residual's `holes` (`b`), asserting
   the residual `constraints` (`11 ≤ b ≤ 100`) **and** its own extra
   constraint (`b = 2*x + 30 = 50`).
3. Z3 resolves `b = 50` — which is in `[11, 100]`, so SAT — and unions
   it with the determined `a = 11`.

The witness policy could not have done this: had it committed `b = 47`
at step 1, the caller's `b = 50` would contradict it, and the only
recoveries are an unsound override (drop the witnessed value) or a
spurious UNSAT. Defer keeps `b` open precisely until the constraint that
pins it is in scope. That is the whole point of the policy.

---

## § 4 — Soundness: the global-freedom boundary (CRITICAL)

This is the load-bearing correctness section. Get it wrong and witness
*and* a careless residual produce **silent SAT-but-wrong answers** — a
satisfying-looking assignment that violates a constraint the local slice
couldn't see. In Evident, that is the worst failure mode: no error, no
panic, just a wrong model indistinguishable from a right one.

### Locally free ≠ globally free

Decomposition (`decompose_simplified` in `query.rs`) splits a claim's
body into connected components over the *free* variables, treating
`given` as broadcast constants. A variable can be **free within its
extracted component** and yet **constrained elsewhere**:

- **Cross-component.** Decomposition routes some assertions to a
  `global` bucket — those touching only broadcast (given) vars, and any
  *pure-intermediate islands* nothing observes. A constraint that lives
  in `global`, or in a sibling component, is invisible to a
  component-local witness.
- **Cross-tick.** An FSM body runs every tick. A variable free *this*
  tick may be pinned by a constraint that only materializes via the
  feedback edge (`_var`, `world.X`) on a *later* tick. The
  `unsafe_free` comment in `query.rs` names this directly: an
  empty-`given` model value "would be Z3's free choice, **wrong on later
  ticks**."

Witnessing `x = 5` here when a sibling component, a global assertion, or
next tick needs `x = 7` is a silent SAT-but-wrong bug. **Witness is
sound only when the variable is globally free, not merely locally
free.**

### Where `query.rs` already encodes the boundary

The decomposition gap-fill path in `compile_one_component` faces an
adjacent question — *is it safe to bake a single model value for a
missing output?* — and answers it with the `unsafe_free` check:

```rust
// for each var the component's assertions touch:
let in_given   = given.contains_key(n);
let is_covered = output_set.contains(n) && !missing_set.contains(n); // a computed output
let in_env     = cached.env.contains_key(n);
let is_const   = matches!(cached.env.get(n),
    Some(Var::PinnedInt(_) | Var::EnumValue { .. } | Var::EnumCtor { .. }));
if in_env && !in_given && !is_covered && !is_const {
    unsafe_free = true;   // free choice → unsafe to bake
}
```

A variable that is **in the env but neither given, nor a computed
output, nor a constant** is a free choice — baking Z3's arbitrary model
value for it is unsound. When that happens, `query.rs` does *not* commit
a value:

- If the component carries a **defining assertion**
  (`component_has_defining_assertion` — anything beyond a bare
  type-bound `≤`/`<`/`≥`/`>`), the missing outputs are actually
  determined; route to the **scoped slow solve**, which sees the *real*
  `given` and recovers them. (`ComponentOutcome::Slow`)
- If every assertion is just a type bound, the output is genuinely
  unconstrained (likely a dropped constraint); **bail** to the
  non-lenient `evaluate`, which surfaces it as an error rather than
  masking it with an arbitrary value. (`ComponentOutcome::Bail`)

The runtime never silently picks a value for a variable whose
constraints it cannot fully see. That is the global-freedom boundary, in
code, today.

### The rule the witness and residual policies must obey

> **Only a variable whose *every* constraint lies inside the current
> component may be witnessed. A variable touched by any global
> assertion, any sibling component, or any cross-tick feedback edge must
> be deferred (residual) or sent to the full solve — never chosen
> locally.**

Operationally, before witnessing `v`: confirm `v` appears in no `global`
assertion and in no other component, i.e. `decompose_simplified` placed
*all* of `v`'s assertions in this component. The satisfier's existing
conservatism (refuse if any `check`/`predicate` survives) is a *coarse*
version of this rule; the precise version is the decomposition's
component membership.

### Residual is the safe answer because it does not choose

The residual functionizer sidesteps the boundary entirely: it never
commits a value to a free variable, so it can never commit a *wrong*
one. It preserves the constraint and hands it to the context that can
see all of `v`'s constraints — exactly the context entitled to resolve
it. Where witness is sound *only* under the global-freedom precondition,
residual is sound *unconditionally* (it defers the decision to whoever
holds the complete picture). That is why, for an intermediate sub-model,
defer is not merely a performance choice — it is the *correct* one.

---

## § 5 — Two invariants the doc must pin

### Referential transparency

A functionized output — even a witnessed or residual-resolved one — must
be a **pure function of its inputs**: same `given` → same output. The
runtime's value cache keys on `(claim, given-keys, given-values)`, and
the FSM scheduler replays and memoizes across ticks; a function that
returned different outputs for identical inputs would poison the cache
and break replay.

- **Witness achieves this by seeding on the inputs.** The satisfier's
  PRNG seed is `EVIDENT_DISPATCH_SEED ⊕ program-shape-salt ⊕
  given-values-hash` (SplitMix64, key-sorted fold over `given`). The
  "randomness" is a **deterministic pseudo-choice**: it varies with the
  inputs (so successive ticks, with different prev-tick state, draw
  different values) but is *stable* for any fixed input. It is a
  function, not a coin flip.
- **The residual must preserve it too.** The determined bindings are
  pure (substitution of `given`). The residual `constraints` are a pure
  function of `given` as well (the determined values are substituted
  before they're handed up). The obligation propagates: when the caller
  *resolves* the residual, that resolution must itself be referentially
  transparent — i.e. the parent solve must be deterministic, and any
  parent *witness* must seed on its inputs by the same rule. A residual
  resolved by a non-deterministic parent re-introduces the
  cache-poisoning the witness policy was careful to avoid.

### Execute vs verify

A witnessed or residual-resolved function **explores points** of the
relation — it returns *a* valid assignment, one element of `R(given,
·)`. It cannot **prove a ∀-property**: that *every* element of the
relation has some property, or that the relation is empty (UNSAT), is
beyond what point-sampling can establish.

- **Sound for running / simulation.** Each tick needs one valid
  assignment; witness/defer deliver exactly that, fast.
- **Unsound for verification.** "Does this claim hold for all inputs?"
  / "Is this `unsat_*` test actually unsatisfiable?" must go to the full
  Z3 solve. A function that only ever produces points can confirm
  satisfiability by example but can never refute it for all cases.

This is the **same split** the runtime already draws for recursive
claims (`define-fun-rec` evaluates, doesn't prove inductive properties)
and that the loop-functionizer / CEGAR work draws for unrolled FSM
bodies (the cheap artifact *executes*; the oracle *verifies*). The
selection-policy axis lives entirely on the *execute* side of that line.

---

## § 6 — Relationship to the other strategies

### vs the Satisfier (witness, W)

Same trigger (a free, bounded variable), opposite move:

- **Witness** = "choose now." Best for a **leaf** — the value's final
  consumer, with no composition above that could need a different value.
- **Defer** = "don't choose; hand it up." Best for an **intermediate** —
  a sub-model whose free outputs feed a larger model that will add
  constraints on them.

The satisfier is also the natural *delegate* for the determined half of
a residual: just as `SatisfierFunctionizer` strips its `Sample*` steps
and hands the computed remainder to Cranelift, a `ResidualFunctionizer`
hands its determined steps to Cranelift (or the satisfier) and carries
only the genuinely-free dimension as residual.

### vs the composition story (loop-functionizer II / CEGAR GG)

Residual is the **mechanism by which a sub-model's freedom survives into
a larger function.** The function-izer's allegoric framing
([`compile-claims-to-functions.md`](compile-claims-to-functions.md))
calls for *factoring a relation into a map + a residual*; until now the
"map" half had implementations (the five functionizers) and the
"residual" half was simply discarded (refuse → slow solve). The residual
functionizer is the missing half: it lets the map run natively while the
residual is composed upward rather than thrown away.

In the [CEGAR scaffolding](cegar-scaffolding.md) framing, a residual is
a *structured* form of the `call → None` refusal: instead of "I can't
answer, ask the oracle for everything," it is "here is what I *can*
answer, and here is the precise residual the oracle still needs to
resolve." That shrinks the oracle's job to the genuinely-free dimension.
(The sibling loop-functionizer doc from session II is not yet present; if
it lands, cross-link it here — it is the same "cheap artifact composes
into a larger function" story applied to unrolled FSM bodies.)

### Distribution-as-policy (aside — mention, don't design)

Witness today draws **uniformly** from the finite domain. That uniform
draw is one choice among many: a **lower-bound** policy (`b := a`,
always the tightest), a **biased** distribution, a **true-random**
(unseeded) draw, or a learned distribution are all alternative *witness
policies* — sub-axes of "choose now," differing only in *which*
element. This is the probabilistic-programming direction (the satisfier
already frames an unbound-but-bounded var as a distribution and a query
as a draw). Worth noting that the selection-policy axis has this finer
structure under `witness`; designing those distributions is out of scope
here.

---

## § 7 — Implementation plan + open questions

### Recommended first slice

The smallest useful residual functionizer plus exactly one consuming
caller — both targeting machinery that already exists:

1. **`Residual` carrier + `CallResult` enum** (`core/`): holes
   (`Vec<(String, Var)>`) + `Vec<Bool<'static>>` constraints, under the
   `'static`-leaked-context discipline `SlowPart` already uses. Plus the
   `PartialFunctionizer` / `PartialCompiledFunction` sibling traits and
   the blanket `Total<C>` adapter so the five existing impls participate
   unchanged.

2. **`ResidualFunctionizer`** (`functionize/residual.rs`): partition the
   `Z3Program` exactly as the satisfier does — determined `steps`
   delegate to Cranelift; but *instead of refusing* when `checks` /
   `predicates` survive, package those survivors (plus the free vars
   they touch) into a `Residual`. `call_partial` runs the inner Cranelift
   function for the determined bindings and returns `Partial { determined,
   residual }`.

3. **One consuming caller: the `unsafe_free && has_defining_assertion`
   branch of `compile_one_component`.** Today that branch routes the
   *whole* component to the scoped slow solve (`ComponentOutcome::Slow`),
   abandoning the determined outputs. Replace it with: compile the
   determined part natively (the residual functionizer's `determined`
   half) and hand only the `Residual` to a `SlowPart` (which the
   `Residual` is already shaped to become). The component's native
   dimension runs fast; only its genuinely-free dimension hits Z3. This
   is the concrete payoff and it reuses the existing slow-path plumbing
   end-to-end.

Gate it behind an env var (`EVIDENT_RESIDUAL=1`), mirroring
`EVIDENT_SATISFIER`, so the default pipeline is byte-for-byte unchanged
until the path is proven.

### Open questions

- **Residual representation efficiency.** Carrying live `Bool<'static>`
  ASTs and a leaked context per residual is not free; how many residuals
  are live at once, and can they share a context with the `SlowPart`
  they feed? (Likely yes — they're the same context by construction.)
- **Composing two residuals that share a variable.** If sub-models `P`
  and `Q` both defer `b` with constraints `11 ≤ b` and `b ≤ 100`
  respectively, the caller must conjoin both before solving `b` — not
  resolve them independently (which could pick inconsistent values). The
  composition operator is "union the holes, conjoin the constraints,
  solve once." Needs a clear contract for *when* the caller is obligated
  to merge vs solve-in-order, and detection of a hole appearing in two
  residuals.
- **Trait change vs separate code path.** Is the
  `PartialFunctionizer` sibling trait worth it, or should the residual
  path be a bespoke branch inside `query.rs` that never touches the
  `Functionizer` abstraction? The trait buys composability (any future
  partial-aware strategy plugs in) at the cost of two more traits; the
  bespoke path is smaller but doesn't generalize.
- **Debugging a partial result.** A `Total` result is a flat map you can
  print. A `Partial` is "these are bound, these are still constrained
  by *this Z3 formula*" — the `EVIDENT_FZ_DUMP_PROGRAM` analog needs to
  render the residual's holes + constraints legibly so a partial result
  is as inspectable as a total one.
