# FSMs as constraints — the implementation spec

> **What this is.** [`fsms-as-functions.md`](fsms-as-functions.md) is the
> *concept*: an `fsm` is a transition system; nesting one inside another
> schema lets the **parent constrain the child's whole run**; the three
> execution tiers are one idea on a condensability gradient. This doc is
> the *buildable plan*. It supersedes an earlier draft of this file that
> framed the embed as a functional `result = F(init)` and kept `run()` /
> `halts_within` as deprecated aliases — **both of those are rejected**
> (see the box below). The corrected model is:
>
> > **An `fsm` embedded in another schema is a CONSTRAINT, written
> > `F(seed, fsm_state)`. There is no return value. `fsm_state` is an
> > ordinary parent-scope variable that the constraint binds to F's
> > settled state — and that the parent may *further constrain*. The
> > terse form (`fsm F(state ∈ T, halt ∈ Bool)` + `_state`) is the ONLY
> > way to write a transition; `state_next` as a source pattern is a
> > load-time error; `run()` and `halts_within` are removed outright.**
>
> ### Why the earlier draft was wrong (the corrections this rewrite makes)
>
> 1. **No return value.** `result ∈ T = F(init)` reads as a function call
>    that *returns*. An fsm does not return — "that is anathema to the
>    project." Everything in Evident is a constraint over a set of
>    variables. The embed must be a constraint too: `F(seed, fsm_state)`
>    relates a seed and a state variable, exactly as `F(state, state_next)`
>    relates the two halves of one tick. `fsm_state` is `state_next` lifted
>    to the parent's scope.
> 2. **The parent must be able to constrain the child.** Because
>    `fsm_state` is a plain parent variable, the parent can write
>    `fsm_state.count = 0` (or any predicate) alongside `F(seed, fsm_state)`.
>    The solver must find a run consistent with *both* the child's
>    transition *and* the parent's property. This is the load-bearing
>    capability — the user wants it "for every fsm I make" — and it is what
>    makes the embed a verification/synthesis harness, not a function call.
> 3. **`run()` / `halts_within` are gone, not aliased.** They are
>    redundant with the constraint surface (`run` *was* the return-value
>    form; `halts_within` is subsumed — see § 2). No deprecation alias;
>    the parser hooks are deleted and `Expr::RunFsm` is produced *only* by
>    the `F(seed, fsm_state)` lowering.
> 4. **Solvable two ways.** The same `F(seed, fsm_state)` constraint must
>    be dischargeable by **both** in-solve unrolling/CHC (when the body
>    condenses or fits a Spacer theory) **and** by execution (when it does
>    not). The lowering is one node; the *strategy selector* (§ 6) picks
>    the discharge mechanism. This is the condensability→guarantee
>    spectrum from `fsms-as-functions.md` § 5 and
>    `docs/research/fsm-behavioral-constraints.md` § 6.
>
> **Dependency.** The terse-form rewrite this spec relies on
> (`runtime/src/runtime/desugar.rs` + `inject.rs`) is being implemented by
> the in-flight **STATE-terse** session (universal `_state`). This spec is
> docs-only and parallel-safe; § 6 (terse) is already in flight, and the
> embed surface (§ 4) + ban (§ 5) fire after it lands.
>
> **Reading order to implement.** This doc, then the concept
> ([`fsms-as-functions.md`](fsms-as-functions.md)), then
> [`../research/fsm-behavioral-constraints.md`](../research/fsm-behavioral-constraints.md)
> (the engine decision — CHC/Spacer primary, BMC fallback, CEGAR for the
> recursive case), then [`nested-fsm-strategies.md`](nested-fsm-strategies.md)
> + [`loop-functionizer.md`](loop-functionizer.md) for the tier machinery.
> Source anchors are inline at each section.

---

## § 1 — The end state (one consistent story)

Four things become true, and they are one story — *a `SchemaDecl`'s
keyword is the rule, and an `fsm` is a constraint over (seed, settled
state)*:

1. **Terse is universal.** `fsm F(state ∈ T, halt ∈ Bool)` with `_state`
   for the previous tick is the way to write a transition, for **any**
   state var (enum / record / `Int`), in **any** FSM — scheduler-driven
   *or* embedded — **not just `world`**. `_state.X` reads the previous
   tick; `state.X = …` writes the current tick. The author never names a
   `_next` var. (Being implemented now — § 6.)

2. **An embedded `fsm` is a constraint: `F(seed, fsm_state)`.** No return
   value. `fsm_state` is a parent-scope variable bound to F's settled
   state when started from `seed`:

   ```evident
   fsm_state ∈ CountState           -- a plain parent variable
   countdown(seed, fsm_state)       -- constraint: fsm_state IS countdown's settled state from seed
   ```

   This **replaces** `run(F, init)` entirely. The disambiguator is the
   **keyword on `F`'s `SchemaDecl`**: `F(a, b)` where `F` is a `claim` →
   inline (conjunction, unchanged); where `F` is an `fsm` → the
   settled-state constraint (§ 4). Composite `seed` (a recursive-enum or
   `Seq` literal) flows exactly as today's `run(F, init)` did.

3. **The parent constrains the child.** Because `fsm_state` is an
   ordinary variable, the parent adds predicates over it (and over the
   seed):

   ```evident
   seed ≥ 0                         -- parent precondition over the input
   countdown(seed, fsm_state)       -- the child's transition
   fsm_state.count = 0              -- parent postcondition over the settled state
   ```

   The solver must satisfy the child's run *and* both parent predicates.
   This is verification (does the property hold for the chosen seed?) and,
   when the seed is left free, synthesis (find a seed making it hold). § 4
   lowers it; § 6 picks the engine.

4. **`state, state_next` as a source pattern is a load-time error.** An
   `fsm` (or embedded target) that declares a `state_next ∈ T` membership
   or writes `state_next = …` is rejected at load with a message showing
   the terse rewrite (§ 5). The ban is purely *source-level*: the internal
   IR still composes a `state, state_next` pair, synthesized by the terse
   rewrite (§ 3) and consumed by the detectors unchanged.

The internal IR is **unchanged**: the runtime still composes / schedules
a `state, state_next` pair (`effect_loop/fsm.rs::resolve_fsm`,
`fsm_unroll/compose.rs::detect_state_pairs`,
`effect_loop/nested.rs::detect_state_pairs`). `seed` is the pair's input
const; `fsm_state` is its settled output. The terse front-end synthesizes
the pair the machinery expects; the embed surface binds the parent's
`fsm_state` to the settled output. Smallest blast radius — and the same IR
serves both discharge mechanisms (§ 6).

---

## § 2 — `run()` and `halts_within` are removed (no alias)

The earlier draft kept `run(F, init)` as a one-release deprecated alias.
**Rejected.** `run` *is* the return-value form the project disowns; an
alias would keep the rejected mental model alive in the corpus. Remove it
outright in the same change that lands the embed surface.

- **Delete** the `run(...)` parser hook (`runtime/src/parser/atoms.rs:26-41`).
  After this, `Expr::RunFsm` is produced **only** by the `F(seed, fsm_state)`
  load-time lowering (§ 4). The downstream consumers of `RunFsm`
  (`resolve_runs`, `collect_run_targets`, `embedded_fsm_targets`,
  `expr_has_run`, the tier dispatch) are unchanged — they keep handling the
  node; only its *source* changes.
- **Migrate** every `run(F, i)` call site to `F(i, out)` in the same sweep
  (§ 7). Because the corpus is small and in-repo, this is a flag-day the
  test suite catches immediately — no incremental-alias window needed.

### `halts_within` is subsumed, not ported

`halts_within(F, N)` (`BodyItem::HaltsWithin`, lowered by
`fsm_unroll/compose.rs::assert_halts_within`) asked a Bool question: "does
F reach `halt` within N ticks?" In the corrected model that question is
**implicit in `F(seed, fsm_state)`**:

- `F(seed, fsm_state)` asserts `fsm_state` *is* a settled (halted) state
  reachable from `seed`. If F cannot halt (within the discharge mechanism's
  bound, § 6), there is no such `fsm_state` → the constraint is **UNSAT**
  (or `unknown` under CHC divergence). A parent that wants to *verify*
  termination simply embeds `F(seed, fsm_state)` and reads SAT/UNSAT.
- The "within N" bound is no longer a user-facing predicate; it is the
  **unroll depth / fixedpoint budget** the strategy selector carries
  (§ 6.2). A parent that wants a *bounded* termination check gets it from
  the BMC-fallback depth; an unbounded one from CHC.

So **`halts_within` is deleted too** — its `BodyItem` variant, its parser,
and `assert_halts_within` (the *unroller* `build_f1`/`double`/`series`
inside `compose.rs` is **kept** — it is reused to discharge
`F(seed, fsm_state)` in the condensable regime, § 6). What goes is the
*surface predicate*; what stays is the *unrolling engine* behind it.

> If a future need arises for an explicit bounded "halts within N as a
> Bool I can branch on," it is re-introduced as a *parent constraint over
> a tick-count variable* (`F(seed, fsm_state)` ∧ `fsm_state.ticks ≤ N`),
> not as a special predicate. Out of scope here; noted so the deletion is
> not mistaken for a capability loss.

---

## § 3 — How the terse form emits the internal pair

**Key decision: keep the `state, state_next` pair as the internal IR; ban
it only at the *source* level; generalize the front-end rewrite.** The
machinery (`fsm_unroll`/`nested`/the scheduler) is **unchanged** — it keeps
consuming the literal pair. Only the front-end rewrite (this section) and
the ban (§ 5) change.

*(This section is the design the **STATE-terse** session is implementing
now; it is restated here so the embed surface and ban build on a fixed
description.)*

### Where the rewrite lives

`runtime/src/runtime/desugar.rs::unify_world_syntax` is today the
**world-only** instance of exactly this rewrite. It:

1. fires only for `world ∈ World` with no `world_next` declared, and only
   when the body uses `_world.X`;
2. rewrites identifier strings: `_world.X` → `world.X` (prev read),
   `world.X` → `world_next.X` (current read/write);
3. injects `world_next ∈ World` so the scheduler's writer detection finds
   the pair.

**Generalize it to `unify_state_syntax`** — the same walk, with the
hardcoded `world` name replaced by "any *terse state var*." It runs at the
same point in the load pipeline (`runtime/src/runtime/load.rs`), *before*
`inject_fsm_params` and the three pair detectors, so they see the
already-paired IR.

> **Why generalize the rewrite rather than teach the machinery the terse
> form natively.** Three detectors require the literal pair
> (`resolve_fsm`, `compose.rs::detect_state_pairs`,
> `nested.rs::detect_state_pairs`). Rewriting at the front end touches
> **one** pass + the ban; every detector keeps working byte-for-byte.

### The terse-state-var trigger

A declared membership `s ∈ T` in an fsm/embedded schema is a **terse state
var** when the body references `_s` and no `s_next` is declared. The
rewrite produces the `s, s_next` pair, **except** for the primitive
self-feedback path the scheduler already owns:

> **Rewrite a terse state var to the `s, s_next` pair UNLESS it is a
> primitive (`Int`/`Bool`/`Real`/`String`) self-feedback var in a
> *scheduler* FSM (no `halt ∈ Bool`).** That one exception keeps the
> existing `_var` path (`test_19_prev_tick`, `test_20_pure_counter`)
> untouched — those vars are fed back by `inject_prev_tick_decls` + the
> per-tick prev pin, not by a pair.

