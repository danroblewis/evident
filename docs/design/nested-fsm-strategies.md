# Nested-FSM execution strategies + the strategy selector

> A parent FSM (or claim) sometimes needs the *result of running another
> FSM to completion* — a sub-computation that loops, a tree-walk driven
> to drain, a fixpoint. There are three ways to produce that result, in
> descending order of speed and ascending order of generality:
> **symbolic-unroll → JIT** (CC), **loop-functionizer** (II), and
> **blocking-interpret** (this doc). The first two compile; the third
> compiles nothing and reuses the existing multi-FSM scheduler verbatim,
> so it is **always correct** and serves as the **equivalence oracle**
> the faster two are validated against.
>
> This doc specifies blocking-interpret in full and the **strategy
> selector** — a precedence chooser analogous to the existing
> functionizer fall-through (`runtime/src/runtime/query.rs`) that tries
> the most performant applicable strategy first and falls through to the
> baseline.
>
> Companion reading:
> [`loop-functionizer.md`](loop-functionizer.md) (tier 2 — the native
> run-to-halt loop; this selector's middle tier),
> [`fsm-halts-within.md`](fsm-halts-within.md) (tier 1's verification
> sibling — the symbolic-unroll composer the run-to-halt path reuses),
> [`selection-policy.md`](selection-policy.md) (the sibling chooser: the
> functionizer *selection-policy* axis, the model this selector mirrors
> one level up), [`cegar-scaffolding.md`](cegar-scaffolding.md) (the
> abstraction/oracle framing — blocking-interpret is the ground-truth
> oracle), and [`fsm-spawning.md`](fsm-spawning.md) (the *concurrent*
> sibling of nested-FSM-as-value — distinguished in § 2).

## § 1 — What a "nested FSM" is, and the surface syntax

### The construct

A **nested FSM** is a parent FSM (or claim) whose body needs the *final
state / accumulated output* of running another FSM `F` to completion.
The parent doesn't want one step of `F`; it wants `F` driven from an
initial state through however many ticks it takes to halt, and the
*result* bound into the parent's scope. Three motivating shapes, all
already in the corpus:

- **A sub-computation that loops.** A counter run-to-zero, an iterative
  refinement that converges. The result is the terminal scalar / record.
  (`examples/test_34_halts_within.ev`'s `decrement` is exactly this
  transition, today only *verified*, not *run for a value*.)
- **A tree-walk driven to drain.** `loop-functionizer.md` § 4's
  `walk_step` — pop a node, push children, fold the accumulator — run
  until the work-stack empties. The result is the accumulated set /
  string (`subscriptions::walk_expr`'s `reads`/`writes`).
- **A fixpoint.** Iterate a transition until the state stops changing;
  the result is the fixed point.

In every case the parent reaches a point where it must *consume a value
that only exists after another FSM has run*.

### Distinguished from `halts_within(F, N)`

CC's `halts_within(F, N)` (`fsm-halts-within.md`) and a nested-FSM run
look superficially similar — both name an FSM `F` and care about its
halt — but they ask **opposite kinds of question**, on opposite sides of
the *execute vs verify* line (§ 5, and JJ's `selection-policy.md` § 5):

| | `halts_within(F, N)` | nested-FSM run |
|---|---|---|
| Question | *Does* `F` halt within `N` ticks? | *Run* `F`; what is its final state? |
| Kind | **verify** — a ∀/∃ property | **execute** — one trajectory's result |
| Produces | a `Bool` / SAT-UNSAT verdict | a **value** (final state / accumulator) |
| Runs `F`? | No — composes the transition symbolically, never ticks it | Yes (tier 3) or computes the equivalent value (tiers 1/2) |
| Over which inits? | all of them (init free) or a pinned one | exactly the one supplied |

`halts_within` is a *proof obligation*; a nested-FSM run is a *call*.
The two share machinery — tier 1 of this selector reuses CC's composer
(§ 3) — but the contract differs: `halts_within` asserts the halt
witness `∃k∈[1,N] : halt_k`; a nested run *returns* `state_k` at the
halting `k`.

### The surface syntax

Three candidates were weighed.

**(a) An explicit run-to-halt call: `result = run(F, initial)`.** `run`
is a runtime-recognized builtin in expression position; `F` names a
registered FSM-shaped schema (the same way `halts_within`'s first
argument does), `initial` seeds its state, and the expression evaluates
to `F`'s final state.

**(b) A claim-call the runtime recognizes as FSM-shaped and runs.**
Write `result = F(initial)` and have the runtime notice that `F` is
FSM-shaped (state pair + `halt`) and run it to completion instead of
inlining one step.

**(c) Reuse the subclaim machinery with a "this one loops" marker** —
e.g. `subclaim Loop ⟲ …`, a decorated subclaim the runtime drains.

**Recommendation: (a), `result = run(F, initial)`.** The justification
is almost entirely about the *detection* problem the SESSION flags — "how
does the runtime know this call is a nested-FSM run vs an ordinary claim
inline?" — and (a) is the only candidate that answers it cleanly:

- **Detection is trivial and local.** `run(IDENT, …)` in expression
  position is the signal — nothing else. The parser intercepts it
  exactly as `runtime/src/parser/body_item.rs` already intercepts
  `halts_within(IDENT, N)` at body-item level. No body-shape analysis,
  no heuristic. (a) is a slightly *larger* hook than `halts_within`
  because it is value-producing — it lives in expression position, not
  just at body-item level — but the recognition rule is the same one
  keyword.
- **(b) is dangerous.** It silently overloads ordinary claim-call syntax:
  the same `F(x)` means "inline one step" for most claims and "run to
  halt" for FSM-shaped ones. A claim that *becomes* FSM-shaped (someone
  adds a `halt`) would silently flip from a single inline to an unbounded
  run — a footgun in the family of the `True`/`true` and `⇒`-precedence
  traps the project already documents. Worse, the cost model inverts
  invisibly: a one-step inline and a full run-to-halt should not share
  surface syntax, because their costs differ by orders of magnitude.
- **(a) makes the cost visible.** `run(...)` reads as "this is an
  execution, possibly an expensive one" — the reader sees that a whole
  FSM is driven here, the way a programmer sees a loop. Cost that is
  invisible in the source is cost nobody budgets for.
- **(c)** couples a value-returning construct to dispatch machinery built
  for *branch selection*, not iteration; it muddies the subclaim's
  meaning and still needs a marker the reader has to learn. No advantage
  over (a).
- **`F` needs no new "claim-as-value" type.** Like `halts_within`, `run`
  takes `F` *by name* — a reference to a registered schema resolved at
  translate time, not a first-class function value. No higher-order
  machinery is introduced.

**Seeding the initial state.** `initial` maps to the FSM's state the
same way the scheduler already seeds spawned FSMs (`effect_loop/state.rs`
`seed_state_with_arg`): a bare `Int` seeds a single-Int-payload first
variant (`run(decrement, 50)` → state `count = 50`); a record/enum value
seeds the matching state type (`run(walk, ⟨root⟩)` → `stack = ⟨root⟩`).
Whatever the scheduler can seed, `run` can seed — by construction,
because tier 3 *is* the scheduler (§ 2).

**Detection summary.** `run(F, init)` ⇒ nested-FSM run; any other
`G(args)` ⇒ ordinary claim inline (unchanged). The runtime then resolves
`F` via `resolve_fsm` / the `halt`-convention check and routes it through
the selector (§ 3). An ordinary claim that is *not* FSM-shaped passed to
`run` is a load-time error ("`run`'s first argument must name an
FSM-shaped schema — a state pair + `halt ∈ Bool`").

## § 2 — Strategy 3: blocking-interpret (the correctness baseline)

Blocking-interpret is the strategy that **compiles nothing**: it runs
`F` on the existing multi-FSM scheduler, to halt, and hands the final
state back. It is the universal fallback and the oracle.

### Control flow

```text
parent query reaches   result = run(F, init)
   │
   ├─ parent SUSPENDS at this expression
   │
   ├─ seed a sub-scheduler:  fsms = [ resolve_fsm(rt, F) ]
   │                         state seeded from `init` (seed_state / seed_state_with_arg)
   │                         fresh isolated world + DispatchContext
   │
   ├─ tick to halt via the EXISTING effect_loop:
   │     run_with_ctx(rt, &LoopOpts { max_steps }, &mut nested_ctx)
   │     (each tick: solve/JIT F's step, decode state_next, loop)
   │
   ├─ read F's final state at halt  (state::model_matches_value, or
   │                                 scheduler quiescence, or Exit)
   │
   ├─ marshal final state Value back as the value of run(F, init)
   │
   └─ parent RESUMES with `result` bound
```

### How it reuses `runtime/src/effect_loop/`

It is a **recursive `effect_loop::run_with_ctx` on a single-FSM
sub-scheduler instance** — not a new scheduler, not a forked loop. The
existing scheduler already does everything required:

- It takes a `Vec<MainShape>` of FSMs and ticks them to halt
  (`scheduler::run_scheduler`). A nested run passes a one-element vec,
  `[resolve_fsm(rt, F)]`.
- It already **seeds** FSM state from a value and **grows its FSM set at
  runtime** — that is exactly what `Effect::SpawnFsm` does
  (`scheduler.rs` Phase 3d, `seed_state_with_arg`). The nested run is the
  same seed-and-schedule, scoped to one FSM and *waited on* instead of
  fired-and-forgotten.
- It already detects halt (`state::model_matches_value` — the `Done` /
  `Halt` variant convention) and caps iterations (`LoopOpts.max_steps`,
  default 10 000).

So blocking-interpret is a thin `run_nested(rt, F, init) -> Value`
adapter onto `run_with_ctx`. The new code is the adapter, the world
isolation, and the final-state read — *not* a scheduler.

**State isolation.** The nested FSM's ticks must not bleed into the
parent's world. Two isolation rules:

1. **Fresh world.** The sub-scheduler builds its own world snapshot,
   independent of the parent's. By default it is *empty* (the nested FSM
   is a pure function of `init`; anything it needs must arrive through
   `init`). The parent's world is never written by the nested run.
2. **Scoped `DispatchContext`.** The nested run gets its own
   `DispatchContext` so its handle registry, stdin ownership, and
   `pending_spawns` are private. (For an effect-free FSM — the v1
   restriction, § 5 — this context dispatches nothing.)

Only the **final state Value** crosses back. If a nested FSM legitimately
needs to *read* parent world, that field is passed in via `init` (making
the dependency explicit and `run` still a pure function of its
arguments); a nested FSM never reaches up into the live parent world.

### Halt

Reuse CC's `halt ∈ Bool` convention as the primary signal — the *same*
`halt` `halts_within` reads — plus the scheduler's existing implicit
halt (a tick in which no FSM is scheduled: nothing more can happen) and
`Effect::Exit`. The **max-iteration guard** is `LoopOpts.max_steps`: a
nested FSM whose `halt` never fires fails loudly at the cap rather than
hanging. This is the scheduler-level analogue of the loop-functionizer's
native `max_iters` (`loop-functionizer.md` § 3, "Termination") — same
role, one layer up: scheduler steps instead of native-loop iterations.

### Why it is always correct

Blocking-interpret **is the same execution path as a top-level FSM**,
driven to completion inline. The nested FSM is scheduled and ticked by
the exact `run_scheduler` that runs top-level FSMs; there are no new
semantics to get wrong. The proposition is short:

> If `F` runs correctly as a top-level FSM, it runs correctly nested.

That equivalence is what makes tier 3 the baseline and the oracle (§ 4):
it inherits the correctness of the scheduler wholesale, adding only "seed
from `init`, read the final state."

### Cost

A **full scheduler run per `run(F, init)` invocation**, with no
compilation. Each tick is whatever the scheduler normally costs per
tick — which is *not* necessarily a Z3 solve: the per-component JIT
(`query.rs` `try_functionize_z3`) already compiles most step bodies, so a
nested tick is often a native step call. But the *loop* is the
scheduler's round-trip (encode state → solve/JIT → decode → repeat), with
its per-tick overhead, run N times. This is the slow path: acceptable as
a baseline and as the fallback for bodies the faster tiers refuse, but
not for a hot loop that runs every parent tick (see § 8). Tiers 1 and 2
exist precisely to collapse this cost.

### The "block the parent" question

The SESSION raises it directly: does the parent's own loop **block**
while the nested FSM runs (synchronous, simplest, correct), or does the
nested FSM get **its own state and run concurrently**?

**Recommendation: synchronous-blocking for the baseline.** Reasons:

- **Simplest.** It is a function call that happens to be expensive. The
  parent suspends, the result is computed, the parent resumes. No
  scheduling interleave, no completion polling, no partial results.
- **Correct by construction.** The full result is available before the
  parent resumes, so `run(F, init)` is a plain value — it slots into the
  parent's constraint solve like any other bound variable. Referential
  transparency (§ 5) falls out trivially: the nested run is a closed
  computation over `init`.
- **Isolation is clean.** A blocked sub-scheduler with its own world and
  context shares nothing with the parent's tick; there is no concurrent
  write to reason about.

**The trade.** Synchronous blocking *serializes*: a parent needing two
independent nested runs does them in sequence, not in parallel, and a
long nested run stalls the parent's tick. For a *correctness baseline*
that is the right call — we are buying "always right," not throughput.

**Distinguish from spawning** (`fsm-spawning.md`). The "nested FSM gets
its own state and runs concurrently" alternative *is* the spawning
model: `Effect::SpawnFsm` pushes a new, independent FSM instance into the
*same* scheduler, fire-and-forget, communicating via world fields. That
is a **concurrency primitive** — N actors coordinating through shared
state — **not a value-returning sub-call**. Nested-FSM-as-value
deliberately chooses the value primitive: one synchronous run, one
returned result, no shared world. The two are siblings that share seed
plumbing (both use `seed_state*`), but they are different features and
should not be conflated. A future concurrent nested-run (parallelize
independent `run`s, return when all complete) is possible but complicates
the value semantics and is out of scope for v1.

## § 3 — The selector / precedence

### The model: the functionizer fall-through in `query.rs`

The runtime already has a precedence chooser for *single solves*, and the
nested-FSM selector mirrors it one level up. In
`runtime/src/runtime/query.rs`:

```text
query(name, given)
   ├─ if EVIDENT_FUNCTIONIZE (default on):
   │     try_functionize_z3(name, schema, given)
   │        ├─ decompose body into components
   │        ├─ compile_one_component(...) per component →
   │        │     ComponentOutcome::{ Compiled(fn) | Slow | Bail }
   │        ├─ cache the ClaimPlan, keyed on (schema, structural-signature)
   │        └─ Some(result)  on a full plan
   │     └─ Some → return it
   └─ fall through → evaluate(...)   -- the full Z3 solve (always correct)
```

The shape to copy: **try the fast path; on refusal fall through to a
slow path that is always correct; cache the chosen plan per body.** The
functionizer's "always correct" floor is the full Z3 solve. The
nested-FSM selector's floor is blocking-interpret.

### The analogous nested-FSM chooser

```text
run(F, init)
   ├─ plan = nested_strategy_for(F)        -- cached per F's body (below)
   ├─ match plan:
   │     Tier1(jit) => jit.call(init)      -- symbolic-unroll → final-state value, JIT'd
   │     Tier2(loopfn) => loopfn.call(init)-- native while-loop over the compiled step
   │     Tier3 => run_nested(rt, F, init)  -- blocking-interpret (§ 2)
   └─ a faster tier may still bail at call time (a step refuses mid-run);
      on bail, fall through to the next tier — Tier3 never bails.
```

### Per-strategy applicability detector

| Tier | Strategy | Detector | Inputs inspected |
|---|---|---|---|
| 1 | symbolic-unroll → JIT | **CC's affine-step detector accepts** (`fsm_unroll/detector.rs`: probe 3 doublings, last-doubling state-node ratio < 1.5) | `F`'s simplified transition body; the doubling ratios |
| 2 | loop-functionizer | **the step is Cranelift-compilable** (`functionizer.compile(step_program)` returns `Some`) AND a clean loop contract exists (`fsm_unroll/compose.rs::detect_state_pairs` finds the `(x, x_next)` pairs + `halt` or a work-stack) | `F`'s step `Z3Program`; the functionizer's accept/refuse; the state-pair detection |
| 3 | blocking-interpret | **always** — no detector | (none) |

### Fall-through order + decision logic

The selector tries the tiers **in order**, taking the first applicable:

1. **Tier 1.** Run CC's affine detector on `F`. If it *accepts*, the
   transition collapses to closed form, so `F^k(init)` is a closed-form
   value: compose `F` (CC's `compose.rs`), solve for the halt step `k`
   (for an affine body `k` is itself a closed-form function of `init` —
   `count=50, halt=count≤0` ⇒ `k=51`), substitute `init`, read the
   final-state expression, and JIT it via the existing Cranelift
   functionizer. This is the SESSION's "JIT-wiring TODO": the unroll
   *landed* (it builds `F^N` and the halt aggregate); what remains is
   wiring it to **return the final-state value** rather than only assert
   the halt `Bool`.
2. **Tier 2.** If the affine detector *refuses* (branching body — the
   common case for tree-walks and game steps), try to functionize `F`'s
   step. If `functionizer.compile(step) = Some`, build the
   `loop-functionizer.md` `LoopFn` (the compiled step + a `LoopContract`
   from `detect_state_pairs`) and use it. The branch is then a
   per-iteration `ite`, never a growing formula.
3. **Tier 3.** If the step *refuses* (`compile = None` — an unsupported
   shape in the step itself), fall through to blocking-interpret. Always
   applicable.

The decision inputs are therefore exactly three, all already computed by
existing machinery: **(i)** `F`'s body shape (state pair + `halt`
presence, via `detect_state_pairs` / `MainShape` resolution), **(ii)**
the affine-step detector verdict (`fsm_unroll/detector.rs`), **(iii)**
the step-functionizer's accept/refuse (`Functionizer::compile`).

> **A subtlety worth stating: tier 1 here is not `halts_within`.**
> `halts_within` produces a *constraint* (the halt witness); a nested run
> needs a *value* (the final state). Tier 1 reuses CC's composer but asks
> a different question of it — "what is `F^k(init)`?" not "is `halt_k`
> ever true?". The composer already builds the composed *state*
> transition (that is what collapses for affine bodies); tier 1 reads the
> composed state expression at the halting `k`, instead of reading the
> halt disjunction. Same machinery, execute-side question (§ 5).

### Where the selection happens

**Cached per `F`'s body, decided at first invocation** — mirroring the
functionizer plan cache (`fn_cache`, keyed on structural signature):

- **Not load time.** Load time doesn't know `F` will be `run`, and the
  affine detector and step-functionizer both want `F`'s *simplified*
  body, which the first call materializes via `build_cache`. Deciding
  early would either do that work eagerly for every FSM or guess.
- **At first `run(F, ·)`.** Materialize `F`'s body, run the detectors,
  pick the tier, and **cache the plan keyed on `F`'s structural
  signature** (the body, *not* `init` — the strategy depends on `F`'s
  shape, not the particular initial state). Subsequent `run(F, init')`
  calls reuse the plan and just supply a new `init`. This is
  `try_functionize_z3`'s "compile once, cache the plan" applied to
  strategy selection.
- **Invalidation** matches the functionizer plan cache: a schema change
  or functionizer change drops the plan; the structural signature on
  `F`'s body is the key, so a body edit re-selects.

Gate the whole path behind an env var — `EVIDENT_NESTED_STRATEGY`
(values `auto` | `unroll` | `loop` | `blocking`, default `auto`) —
mirroring `EVIDENT_FUNCTIONIZE` / `EVIDENT_SATISFIER` /
`EVIDENT_RESIDUAL`. `auto` runs the fall-through above; the explicit
values force one tier, which is precisely what the equivalence harness
needs (§ 4).

## § 4 — Blocking-interpret as the equivalence oracle

The faster tiers must produce **the same value** as tier 3 on the same
`init`. They compute the same mathematical function (run `F` to halt);
any divergence is a bug in the faster tier, never a "known difference."

### The equivalence-test pattern

Mirror the self-hosting cross-validation tests already in the tree —
`runtime/tests/subscriptions_equivalence.rs`,
`runtime/tests/validate_equivalence.rs`,
`runtime/tests/pretty_equivalence.rs`. Those run a corpus through two
implementations and assert byte-identical output, with the Rust impl
canonical. Here:

- **Corpus**: a set of nested-FSM fixtures, smallest-first — the counter
  (`run(decrement, 50)` → `0`), a sum-a-tree (`enum Tree = Leaf(Int) |
  Node(Tree, Tree)`, `run(sum, root)` → the recursive sum), the
  `walk_step` of `loop-functionizer.md` § 4 (`run(walk, ⟨body⟩)` → the
  `reads`/`writes` sets).
- **For each fixture**, run `run(F, init)` under **each applicable
  strategy** — forced via `EVIDENT_NESTED_STRATEGY={blocking,loop,unroll}`
  (§ 3) — and assert the returned Values are identical, with the
  **`blocking` result canonical**.
- **No known-divergence tier.** Unlike `pretty_equivalence` (which has a
  sentinel tier for shapes the Evident impl can't yet render), the three
  nested strategies compute the *same function by definition*. A
  divergence is always a bug — there is nothing to whitelist.

```text
for fixture in CORPUS:
    base = run_forced(fixture, "blocking")            -- canonical
    if affine(fixture.F):  assert run_forced(fixture, "unroll")  == base
    if step_compiles(F):   assert run_forced(fixture, "loop")    == base
```

### Why tier 3 is built first

Two reasons, both decisive:

1. **It is the oracle.** You cannot write the tier-2 / tier-1 equivalence
   tests until tier 3 exists to be the canonical answer. The optimized
   tiers are *defined as* "faster ways to get the blocking-interpret
   result"; without that result there is nothing to check against. (For
   the *recursive* class this is sharper still — Z3 is **not** a sound
   oracle there, per `loop-functionizer.md` § 6: a Z3 solve over an
   unbounded recursion is the recursion gap, COUNTEREXAMPLES #15. Tier 3
   *runs* the walk, so it is the only available oracle for that class.
   See § 6.)
2. **It makes nested FSMs work — correctly, if slowly — before any
   compilation exists.** Tier 3 reuses 100% of the scheduler, so it is
   the cheapest tier to build (§ 7) and the most obviously correct. Ship
   it, and `run(F, init)` works everywhere; the later tiers then
   *accelerate* a feature that already functions, validated at every step
   against the baseline that shipped first.

## § 5 — Referential transparency + the two invariants

This selector lives entirely on the **execute** side of JJ's two
invariants (`selection-policy.md` § 5), and must honor both.

### Referential transparency

`run(F, init)` must be a **pure function of `init`**: same `init` → same
result, regardless of which tier ran it. The runtime's value cache keys
on `(claim, given-keys, given-values)` and the FSM scheduler replays /
memoizes across ticks; a nested run that returned different values for
the same `init` would poison that cache and break replay.

- **Tier 3 is pure** because the nested run is a *closed computation over
  `init`* in an isolated, empty world (§ 2): no shared mutable state, no
  reach into the parent. Given the same `init`, the sub-scheduler ticks
  the same trajectory to the same final state.
- **Tiers 1 and 2 are pure** because they are deterministic functions of
  `init` (a closed-form JIT'd expression; a native loop with no
  nondeterminism). § 4's equivalence guarantee *is* the cross-tier purity
  guarantee: all three agree, so the result is independent of the tier.
- **The obligation propagates to `F`'s body.** Determinism holds only if
  `F` itself is deterministic — no unseeded randomness, no wall-clock
  read, no async-source dependence. An `F` that subscribes to a
  `FrameTimer` / stdin source is *not* a pure function of `init` and must
  not be `run` as a value (it is a top-level / spawned FSM instead).

### Interaction with effects

If the nested FSM *emits effects* (`Println`, `LibCall`, …), is
`run(F, init)` still a pure function? **No** — effects are observable; a
run that printed once per call is not referentially transparent, and the
value cache would suppress the second print. Therefore:

> **v1 restriction: `run(F, init)` is permitted only for effect-free
> FSMs.** `F` must declare no `effects` membership, or its `effects` must
> be provably always `⟨⟩`. Checked at load — an `F` that can emit a
> non-empty effect list is rejected ("`run`'s target must be
> effect-free; `F` emits effects — run it as a top-level or spawned FSM
> instead").

This is not a permanent limit, just the honest v1 line. A future
**effect-collecting** nested run could return *both* the final state and
the *accumulated effect Seq* as a value (effects-as-data: the nested run
**does not dispatch**, it hands the effect list back for the parent to
dispatch in its own tick). That preserves purity — the nested run
produces only data — and is a clean extension, but it widens `run`'s
return type and is out of scope here. Noted in § 8.

### Execute vs verify

A nested run **explores one point** of `F`'s behavior — the trajectory
from `init`. It returns *a* result; it does **not** prove a ∀-property
("`F` halts for every init," "`F`'s output always satisfies P"). That is
`halts_within`'s job (verify), or a full Z3 solve's. The split is the
same one the runtime already draws for `define-fun-rec` (evaluates,
doesn't prove) and for the loop-functionizer / CEGAR work (the cheap
artifact *executes*; the oracle *verifies*). Tier 1 is the interesting
boundary case: it reuses CC's *verification* composer to *execute*
(extract a value) — the machinery is shared across the line, the
question asked is not.

## § 6 — Relationship to the other strategies + composition

### The three tiers compute one function, three ways

All three tiers compute the same thing — `F` run to halt on `init` — and
differ only in **where the repetition lives**, the exact axis
`loop-functionizer.md` § 1 draws, now extended with tier 3:

| Tier | Repetition lives | Produces | Survives | Cost |
|---|---|---|---|---|
| 1 symbolic-unroll → JIT | **compile time**, in Z3, symbolically | closed-form value, JIT'd | affine bodies only | O(1) per call (after compile) |
| 2 loop-functionizer | **run time**, native `while` over the compiled step | value | branching OK; step must JIT | O(N) native steps (~µs each) |
| 3 blocking-interpret | **run time**, the multi-FSM scheduler | value | anything | O(N) scheduler ticks |

Tier 2 *is* the loop-functionizer: II's `LoopFn` — a compiled step
wrapped in a native run-to-halt loop — is precisely this selector's
middle tier. Tier 1 *is* CC's unroll, with the final-state-value wiring
added. Tier 3 is new, and is the floor.

### "The stack is a stack of FSMs" — three realizations

`loop-functionizer.md` § 4's claim ("a recursive tree-walk is an
iteration over an explicit work-stack") realizes differently per tier,
but it is the *same abstraction* — drain a work-stack to fixpoint:

- **Tier 3:** the stack lives in `F`'s **state**, drained by **scheduler
  ticks** (II's design A — marshal the whole stack through the step each
  tick). O(n²) marshaling for an n-node walk, but correct and needing no
  new machinery: it is just an FSM whose state carries a `Seq`.
- **Tier 2:** the stack lives in a **native `Vec<Value>`** held by the
  loop wrapper (II's recommended design B — `WorkStack`). O(n), native
  push/pop; the step is a pure per-node function.
- **Tier 1:** the stack is **symbolic** — which only collapses for an
  *affine* walk, so tier 1 essentially never applies to tree-walks (their
  branching dispatch is exactly what the affine detector refuses). Tree
  walks live at tiers 2/3.

Same work-stack drain, three substrates: scheduler work / native `Vec` /
symbolic composition.

### CEGAR (GG): tier 3 is the ultimate oracle

In CEGAR's vocabulary (`cegar-scaffolding.md`), tiers 1 and 2 are
**abstractions** — fast callables — and the run is audited against an
**oracle of truth**. For a single solve the oracle is the full Z3 solve.
For a *nested-FSM run* the oracle is **blocking-interpret on the real
scheduler**, and this matters sharply:

> For the recursive tree-walk class, **Z3 is not a sound oracle** — a Z3
> solve over an unbounded recursion *is* the recursion gap
> (`loop-functionizer.md` § 6, COUNTEREXAMPLES #15): unconstrained
> leaves come back SAT-but-arbitrary. **Tier 3 is sound there**, because
> it actually *runs* the walk to a fully-constrained, concrete result.

So blocking-interpret fills the oracle role precisely where Z3 cannot.
Where the functionizer's CEGAR loop falls back `None → Z3`, the
nested-FSM selector falls back `tier1/2 refuse → tier3`, and tier 3 is
the ground truth the faster tiers are refined against (§ 4). It is the
ultimate oracle in the literal sense: there is nothing below it to defer
to.

## § 7 — Implementation plan

Build smallest-and-most-foundational first. The ordering is forced by
the oracle relationship (§ 4): the baseline must exist before anything
can be validated against it.

### 1. Tier 3 — blocking-interpret (the baseline + oracle) — **LANDED (session LL)**

**Built first**, as planned. Shipped:

- **Parser + AST.** `run(F, init)` is `Expr::RunFsm { fsm, init }`
  (`core/ast.rs`), intercepted in expression position by the atom
  parser (`parser/atoms.rs`) — the value-producing sibling of
  body-item-level `halts_within`. `run(IDENT, …)` is the trigger;
  anything else named `run` falls through to a normal call.
- **Execution** — `effect_loop/nested.rs::run_nested(rt, F, init,
  max_steps) -> Value`. It drives `F` with the **same per-tick solve the
  scheduler uses** (`query_with_pins_and_given`) to halt, reusing the
  scheduler's state-encode (`state::encode_state_value`) and the
  `LoopOpts.max_steps` cap as its **max-iteration guard** (overrun =
  loud error, never a hang).
  - *Why a dedicated loop, not `run_with_ctx` verbatim:* the prescribed
    baseline FSM (`decrement`) is the `halts_within` shape — a `claim`
    with a primitive `count, count_next ∈ Int` state pair + `halt ∈
    Bool` — which the scheduler's enum-state/`fsm`-keyword resolution
    (`resolve_fsm`, `MainShape`) doesn't recognise, and `run_scheduler`
    reports a *best-effort `Done`-variant* final state (losing the
    carried value), whereas `run` needs the **full** final state value.
    So tier 3 reuses the scheduler's *primitives* (per-tick solve, state
    encode, step cap) rather than `run_scheduler` wholesale. Halt is the
    explicit `halt ∈ Bool` (read on each tick's input state), so the run
    returns the state at the first halting tick.
- **Evaluation timing (the crux), resolved.** `run(F, init)` is
  evaluated to a concrete `Value` **before the outer solve** and the
  `RunFsm` node is **rewritten to a literal expression** — the same
  "compute a value, pin it as a constant" discipline a `given` follows.
  The hook is `runtime/nested.rs::resolve_runs`, called at the top of
  every query entry point (`query`, `query_with_core`, `query_cached`,
  `query_with_pins_and_given`); a body with no `run` skips it (no clone).
  - **v1 restriction (stated):** `init` must be computable from values
    known at that point — literals, the query's givens, or integer
    arithmetic over those (`eval_const_init`). An `init` depending on an
    undetermined outer variable is a **loud error**, never a silent
    wrong value. (No solve→run→solve cycle in v1.)
  - Run-containing bodies bypass the functionizer/value caches in v1
    (those key on given-*keys*, but a `run`'s literal depends on given-
    *values*), keeping them on the always-fresh Z3 path.
- **Seeding** mirrors the scheduler: a bare `Int` seeds a primitive Int
  state directly, or the state enum's first single-Int-payload variant
  (the `seed_state_with_arg` convention); an enum/record value seeds the
  matching state type.
- **`EVIDENT_NESTED_STRATEGY`** gate (`auto` | `blocking` | `loop` |
  `unroll`, default `auto` → `blocking`). Forcing `loop`/`unroll` errors
  clearly — those tiers land later.
- **v1 restrictions enforced at load** (`runtime/nested.rs::
  validate_run_targets` + `effect_loop::validate_run_target`): a non-FSM
  -shaped `F` (no state pair / no `halt ∈ Bool`) or an **effect-emitting**
  `F` (§ 5) is rejected at load with a clear message; a non-halting `F`
  hits the max-iteration guard at run time.

**Proof + tests.**
- `examples/test_35_run_fsm.ev` — the counter (`run(decrement, 50)` → 0)
  as a value, an enum-state seeding variant (`run(accumulate, 0)` →
  `Acc(5)`), `sat_*` claims asserting each, and a runnable `fsm main`
  that computes the run and exits (one row in `runtime/tests/demos.rs`).
- `runtime/tests/run_fsm.rs` — the equivalence-oracle harness: the
  counter's final state, Int + enum seeding, end-to-end pinning into an
  outer query, load-time rejection of a non-FSM / effect-emitting `F`,
  and the max-iteration guard. Structured so a later session adds
  "tier 2/1 result == tier 3 result" against the same `oracle(...)`.

**Not yet `run_with_ctx`-based.** Driving an enum-state, `fsm`-keyword'd
`F` through the *real* `run_scheduler` (returning its full final state)
is the natural extension once the value-return contract is reconciled
with the scheduler's implicit-halt model; tier 3 v1 deliberately reuses
the per-tick primitives instead.

*Size:* small–medium. *Risk:* low — it is orchestration over the existing
scheduler; the scheduler's correctness is inherited. The two real pieces
of new work are the **value-position parser hook** (`run` returns a value,
unlike body-item-only `halts_within`) and **world isolation**.

### 2. Tier 2 — loop-functionizer

Per `loop-functionizer.md` § 7, which already breaks it into three
landable steps (the `LoopFn`/`LoopContract` mechanism on the counter; the
`WorkStack` on a toy tree; self-hosting `walk_step`). Wire the resulting
`LoopFn` in as a tier-2 plan. Validate against tier 3 (§ 4) at each step.

*Size:* medium (II's three steps). *Risk:* medium — depends on the
step-JIT compiling the body shapes; refusals fall through to tier 3, so
correctness holds even where speed doesn't.

### 3. Tier 1 — symbolic-unroll → JIT wiring

The unroll *landed* (`fsm_unroll/`); the remaining work is wiring it to
**return the final-state value** rather than the halt witness (§ 3):
solve for the halt step `k` from `init`, read `F^k`'s composed
state expression, substitute `init`, and JIT via the existing Cranelift
functionizer. Validate against tier 3 on the affine fixtures (the
counter).

*Size:* medium. *Risk:* medium, but *narrow* — affine bodies only, so the
applicability is small and the failure mode is "detector refuses, fall to
tier 2/3," never a wrong value.

### 4. The selector

The fall-through chooser (§ 3): the per-`F`-body detector cascade, the
plan cache keyed on `F`'s structural signature, the `auto` mode of
`EVIDENT_NESTED_STRATEGY`. Mirror `try_functionize_z3`'s structure
directly.

*Size:* small. *Risk:* low — it is plumbing over tiers that already exist
and are already individually validated.

### The dependency to respect

**The selector needs ≥ 2 strategies to be worth building.** Until tier 2
or tier 1 lands, there is nothing to choose between, and **tier 3 alone
is the implementation** — every `run` goes to blocking-interpret. So the
selector is step 4, not step 1; steps 1–3 each ship a usable feature
(step 1: nested FSMs work; steps 2–3: they get fast on their respective
body classes) before the chooser ties them together.

## § 8 — Limits & open questions

Honest unknowns, roughly in order of how much they bite:

- **Hot-path cost of repeated blocking-interpret.** A parent that calls
  `run(F, init)` *every tick* re-runs the whole sub-scheduler every tick.
  The value cache helps only when `init` repeats across ticks (then the
  memoized result is returned without re-running). When `init` varies
  every tick, tier 3 is genuinely expensive — which is the entire
  motivation for tiers 1/2. The selector's job is to keep hot `run`s off
  tier 3; the open question is whether the per-body strategy choice is
  enough, or whether a hot-`run` site needs its own caching policy.

- **Effects in nested FSMs.** v1 forbids them (§ 5). The effect-collecting
  extension (return the accumulated effect Seq as data, parent dispatches)
  is the natural next step but widens `run`'s return type from "final
  state" to "(final state, effects)" and needs a decoding story for the
  effect list as a value.

- **Nested-FSM recursion depth.** An `F` whose body itself contains
  `run(G, …)`, whose `G` contains `run(H, …)`. The sub-scheduler is
  driving FSMs that may suspend on their own nested runs. This is a real
  recursion (the *parent* stack, not the work-stack) and needs a max
  nesting-depth guard analogous to `EVIDENT_MAX_INLINE_DEPTH`, plus a
  clear cost story (costs compound multiplicatively).

- **State isolation granularity.** § 2 recommends a fresh *empty* world,
  with any needed parent state passed through `init`. The open question
  is the ergonomics when a nested FSM needs to read a lot of parent
  world: is threading it all through `init` reasonable, or is a
  read-only copy-on-write snapshot of the parent world worth the
  complexity (and the risk of re-introducing the "is it really pure?"
  question)?

- **Tier-1 halt-step determination.** For the pure counter the halt step
  `k` is a one-line closed form. For a less trivial affine recurrence
  (`x_next = a·x + b`, `halt = x ≥ T`), solving for the smallest `k` with
  `halt_k` is still closed-form but more involved; the boundary of "what
  the composer can solve `k` for" coincides with — but may be narrower
  than — the affine detector's accept set. Worth measuring.

- **Selector cache invalidation.** The plan is keyed on `F`'s structural
  signature (§ 3). The open edge: a functionizer swap (e.g. enabling
  `EVIDENT_SATISFIER`) changes which step bodies compile, hence the
  tier-2 detector's verdict — the nested-strategy cache must invalidate
  on functionizer change, exactly as the `fn_cache` does.

- **Relationship to spawning.** Nested-run (synchronous value) and
  `Effect::SpawnFsm` (concurrent fire-and-forget) share seed plumbing but
  differ in semantics (§ 2). Whether they should eventually share a
  single "instantiate an FSM" primitive — with `run` as the "await it"
  variant and `spawn` as the "detach it" variant — is an open design
  question that `fsm-spawning.md` and this doc jointly bound.
