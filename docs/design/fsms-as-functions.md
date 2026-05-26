# FSMs as functions — the unified composition model

> **⚠️ Surface superseded — read this first.** This doc's *conceptual
> core* still stands: nesting an `fsm` lets the parent constrain the
> child's whole run; the condensability→guarantee gradient; the
> execute/verify faces; the three tiers. But its *surface syntax is
> rejected and replaced.* The user has ruled that an fsm has **no return
> value** ("anathema to the project"), so the functional spelling
> `result = F(init)` used throughout below is **wrong**. The corrected
> surface is a **constraint with no return value**:
>
> > `F(seed, fsm_state)` — `fsm_state` is a plain parent variable the
> > constraint binds to F's settled state, and which the parent may
> > *further constrain* (`fsm_state.count = 0`). `run(F, init)` and
> > `halts_within(F, N)` are **removed outright** (not aliased) — the
> > former is the disowned return-value form, the latter is subsumed
> > (an unsatisfiable settled-state constraint *is* "does not halt").
>
> Read every `result = F(init)` below as `F(seed, fsm_state)` + a parent
> constraint on `fsm_state`; read "the result value" as "the settled
> state variable." The authoritative, corrected surface +
> implementation plan is
> [`fsms-as-functions-impl.md`](fsms-as-functions-impl.md). This doc is
> retained for the conceptual narrative (the gradient, the tiers, the
> CHC/Spacer engine decision) only.
>
> **The capstone.** Five things the project built or designed — `run(F,
> init)`, `halts_within(F, N)`, the three-tier nested-FSM selector, the
> loop-functionizer's stack-of-FSMs, CEGAR — are not five features. They
> are one idea with a few realizations. This doc is the single narrative
> that makes them one idea, and supersedes the separate `run` /
> `halts_within` framings.
>
> The thesis, in one paragraph:
>
> > A **schema** is the one structure; the *keyword* says how it
> > composes. A **`claim`** is a *constraint* — it composes by
> > **conjunction** (inline it; AND its body into the surrounding model).
> > An **`fsm`** is a *function* — it composes by **application to
> > completion** (`result = F(init)`). Embedding an `fsm` means *run it to
> > completion and yield its final state*; the single-step body is just
> > the unit the runtime iterates to get there. The **fsm is the
> > implementation; the surrounding claims are the specification**;
> > nesting checks the implementation against the spec, recovering the
> > whole-output correctness guarantee that flat FSMs gave up.
>
> **Building this?** [`fsms-as-functions-impl.md`](fsms-as-functions-impl.md)
> is the turnkey implementation spec — it pins the four edges § 9 leaves
> open (the `run()` alias, the terse→internal-pair rewrite, the embed-call
> disambiguation, the `state_next` ban), scopes universal `_state` as the
> first mergeable step, and lays out the corpus migration + session
> sequence.
>
> Companion reading — this doc is the roof over all of them:
> [`fsm-halts-within.md`](fsm-halts-within.md) (the **verify** face of
> `result = F(init)`), [`nested-fsm-strategies.md`](nested-fsm-strategies.md)
> (the **execute** face + the tier selector),
> [`loop-functionizer.md`](loop-functionizer.md) (the run-to-halt loop and
> the stack-of-FSMs realization), [`selection-policy.md`](selection-policy.md)
> (the determine/witness/defer axis and the two invariants every
> realization honors), [`cegar-scaffolding.md`](cegar-scaffolding.md) (the
> output-feedback / search regime), and
> [`minimal-runtime-implementor-contract.md`](minimal-runtime-implementor-contract.md)
> (why the to-completion-on-a-recursive-enum capability is *kernel*, not
> an accelerator). The worked corpus is `examples/test_35_run_fsm.ev`,
> `test_36_sum_tree.ev`, `test_37_tree_walk.ev`.

---

## § 1 — The problem FSMs created

A constraint solver makes a promise: *describe the whole answer as
constraints, and the solver hands you a whole answer that satisfies all
of them at once.* The answer is correct over its entire extent, by
construction — there is no part the solver didn't account for.

