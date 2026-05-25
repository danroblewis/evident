# FSMs as functions — the implementation spec

> **What this is.** [`fsms-as-functions.md`](fsms-as-functions.md) is the
> *concept* — `fsm` is a function (`result = F(init)`), `claim` is a
> conjunction, nesting recovers the whole-output guarantee, the three
> tiers are one idea on a condensability gradient. This doc is the
> *buildable plan*: it pins the four edges that the capstone left open
> (§9 of the capstone), scopes the **two implementations** the user wants
> — universal `_state` (the terse form for *any* state var, in *any* FSM)
> and the embed surface (`result = F(init)` replacing `run(F, init)`) —
> specs the **`state_next` ban**, and lays out the **corpus migration**.
>
> The end state the user wants, in one line:
>
> > **The terse form (`fsm F(state ∈ T, halt ∈ Bool)` + `_state`) is the
> > ONLY way to write an FSM transition; `state_next` as a source pattern
> > is banned forever; an `fsm` embedded in another schema runs to
> > completion as `result = F(init)`.**
>
> **Dependency.** The two terse-form passes this spec rewrites —
> `runtime/src/runtime/desugar.rs::unify_world_syntax` and
> `runtime/src/runtime/inject.rs::inject_prev_tick_decls` — are the exact
> files the in-flight **REVIVE-inject / REVIVE-desugar** sessions are
> mid-cutover on. **Implementation fires after REVIVE lands.** This spec
> is docs-only and parallel-safe; the implementation sessions it describes
> are not.
>
> **Reading order to implement.** This doc, then the capstone, then
> [`nested-fsm-strategies.md`](nested-fsm-strategies.md) +
> [`loop-functionizer.md`](loop-functionizer.md) for the tier machinery the
> surface drives. The source anchors are inline at each section.

---

## § 1 — The end state (one consistent story)

Three things become true, and they are one story (a `SchemaDecl`'s
keyword is the rule; `fsm` means "function"):

1. **Terse is universal.** `fsm F(state ∈ T, halt ∈ Bool)` with `_state`
   for the previous tick is the way to write a transition, for **any**
   state var (enum / record / `Int`), in **any** FSM — scheduler-driven
   *or* `run()`/embedded — **not just `world`**. `_state.X` reads the
   previous tick; `state.X = …` writes the current tick. The author never
   names a `_next` var.

2. **`fsm` referenced by name is function application.** An `fsm`
   referenced inside another schema runs to completion as a child:

   ```evident
   result ∈ T = F(init)        -- F is an `fsm` → run-to-completion, yields final state
   ```

   This **replaces** `run(F, init)`. The disambiguator is the **keyword
   on `F`'s `SchemaDecl`**: `F(init)` where `F` is a `claim` → inline
   (conjunction, unchanged); where `F` is an `fsm` → child run
   (application to completion). Composite `init` (a recursive-enum or
   `Seq` literal, per session NN) flows exactly as today's
   `run(F, init)`.

3. **`state, state_next` as a source pattern is a load-time error.** An
   `fsm` (or embedded target) that declares a `state_next ∈ T` membership
   or writes `state_next = …` is rejected at load with a message that
   shows the terse rewrite. The error's domain is precisely the things
   that *have* a transition (fsm-keyword schemas + embedded targets); a
   non-fsm `claim` is simply outside the ban's domain (no escape hatch
   *inside* it — see § 5).

The internal IR is **unchanged**: the runtime still composes / schedules
a `state, state_next` pair (`effect_loop/fsm.rs::resolve_fsm`,
`fsm_unroll/compose.rs::detect_state_pairs`,
`effect_loop/nested.rs::detect_state_pairs` all key on the literal pair).
The ban is **purely source-level**; the terse front-end rewrite
synthesizes the pair the machinery already expects (§ 3). This is the
smallest-blast-radius design and the one this spec recommends throughout.

---

## § 2 — Edge 1: does `run()` stay as a deprecated alias?

**Recommendation: keep `run(F, init)` working as a thin alias for one
release, emit a deprecation note, remove after the corpus is migrated.**

