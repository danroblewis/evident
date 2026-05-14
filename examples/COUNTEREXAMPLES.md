# Counterexamples found while building the new demo set

This is the punch list of edge cases / footguns / runtime gaps
discovered while rebuilding `examples/` from scratch (one
demo per primitive, every program tested via inline `sat_*` /
`unsat_*` claims plus `evident effect-run` end-to-end).

The runtime works for **every demo we shipped**, but each item
below is a place where the user had to know something subtle to
make the program work — the runtime should ideally make these go
away or surface a clearer error.

## 1. First state-variant must be nullary

**Where:** `test_02_counter` (note in header)

If the FSM's state enum has a payload first variant
(`enum S = Count(Int) | Done`), the runtime can't seed tick 0 —
Z3 picks the simplest satisfying state (often `Done`), and the
program exits immediately.

Workaround: prepend a nullary `Start` variant.

Fix idea: let `state` be supplied as an init pin (like FTI
config pins).

## 2. Nested constructor patterns in `match` don't parse

**Where:** `test_04_parse_int` (note in body)

`ResCons(_, ResCons(r, _))` fails with `parse error: expected
RParen, got LParen`. The match parser doesn't recurse into
constructor patterns inside a constructor pattern.

Workaround: descend with an intermediate `match` that pulls
`tail`, then match on `tail`.

Fix idea: extend the pattern parser to recurse into nested
ctor args.

## 3. Enum variant names are global

**Where:** `test_09_two_fsms` (note in header)

Two enums in the same file can't both have a variant named
`Done`. (Documented in CLAUDE.md but very easy to trip on with
two short FSMs in one file.)

Workaround: prefix variants per enum (`PEnd`, `CEnd`).

Fix idea: scope variant names per-enum, or auto-suffix on
collision with a warning.

## 4. FTI pins parse only in claim BODY, not signature

**Where:** `test_13_timer`, `test_17_sdl_gl_window` (notes in
header / body)

`claim x(t ∈ Timer (interval_ms ↦ 50), …)` is a parse error
(`expected ',' or ')' after param group`). Moving the
declaration into the body works:

```evident
claim x(state, …, effects ∈ EffectList)
    t ∈ Timer (interval_ms ↦ 50)
    …
```

Fix idea: extend the param-list grammar to accept the pin
syntax inline.

## 5. FTI values don't propagate into `match state` transitions

**Where:** `test_11_frameclock`, `test_13_timer` (notes)

A state-transition that reads an FTI value:

```evident
state_next = match state
    Watching ⇒ (clock.tick_count ≥ 5 ? Done : Watching)
```

never picks `Done` — Z3 sees the threshold as un-met every tick,
even when the bridge has written `clock.tick_count = 5`.

Workaround: gate exit on `effects` directly:

```evident
state_next = Watching
effects = (clock.tick_count ≥ 3 ? ⟨Exit(0)⟩ : ⟨⟩)
```

Fix idea: trace why the per-FSM view's FTI-prefix-stripped
pins don't bind into the state-transition equation. Likely an
encoding-order issue where the state pin is built before the
FTI pins are merged.

## 6. Bool result from binding inside match arm doesn't propagate

**Where:** test_07_time investigation (workaround already in the
file)

```evident
got = match last_results
    ResCons(r, _) ⇒ match r
        IntResult(n) ⇒ n > 0      -- Z3 picks false even when n is large
        _            ⇒ false
```

The bound payload `n` is in scope for the arm but `n > 0`
yields false. Returning `n` as an Int and computing the
comparison outside the match works.

Fix idea: pattern-bound payload values may not be inserted
into the env that the arm's RHS expression sees.

## 7. SDL+GL renders black through Effect dispatch

**Status:** unfixed. The demo file was REMOVED from
`examples/` because its presence implied it worked. The
source is embedded at the bottom of this file under
`Appendix A: SDL+GL counterexample source` so contributors can
reproduce.

Per-frame `glClearColor` / `glClear` / `SwapWindow` calls
dispatched through Evident's effect loop don't visually
present, even though:

  - Same thread (ThreadId(1)) as bridge install
  - Same args, same function pointers
  - GL context current (`glGetString(GL_VERSION)` returns
    `"4.1 Metal - 89.3"`)
  - `glGetError` returns 0