A **flat FSM** quietly breaks that promise. The multi-FSM scheduler
slices a program into per-tick steps and solves *one step at a time*:
given `(state, last_results)` it solves for `(state_next, effects)`,
dispatches the effects, feeds the results forward, and ticks again. Each
transition is individually correct — the solver guarantees the step
relation holds for that tick. But **nothing constrains the total
behavior.** The trajectory the FSM actually traces over N ticks is
*emergent*: it falls out of iterating the step, and no constraint in the
program ever sees the whole of it.

That is the trade flat FSMs made, stated honestly:

> We traded *"the solver guarantees the whole answer"* for *"the solver
> guarantees each transition."*

Per-transition correctness is not whole-output correctness. A step that
is valid every single tick can still compose into a trajectory that is
wrong: an off-by-one that compounds tick over tick, a counter that
overshoots, a walk that visits the right nodes in the wrong order, a
loop that is individually-legal forever but never reaches its goal.
These bugs hide in the emergent trajectory precisely because no claim
ranges over it — the solver was never asked whether the *whole run* is
correct, only whether *this tick* is.

Everything below is the recovery: a way to get the whole-output
guarantee *back* for a bounded chunk of FSM execution, without giving up
the per-tick execution model that makes FSMs runnable in the first
place.

---

## § 2 — `fsm` is a function; `claim` is a conjunction

All three keywords — `type`, `claim`, `fsm` (and the discouraged
`schema`) — produce the same AST node, a `SchemaDecl` (`core/ast.rs`).
The structure is one structure. **The keyword is a statement about how
the thing composes into a larger model.**

**A `claim` is a constraint.** It is a predicate over a set of values.
To embed a claim in a larger model is to **conjoin** it — AND its body
into the surrounding constraints. This is exactly what the
composition vocabulary the project already documents does: names-match
(`valid_conference` names `rooms_conflict_free`, whose constraints get
conjoined into the parent), `..passthrough` (flat mixin of constraints),
guarded invocation (`cond ⇒ ClaimName`, each constraint wrapped in the
guard). Every one of those is "inline the constraints, conjoined." A
claim composes by **conjunction**.

**An `fsm` is a function.** Its body is a single-step transition
relation, but the *thing it denotes* is a function from an initial state
to a final state. To embed an fsm is to **apply** it:

```evident
result = F(init)        -- result IS F applied to init, run to completion
```

That is function application, not conjunction. You are not AND-ing F's
step relation into the parent; you are binding `result` to the *value*
that F produces when run to completion on `init`. A function composes by
**application to completion**.

### The keyword should be the rule; today it is half shape

`Keyword::Fsm` exists in `core/ast.rs`, and the multi-FSM scheduler
already dispatches on it: `effect_loop/fsm.rs::resolve_fsm` auto-
instantiates a top-level schema **iff** its keyword is `Fsm` — "purely
the parse-time keyword tag," not a body-shape heuristic. So for the
*scheduler*, the keyword already is the rule.

For *composition*, it is not — yet. The two composition surfaces shipped
so far, `halts_within(F, N)` and `run(F, init)`, resolve `F` by **shape**
(a state pair + `halt ∈ Bool`), and `F` is deliberately declared a
**`claim`**, not an `fsm` — see `fsm-halts-within.md`, "Why `F` is a
`claim`, not an `fsm`": tagging it `fsm` would make the scheduler
auto-instantiate the transition you only wanted to *reason about*. So
today the same logic wears two hats: `claim decrement` (the pure
transition, embedded by shape) and a separate `fsm countdown` (its
runnable twin). `examples/test_35`–`37` all declare the embedded
transition as `claim sum_tree` / `claim collect_labels` and let `run`
find it by shape.

The unified model says: **use the keyword.** An `fsm` referenced by name
in an equation reads as function application — *declarative*: "`result`
*is* F of `init`," a value defined in the model. Not *procedural*: "go
run F now." That declarative reading is what makes § 4 work — a value in
the model can be constrained by surrounding claims; a procedural command
cannot. The `run(...)` / `halts_within(...)` wrappers then become
**legacy surfaces over this one idea**: `run(F, init)` is the explicit
spelling of `result = F(init)` (execute it), and `halts_within(F, N)` is
a verification-flavored query over the very same composition (does F's
completion happen within N?). Migrating the *detection* from shape to
keyword is the back-compat work in § 9.