The migration is not a flag-day because the two surfaces *already lower
to the same node*. `run(F, init)` is parsed to `Expr::RunFsm { fsm, init }`
by the atom parser (`runtime/src/parser/atoms.rs:26-41`). The new
`result = F(init)` lowers to the **same `Expr::RunFsm`** at load time
(§ 4). So both can coexist with zero divergence in the execution path:
the parser hook stays; the load-time lowering is added; everything
downstream (`resolve_runs`, `collect_run_targets`, `embedded_fsm_targets`,
the three tiers) sees `RunFsm` either way.

Concretely:

- **Keep** the `run(...)` parser hook (`atoms.rs`) for one release.
- **Add** a deprecation note when the `run(...)` *spelling* is seen at
  load (a one-line stderr warning, gated so it fires once per file:
  `note: run(F, init) is deprecated; write result = F(init)`). The hook
  already knows it produced a `RunFsm` from the `run` keyword, so tag
  that node (a `bool deprecated_spelling` on `RunFsm`, or a load-time
  scan for the `run` token) and warn.
- **Migrate** the corpus to `F(init)` (§ 7).
- **Remove** the `run(...)` parser hook in the release after migration.
  At that point `RunFsm` is produced *only* by the `F(init)` lowering.

Why not remove immediately + migrate atomically: the corpus touch (§ 7)
is large and lands across several files that the REVIVE sessions also
touch; a flag-day forces the migration and the surface change into one
non-bisectable commit. The alias decouples them — surface lands, corpus
migrates incrementally, alias drops last — and each step keeps
`./test.sh` green.

### `halts_within` — keep it separate

`halts_within(F, N)` is a **verification** predicate (a `BodyItem`,
`BodyItem::HaltsWithin`, lowered by `fsm_unroll/compose.rs::
assert_halts_within` to a halt-aggregate *constraint*), not a value-run.
The capstone (§ 9) frames it as the *verify* face of the same
composition, but it produces a `Bool`/SAT-UNSAT verdict, not a value.

**Recommendation: `halts_within` stays as-is, a distinct surface.** It is
*not* rewritten to `F(init)`-based, because `F(init)` yields a value and
`halts_within` yields a proof obligation over *all* (or a pinned) init —
a different question (`nested-fsm-strategies.md` § 1, the
execute-vs-verify table). The only change it shares with this spec is the
**keyword migration**: its target must be `fsm`-keyword'd (already
enforced — `compose.rs::build_f1:253` and `nested.rs::validate_run_targets:126`),
and its target FSM migrates to terse like any other (§ 6/§ 7). A future
`F(init)`-fed verification assertion is possible but out of scope.

---

## § 3 — Edge 2: how the terse form emits the internal pair

**Key decision, and this spec's load-bearing recommendation: keep the
`state, state_next` pair as the internal IR; ban it only at the *source*
level; generalize the front-end rewrite.** The machinery
(`fsm_unroll`/`nested`/the scheduler) is **unchanged** — it keeps
consuming the literal pair. Only two things change: the front-end rewrite
(this section) and the ban (§ 5).

### Where the rewrite lives

Today, `runtime/src/runtime/desugar.rs::unify_world_syntax` is the
**world-only** instance of exactly this rewrite. It:

1. fires only for `world ∈ World` with no `world_next` declared, and only
   when the body uses `_world.X` (`desugar.rs:150-206`);
2. rewrites identifier strings: `_world.X` → `world.X` (prev read),
   `world.X` → `world_next.X` (current read/write) (`desugar.rs:212-220`);
3. injects `world_next ∈ World` so the scheduler's writer detection finds
   the pair (`desugar.rs:266-273`).

**Recommendation: generalize this into `unify_state_syntax`** — the same
walk, with the hardcoded `world` name replaced by "any *terse state var*"
(below). `world` becomes one instance of the generalized pass. It runs at
the **same point** in the load pipeline (`runtime/src/runtime/load.rs:71`),
*before* `inject_fsm_params` (`load.rs:77`) and the three pair detectors,
so they see the already-paired IR.