`world` is the pre-existing instance (T = `World`, injected `world_next`
routed by `resolve_fsm`'s world-specific slots). One walk handles both;
which downstream slot the `_next` lands in is decided by `resolve_fsm`
keying on the name — so preserving world's special-casing needs no extra
code.

### Inert until migrated

The trigger requires a `_s` reference. Every un-migrated FSM writes
`state_next = match state` (explicit pair, no `_state`), so the generalized
rewrite is a **no-op on the un-migrated corpus** — it activates only when a
file is rewritten terse. This is what makes the migration safe to land
incrementally.

### Interaction with `inject_prev_tick_decls`

`unify_state_syntax` runs **first** and *consumes* the `_state` references
(rewriting them to `state`), so `inject_prev_tick_decls` sees no leftover
`_state` for a paired var and injects nothing for it — the pair carries the
prev value. The primitive `_var` exception keeps its `_count` references, so
`inject_prev_tick_decls` still injects `_count` + `is_first_tick` for it.
**This ordering is essential.**

---

## § 4 — The embed surface: `F(seed, fsm_state)` as a constraint

`F(seed, fsm_state)` must, when `F` is an `fsm`, constrain `fsm_state` to
F's settled state reachable from `seed` — and inline as a conjunction when
`F` is a `claim`. The disambiguator is `F`'s keyword.

### Shape: a two-argument call in constraint (BodyItem) position

```evident
fsm_state ∈ T                 -- the parent declares the output var
F(seed, fsm_state)            -- BodyItem::Constraint(Expr::Call("F", [seed, fsm_state]))
```

- `seed` (arg 0): the **input** state — F's `state` const. Any expression
  `eval_const_init` already handles (literal / constructor / nullary
  variant / `SeqLit` / given / integer arithmetic over those).
- `fsm_state` (arg 1): a **parent variable** of F's state type. It is *not*
  required to be free — the parent may bind or further-constrain it. It is
  F's `state_next`/settled output, lifted to parent scope.

### The resolution point is load time, not parse time

The parser cannot know `F`'s keyword (`F(a, b)` is just `Expr::Call`).
Resolution happens once the schema table is populated — a **load-batch
desugar pass**, `lower_fsm_application`, run after all schemas (and their
monomorphizations) are loaded, where `runtime/src/runtime/load.rs:132-136`
already sits (alongside `validate_run_targets`).

The pass walks every loaded schema body (and subclaim bodies) and rewrites:

```text
Expr::Call(name, [seed, out])   where schemas[name].keyword == Keyword::Fsm
   →  a constraint binding `out` to RunFsm{ fsm: name, init: seed }:
        Expr::Binary(Eq, out, Expr::RunFsm { fsm: name, init: Box::new(seed) })
```

- **Reuse `Expr::RunFsm`** as the settled-state node — do **not** add a new
  node. The equality `out = RunFsm{F, seed}` is the constraint: `out`
  *is* the settled state. Every downstream consumer already handles
  `RunFsm` (`resolve_runs`, `collect_run_targets`, `embedded_fsm_targets`,
  `expr_has_run`, the tier dispatch).
- **Why an equality rather than a value-substitution.** In the *forward*
  regime (parent does not constrain `out`), `resolve_runs` pre-evaluates
  `RunFsm` to a literal and the equality binds `out` to it — identical to
  what `run(F, seed)` did, now expressed as a constraint. In the *feedback*
  regime (parent constrains `out`), the equality stays symbolic and the
  transition is asserted (unrolled / CHC), so Z3 can backtrack to a seed/
  run consistent with the parent's predicate. **One lowering, both
  regimes** — the selector (§ 6) chooses; the node is the same.

### Arity + error shape

The lowering fires only on an **arity-2** call to an `fsm`-keyword schema.
Other arities are a load-time error:

```
error: `F` is an `fsm`; embed it as a constraint with a seed and a state
       variable: `F(seed, fsm_state)`. Got F(<n> args).
```

A bare `F` (no call) in value position is *not* lowered (no seed/out); that
stays whatever it is today (names-match composition or undefined-name
error). v1 is `F(seed, out)` only — no `F` as a first-class value.

### The inject-pass ordering caveat (and the fix)

The per-schema inject passes run **as each schema loads**, *before*
`lower_fsm_application` (a batch pass). So they see `F(seed, out)` as an
ordinary `Expr::Call`. The only pass that could misread it is
`inject_claim_arg_types` (`inject.rs`), which resolves a call name against
the schema table and may inject an arg's type from the called claim's
params. **Add a one-line guard to `inject_claim_arg_types::resolve`: skip
names whose schema is `Keyword::Fsm`** (an fsm is embedded, not
arg-type-inferred). `validate_run_targets` runs *after* the lowering, so
the new `RunFsm` nodes get the same FSM-shape validation `run(...)` got.

---

## § 5 — The `state_next` ban

### Where it fires

A new load-time check, `forbid_state_next_source` — placed in
`runtime/src/runtime/validate.rs` (alongside `enforce_external_only`) and
called from `load.rs` **on the parsed body, BEFORE `unify_state_syntax`**.
It must see the *original source*, because `unify_state_syntax` *injects*
`state_next` (and `world_next`) as IR — those injections are legitimate and
must not trip the ban.

### What it rejects

The ban's **domain** is schemas that have a transition: `Keyword::Fsm`
schemas and embedded targets. Within that domain, reject either source
shape that names a `_next` partner of a state var:

1. a **membership** `<base>_next ∈ T` where `<base> ∈ T` is also declared
   (the explicit pair), or
2. a **write** `<base>_next = …` (or `<base>_next.field = …`) on an
   equation LHS.

### The error message

```
error: `state_next` is not a valid source declaration. Evident FSMs are
       written in the terse form — `state` is this tick's value, `_state`
       the previous tick's. Rewrite:

           fsm F(state ∈ T, state_next ∈ T, halt ∈ Bool)      ✗ banned
               state_next = f(state)

           fsm F(state ∈ T, halt ∈ Bool)                      ✓ terse
               state = f(_state)

       (the runtime still composes a state/state_next pair internally —
        the ban is purely on how you write it.)
```

Name the offending var and the schema.

### No escape hatch; `external` exempt

There is no opt-out within the domain. The internal IR may still use
`state_next` (synthesized by `unify_state_syntax`, consumed by detectors) —
the ban is purely source-level, so no contradiction. `external fsm` bridge
contracts (`stdlib/runtime.ev`'s `StdinSource`, …) declare `state_next`-
shaped slots naming Rust-side bridge state; they are skipped by every
inject / rewrite pass, and the ban must **skip `external` schemas** too.

The non-fsm static test claims (`sat_*`/`unsat_*`) that pin a `state =` /
`state_next =` transition are **outside the ban's domain** (they are
`claim`s, not fsms). They migrate to `F(seed, fsm_state)` whole-run
constraints (§ 7) because that is strictly better, but the ban does not
*force* them — keeping "no escape hatch" honest with a precise domain.

---

## § 6 — Universal `_state` (in flight) + discharging `F(seed, fsm_state)`

### 6.1 Universal `_state` — the STATE-terse session

The first mergeable step is § 3's rewrite, scoped to the `run()`-driven
enum-state passes (`stdlib/passes/{pretty,subscriptions,validate,generics,
desugar,inject}.ev`) and `examples/test_34/35`. It generalizes
`unify_world_syntax` → `unify_state_syntax` and converts those FSMs to
terse, validated byte-for-byte by the `*_correctness.rs` harnesses. **This
is being implemented now** by the STATE-terse session — this spec's § 3 is
its design. The embed surface (§ 4) and ban (§ 5) fire after it lands.

### 6.2 Discharging the constraint — the strategy selector

`F(seed, fsm_state)` lowers to `fsm_state = RunFsm{F, seed}` (§ 4). *How*
that constraint is discharged is chosen by the selector, completing the
condensability→guarantee spectrum (`fsms-as-functions.md` § 5;
`docs/research/fsm-behavioral-constraints.md` § 6):

```
F(seed, fsm_state), with parent claims around fsm_state
   │
   ├─ NO feedback (parent does NOT constrain fsm_state; seed determined up front)
   │     → FORWARD-EXECUTE: pre-evaluate RunFsm to a constant (tiers 1–3),
   │       bind fsm_state to it, check parent claims once. UNSAT on violation, no retry.
   │
   └─ FEEDBACK (parent constrains fsm_state; the satisfying seed is NOT known up front)
         │
         ├─ CONDENSABLE (affine step; compose.rs detector accepts) OR step in a
         │   Spacer theory (LIA/LRA, simple ADT):
         │     → CHC / SPACER  — unbounded proof the parent property holds over the
         │        whole run, for all seeds in the parent precondition.
         │        BMC (compose.rs unroller) is the bounded fallback on unknown/divergence;
         │        k-induction the cheap unbounded-from-bounded strengthening.
         │
         └─ NON-CONDENSABLE + RECURSIVE (tree-walk; Z3 not a sound oracle):
               → CEGAR (GG design) with blocking-interpret (tier 3) as the ground-truth oracle.
```

| Regime | Dependency | Recovered guarantee | Engine |
|---|---|---|---|
| **Dissolve** | forward, affine | full — one solve | BMC closed-form / CHC |
| **Forward-execute** | forward, branching | checked (UNSAT on violation) | pre-evaluate + check |
| **Feedback, condensable/arithmetic** | output-feedback | **unbounded proof** | **CHC / Spacer** |
| **Feedback, recursive** | output-feedback, ADT recursion | searched (bounded) | CEGAR + blocking-interpret |

The selector's inputs are three already-computed signals plus one new bit:
body shape (`detect_state_pairs` / `MainShape`), the affine-step detector
verdict (`fsm_unroll/detector.rs`), a **theory classifier** (is the step's
state + transition Spacer-friendly?), and **does the parent constrain
`fsm_state`?** (forward vs feedback) — which a read/write-set analysis over
the embedding constraint already has the ingredients for.

### 6.3 The CHC lowering (the feedback/condensable core)

Per `docs/research/fsm-behavioral-constraints.md` (§ 2.6, § 3 verdict (b),
§ 6.1): the parent-constrains-child question lowers to a Constrained Horn
Clause query over a relation `Inv(s)`:

```
I(s)                       → Inv(s)          -- from the parent precondition on seed
Inv(s) ∧ ¬halt(s) ∧ s' = step(s)  → Inv(s')  -- from build_f1's state_exprs
Inv(s) ∧ halt(s) ∧ ¬ParentProp(s) → false    -- the parent postcondition on fsm_state
```

reachable via a **raw `z3-sys` `Z3_fixedpoint_*` wrapper** (the safe `z3`
crate has no `Fixedpoint`; `z3-sys` exposes the full C API at `lib.rs:6215+`;
a `raw_ctx` bridge precedent already ships — see the research report § 3).
Spacer returns an inductive invariant (property proved, unbounded) or a
counterexample trace. The same `build_f1` front-end that fed
`assert_halts_within` feeds a new `chc::prove(F, parent_prop)` — emitting
Horn rules into a `Z3_fixedpoint` object instead of an N-fold Bool into the
outer solver.

> **Important honesty (research § 5.3, § 7.1):** CHC/Spacer is for the
> **arithmetic/LIA-LRA + condensable** case. Enum-state and recursive
> tree-walk FSMs are where Spacer is weak and Z3 is *not* a sound oracle —
> the theory classifier must route those to **CEGAR + blocking-interpret**,
> never to CHC. A CHC `unknown` must **never** silently become "property
> holds" — fall back to the bounded BMC answer with an explicit bound.

### 6.4 The user's "what if the parent picks a seed the child can't satisfy?"

A worry the user raised: under feedback, if the parent's solver picks
candidate variable values that make the child's constraint unsatisfiable,
does it wedge? Answer, by regime:

- **In-solve (dissolve / CHC / BMC-unroll):** there is **one** solver. The
  child's transition and the parent's property are asserted *together*; Z3
  backtracks like any other conjunction and returns only a globally
  consistent model, or UNSAT. There is no "parent picked a bad seed and got
  stuck" — the seed is a solver variable, not a committed choice.
- **Forward-only:** the seed is concrete (no feedback), so the question
  doesn't arise — F is pre-evaluated and the parent checks the result once.
- **CEGAR:** the loop *recovers* backtracking explicitly — a candidate that
  the child refutes becomes a blocking clause, and the outer solver picks
  another. This is the regime where the worry is real and CEGAR is the
  answer (and why CEGAR, not in-solve, owns the recursive case).

So the capability is sound in all three regimes; only the *mechanism* of
backtracking differs (solver-native vs blocking-clause loop).

---

## § 7 — Corpus migration plan

One consistent sweep over the `.ev` corpus, run **after** the STATE-terse
session lands (so there is one sweep, not two). Counts from the current
tree: ~35 files declare a `state_next ∈ …` membership (the ban target); 7
use `run(…)`; one uses `halts_within`.

### Category A — `run(F, i)` → `F(i, out)` (flag-day, no alias)

The `run(…)` files. A `run(F, init)` value expression becomes a declared
output var + a constraint:

```evident
-- before
final ∈ SW = run(subscriptions_walk, seed)
-- after
final ∈ SW
subscriptions_walk(seed, final)
```

The parser hook is deleted in the same commit (§ 2); the test suite catches
any missed site immediately.

### Category B — `halts_within(F, N)` → embedded constraint

The one `halts_within` site becomes `F(seed, fsm_state)` (halting is
implicit — § 2). If the test specifically asserts bounded termination, add
a tick-count parent constraint; otherwise the SAT/UNSAT of the embed is the
verdict. Delete `BodyItem::HaltsWithin`, its parser, and
`assert_halts_within` (keep the `build_f1` unroller).

### Category C — scheduler enum FSM transitions → terse

`fsm` bodies that write `state_next = match state` on an enum/record state
(e.g. `test_02_counter.ev`). Rewrite to `state = match _state` (terse, § 3).
Mechanical; verify each runs end-to-end + the inline `sat_*`.

### Category D — embedded `Int`/enum transitions → terse

`test_35`'s `decrement` / `accumulate` and the pass FSMs (done in § 6.1).
`fsm decrement(count ∈ Int, halt ∈ Bool)` / `count = _count - 1` /
`halt = (_count ≤ 0)`. Care: `halt` reads `_count` (the input); confirm the
forward + CHC/BMC paths return the same settled state.

### Category E — static `sat_*`/`unsat_*` one-tick harnesses → whole-run constraints

The bulk of the 35. Today they pin `state =` + assert `state_next =` around
a names-match transition. They are **not** fsms (ban doesn't force them),
but the user wants the pair gone everywhere. Migrate to whole-run
constraints:

```evident
claim sat_start_settles_to_count_five
    final ∈ CountState
    counter(Start, final)         -- run to completion
    final = Count(5)              -- assert on the settled state
```

Where a genuinely *single-tick* property is the point, two honest options:
(a) keep as a non-fsm `claim` naming two relation endpoints (outside the
ban — the language always allowed a relation's input/output pair), or (b)
migrate to the whole-run form. **Prefer (b) where the fsm halts quickly**
(most do). This is the one part of the sweep that is not find-and-replace;
budget review time per file.

### Category F — exempt: `external fsm` contracts

`stdlib/runtime.ev`'s bridge contracts declare `state_next`-shaped slots
naming Rust bridge state. They are `external`, skipped by every pass; the
ban skips them (§ 5). No migration.

---

## § 8 — Implementation sequence + CLAUDE.md

All of it builds on the STATE-terse session landing first (it owns
`desugar.rs` / `inject.rs`).

1. **Universal `_state` (§ 3, § 6.1).** *In flight (STATE-terse).*
   Generalize `unify_world_syntax` → `unify_state_syntax`; migrate the
   `run()`-driven passes to terse. Validated by the `*_correctness.rs`
   harnesses.

2. **Embed surface (§ 2, § 4).** Add `lower_fsm_application` (load-batch
   2-arg `F(seed, out)` → `out = RunFsm{F, seed}`); add the
   `inject_claim_arg_types` fsm-skip guard; **delete the `run(...)` and
   `halts_within` parser hooks + `assert_halts_within` surface** (keep the
   `build_f1` unroller). Validated by `runtime/tests/run_fsm.rs` rewritten
   to `F(seed, out)` spellings asserting the same settled states.

3. **`state_next` ban (§ 5).** Add `forbid_state_next_source`, on the
   parsed body before `unify_state_syntax`, scoped to fsm-keyword +
   embedded targets, `external` exempt. Comes **after** the terse rewrite
   covers all fsm classes, or it would reject FSMs with no terse path.

4. **Corpus migration (§ 7).** The single sweep: Cat A (`run`→embed) + B
   (`halts_within`) + C (scheduler enum) + D (embedded) + E (static
   harnesses — the careful part) + F (`external` left alone).

5. **Strategy selector + CHC discharge (§ 6.2–6.3).** The forward-vs-
   feedback branch on top of the existing tier selector; the
   `chc::prove` raw-`z3-sys` wrapper for the feedback/condensable regime
   (per the research report's first slice). This is the largest piece and
   can land *after* the surface + migration — until it does, the embed
   discharges via forward-execute + BMC, which already exist. Sequence it
   last; it is an *engine* upgrade behind a stable surface.

6. **Rewrite CLAUDE.md to the one consistent story.** Do this last, when
   the surface is real:
   - Generalize the "`_world` / `world` syntax" block to "`_state` /
     `state` for *any* FSM state var"; the `_world` example becomes one
     case.
   - Restate the `examples/` integration-test shape as `state ∈ T` (terse)
     + `halt`/`last_results`/`effects`.
   - Add the **`F(seed, fsm_state)` embed constraint** to the composition
     decision guide and the `fsm` keyword section: *claim → inline
     (conjunction); fsm → settled-state constraint (`F(seed, fsm_state)`),
     and the parent may further constrain `fsm_state`.* Emphasize: **no
     return value.**
   - Add `state_next` to the footgun list: a source `state_next` is a load
     error, not a silent legacy form.
   - Remove all mention of `run(...)` and `halts_within`.

---

## Appendix — source anchors (where each change lands)

| Change | File:line | Section |
|---|---|---|
| Terse→pair rewrite (generalize) | `runtime/src/runtime/desugar.rs` (`unify_world_syntax` → `unify_state_syntax`) | § 3, § 6.1 |
| Rewrite call-site position | `runtime/src/runtime/load.rs` (where `unify_world_syntax` runs) | § 3 |
| `_state` read-decl interaction | `runtime/src/runtime/inject.rs` (`inject_prev_tick_decls`) | § 3 |
| Embed lowering `F(seed,out)`→`out = RunFsm` | new `lower_fsm_application` at `runtime/src/runtime/load.rs:132-136` (batch, after monomorphize) | § 4 |
| Lowered node (reuse) | `Expr::RunFsm` (`core/ast.rs`) | § 4 |
| **Delete** `run(...)` parser hook | `runtime/src/parser/atoms.rs:26-41` | § 2 |
| **Delete** `halts_within` surface | `BodyItem::HaltsWithin` + parser + `fsm_unroll/compose.rs::assert_halts_within` | § 2 |
| Keep the unroller | `fsm_unroll/compose.rs` (`build_f1`/`double`/`series`) — reused by BMC discharge | § 2, § 6.2 |
| inject-pass fsm guard | `runtime/src/runtime/inject.rs` (`inject_claim_arg_types::resolve`) | § 4 |
| `state_next` ban | new `forbid_state_next_source` in `runtime/src/runtime/validate.rs`, before the terse rewrite | § 5 |
| Pair detectors (unchanged) | `effect_loop/fsm.rs::resolve_fsm`, `fsm_unroll/compose.rs::detect_state_pairs`, `effect_loop/nested.rs::detect_state_pairs` | § 3 |
| Strategy selector + CHC | new `chc.rs` (raw `z3-sys` `Z3_fixedpoint_*`) + selector branch | § 6.2, § 6.3 |
| `run`/embedded validation (unchanged) | `runtime/src/runtime/nested.rs` (`validate_run_targets`) | § 4 |
| `external` exemption | existing pattern in `desugar.rs` / `inject.rs` / `fsm.rs` | § 5, § 7 |
| The terse-migration passes | `stdlib/passes/{subscriptions,validate,pretty,generics,desugar,inject}.ev` | § 6.1 |
| Embedded transition demos | `examples/test_34_halts_within.ev`, `test_35_run_fsm.ev`, `test_36`, `test_37` | § 7 |