---

## § 3 — The two faces of an fsm: single-step vs to-completion

An fsm body **is** a single-step transition relation — `(state, inputs)
→ (state_next, effects)`. So an fsm presents two faces, and the whole
design lives in the gap between them:

| Face | What it is | Composes via | Computed by |
|---|---|---|---|
| **step-relation** | a *constraint* relating one tick's input to its output | conjunction (it's a `claim` in disguise) | translate → Z3 |
| **run-to-completion** | a *function* `λ init. iterate step until halt` | application (`result = F(init)`) | the runtime, somehow |

The first face is a constraint; the second is a function. They are the
same body seen two ways.

**Embedding an fsm always means the to-completion face.** You never
"embed a single step" — a single step's constraints are just a `claim`,
and you'd embed *that* by conjunction (§ 2). The `fsm` keyword is
precisely the marker that says: *the thing to embed is the
to-completion function, and the body you see is the step it's built by
iterating.*

The two faces also split the execute/verify line the rest of the project
draws:

- The **step-relation** face is what **symbolic-unroll composes** to
  *verify*. `fsm-halts-within.md` builds `F^N` by composing the step with
  itself (`step ∘ step ∘ …`) via Z3 substitution and exponentiation by
  squaring, then asserts the halt witness. It never ticks F; it reasons
  about the transition symbolically.
- The **run-to-completion** face is what the **scheduler executes**.
  Tier-3 blocking-interpret (`nested-fsm-strategies.md` § 2) drives the
  step to halt with the same per-tick solve the scheduler uses, and reads
  the final state.

Both compute *"F to completion."* They differ only in **how** — one
symbolically at compile time, the other concretely at run time. That
shared "F to completion, computed two ways" is the seam the tiers sit on
(§ 8), and the reason `halts_within`'s composer and `run`'s executor are
siblings rather than rivals: same function, opposite-side question.

---

## § 4 — Nesting recovers the guarantee: spec vs implementation

Here is the recovery § 1 promised.

The fsm is the **implementation**: it is *executed* — a concrete
trajectory is traced from `init` to a final state. The surrounding
claims are the **specification**: constraints over the implementation's
*total* input/output.

```evident
final ∈ Walk = run(collect_labels, tree)              -- the implementation, run
final = WDone(LCons("b", LCons("a", LCons("r", LNil)))) -- the specification, over its WHOLE output
```

(`examples/test_37_tree_walk.ev`, `sat_walk_flat`.)

`result = F(init)` collapses an entire trajectory into a single value.
`constraint(result)` then constrains *that value* — i.e. constrains the
total behavior, the exact thing § 1 said flat FSMs surrendered. The
nesting **re-introduces a constrainable boundary around a chunk of
execution**: outside the boundary we are back in constraint-land, where
the solver guarantees the whole answer; inside it, the fsm runs.

The parent can therefore **reject solutions where the child fails**:

- The child **doesn't halt** → the max-iteration guard fires (a loud
  error, never a hang — `nested-fsm-strategies.md` § 2,
  `loop-functionizer.md` § 3), or the halt witness is never satisfied.
- The child **halts but violates the spec** → `constraint(result)` is
  UNSAT and the surrounding solve rejects it. `test_36`'s
  `unsat_balanced_wrong` (`final = Done(5)` when the tree sums to 6) is
  exactly this rejection.

The decisive ergonomic fact: **the parent is built from ordinary
`claim`s.** You do not write an fsm to verify an fsm. You write a
specification as claims (conjunction) and constrain the fsm's output
(application). This is **property-based / bounded verification of an
FSM, expressed natively** — no temporal logic, no separate model
checker, no proof harness. The spec is claims; the impl is an fsm;
nesting is the function-application that binds them.

So the difference between a flat FSM and a nested one is precisely the
difference between an implementation with no spec boundary and an
implementation wrapped in a spec the solver enforces over its whole
output. The guarantee § 1 lost is the guarantee § 4 returns.

---

## § 5 — The condensability → guarantee spectrum