The same calls work when issued INLINE inside the bridge
install, OR when the entire SDL+GL init is bundled into one
`Effect::Seq` as the (now-deleted) `effect_multi_fsm_triangle`
demo did.

**Things tried (none fixed it):**

  1. `glViewport(0, 0, w, h)` at install time — Apple's
     GL-on-Metal default viewport is 0×0; setting it didn't
     restore rendering (still needed though).
  2. `SDL_GL_SetAttribute` reordered to BEFORE
     `SDL_CreateWindow` (was being silently ignored in the
     wrong order — fixed independently).
  3. `glLinkProgram` status check (would have caught silent
     link failures — wasn't the cause).
  4. `SDL_ShowWindow` + `SDL_RaiseWindow` after
     CreateWindow — got the window onscreen, didn't fix the
     black render.
  5. Two priming swaps inside the bridge install (so the
     drawable is "exercised" before the first user tick) —
     no effect.
  6. Re-`SDL_GL_MakeCurrent` per frame from the user FSM —
     no effect.
  7. `glFlush` + `glFinish` before `SDL_GL_SwapWindow` from
     the user FSM — no effect.
  8. `NSApplicationLoad()` at bridge install (Cocoa
     bootstrap for command-line tools) — no effect.

**Working hypothesis:** a Cocoa runloop / NSOpenGLContext
drawable-liveness boundary between bridge return and the
first FSM tick. Likely needs either:

  * a Cocoa-aware runloop driver in the runtime
    (NSApp.run-style, with the FSM scheduler integrated as
    a runloop source), OR
  * deferred FTI install — bridge waits to do
    SDL_CreateWindow + GL context creation until INSIDE the
    first user tick's Effect dispatch, so the drawable's
    creation, first use, and first swap all happen on the
    same Cocoa runloop iteration.

The working multi-FSM GL demo (`effect_multi_fsm_triangle`,
deleted) put the entire SDL+GL init inside a single user
`Effect::Seq` on tick 0 and rendered fine. That's the only
known-working GL pattern in this runtime.

## 8. SpawnFsm + same-tick Exit drops the spawned FSM's first effect

**Where:** `test_10_spawn` (note in header)

If parent emits `⟨SpawnFsm("worker", N), Exit(0)⟩` in a single
tick, the worker is registered but `Exit(0)` halts the runtime
before the worker ticks → "worker spawned" never prints.

Workaround: parent transitions to a Wait state and exits a
few ticks later, giving the spawned FSM time to fire.

Fix idea: drain newly-spawned FSMs' tick-0 effects before
honoring `exit_requested`.

## 9. `Effect::Seq` doesn't share renderer/window handles across ticks

**Where:** `test_16_sdl_red` (note in body)

A renderer pointer created via `SDL_CreateRenderer` inside one
`Effect::Seq` (the setup tick) isn't accessible to subsequent
`Effect::Seq` invocations (the per-frame ticks) — there's no
cross-Seq state. The workaround is to call `SDL_CreateRenderer`
again at the head of each frame's Seq and reference its result
via `ArgPriorResult(0)`. Functionally OK (libffi caches lib +
sym handles) but wasteful.

Fix idea: an `SDL_Renderer` FTI bridge, analogous to
`GL_Program`, that owns the renderer pointer and exposes it as
a known `Int` field on the type. Then per-frame ops can be
plain stdlib calls on the known handle — no `Seq`, no
`PriorResult`.

## 10. Stdlib helpers can't take `ArgPriorResult` without explicit `*_after` variants

**Where:** `packages/sdl/render.ev` (the new `*_after` family)

A wrapper claim like `render_clear(renderer ∈ Int, out)` builds
its own `ArgList` with `ArgHandle(renderer)`. To get an
`ArgPriorResult(N)` slot in that list instead, the wrapper has
to be re-coded with `ArgPriorResult(prior_idx)` and the
`prior_idx` exposed as a parameter (`render_clear_after`). So
every stdlib FFI helper grows a parallel `_after` variant for
in-Seq use. Not great.

Fix idea: a generic mechanism for converting a wrapper's typed
`Int` arg into an `ArgPriorResult` inside a Seq (perhaps a
phantom value `prior_at(N)` that the call-site translator
recognizes), or move toward FTI bridges so most C resources
have known typed handles instead of needing in-Seq chaining.

## Conformance gaps surfaced by triage

These are bugs found while triaging the conformance suite (`tests/conformance/`)
against the Rust runtime. The original assertions captured the intended
language semantics; they were deleted from the suite (rather than rewritten
to match wrong behavior) and parked here.

### 11. `Nat` accepts negative values

**Where:** `tests/conformance/test_errors.py::test_nat_cannot_be_negative` (deleted)

```evident
schema S
    x ∈ Nat
    x = -1
```

The Rust runtime returns `{"satisfied": true, "bindings": {"x": -1}}`. `Nat`
is being treated as `Int` — the implicit non-negativity invariant on the sort
isn't being asserted.

Fix idea: when `instantiate` creates a Z3 constant for a `Nat`-typed
identifier, also assert `x >= 0`. Same goes for any other refinement-typed
sort (e.g. `Pos` if/when added).

### 12. `var ∈ SomeSchema` doesn't inherit the sub-schema's body constraints

**Where:** `tests/conformance/test_errors.py::test_sub_schema_inherits_unsat` (deleted)

```evident
schema Inner
    x ∈ Nat
    x < 0      -- unsat with the Nat invariant fixed; even with #11 unfixed
               -- this is unsat because we then expect x = -1 to fail too

schema Outer
    i ∈ Inner
```

Querying `Outer` returns SAT with `i.x = 0` — `Inner`'s `x < 0` constraint is
not enforced when `i ∈ Inner` is used as a field declaration in `Outer`. Only
`Inner`'s field shape (the dotted `i.x` slot) is brought into the parent env;
the body constraints are dropped.

Compare `..Inner` (passthrough) which DOES enforce `Inner`'s body constraints
in the including claim — `tests/conformance/test_errors.py::test_passthrough_unsat`
passes. So the asymmetry is: passthrough composes constraints, "variable of
schema type" composes only the field shape.

Fix idea: when `instantiate` expands a sub-schema field (`i ∈ Inner` becoming
`i.x`, `i.y`, …), also translate `Inner`'s body constraints under the dotted
prefix and assert them. This matches the documented contract in CLAUDE.md
("Using a type inside a claim: variable ∈ TypeName … the type's invariants are
automatically enforced").

### 13. `⟸` (reverse implication) is not lexed

**Where:** `tests/conformance/test_subclaim_and_reverse_implies.py` (deleted)

```evident
claim Foo
    x ∈ Nat
    y ∈ Nat
    x > 0 ⟸ y = 1   -- meant: y = 1 ⇒ x > 0
```

The Rust lexer rejects `⟸` outright: `parse error: lex error at line 4,
col 11: unexpected character '⟸'`. Same source against the Python reference
parses fine (the operator is in `parser/src/normalizer.py`).

`⟸` is documented in CLAUDE.md ("`⟸` (reverse implication): dispatch
tables") as the natural-reading form of `B ⇒ A` — `A ⟸ B`. With it
unlexed, every dispatch-table-style claim has to be written backwards.

Fix idea: add `⟸` to the lexer's symbol table in
`runtime/src/lexer.rs` and desugar `A ⟸ B` to `B ⇒ A` at parse time
(or add a dedicated `RevImplies` AST node and lower in `translate.rs`).

### 14. `subclaim` invocation as a body item is dropped

**Where:** `tests/conformance/test_subclaim_and_reverse_implies.py` (deleted)

```evident
claim Outer
    x ∈ Nat

    subclaim BothPositive
        x > 0

    BothPositive            -- bare invocation; should enforce x > 0
```

Querying `Outer` errors with `dropped constraint (couldn't translate to
Bool): BothPositive`. The Rust parser DOES lex `subclaim` and registers
the nested decl (`runtime/src/runtime.rs::register_subclaims`), but the
translator doesn't recognise the bare-name reference at the parent's body
level as a names-match invocation of the subclaim. Top-level claim
composition (`MustBePositive` referenced from a separate top-level claim
of the same name) DOES work — only the nested-subclaim path is broken.

`subclaim` is documented in CLAUDE.md ("`subclaim`: nested claim scoped
to a parent") as a first-class composition primitive — internal vars
hidden, parent vars inherited. Without invocation translation, the
keyword is effectively a no-op decoration: the body is parsed and
ignored.

Fix idea: in `translate.rs`, when an `Identifier` body item resolves
to a name registered via `register_subclaims`, inline the subclaim's
body under the parent env (Z3 `FreshConst` for body-only vars,
parent-scope lookup for inherited names) the same way top-level
names-match invocation already works. Once that's in, `⟸` (gap #13)
plus this gap together unlock the dispatch-table pattern from
CLAUDE.md.

### 15. `Set` of composite/record types is unsupported

**Where:** `tests/conformance/test_composite_elements.py::test_set_composite_simple`,
`test_set_composite_forall_field_access`, `test_set_composite_forall_unsat` (deleted)

```evident
type Item
    id   ∈ Nat
    kind ∈ Nat

claim sat_items
    items ∈ Set(Item)        -- "warning: unsupported Set element type Item for items"
    i     ∈ Item
    i.id   = 42
```

`runtime/src/translate/declare.rs` (~L184) only handles `Set(Int)`,
`Set(Bool)`, and `Set(String)` — anything else prints "warning: unsupported
Set element type" and the binding is silently dropped, so `∀ b ∈ items :
b.field` constraints over the set are unenforceable. Bare `Set Type`
(no parens) is also a parse error in the Rust grammar.

There's no clear path to fix this with the current Z3-Set encoding because
Z3 sets are over a single sort and composites are exploded into per-field
leaves at declare time. The supported workaround is `Seq(Item)` with a
pinned length — which the runtime translates correctly and supports forall
over (see #16).

Fix idea: encode `Set(Composite)` the way `Seq(Composite)` is already
encoded (parallel arrays per field, indexed by membership), or restrict
`Set` to scalar sorts and emit a real parse-time error for the composite
case so the failure is loud instead of a silent drop.

### 16. `∀ x ∈ Seq(Composite) : ...` requires a pinned length

**Where:** `tests/conformance/test_composite_elements.py::test_seq_composite_forall_field_access`
(rewritten to add `#tasks = N`)

```evident
type Task
    duration ∈ Nat
    priority ∈ Nat

claim sat_tasks_bounded
    tasks ∈ Seq(Task)
    ∀ t ∈ tasks : t.duration ≥ 0     -- "dropped constraint (couldn't translate to Bool)"
```

Without a `#tasks = N` length pin, the forall over a Seq-of-composite is
silently dropped by the translator. Adding `#tasks = 3` lets it through —
the constraint binds and SAT is returned with field-correct values per
element.

This is the same family of issue as the existing CLAUDE.md guidance for
`coindexed(...)` ("parallel-Seq lengths must be pinned in `type main`'s
body"). The user-facing error message ("dropped constraint") doesn't
hint at the length-pin workaround.

Fix idea: synthesise a finite-length unfolding when the seq length isn't
pinned (using a configurable bound similar to existing translator-gap
policy), or upgrade the error message to suggest pinning the length.

### 17. `SeqComposite` model values JSON-serialize via Debug as a string

**Where:** `tests/conformance/test_composite_elements.py::test_seq_composite_model_extraction`,
`test_seq_composite_model_values` (deleted)

```evident
type RGB
    r ∈ Nat
    g ∈ Nat
    b ∈ Nat

claim sat_colors
    c1   ∈ RGB
    c2   ∈ RGB
    c1.r = 255 ; c1.g = 0   ; c1.b = 0
    c2.r = 0   ; c2.g = 255 ; c2.b = 0
    colors ∈ Seq(RGB)
    colors = ⟨c1, c2⟩
```

`evident query --json` returns:

```json
{"colors": "SeqComposite([{\"r\": Int(255), ...}, ...])"}
```

i.e. the Rust `Debug` rendering wrapped as a JSON string — not a JSON list
of dicts. `runtime/src/commands/common.rs::value_as_json` falls through to
`json_str(&format!("{:?}", other))` for any `Value` variant it doesn't
explicitly handle; `Value::SeqComposite` and `Value::Composite` are both
in that fallback bucket.

The data is correctly extracted in-memory (see `extract.rs` ~L179 / L252),
just not formatted for JSON consumers. So a `colors[0].r` style assertion
out of `--json` output is impossible to write without parsing the inner
Debug string.

Fix idea: add `Value::SeqComposite(items)` and `Value::Composite(map)`
arms to `value_as_json` that emit a real JSON array / object — fields
recursively formatted via the same fn.

### 18. String substring membership (`text ∋ "!"`) doesn't translate

**Where:** `tests/conformance/test_claim_composition.py` (rewritten to
use string equality instead of substring containment)

```evident
claim ContainsBang
    text ∈ String
    text ∋ "!"        -- parses as `"!" ∈ text`
```

`evident check` reports:

```
error: dropped constraint (couldn't translate to Bool):
       "!" ∈ text
```

The `Expr::InExpr` arm in `runtime/src/translate/exprs.rs` only handles
two RHS shapes: a `SetVar` identifier and a literal `SetLit`. There is
no String/SeqStr arm that maps `lhs ∈ rhs` to `Z3Str::contains` (or
`prefix_of` / `suffix_of` for the analogous keywords).

This made every claim-composition test that relied on the original
`ContainsBang` example "pass" spuriously: parse/translate failure
yielded exit 1, the test helper interpreted that as `{satisfied: False}`,
and `assert_unsat` was happy. The SAT variants were XFAIL-listed; the
UNSAT variants passed for the wrong reason. The rewrite uses
`text = "hi"` instead — equally exercises the composition shape, no
translator gap.

Same gap for related ops (`#text > N` for length doesn't translate either).

Fix idea: extend `InExpr` translation with a `Z3Str::contains` arm when
both operands are String-typed; add explicit translations for `text
starts_with "..."`, `text ends_with "..."`, and `#text` (string length)
in the appropriate translator dispatchers.

### 19. `cond ⇒ ClaimName(slot mapsto value)` doesn't parse inside `⇒`

**Where:** `tests/conformance/test_claim_composition.py::test_mapped_renames_variable_sat`,
`test_mapped_vacuous_when_antecedent_false` (deleted; the unconditional
mapsto-call form is still tested)

```evident
type T
    greeting ∈ String
    greeting = "hi" ⇒ ContainsBang(text mapsto greeting)
```

`evident check` reports:

```
parse error: expected RParen, got MapsTo
```

The body-item parser in `runtime/src/parser.rs` recognises the
mapsto-call shape (`IDENT(slot mapsto value, …)`) explicitly via a
two-token lookahead before delegating to `parse_expr`. The expression
parser used inside an implies RHS does not have that shortcut — the
tokens `IDENT LPAREN IDENT MapsTo …` parse as a function-call
expression, which expects an expression after the first `Ident` and
fails on `MapsTo`.

The unconditional form `ClaimName(slot mapsto value)` works fine
because it hits the body-item parser directly. The
`(slot mapsto value)` trailing-pin form on a type declaration also
works (separate branch in the parser).

Fix idea: lift the mapsto-call lookahead into the expression parser
so the same shape parses anywhere an expression is expected; or have
implies emit a body-item RHS in the special case where the consequent
is a bare identifier followed by `(IDENT mapsto …`.

### 20. `--given verb=Add` doesn't pin enum-typed givens via the CLI

**Where:** `tests/conformance/test_claim_composition.py::test_dispatch_via_claim_consequent`
(rewritten to use Bool dispatch instead of enum dispatch)

```evident
enum Verb = Add | Remove

type BudgetStep
    verb ∈ Verb
    n    ∈ Nat
    verb = Add ⇒ ...
```

`evident query <prog> BudgetStep --given verb=Add n=0` prints:

```
warning: type mismatch for given "verb"
{"satisfied": true, "bindings": {"n": 0, "verb": "Remove"}}
```

`commands/common.rs::infer_value` parses the bareword `Add` as
`Value::Str("Add")`. `run_cached` in `translate/eval.rs` matches
`(Var::EnumVar, Value::Str(_))` against no arm, falls through to the
catch-all, prints the warning, and skips the assertion. Z3 then
chooses any verb value that satisfies the body — typically picking
the variant that makes the implies vacuous, which makes the test
quietly pass with the wrong dispatch branch.

Fix idea: in `infer_value`, return `Value::Enum { variant: v, … }` (or a
new `Value::EnumVariant(name)` placeholder) when the bareword is a
syntactically valid identifier that isn't a bool / int literal.
Resolve it in `run_cached` against the EnumRegistry: look up the
constructor by name on the var's enum sort and assert
`var._eq(&ctor.apply(&[]))`. Reject as `type mismatch` only if the
named variant doesn't exist on that sort.

### 21. `∃` is not accepted as an expression

**Where:** parser; surfaced while writing `examples/test_21_mario.ev`.

```evident
on_ground ∈ Bool = ∃ i ∈ {0..#platforms - 1} : cond_i    -- parse error
on_ground ∈ Bool = (∃ i ∈ …)                              -- parses, but
                                                          -- translator drops it
```

`parse_expr` handles `Token::Exists` at the top, but the `=` of a
chained-membership / equality constraint sits at compare-level — the
RHS is parsed via `parse_compare` ⇒ … ⇒ `parse_atom`, which has no
quantifier branch. Wrapping in parens lifts to `parse_expr` via
`LParen → parse_expr` and parses successfully, but the translator then
rejects it: `∃` is only supported as a top-level Bool constraint, not
as a value to bind to a Bool var.

Workaround pattern (used in Mario for `on_ground` / `any_landing`):

```evident
on_ground ∈ Bool
∀ i ∈ {0..#platforms - 1} : (cond_i ⇒ on_ground)
¬on_ground ⇒ (∀ i ∈ {0..#platforms - 1} : ¬cond_i)
```

Forward direction couples each `cond_i` to `on_ground`;
contrapositive direction realizes "no cond holds when on_ground is
false." Together this expresses `on_ground = (∃ i : cond_i)` as two
top-level ∀ constraints — verbose but each piece is in a slot the
translator accepts.

Fix idea: in the translator's expression dispatch, recognize
`Expr::Exists` in Bool-valued position and lower it to a disjunction
of unrolled instances (mirror of how `Forall` already lowers to a
conjunction). Or, less invasively, recognize `name = ∃ …` at body-item
shape and rewrite to the bidirectional ∀ form here so user code can
stay compact.

### 22. ~~∀-unroll over `Seq(UserType)` can't see element values defined via `..Passthrough`~~ — FIXED

**Was:** `examples/test_21_mario.ev` had to duplicate its
`platforms[i] = Body(...)` pins into both fsms because
`collect_seq_lengths` and `evaluate_with_extra_assertions`'s
Pass 1 didn't follow `Passthrough(name)`.

**Fix:** `collect_seq_lengths_with_schemas` recurses into
passthrough'd claim bodies for cardinality pins, and every
`evaluate*` entry point declares Memberships from passthroughs
in Pass 1 (mirroring `evaluate`'s existing behavior). Mario's
`Level` claim now consolidates the entity-Seq data and both
fsms `..Level` once.

### 23. Writing to a 3-level-nested field through `world_next` is dropped

**Where:** `examples/test_21_mario.ev`; surfaced by trying to write
`world.player.pos.x = …` (post-unify: `world_next.player.pos.x = …`).

```evident
-- DROPPED:
world.player.pos.x = (is_first_tick ? 304 : res_x)

-- works (1-level nested write to a top-level world field):
world.player = Mover(IVec2(new_px, new_py), IVec2(new_vx, new_vy))
```

The translator handles 2-level writes (`world.player = Mover(...)`)
through Datatype update / fresh-const + equality, but the
3-level form (`world_next.player.pos.x = X`) bottoms out in
"couldn't translate to Bool." Same shape inside a `∀` over a Seq
also fails when the LHS is `seq[i].field.subfield = X`.

Workaround: build the new value at the highest-level field site and
assign the whole record at once. For Mario this means computing
`new_px` / `new_py` / `new_vx` / `new_vy` as plain Ints, then a
single `world.player = Mover(IVec2(…), IVec2(…))`. Inside `∀
(cur, nxt) ∈ coindexed(...)` the same pattern works: write
`nxt = Mover(IVec2(…), IVec2(…))` per guarded implication.

Fix idea: extend the Datatype-write translator to compose nested
field updates (build the inner record from the existing one with
just the leaf field replaced; for Seq-of-record writes, build the
new element similarly and `set_at(i, …)`).

### 24. `Seq = Seq` (whole-sequence assignment) is dropped

**Where:** `examples/test_21_mario/main.ev`; surfaced wiring the
`level_gen` FSM. Trying to publish a local `plat_x ∈ Seq(Int)`
into `world.plat_x` as a single assignment fails:

```evident
world.plat_x = plat_x          -- DROPPED
plat_x = _world.plat_x         -- also DROPPED
```

Workaround: element-wise via `∀`, with the length pinned:

```evident
∀ i ∈ {0..2} : world.plat_x[i] = plat_x[i]
```

The translator handles `Seq[i] = expr` per element but bails on
the whole-Seq form. Same shape for any element type — Int / Bool /
String / record Seq.

Fix idea: in the assignment translator, when both sides are
Seq vars with the same pinned length, expand to an element-wise
constraint AND to a sequence-array equality on the Z3 side (so
unification of derived sub-fields propagates). Or simply desugar
`A = B` to `#A = #B ∧ ∀ i : A[i] = B[i]` upstream of translation.

## What works without caveat

Every demo ships in green:

| # | Demo | Primitive |
|---|---|---|
| 01 | hello | Println, Exit |
| 02 | counter | state-pair, payload-state via Start prefix |
| 03 | seq_chain | Effect::Seq |
| 04 | parse_int | ParseInt → Int / Error result |
| 05 | int_to_str | IntToStr → String result |
| 06 | shell_run | ShellRun → captured stdout |
| 07 | time | Time → IntResult |
| 08 | exit_code | non-zero exit propagation |
| 09 | two_fsms | shared World, writer-first scheduling |
| 10 | spawn | SpawnFsm with Int arg, spawnable_only marker |
| 11 | frameclock | FrameClock FTI |
| 12 | hostname | Hostname FTI (one-shot bridge) |
| 13 | timer | per-instance Timer with `interval_ms ↦ N` |
| 14 | stdin | StdinSource plugin-as-writer |
| 15 | signal | SigintSource plugin-as-writer |
| 16 | sdl_red | SDL_Renderer (renderer-based, not GL) |
| 17 | sdl_triangle | SDL_RenderGeometry triangle (everything in one Seq on tick 0) |

Plus inline `sat_*` / `unsat_*` static tests and the Rust
driver in `runtime/tests/demos.rs`.

---

## Appendix A: SDL+GL counterexample source (counterexample #7)

This file used to live at `examples/test_17_sdl_gl_window.ev`.
It was removed because its presence in the demos directory
implied it worked. The runtime can't currently render through
this pattern — see counterexample #7 above for the diagnostic
findings and what's been tried.

Reproduces the bug: window appears (titled "Counterexample")
but stays black. Save as a `.ev` file and run with
`evident effect-run`.

```evident
import "stdlib/runtime.ev"
import "packages/sdl/gl.ev"
import "packages/sdl/window.ev"
import "packages/gl/program.ev"

enum WState = WInit | WLoop(Int) | WEnd

claim gl_demo(state, state_next ∈ WState,
              last_results ∈ ResultList,
              effects ∈ EffectList)
    win ∈ SDL_Window (title ↦ "Counterexample", width ↦ 640, height ↦ 480)

    state_next = match state
        WInit    ⇒ WLoop(60)
        WLoop(n) ⇒ (n ≤ 1 ? WEnd : WLoop(n - 1))
        WEnd     ⇒ WEnd

    set_color_eff ∈ Effect
    gl_clear_color(0.9, 0.1, 0.1, 1.0, set_color_eff)
    clear_eff ∈ Effect
    gl_clear(16384, clear_eff)
    swap_eff ∈ Effect
    gl_swap_window(win.handle, swap_eff)
    pump_eff ∈ Effect
    sdl_pump_events(pump_eff)
    delay_eff ∈ Effect
    sdl_delay(33, delay_eff)

    frame_inner ∈ EffectList
    frame_inner = ⟨set_color_eff, clear_eff, swap_eff, pump_eff, delay_eff⟩
    frame_seq ∈ Effect
    frame_seq = Seq(frame_inner)

    effects = match state
        WInit    ⇒ ⟨⟩
        WLoop(n) ⇒ (n > 0 ? ⟨frame_seq⟩ : ⟨Println("done"), Exit(0)⟩)
        WEnd     ⇒ ⟨⟩
```