> **Why generalize the rewrite rather than teach the machinery the terse
> form natively.** Three detectors require the literal pair —
> `effect_loop/fsm.rs::resolve_fsm` (scheduler `MainShape`),
> `fsm_unroll/compose.rs::detect_state_pairs` (tier-1 / `halts_within`),
> `effect_loop/nested.rs::detect_state_pairs` (tier-3 `run_nested`).
> Teaching all three (plus the scheduler's `_var` runtime machinery) the
> single-var-`_state` form is a wide, three-site change in the hot
> machinery. Rewriting at the front end touches **one** pass + the ban,
> and every detector keeps working byte-for-byte. Smallest blast radius
> wins.

### The terse-state-var trigger (the crux)

A declared membership `s ∈ T` in an fsm/embedded schema is a **terse
state var** when the body references `_s` (the previous-tick form) and no
`s_next` is declared. The rewrite then produces the `s, s_next` pair.
**But** the rewrite must not swallow the existing primitive `_var`
self-feedback path (`test_20_pure_counter.ev`: `count ∈ Int = (is_first_tick
? 0 : _count + 1)` — a primitive var fed back by the scheduler's `_var`
machinery, *not* a pair). The distinguisher is **whether the schema is an
embedded target and the var's type**:

| Schema class | Signal | `s ∈ T`, T enum/record | `s ∈ T`, T primitive (`Int`/`Bool`/`Real`/`String`) |
|---|---|---|---|
| **Embedded** (declares `halt ∈ Bool`) | `_s` read + `s` written | rewrite to `s, s_next` pair | **rewrite to `s, s_next` pair** |
| **Scheduler** (no `halt`) | `_s` read + `s` written | rewrite to `s, s_next` pair | **leave on `_var` self-feedback path** (unchanged) |

The asymmetry is forced by the runtime: a `run()`/`halts_within` target
is driven by `run_nested` / the unroll composer, which have **no `_var`
self-feedback machinery** — they require the literal pair for *every*
state type, including `Int` (this is why `decrement` is
`count, count_next ∈ Int` today). A scheduler FSM, by contrast, *does*
have the `_var` machinery (`inject_prev_tick_decls` + the per-tick prev
pin), so a primitive scheduler self-feedback var stays there and is never
paired. Equivalently, stated as one rule:

> **Rewrite a terse state var to the `s, s_next` pair UNLESS it is a
> primitive (`Int`/`Bool`/`Real`/`String`) self-feedback var in a
> scheduler FSM (no `halt ∈ Bool`).** That one exception keeps the
> existing `_var` path (`test_19_prev_tick`, `test_20_pure_counter`)
> untouched.

`world` is the pre-existing instance: T = the `World` record, name =
`world`, the injected `world_next` is routed by `resolve_fsm`'s
*world-specific* slots (`fsm.rs:158-163`) and the multi-writer disjoint
check. A generic `state` injects `state_next`, routed to `resolve_fsm`'s
*generic* state-pair detection (`fsm.rs:182-201`). **One walk handles
both**; which downstream slot the `_next` lands in is already decided by
`resolve_fsm` keying on the name `world`/`world_next` vs anything else.
So preserving world's special-casing needs no extra code — keep emitting
`world_next` for the `world` var and a generic `<s>_next` otherwise.

### The rewrite is inert until a file is migrated

The trigger requires a `_s` reference. Every un-migrated FSM in the
corpus writes `state_next = match state` (an explicit pair, **no
`_state`**), so the generalized rewrite is **a no-op on the un-migrated
corpus** — it activates only when a file is rewritten to the terse form.
This is what makes § 6 safe to land independently: shipping
`unify_state_syntax` perturbs nothing until the 3 passes are migrated in
the same step.

### Worked rewrite (the `subscriptions_walk` shape)

Source (terse, post-migration):

```evident
fsm subscriptions_walk(state ∈ SW, halt ∈ Bool)
    state = match _state
        SWSeed(w) ⇒ SWStep(WSCons(w, WSNil), NameNil)
        …
    halt = match _state
        SWDone(_) ⇒ true
        _         ⇒ false
```

After `unify_state_syntax` (the IR the detectors see — identical to
today's hand-written pair):

```evident
fsm subscriptions_walk(state ∈ SW, state_next ∈ SW, halt ∈ Bool)
    state_next = match state            -- _state → state, state(write) → state_next
        SWSeed(w) ⇒ SWStep(WSCons(w, WSNil), NameNil)
        …
    halt = match state                  -- _state → state (halt reads the input)
        SWDone(_) ⇒ true
        _         ⇒ false
```

Note `halt = match _state` rewrites to `halt = match state` — `halt` is
read on the tick's *input* state (the convention `run_nested` /
`compose.rs` already use; `nested-fsm-strategies.md` § 2, "Halt"). The
author writes `_state` for "the state I'm dispatching on," and the
rewrite maps it to the detector's input const.

### Interaction with `inject_prev_tick_decls`

`inject_prev_tick_decls` (`inject.rs:140`) injects `_name ∈ T` read slots
+ `is_first_tick` for any `_name` referenced. Because `unify_state_syntax`
runs **first** (`load.rs:71` < `load.rs:84`) and *consumes* the `_state`
references (rewriting them to `state`), `inject_prev_tick_decls` sees no
leftover `_state` for a paired var and injects nothing for it — correct,
the pair carries the prev value. The primitive `_var` self-feedback path
(the one exception above) keeps its `_count` references untouched, so
`inject_prev_tick_decls` still injects `_count` + `is_first_tick` for it,
exactly as today. **This ordering is essential and must be preserved when
REVIVE-inject lands.**

---

## § 4 — Edge 3: parsing / disambiguating the embed call

`result = F(init)` must run `F` to completion when `F` is an `fsm`, and
inline it when `F` is a `claim`. The disambiguator is `F`'s keyword.

### The resolution point is load time, not parse time

The parser cannot know `F`'s keyword — `F(init)` is just
`Expr::Call("F", [init])`, and `F` may be a forward reference or in a
later-imported file. Resolution must happen **once the schema table is
populated**. The natural hook is a **load-batch desugar pass**,
`lower_fsm_application`, run after all schemas (and their
monomorphizations) are loaded — i.e., right where
`runtime/src/runtime/load.rs:132-136` already sits (after
`monomorphize_generics`, alongside `validate_run_targets`).

The pass walks every loaded schema body (and subclaim bodies) and
rewrites:

```text
Expr::Call(name, [init])   where schemas[name].keyword == Keyword::Fsm
   →  Expr::RunFsm { fsm: name, init: Box::new(init) }
```

- **Reuse `Expr::RunFsm`** as the lowered node — do **not** add an
  `Expr::FsmCall`. Every downstream consumer already handles `RunFsm`:
  `resolve_runs` (`nested.rs:166`), `collect_run_targets`
  (`nested.rs:501`), `embedded_fsm_targets` (`nested.rs:147`),
  `expr_has_run` (`nested.rs:544`), and the tier dispatch. A new node
  would mean touching all of them for no semantic gain.
- **`init` flows unchanged.** The single arg becomes `RunFsm.init`;
  `eval_const_init` (`nested.rs:331`) already evaluates composite inits
  (constructors, nullary variants, `SeqLit`, nested `run`). So
  `result = F(Node(Leaf(1), Leaf(2)))` works the moment the lowering
  fires, identically to `run(F, …)`.

### Arity + error shape

The lowering fires only on an **arity-1** call to an `fsm`-keyword
schema. Other arities are a load-time error:

```
error: `F` is an `fsm` (a function); apply it to exactly one init
       argument: `result = F(init)`. Got F(<n> args).
```

A bare `F` (no call) referencing an fsm in value position is *not*
lowered here (it has no init); that stays whatever it is today (likely a
names-match composition or an undefined-name error). v1 is `F(init)`
only — no `F` as a first-class value.

### The inject-pass ordering caveat (and the fix)

The per-schema inject passes (`load.rs:77-89`) run **as each schema
loads**, *before* `lower_fsm_application` (a batch pass at `load.rs:132+`).
So they see `F(init)` as an ordinary `Expr::Call`. The only pass that
could misread it is `inject_claim_arg_types` (`inject.rs:267`), which
resolves a call name against the schema table and may inject an arg's
type from the called claim's params. For `F` an fsm:

- `resolve()` (`inject.rs:337`) matches `schemas.contains_key("F")` → it
  *would* treat `F(init)` as a claim call and, if `init` were a fresh
  multi-use identifier, inject `init ∈ <F's first param type>`.
- In practice v1 `init` is a constant expr (literal / constructor /
  given — `eval_const_init`'s domain), so the multi-use-fresh-name path
  rarely fires. But to be correct, **add a one-line guard to
  `inject_claim_arg_types::resolve`: skip names whose schema is
  `Keyword::Fsm`** (an fsm is applied, not arg-type-inferred). This is
  the single defensive change the embed surface needs in the existing
  inject pipeline.

`validate_run_targets` (`load.rs:136`) then runs *after* the lowering
(both are batch-level; order the lowering first), so the new `RunFsm`
nodes get the same load-time FSM-shape validation
(`nested.rs::validate_run_targets:103`) as `run(...)` does today.

---

## § 5 — Edge 4: the `state_next` ban

### Where it fires

A new load-time check, `forbid_state_next_source` — recommend placing it
in `runtime/src/runtime/validate.rs` (alongside `enforce_external_only`)
and calling it from `load.rs` **on the parsed body, BEFORE
`unify_state_syntax`** (i.e., before `load.rs:71`). It must see the
*original source*, because `unify_state_syntax` *injects* `state_next`
(and `world_next`) as IR — those injections are legitimate and must not
trip the ban.

### What it rejects

The ban's **domain** is schemas that have a transition:
`Keyword::Fsm` schemas and `run()`/`halts_within` targets. Within that
domain, reject either source-level shape that names a `_next` partner of
a state var:

1. a **membership** `<base>_next ∈ T` where `<base> ∈ T` is also
   declared (the explicit pair: `state ∈ SW`, `state_next ∈ SW`), or
2. a **write** `<base>_next = …` (or `<base>_next.field = …`) on an
   equation LHS.

`world`/`world_next` is the one carve-out the *terse* path already
supersedes: the `world` var is migrated to `_world` (the unified syntax,
already landed), so a *source* `world_next` membership is likewise banned
— authors write `_world.X`/`world.X`, never `world_next`. (The
`unify_state_syntax`-injected `world_next` is post-ban, exempt.)

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

Name the offending var (`state_next`, `count_next`, …) and the schema in
the message.

### No escape hatch

There is no opt-out *within the domain*. The internal IR may still use
`state_next` (it is synthesized by `unify_state_syntax` and consumed by
the detectors) — the ban is purely source-level, so this is not a
contradiction: no *human-written* source declares or writes a `_next`
var, and the rewrite owns the IR.

The non-fsm static test claims (`sat_*`/`unsat_*`) that today pin
`state =` and assert `state_next =` around a names-match transition (e.g.
`test_02_counter.ev:41-47`) are **outside the ban's domain** (they are
`claim`s, not fsms). The recommended migration for them is to
`result = F(init)` whole-run assertions (§ 7), which is strictly better
(tests the whole trajectory, not one tick) — but the ban does not *force*
them, because they are not FSMs. This keeps "no escape hatch" honest: the
ban has a precise domain rather than a loophole.

---

## § 6 — Universal `_state` for non-scheduler FSMs (fires first)

**This is the highest-priority sub-implementation** — the piece the user
wants most, and the first mergeable step. It is § 3's rewrite, scoped to
ship and prove on the `run()`-driven enum-state passes, **before** the
embed-surface change (§ 4) and **before** the ban (§ 5).

### What it delivers

`pretty_walk` / `subscriptions_walk` / `validate_walk`
(`stdlib/passes/{pretty,subscriptions,validate}.ev`) — all three
`run()`-driven, `halt ∈ Bool`-declaring, enum-state FSMs — can **drop
`state_next`** and be written terse:

```evident
fsm subscriptions_walk(state ∈ SW, halt ∈ Bool)
    state = match _state
        …
    halt = match _state
        SWDone(_) ⇒ true
        _         ⇒ false
```

### What it touches

- **`runtime/src/runtime/desugar.rs`** — generalize `unify_world_syntax`
  → `unify_state_syntax` (§ 3). For § 6's scope, the trigger is the
  embedded row of the § 3 table: **a `halt ∈ Bool`-declaring schema, any
  terse state var `s ∈ T` (read `_s` + write `s`, no `s_next`) →
  rewrite to the `s, s_next` pair**, regardless of T. (The scheduler /
  primitive rows of the table land with the corpus migration in § 7 —
  but they use the same code path; § 6 simply doesn't exercise them.)
- **`runtime/src/runtime/inject.rs::inject_prev_tick_decls`** — verify it
  no-ops on the rewritten body (the `_state` refs are already consumed;
  § 3's "Interaction" note). No new code expected; if REVIVE-inject has
  reshaped this, re-confirm the ordering invariant.
- **No change** to `fsm_unroll/compose.rs`, `effect_loop/nested.rs`,
  `effect_loop/fsm.rs` — they keep seeing the literal `state, state_next`
  pair the rewrite synthesizes. This is the payoff of the "keep the
  internal pair" decision (§ 3).

### Acceptance test

A `run()`-driven enum-state FSM written terse produces byte-identical
behavior to its explicit-pair form. Concretely:

- The three `*_equivalence.rs` harnesses
  (`runtime/tests/{subscriptions,validate,pretty}_equivalence.rs`) stay
  green with the three passes rewritten terse. These already cross-check
  the Evident pass against the Rust oracle byte-for-byte over the corpus
  — they are the regression net for "the rewrite preserves semantics."
- The inline `sat_*`/`unsat_*` claims in each pass file (e.g.
  `subscriptions.ev:219-249`) still pass under `evident test`, now
  expressed against the terse FSM via `run()`/`F(init)`.
- `./test.sh` green.

### Conflict + ordering

§ 6 **rewrites `unify_world_syntax` and reads `inject_prev_tick_decls`**
— the two files REVIVE-inject / REVIVE-desugar are mid-cutover on. **§ 6
fires after both REVIVE sessions land**, never in parallel. It is
otherwise self-contained: it does not need the embed surface (§ 4) or the
ban (§ 5), and it leaves the corpus's explicit-pair FSMs working (the
rewrite is inert on them, § 3).

---

## § 7 — Corpus migration plan

Two mechanical sweeps over the `.ev` corpus, plus a small set that needs
care. Counts from the current tree:

- **35 files** declare a `state_next ∈ …` membership at source level
  (the ban target).
- **7 files** use `run(…)`.
- Most of the 35 are **`sat_*`/`unsat_*` static test claims** that pin a
  transition, plus the multi-FSM lang tests and examples; a handful are
  the real fsm transitions.

### Category A — mechanical: `run(F, i)` → `F(i)`

The 7 `run(…)` files (`stdlib/passes/{subscriptions,validate,pretty}.ev`,
`examples/test_{35,36,37,38}…`). A textual `run(F, init)` → `F(init)`
rewrite, once the embed lowering (§ 4) is in. The alias (§ 2) means this
can proceed file-by-file without breaking the build. The pass `sat_*`
claims (`final ∈ SW = run(subscriptions_walk, …)`) become
`final ∈ SW = subscriptions_walk(…)`.

### Category B — mechanical: scheduler enum FSM transitions

`fsm` bodies that write `state_next = match state` on an **enum/record**
state (e.g. `test_02_counter.ev:21-25`: `fsm counter(state ∈ CountState)`
+ `state_next = match state`). Rewrite to terse:

```evident
fsm counter(state ∈ CountState)
    state = match _state
        Start    ⇒ Count(5)
        …
```

The § 3 enum/record rule (scheduler row) fires; `resolve_fsm` finds the
synthesized pair. These are mechanical: `state_next` (write LHS) → `state`,
`state` (read in the transition) → `_state`. Verify each still runs
end-to-end (`cargo test --test demos` + the inline `sat_*`).

### Category C — needs care: the embedded fsm transitions (`Int` + enum)

`test_35`'s `fsm decrement(count ∈ Int, count_next ∈ Int, halt ∈ Bool)`
and `fsm accumulate(state ∈ Acc, state_next ∈ Acc, halt ∈ Bool)`, and the
three pass FSMs (§ 6). Terse form drops the `_next`:

```evident
fsm decrement(count ∈ Int, halt ∈ Bool)
    count = _count - 1
    halt  = (_count ≤ 0)
```

The embedded row of the § 3 table applies (`halt` present → pair for any
T, *including* `Int`). The 3 pass FSMs are done in § 6; `decrement` /
`accumulate` here. Care points: (i) `halt` must read `_count` (the input)
— the author writes `halt = (_count ≤ 0)`, the rewrite maps it to the
detector's input const; (ii) confirm tier-1 (`collapse_run`) and tier-3
(`run_nested`) still find the pair and return the same values
(`runtime/tests/{run_fsm,tier1_jit}.rs`).

### Category D — needs care: static `sat_*`/`unsat_*` one-tick harnesses

The bulk of the 35 (e.g. `test_02_counter.ev:41-60`): `claim sat_…` that
declare `state ∈ T` + `state_next ∈ T`, pin `state = Start`, name the fsm
(names-match), and assert `state_next = Count(5)`. These are **not** fsms,
so the ban (§ 5) does not force them — **but** the user wants the pair
gone everywhere. The recommended migration is to whole-run assertions:

```evident
claim sat_start_seeds_count_five
    final ∈ CountState = counter(Start)     -- run one-or-more ticks
    final = …                               -- assert on the trajectory's result
```

For genuinely *single-tick* assertions where "what is `state_next` after
exactly one tick of `state = Start`?" is the property under test, two
honest options: (a) keep them as non-fsm `claim`s that name two distinct
relation endpoints (outside the ban's domain — the pair here names input
and output of a *relation*, which the language always allowed), or (b)
migrate to `F(init)` whole-run form and assert on the final state.
**Recommend (b) where the fsm halts quickly** (most do), falling back to
(a) only where a single-tick property is specifically what's being
tested. This is the one part of § 7 that is not a find-and-replace; budget
review time per file.

### Category E — exempt: `external fsm` contracts

`stdlib/runtime.ev`'s `external fsm` bridge contracts
(`StdinSource`, `FrameTimerSource`, …, `runtime.ev:324-348`) declare
`state_next`-shaped slots that name Rust-side bridge state. They are
**not** user logic — `external` schemas are skipped by every inject /
rewrite pass (`unify_world_syntax:153`, `inject_fsm_params:28`,
`resolve_fsm:88`). The ban (§ 5) must likewise **skip `external`
schemas**. No migration; just ensure the ban's domain excludes
`external`.

### Sweeps the freshly-landed REVIVE passes too

REVIVE-inject / REVIVE-desugar land their own `.ev` and pass code just
before this migration. § 7 is the **single consistent pass over the whole
corpus** — it sweeps the REVIVE-touched files in the same go. Do the
migration *after* REVIVE merges so there is one sweep, not two.

---

## § 8 — Implementation sequence + CLAUDE.md

The ordered session plan. **All of it depends on the REVIVE-inject /
REVIVE-desugar sessions landing first** (they own `inject.rs` /
`desugar.rs` mid-cutover).

1. **Universal `_state` (§ 6).** Generalize `unify_world_syntax` →
   `unify_state_syntax`; migrate the 3 `run()`-driven passes to terse.
   Smallest, highest-priority, self-contained. Validated by the three
   `*_equivalence.rs` harnesses. *Fires after REVIVE.*

2. **Embed surface (§ 2, § 4).** Add `lower_fsm_application` (load-batch
   `F(init)` → `RunFsm`); add the `inject_claim_arg_types` fsm-skip
   guard; keep `run(...)` as a deprecated alias with a one-shot note.
   Validated by `runtime/tests/run_fsm.rs` extended with `F(init)`
   spellings asserting identical results to `run(F, init)`.

3. **`state_next` ban (§ 5).** Add `forbid_state_next_source`, called on
   the parsed body before `unify_state_syntax`, scoped to fsm-keyword +
   embedded targets, `external` exempt. This must come **after** the
   terse rewrite covers *all* fsm classes (enum/record + embedded-`Int`),
   or it would reject FSMs with no terse path.

4. **Corpus migration (§ 7).** The single sweep: `run(F,i)` → `F(i)`
   (Cat A), scheduler enum transitions (Cat B), embedded transitions
   (Cat C), static harnesses (Cat D, the careful part), `external` left
   alone (Cat E). Drop the `run(...)` parser alias once nothing uses the
   spelling.

5. **Rewrite CLAUDE.md to the one consistent story.** Kill the
   "state pair (`state` + `state_next`)" guidance and make terse the only
   documented FSM form:
   - The "Multi-FSM shared state: `_world` / `world` syntax" block
     generalizes to "`_state` / `state` for *any* FSM state var" — the
     `_world` example becomes one case.
   - The `examples/` "Demo files are integration tests" rule (`state pair
     + last_results + effects`) is restated as `state ∈ T` (terse) +
     `halt`/`last_results`/`effects`.
   - Add the `result = F(init)` embed form to the composition decision
     guide (the row next to "Inline a claim only when a condition holds")
     and to the `fsm` keyword section: *claim → inline (conjunction);
     fsm → run-to-completion (application)*.
   - Add `state_next` (capitalized-`True`-style) to the footgun list: a
     source `state_next` is now a load error, not a silent legacy form.

   Do this *last*, when the surface is real, so the docs describe shipped
   behavior.

---

## Appendix — source anchors (where each change lands)

| Change | File:line | Section |
|---|---|---|
| Terse→pair rewrite (generalize) | `runtime/src/runtime/desugar.rs:150` (`unify_world_syntax` → `unify_state_syntax`) | § 3, § 6 |
| Rewrite call site (unchanged position) | `runtime/src/runtime/load.rs:71` | § 3 |
| `_state` read-decl interaction | `runtime/src/runtime/inject.rs:140` (`inject_prev_tick_decls`) | § 3, § 6 |
| Embed lowering `F(init)`→`RunFsm` | new `lower_fsm_application` at `runtime/src/runtime/load.rs:132-136` (batch, after monomorphize) | § 4 |
| Lowered node (reuse) | `Expr::RunFsm` (`core/ast.rs`); parsed by `runtime/src/parser/atoms.rs:26-41` | § 2, § 4 |
| inject-pass fsm guard | `runtime/src/runtime/inject.rs:337` (`inject_claim_arg_types::resolve`) | § 4 |
| `state_next` ban | new `forbid_state_next_source` in `runtime/src/runtime/validate.rs`, called before `load.rs:71` | § 5 |
| Pair detectors (unchanged) | `effect_loop/fsm.rs::resolve_fsm`, `fsm_unroll/compose.rs::detect_state_pairs`, `effect_loop/nested.rs::detect_state_pairs` | § 3 |
| `run`/embedded validation (unchanged) | `runtime/src/runtime/nested.rs:103` (`validate_run_targets`) | § 4 |
| `external` exemption | `desugar.rs:153`, `inject.rs:28`, `fsm.rs:88` (existing pattern) | § 5, § 7 |
| The 3 terse-migration passes | `stdlib/passes/{subscriptions,validate,pretty}.ev` | § 6 |
| Embedded transition demos | `examples/test_35_run_fsm.ev`, `test_36`, `test_37` | § 7 |