This is the key section. *How* the parent rejects-on-failure, and *how
much* of the original whole-problem guarantee it recovers, both depend
on **whether the child condenses** — whether its to-completion function
collapses back into pure constraints the parent solve can absorb.

Three regimes, ordered by condensability:

### 1. Dissolve — the child condenses into the parent solve

When the body is **affine**, the symbolic unroll's `F^N` folds to closed
form (the affine-step detector accepts — `fsm-halts-within.md`'s "state
collapses; halt aggregate is O(N)"; Z's measurement of which shapes
collapse). The child's to-completion behavior becomes a closed-form
*expression*, and that expression goes **into** the parent solve. The
nesting **dissolves**: there is no longer a separate "run the child" and
"solve the parent" — they are *one* constraint model over *one* set of
variables. The solver natively avoids any input that would violate the
spec, because spec and implementation are now the same SAT problem.

This is the limit case, and it is the full recovery: the fsm has been
turned **back into pure constraints**, and we are exactly where § 1
started — the solver guarantees the whole answer, in **one solve**, full
guarantee.

> *Status nuance.* CC's unroll already pushes the child-*constraint* (the
> halt aggregate) into the outer solve. The missing piece for a *value*
> was **keeping the halted-iteration state** instead of discarding it, so
> the condensed child returns a value, not just a halt `Bool`. Session OO
> landed the `halted_state` carry that does this (`fsm_unroll/compose.rs`),
> so an affine `run` can JIT to native code — but the carry is currently a
> K-bounded `ite`-tree (≈`init`-proportional), not yet the true
> `init`-independent O(1) closed form a full dissolve wants. The
> closed-form halt-step synthesis that gets there is the open item in § 9.

### 2. Forward-only execute — inputs pre-determined, checked once

When the body **does not condense** (branching — the common case for
tree-walks and game steps; the affine detector refuses) **but the
child's inputs are determined before it runs**, the child runs **once**
on concrete values and yields a **constant**. The parent constrains that
constant. If it violates the spec, the parent solve is **UNSAT** — a
correct, loud failure, but **no retry**: the child already ran, and there
is nothing to re-propose.

This is what session LL built. `run(F, init)` is evaluated to a concrete
`Value` *before* the outer solve and rewritten to a literal — the same
"compute a value, pin it as a constant" discipline a `given` follows
(`nested-fsm-strategies.md` § 7). The guarantee recovered is "the answer
is *checked* against the spec," strictly weaker than dissolve's "the
solver *searches within* the spec," because the input was fixed up front.

### 3. CEGAR — solve for inputs, discard and retry

When the body does not condense **and the parent constrains the child's
output** (the circular case — the input that yields a satisfying output
isn't known up front), there is nothing to pre-determine and nothing to
dissolve. The parent must **search**: propose inputs, run the child *as
an oracle* to test them, use the result to refine the proposal —
**discard-and-retry**. This is **CEGAR** (`cegar-scaffolding.md`):
abstraction proposes, oracle checks, counterexample refines.

CEGAR only pays off if the child is a **cheap function** — you cannot run
a slow blocking-interpret hundreds of times in a refinement loop. So this
regime *depends on* the JIT / loop-functionizer tiers (§ 8) being
available to make the child affordable as an oracle. Designed (GG), not
built.

### The one line

> **The more the child condenses, the more of the original whole-problem
> guarantee you recover.**

Dissolve (full guarantee, one solve) ▸ forward-execute (checked, UNSAT
on violation) ▸ CEGAR (searched, bounded iteration). The first turns the
fsm back into constraints; the last keeps it an opaque oracle and
recovers the *search* by iterating instead of by dissolution. The
dependency direction (§ 6) is what decides which regime a given nesting
falls into.

---

## § 6 — Parent solves the child's inputs

The mechanism underneath § 5: the parent's constraint model leaves the
child's input variables **unbound**; the parent solve determines them;
the child runs on the solved-concrete values.

- **Forward dependency (parent → child)** is the ordered, easy case, and
  it is § 5's regime 2. The parent solve fixes the inputs first; then the
  child runs once on concrete values. This is what LL's v1 enforces:
  `init` must be computable from values known *before* the outer solve —
  literals, the query's givens, or arithmetic over those
  (`eval_const_init`). An `init` that depends on an *undetermined* outer
  variable is a **loud error**, never a silent wrong value. No
  solve→run→solve cycle in v1.

- **Output-feedback (parent constrains the child's output)** is the
  circular case, and it is § 5's regime 3. The input that produces a
  satisfying output is not known up front, so the parent cannot pre-
  determine it — it must *search* for it. Lifting the v1 "no cycle"
  restriction *is* wiring in CEGAR: propose an input, run the child,
  check the output against the constraint, refine. The forward case is a
  CEGAR loop that happens to converge in one round.

The dissolve regime (§ 5 regime 1) is the case where the dependency
direction stops mattering: once the child is a closed-form expression in
the parent solve, output-feedback is just more constraints over the same
variables, and the single solve handles it. Condensation buys you out of
the circular-search problem entirely.

---

## § 7 — Effects percolate up (the new decision)

A function returns effects as *data*; it does not *perform* them. That
principle forces a decision, recorded here for the first time:

> **A child FSM must not emit effects.** The effects it solves for are
> **captured during the child run (not dispatched) and percolate up to
> the parent**, which is the sole dispatch authority and may **discard
> them** if its constraints reject the solution.

This keeps the child a **pure function** of its inputs:

```
(init) → (final_state, effects)
```

Both halves are *returned data*. The child computes the effect list it
*would* perform; it does not perform it. Only the **parent** — which sees
the whole picture, including whether the child's output satisfies the
spec (§ 4) — decides what actually happens in the world.

Why this is forced, not a preference: § 4's whole-output guarantee
*requires* the parent to be able to reject a child solution. But you
cannot un-print a line. If a rejected child had already dispatched its
effects, the rejection would be **observable** — the function would have
a side effect on a run the model then threw away — and `result = F(init)`
would no longer be referentially transparent (`nested-fsm-strategies.md`
§ 5: "a run that printed once per call is not referentially
transparent"). By **capturing, not dispatching**, the child stays a pure
function, the value cache and FSM replay stay sound, and the parent's
reject-on-failure stays sound: discarding a rejected solution discards
its effects too, because they were only ever data.

This **supersedes the v1 restriction** in earlier docs
(`nested-fsm-strategies.md` § 5, `selection-policy.md`): instead of
*forbidding* effects in a nested run ("`run`'s target must be
effect-free"), we *capture* them. It is the realization of the
"effect-collecting nested run" those docs flagged as the natural
extension — the return type widens from `final_state` to
`(final_state, effects)`, and the parent dispatches (or discards) in its
own tick. Implemented in session RR. The remaining surface — a decoding
story for the effect list as a returned value — is in § 9.

---

## § 8 — How the tiers realize one idea

The three nested-FSM tiers — **symbolic-unroll → JIT** (tier 1),
**loop-functionizer** (tier 2), **blocking-interpret** (tier 3) — are not
three features. They are the **selector** picking *how to realize
`result = F(init)`* given the body shape and how the result is used. Each
tier is a point on § 5's condensability spectrum:

| Tier | Realizes `result = F(init)` by | Face (§ 3) | Spectrum point (§ 5) | Survives |
|---|---|---|---|---|
| **1** symbolic-unroll → JIT | composing `F^N` symbolically; affine → closed form → JIT to O(1) | step-relation (compose) | **dissolve** (or O(1) execute) | affine bodies only |
| **2** loop-functionizer | a native `while !halt { state = step(state) }` over the compiled step | run-to-completion | forward-execute, *fast* | branching OK; step must JIT |
| **3** blocking-interpret | running F on the multi-FSM scheduler to halt | run-to-completion | forward-execute, *always-correct floor* | anything |

The selector (`nested-fsm-strategies.md` § 3) mirrors the single-solve
functionizer fall-through in `query.rs` exactly: **try the fastest
applicable tier; fall through to an always-correct floor; cache the plan
per body.** Tier 1 applies when the affine detector accepts (the body
condenses); tier 2 when the step JIT-compiles; tier 3 always. A tier may
bail at call time and fall through; tier 3 never bails.

Tier 3 is the **floor and the equivalence oracle** — the faster tiers are
*defined as* "faster ways to get the blocking-interpret result," and are
validated against it (`nested-fsm-strategies.md` § 4). For the recursive
tree-walk class this matters sharply: **Z3 is not a sound oracle there**
(a solve over unbounded recursion is the recursion gap, COUNTEREXAMPLES
#15), so tier 3 — which actually *runs* the walk — is the only ground
truth (`loop-functionizer.md` § 6). That is also why the to-completion
capability is **kernel**, not an accelerator
(`minimal-runtime-implementor-contract.md` § 4): every self-hosted
tree-walk pass stands on it, so its slow path must be the always-correct
scheduler, not an optional JIT.

The same one idea shows up once more inside tier 2/3 as the
**stack-of-FSMs** (`loop-functionizer.md` § 4): a recursive tree-walk is
"run an FSM-with-stack to completion," the work-stack made explicit data
in the FSM state. `test_36`/`test_37` are this — recursion expressed as
`result = F(init)` over a recursive-enum work-stack, which is why the
accumulator threads through state (never a free Z3 variable, never the
recursion gap). Three substrates for the stack (scheduler work / native
`Vec` / symbolic), one abstraction — exactly the three tiers again.

So: **one idea** (`result = F(init)`), realized along a gradient. The
selector reads the body and the use site and picks where on the gradient
this particular nesting lands.

---

## § 9 — What this supersedes, and the open questions

### Supersedes

- **`run` and `halts_within` become realizations of "reference an
  fsm."** `run(F, init)` is the *execute* face of `result = F(init)`;
  `halts_within(F, N)` is a *verify*-face query over the same composition
  ("does F's completion happen within N?"). `within N` is the
  max-iteration guard on the execute side and a verification assertion on
  the verify side — the same bound, read for two purposes.
- **The "effect-free only" restriction** (`nested-fsm-strategies.md`
  § 5) is superseded by § 7's capture-and-percolate: effects are returned
  as data, not forbidden.
- **Shape-detection of FSM-ness for composition** is superseded in
  principle by the keyword (§ 2) — the migration is below.

### Open questions

- **Syntax for reading a child's final-state field in an equation.**
  `run` returns the *whole* final state; the surface for
  `result.field`-style access inside the embedding equation (so the
  parent can constrain one field of the completion, not just the whole
  value) is not yet spelled out.
- **Migrating detection from shape to keyword.** Today composition
  resolves `F` by shape on a `claim`, precisely so the scheduler doesn't
  auto-instantiate it (`resolve_fsm` keys on `Keyword::Fsm`). Flipping the
  embedded transitions in `test_34`–`37` to `fsm` would make the
  scheduler run them as top-level FSMs. So the migration needs the
  scheduler to distinguish a *top-level* fsm from an *embedded-only* one
  (a second marker, or "an fsm referenced by `run`/in an equation is not
  auto-instantiated"), with back-compat for the existing demos.
- **The keep-the-state piece for the dissolve path.** OO's `halted_state`
  carry is K-bounded (an `ite`-tree, ≈`init`-proportional), so large
  inits fall through to tier 3 and the plan isn't yet `init`-independent.
  True dissolve (§ 5 regime 1, O(1), reusable across inits) needs the
  **closed-form halt-step `k` synthesis** (`x_next = a·x + b`,
  `halt = x ≥ T` ⇒ `k = closed-form(init)`), then read `e_k`'s affine
  state — narrower than the detector's accept set, but truly O(1)
  (`nested-fsm-strategies.md` § 7–8).
- **The effects-as-data return type.** § 7's `(final_state, effects)`
  return widens `run`'s contract and needs a decoding story for the
  effect list as a returned value, plus the parent-side dispatch/discard
  wiring (`nested-fsm-strategies.md` § 8).
- **Output-feedback → CEGAR.** Lifting the v1 forward-only restriction
  (§ 6) is the CEGAR build (`cegar-scaffolding.md`): it needs the cheap-
  function tiers as the oracle and a bounded refinement loop. Designed,
  not built.
