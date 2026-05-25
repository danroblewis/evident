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

## 11. SDL render-call batching is not a real FPS win (profiled, rejected)

**Where:** `examples/test_21_mario/main.ev` (profiled May 2026)

A tempting optimization: a Mario frame emits ~70 effects, most of
them `SDL_RenderFillRect` LibCalls, so batch consecutive fill-rects
into `SDL_RenderFillRects` to cut FFI overhead. Profiling says don't
bother — FFI dispatch is not the bottleneck. Per-effect dispatch
time at `EVIDENT_TICK_MS=1`:

| Effect | µs/call | calls/frame |
|---|---|---|
| `SDL_Delay(16)` | **16,935** | 1 (deliberate frame cap) |
| `SDL_PumpEvents` | ~800 | 1 |
| `SDL_RenderPresent` | ~90 | 1 |
| `SDL_SetRenderDrawColor` | **0.5** | ~30 |
| `SDL_RenderFillRect` | **0.5** | ~24 |

All ~24 fill-rects total **~12 µs/frame**. Against a ~101 ms frame
(`EVIDENT_LOOP_TIMING`: 67 ms display *solve* + 17 ms dispatch,
of which the intentional 16.9 ms `SDL_Delay` is nearly all),
batching fill-rects saves ~12 µs — unmeasurable. The frame is
gated by the Z3 solve, not by FFI. libffi already amortizes
dlopen/dlsym (cached lib + sym handles), so each marshalled call
is sub-microsecond.

Takeaway: the lever for Mario FPS is the **display-FSM solve**
(JIT / functionizer work — see the constraint-model →
native-compilation plan), not effect-dispatch batching. The
multi-writer enforcement work (single-owner check now resolves
`..Passthrough` write-sets transitively — `effect_loop/fsm.rs:
full_world_access`) was the durable win from that session instead.

## 12. Mario's remaining un-JIT'd components are not Cranelift codegen gaps

**Where:** `examples/test_21_mario/main.ev` (investigated May 2026,
session I)

After per-component JIT (session B), Mario sits at **144/150
components compiled** (display 64/66, game 60/63, keyboard 19/20).
The natural read — "6 Z3Step shapes the Cranelift codegen refuses" —
is wrong. Tracing every component that reaches `compile_program`
(via a `[jit/compile] START` step-count probe) shows the **largest
program reaching codegen is 24 steps**; the 88-step `game` transition
never arrives. So only ONE gap component is a codegen question, and
it is not a *shape* gap:

**display `phase_chain` (the 24-step component).** `phase_chain` is
the flat `Seq(Effect)` assembling the frame's ~70 draw effects. Its
elements index record-Seqs: `(select (effs__arr (select plat_effs 0))
0)` = `plat_effs[0].effs[0]`. The nested-SELECT codegen *itself works*
(traceable through `emit_write_value`'s SELECT → DT_ACCESSOR → SELECT
when the base is in env). Two real blockers, both upstream of
codegen:

1. **A topo cycle from content-free crosslinks.** `extract` +
   `recompose_record_seqs` leave `phase_chain` ordered before its
   dependencies. The `∀ i ∈ {0..N}` HUD seqs (`hud_effs`,
   `coin_hud_effs`) have NO ground value in the simplified body — only
   a length pin and the equality `phase_chain[k] == hud_effs[i].effs[j]`.
   `try_recompose_one` matches that equality from the `hud_effs` side
   too, giving `hud_effs[i].effs[j] := (select phase_chain k)` — a
   back-edge → a 2-cycle that `topo_sort_steps` can't order, so it
   leaves `phase_chain` first and the SELECT codegen bails ("var not
   in env"). The HUD's ground (`world.lives > i ? red : grey` draws)
   is absent because the display body has **no `world.lives` /
   `coin_count` reference at all** — that read was dropped/optimized in
   translation, leaving the HUD effects genuinely unconstrained (the
   slow path picks an arbitrary valid Effect).

2. **Intermediate libcall defs live in *global* assertions.** Even
   ignoring the HUD, `plat_effs[i].effs[j] = draw_rect__color_eff__callN`
   where `draw_rect__color_eff__callN = SDL_SetRenderDrawColor(...)` is
   a separate assertion that touches no claim output, so
   `decompose_simplified` files it under `global_idx` — handed to the
   **slow part**, never to `compile_one_component`. The component's
   extracted program treats `draw_rect__*__callN` as a free input;
   at call time it isn't in `given`, defaults to `Value::Int(0)`, and
   the platform/coin/enemy/mario draws silently vanish. Verified: a
   patch that pre-registered output slots + best-effort-topo'd the
   steps DID make `phase_chain` "compile" (display → 65/66) and passed
   `./test.sh` (exit code + "mario done" unaffected), but an FFI-trace
   diff against `EVIDENT_FUNCTIONIZE=0` showed the world draws dropped
   (brown platforms 1205→5, gold coins 1205→5, purple enemies 723→3).
   A green test with a black world — exactly the visual-regression
   class `./test.sh --examples` exists to catch. Reverted.

**game / keyboard gaps** are world-dependent, guarded transition
components (the 88-step `game` FSM, the `is_first_tick` keyboard
branch) routed to the scoped slow solve by query.rs's unsafe-free
gate before ever reaching codegen.

Takeaway: closing these needs `translate/` (don't drop the HUD
`world.lives` read) and `runtime/runtime/query.rs` (carry a
component's intermediate-defining globals into its extracted program,
or refuse-to-bake instead of defaulting a missing input to `Int(0)`),
not `functionize/`. The codegen is not the limiting factor. See
[Records-as-vectors] note in CLAUDE.md and the
`project_mario_jit_phase_chain_intractable` agent memory.

## 13. Parallel slow-solve covers enums but not `Seq(UserRecord)`

**Where:** `runtime/src/runtime/query.rs` (`env_subset_translatable`,
`translate_var`); investigated + extended May 2026, session S.

Session E parallelized a claim's independent slow components onto
per-thread Z3 contexts, but only when every component variable was a
primitive scalar/seq/set; any enum- or record-typed var forced the
whole claim back to the sequential single-context path. Session S lifts
that for **enums** — both bare `EnumVar` and enum-element `Seq`
(`DatatypeSeqVar` with empty `fields`). The mechanism: Z3's
`Ast::translate` already recreates a datatype sort in the destination
context (verified — a translated `c == Red ∧ c == Green` is correctly
UNSAT in a fresh context, so the distinctness axioms travel), and
`replay_enums_into` re-runs `register_enums` against each worker context
so the registry's tester/accessor `FuncDecl`s used for extraction live
in the *same* context as the model. (`test_27_parallel_solving.ev`'s
`Half` enum exercises this end-to-end, ~3× wall-clock vs the sequential
baseline.)

**Still NOT parallelized: record-element `Seq` (`Seq(UserRecord)`,
e.g. Mario's `world.enemies : Seq(Enemy)`).** Its `Var::DatatypeSeqVar`
carries a `fields: Vec<FieldKind>`, and each `FieldKind::Nested` holds
its OWN `&'static DatatypeSort` bound to the main context. Translating
the var rebinds the *top-level* element sort to the worker context, but
the nested-field sorts inside `fields` stay main-context. Extraction
(`extract_seq_composite`) then applies a main-context accessor
`FuncDecl` to a worker-context value, which Z3 rejects:

```
thread '<unnamed>' panicked at z3-0.12.1/src/func_decl.rs:63:
assertion failed: args.iter().all(|s| s.get_ctx().z3_ctx == self.ctx.z3_ctx)
```

The panic is caught (`solve_slow_parts` joins a panicked worker as
`None` → graceful fall-through to the full Z3 solve), but it is wasted
work, so `env_subset_translatable` excludes record-element `Seq`
up front: a claim with any such component stays fully sequential.
This is why Mario's per-FSM solves (game/display/keyboard all read
`world.{enemies,coins,player,...}`) remain on the sequential path — no
regression, and correct, just not parallelized.

**To close it:** `translate_var` would need to recursively rebuild the
whole `Vec<FieldKind>` tree, rebinding every `FieldKind::Nested.dt` (and
its `sub_fields`) to the worker context's replayed user-record registry
(`get_or_build_datatype` against the worker ctx — the hook for which was
prototyped and reverted to keep this session's surface minimal). The
enum path needs none of that because enum elements have empty `fields`.
Bare record vars (`p : Point`, not in a `Seq`) are already fine: they
expand to primitive `IntVar`/`BoolVar` leaves at declaration, carrying
no datatype handle.

## 14. JIT codegen gaps — audited and partly closed (session T)

**Where:** `runtime/src/functionize/cranelift.rs`; full writeup in
[`docs/jit-codegen-gaps.md`](../docs/jit-codegen-gaps.md).

Some Z3 expression shapes make the Cranelift JIT return `None`, so the
component falls through to a (correct, slower) full Z3 solve. Session T
audited every bail. **Closed**: top-level integer `div`/`mod` (was only
handled as an operand, not as a Scalar's outermost decl); Seq-bodied
`Guarded` steps — the `effects = match state ⇒ ⟨…⟩` shape that was
refused wholesale, hitting 24/27 demos (now compiled, with a runtime
bail flag for the no-branch-matched case); `str.++` concatenation; and a
**silent miscompile** of `#seq` on an unpinned Seq (the `<seq>__len`
length symbol isn't in `given`, so the JIT read length 0 — now refused).

**Still falling back** (perf, not correctness; see the doc for fix
paths): String equality `(= s1 s2)` in a guard (test_12, test_14);
scalar-bodied `match` → scalar (e.g. `match last_results[1] {
StringResult(s) ⇒ s }`); `#seq` computed from the paired Seq value
(deferred upgrade of the refusal above); SDL packed-float vertex lists
and `LibCall`-as-scalar (test_17); test_29's tick-0 bootstrap component
(a `runtime/query.rs` unsafe-free decision, not a codegen shape).

## 15. Recursive claims don't constrain their outputs — ROUTED for tree-walks

**Where:** `runtime/src/translate/inline/recursion.rs`,
`translate/inline/walk.rs`; originally surfaced porting `pretty.rs` to
`stdlib/passes/pretty.ev` (session X). Full writeup:
[`docs/self-hosting.md`](../docs/self-hosting.md).

A claim cannot recursively process a recursive datatype of unknown depth
**via functional recursion**. Bounded inlining exists (depth-capped at
`EVIDENT_MAX_INLINE_DEPTH=64`) but the inlined frames' outputs are left
**unconstrained** — Z3 fills free values, so the result is garbage (both
correct and wrong outputs come back SAT). And a claim call nested inside
an expression (`out = pretty(l) ++ …`) is **silently dropped**
(`walk.rs`). No `define-fun-rec`, no fold/catamorphism. **This gap is
still open for the inline-a-recursive-claim shape** — other passes may
still cite it.

**`pretty` no longer demonstrates this** (as of the `pretty_walk`
rewrite), and neither does `subscriptions`. Both route around the gap the
same way #19 documents: the recursive tree-walk becomes a **stack-FSM** —
iteration over an explicit work-stack carried in FSM state, driven to halt
by `run()`. The recursion's output is threaded through state across ticks,
never left free for Z3 to fill, so a recursive AST→String (`pretty_walk`)
or AST→set (`subscriptions_walk`) pass renders/visits its full sub-tree.
So the old consequence — "a pass can render only leaf / flat AST shapes" —
**no longer holds**: anything with sub-`Expr`s self-hosts via the
stack-FSM. What still bounds a string pass's *byte-fidelity* is #16
(Unicode glyphs) and the no-int→string limit, not recursion.

Eventual full fix for the inline shape: compile recursive claims to Z3
recursive functions (or thread the inductive output constraint through the
unrolling) — see `docs/plans/03-language-prereqs/01-recursive-claims.md`
(acceptance criteria all unchecked). But the stack-FSM means that fix is
no longer on the critical path for the self-hosted tree-walk ports.

## 16. Non-ASCII string literals mangle through Z3

**Where:** `Z3Str::from_str` usage across `translate/eval/*` and the JIT;
surfaced in session X. `Z3Str::from_str` treats a Rust `&str`'s UTF-8
bytes as a byte-sequence of Z3 characters. A source literal `" ∈ "`
comes back as `\u{e2}\u{88}\u{88}` (JIT path) or `â\u{88}\u{88}` (slow
path) — neither recovers `∈`. So an Evident string pass can faithfully
emit **only ASCII**; every operator glyph (`∈ ∀ ⇒ ∧ ¬ ≤ ↦ ⟨⟩ …`) is
lost. (A `Value::Str(" ∈ ")` *given* round-trips only because the JIT
identity-short-circuits, not via real Z3 Unicode support.)

## 17. ~~JIT mishandles a `Bool` payload nested in an enum `given`~~ — FIXED (session GAPB)

**Was:** `match e { EBool(b) ⇒ (b ? "true" : "false") }` returned
`"false"` for both `true` and `false` under the JIT, but was correct on
the slow path (`EVIDENT_FUNCTIONIZE=0`).

**Fix:** same root cause as #18 below — `emit_compute_i64`'s
`DT_ACCESSOR` arm (`runtime/src/functionize/cranelift.rs`) read EVERY
destructured payload field via `ev_load_int`, which returns `0` for a
`Value::Bool`. So the destructured `b` read `0` and the `b ? … : …`
ITE always took the else branch. The loader is now chosen by the
accessor's RESULT sort (`load_bool` for a `Bool` field, `load_int`
otherwise), mirroring the existing `UNINTERPRETED` arm. The `SELECT`
arm (Seq element read) got the same treatment for `Seq(Bool)`. Repro:
`runtime/tests/enum_payload_computed_on.rs::destructured_bool_as_ite_condition`.

## 18. ~~String payload extracted from a given-pinned enum loses equality~~ — FIXED (session GAPB)

**Status:** closed. The destructure-then-compute construct now returns
the right answer through the `given` ⇄ match-extraction path, for both
String-equality and Bool/Int payloads.

**What the bug actually was (vs. the original DD report):** in the
current runtime the *String*-equality shape (`ECall(nm,_) ⇒ nm =
"FFICall"`) already evaluated correctly on both JIT and slow paths —
the byte-equality / nested-given fixes since DD (see #19c) closed that
half incidentally. The genuine remaining miscompile was a **JIT loader
bug** on a destructured payload *computed on*: `emit_compute_i64`'s
`DT_ACCESSOR` / `SELECT` arms (`runtime/src/functionize/cranelift.rs`)
read every extracted field via `ev_load_int`, which returns `0` for a
`Value::Bool`. So a destructured `Bool` used in a comparison / boolean
op (`Decide(rsn,_) ⇒ rsn ∧ …`) or as an ITE condition read **false**
even when true (the inject `(rsn ∧ ¬hsn)` symptom). The fix chooses the
loader by the accessor/element RESULT sort — `load_bool` for a `Bool`,
`load_int` otherwise — closing both this and #17.

**Repros (both JIT default + slow path):**
`runtime/tests/enum_payload_computed_on.rs` (String-eq, Bool-op,
Bool-as-ITE-cond, Int-compare) and
`runtime/tests/validate_unblock.rs` (the real `stdlib/ast.ev` `Expr`
shape, `ECall(nm,_) ⇒ nm ∈ {FFICall,…}` decision — the thing
`validate.ev` had to stub). The pass can now pin `e ∈ Expr` directly;
rewriting `validate.ev` to drop its `nm ∈ String` shim is left to a
later session.

---

Original DD report (kept for history):

The natural shape for an AST inspector — pin an `e ∈ Expr` via `given`,
destructure `ECall(nm, _)` in a `match`, compare `nm = "FFICall"` —
evaluates the comparison to `false` on both the JIT and slow paths,
even when the bytes of `nm` and the literal match exactly.

Reproduction:

```evident
import "stdlib/ast.ev"

claim ValidateExpr
    e ∈ Expr
    out ∈ String
    out = match e
        ECall(nm, _) ⇒ (nm = "LibCall" ? nm : "")
        _            ⇒ ""
```

Rust shim pins `e` to `Value::Enum { enum_name: "Expr", variant: "ECall",
fields: [Value::Str("LibCall"), Value::Enum { ELNil }] }` and queries.
Result: `out = ""`. The destructured `nm` round-trips correctly as a
String (return it bare and `out = "LibCall"` comes back fine), but
`nm = "LibCall"` doesn't fire.

The constructed-in-source form works correctly:
```evident
e = ECall("LibCall", ⟨⟩)
ValidateExpr (e ↦ e, out ↦ out)
-- ⇒ out = "LibCall"
```

So this is specifically a `given` ⇄ match-extraction failure, not a
general string-equality issue. Two strings of the same bytes coming
from different provenances aren't recognised as equal — likely a
character-sort / Z3 string-internalisation mismatch between the pinned
enum's payload and the comparator literal.

**Workaround used in `stdlib/passes/validate.ev`:** the Rust shim
extracts the call name on the Rust side and pins `nm ∈ String`
directly. The Evident pass still owns the decision (`nm ∈ {FFICall,
FFIOpen, FFILookup, LibCall}`); only the recognizer-vs-comparison
choice changes. Byte-equality on a top-level `Value::Str` given works
correctly — the gap is specifically about Strings extracted from a
pinned enum value.

Fix idea: trace where the destructured String diverges from the
literal in the JIT-emitted comparison. Plausible suspects: the
extractor function path (enum-accessor returning a Z3 String sort that
isn't byte-comparable with a constant-built String literal), or the
Cranelift-side handle convention (extracted-from-enum vs.
constructed-from-literal use different opaque handles that don't share
an identity-shortcircuit). Once closed, the pass can drop the
shim-side extraction and pin `e ∈ Expr` directly — the pattern that
matches the canonical Rust walker shape.

## 19. Stack-FSM tree-walk under tier-3 `run()` — composite-state gaps CLOSED (session NN)

**Where:** `examples/test_36_sum_tree.ev` (session MM) +
`examples/test_37_tree_walk.ev` (session NN); the stack-of-FSMs pattern
from `docs/design/loop-functionizer.md` §4, proven under the tier-3
`run(F, init)` that landed in session LL.

**Bottom line:** the pattern *works* — a recursive tree-walk whose
work-stack lives in FSM state, popped/dispatched/pushed per tick, with
the accumulator threaded across ticks, driven to halt by `run`. Session
MM proved the *logic* but had to work around four runtime facts.
**Session NN closed three of those four at the kernel/slow-path level**
(nested-constructor deep-matching #19b, enum-equality-vs-nested-literal
#19c, and composite `run` init/return #19d), so an FSM-with-stack can
now be **seeded with a composite value and return a composite
accumulator** over ANY recursive enum — see `test_37_tree_walk.ev`
(a variable-arity rose-tree label-walk: composite in, composite out,
nested deep-match in the agenda transition). The one remaining fact is
#19a (`Seq(T)` has no in-step pop/tail/cons), so the work-stack is still
carried on a **recursive-enum cons-list** spine, not a `Seq` — the
supported substrate for a dynamic collection in a constraint body.

### 19a. `Seq(T)` has no in-step pop / tail / cons (the anticipated gap)

This is exactly the weak point §4/§8 suspected. A constraint body cannot
read a `Seq`'s head **and** bind a new `Seq` equal to its tail, nor
prepend an element to a non-literal `Seq`.

Minimal repro (dropped constraint, "couldn't translate to Bool"):
```evident
claim pop(s ∈ Seq(Int), head ∈ Int, tail ∈ Seq(Int))
    head = s[0]
    tail = s[1..]          -- no slice syntax; Index is single-index only
```
```evident
claim push(s ∈ Seq(Int), s2 ∈ Seq(Int), x ∈ Int)
    s2 = ⟨x⟩ ++ s          -- ++ with a non-literal operand is left untranslated
```

**Where it breaks:** `runtime/src/runtime/desugar.rs::desugar_seq_concat`
flattens `++` only when *every* operand resolves to a static `SeqLit`
(load-time); an opaque `Seq` var is left as an untranslatable `Concat`.
`runtime/src/translate/exprs/seq_eq.rs` only handles `seq = ⟨literal⟩`,
ternary/match over literal arms, and whole-Seq equality between
*pinned-length* Seqs. `core/ast.rs`'s `Index(seq, i)` takes a single
index — there is no tail/slice node.

**Workaround (the demo):** don't use `Seq(T)` as the stack substrate.
Use a recursive enum cons-list — `enum Stack = Empty | Push(Tree, Stack)`
— where **pop** is `match stk | Push(top, rest) ⇒ …` (head + tail fall
out of the destructure) and **push** is `Push(l, Push(r, rest))` (a
constructor call). Both lower fine. The stack still lives in the FSM
state, marshaled whole through the per-tick solve — tier 3's realization
of §4's option A ("stack is state"), just on an enum spine, not a `Seq`.

**Fix idea:** add an in-step `Seq` tail node (`Index`-range, or a
`seq_tail`/`seq_cons` builtin) lowered in `translate/exprs/seq_eq.rs`
against an unpinned-length Seq. Until then the recursive-enum spine is
the supported way to carry a dynamic stack in a constraint body.

### 19b. ~~Nested constructor patterns aren't deep-matched~~ — FIXED (session NN)

**Was:** `Step(Empty, _)` parsed, but the match tester only checked the
**outer** constructor, so it matched any `Step(_, _)` regardless of the
inner `Empty` — silent wrong dispatch. Nested patterns with parens
(`Node(Leaf(n), r)`) didn't even parse (#2).

**Fix:** `MatchPattern` is now recursive
(`Ctor { name, binds: Vec<MatchPattern> } | Bind(String) | Wildcard`).
The parser (`parser/patterns.rs`) parses sub-patterns recursively, using
the capitalization rule to tell a nullary-constructor sub-pattern
(`Empty`) from a binding (`rest`). `translate_match_arms`
(`translate/exprs/match_expr.rs`) walks the pattern via `compile_pattern`
/ `compile_field`, **conjoining a recognizer tester per constructor
level** into the arm's guard and binding payloads (including nested enum
payloads) — so `Node(Leaf(n), r)` fires only when the outer is a `Node`
AND its first field is a `Leaf`. (Seq-typed payload fields still aren't
bindable from a pattern — that's the open part of #19a.) Tests:
`runtime/tests/composite_tree_walk.rs`,
`examples/test_37_tree_walk.ev`'s agenda transition.

### 19c. ~~Enum equality against a literal with a nested enum field is dropped~~ — FIXED (session NN)

**Was:** `final = Step(Empty, 6)` — comparing an enum var to a
constructor literal whose payload contains another enum value (notably a
*nullary* one like `Empty`) — was reported as dropped.

**Fix:** the enum-literal equality path (`resolve_enum_ast`'s `Call`
arm in `translate/exprs/enums.rs`) already recurses to build nested
enum-typed args, so the in-source form works directly. The remaining
break was specific to `run` *returning* such a value:
`value_to_literal_expr` (`runtime/nested.rs`) emitted a nullary variant
as a zero-arg `Call("Empty", [])`, which the translator can't resolve
(nullary variants are `EnumValue`, not `EnumCtor`) → silent drop. Now it
emits nullary variants as a bare `Identifier`, so
`final = D1(Push(5, Push(6, Empty)))` round-trips. Tests:
`composite_tree_walk.rs::{enum_eq_against_nested_literal,
enum_eq_with_nullary_nested_field,
composite_return_nested_enum_with_nullary_terminator}`.

### 19d. ~~`run`'s `init` can't be a composite~~ — FIXED (session NN)

**Was:** `run(F, Node(Leaf(1), Leaf(2)))` / `run(F, ⟨t⟩)` was rejected
before the solve — `eval_const_init` matched only
`Int/Real/Bool/Str/Identifier/Binary/RunFsm`, and `coerce_init` couldn't
wrap a composite into the state's first variant.

**Fix:** `eval_const_init` (`runtime/nested.rs`) now evaluates
`Expr::Call` (enum constructor → `Value::Enum`, recursively), bare
nullary-variant identifiers (`NLNil`), and `Expr::SeqLit`
(→ `Value::Seq*`) to concrete `Value`s using the enum registry.
`coerce_init` (`effect_loop/nested.rs`) seeds a composite either directly
(when its enum type IS the state type) or by wrapping it into the state
enum's first single-payload variant when the kinds match (generalizing
the bare-Int → first-Int-variant convention to any tree / enum / Seq).
**Composite return** is the dual: `value_to_literal_expr` now lowers
`Value::Enum` (incl. nullary → bare identifier) and `Value::Seq*`
(→ `SeqLit`) back to a literal the outer model pins. Tests:
`composite_tree_walk.rs::{composite_init_enum_literal_seeds_first_variant,
composite_init_through_outer_query}`; end-to-end in `test_37_tree_walk.ev`.

### What this means for the `walk_expr` self-host

The composite `run` init/return + nested-match capabilities the
`walk_expr` self-host needs **now land in tier 3**: an FSM-with-stack can
be seeded with a composite (`run(walk, root)`) and return a composite
accumulator over any recursive enum (`test_37` does exactly this with a
rose-tree → label cons-list). The remaining gap for the *literal*
`Seq(Node)`-children shape from `loop-functionizer.md` §4 is **#19a**
(no in-step `Seq` pop/tail/cons, and no `Seq`-payload binding in a
`match`): `test_37` carries its work-stack and accumulator on
recursive-enum cons-list spines instead of `Seq`s, which is the
supported substrate. Tier 2's native-`Vec` loop wrapper (§4 option B)
remains the path that would hold a real `Seq` stack natively; tier 3 now
proves both the pop/dispatch/push/fold/thread *logic* AND the
composite-state round-trip are correct. See
`docs/design/loop-functionizer.md` §4.

### #19c. The tier-3 recursive-enum walk's per-tick cost is `Value`-clone-bound (session YY)

The stack-FSM walk above (and the self-hosted `subscriptions_walk` it
underpins) **does** functionize — its `match`-dispatch-constructs-a-
recursive-enum-next-state step compiles to native code (`comp=2/2`,
0 runtime bails). What made it ~10 000× slower than the Rust oracle was
*not* a codegen refusal but the per-tick `Value` marshaling around the
native call. Session YY closed the worst of it (read accessors/recognizers
by reference instead of cloning the whole state; move built outputs;
drop the redundant per-tick Z3 pin), giving ~3× on Mario's walks and
fixing an ~8 GB memory blow-up + unbounded per-walk slowdown — see
`docs/jit-codegen-gaps.md` "Session YY". The **residual** floor (still
tens-to-hundreds of ms) is inherent to variant 1: each tick reconstructs
the cons-list state, cloning the stack tail + the growing accumulator
(`Value` has no structural sharing). Collapsing it to native-loop speed
needs structural sharing in `Value` or variant-2 whole-loop compilation
(`docs/design/loop-functionizer.md` §4 option B). This is the perf gate
on the held session-XX subscriptions cutover.

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

### 15. `Set` of composite/record types — v1 supported (was a gap)

`Set(UserType)` now declares to `Var::DatatypeSetVar`, a Z3 Set over the
type's DatatypeSort. Supported operations:

* `S = {a, b, c}` — literal set with composite elements (each element is
  a bare identifier resolving to a flat-expanded composite).
* `x ∈ S` — membership; LHS is an identifier resolving to a composite,
  routes to Z3 `set.member`.
* `∀ x ∈ A : x ∈ B` — subset pattern; emits Z3 native `set_subset`.
  Works for both pinned and free Sets.
* `#S` — cardinality; returns the literal-set element count when pinned
  via `S = {…}`. Free Sets have no cardinality (Z3 sets are
  characteristic functions over potentially infinite domains).

v1 limitations:

* **Model extraction is unsupported**: `check`/`all_solutions` will
  produce SAT but won't print a value for `Set(Composite)` bindings.
  Per-element field-accessor evaluation isn't wired yet; once a
  concrete consumer needs it we'll lift the candidates from the
  literal-set assignment through model-eval.
* **Forall body must be the subset pattern** (`var ∈ other_set`) for
  free Sets. More general bodies (`∀ x ∈ s : x.field > 0`) silently
  drop today; pin `s` via `S = {…}` if you need general forall, but
  the unrolling path for that isn't wired yet either.

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

### 24. ~~`Seq = Seq` (whole-sequence assignment) is dropped~~ — FIXED

**Was:** `world.plat_x = plat_x` (or `plat_x = _world.plat_x`)
dropped at translate time. Required element-wise workarounds like
`∀ i ∈ {0..2} : world.plat_x[i] = plat_x[i]`.

**Fix:** `translate_seq_eq` in `runtime/src/translate/exprs.rs`
recognizes `A = B` where both `A` and `B` are `SeqVar` or
`DatatypeSeqVar` with matching element kinds and known
lengths, and lowers it to an element-wise conjunction
`Array.select(i)._eq(Array.select(i))` over `i ∈ 0..n-1`.
Same routing for `≠`. Element types: Int / Bool / String for
`SeqVar`; whole-record `_eq` on the `Dynamic` for
`DatatypeSeqVar`. Length-mismatch / unknown-length / mixed-kind
cases return None so the dispatch falls through (and the
constraint visibly drops, as before).

### 25. Tree-of-sequences — Seq fields inside composites — supported (was a gap)

A composite type can have a `Seq(T)` field, and `Seq(Composite)` over
that type yields the tree-of-sequences shape. The runtime encodes each
Seq field as TWO accessors on the parent Datatype (an `Array(Int → T)`
and an `Int` length) — see `FieldKind::SeqField` in
`runtime/src/translate/types.rs`. The element type `T` can be primitive
(Int/Bool/String), an enum, or another composite.

```evident
type Group
    items ∈ Seq(Int)
    #items = 2

claim sat_nested_access
    groups ∈ Seq(Group)
    #groups = 3
    groups[0].items[0] = 10
    groups[2].items[1] = 60
```

What works:

* Composite with one or more `Seq(T)` fields, used at the instance level
  (`g ∈ Group`) — fields are addressable as `g.items[i]`, cardinality
  `#g.items` resolves via inherited body pins.
* `Seq(Composite-with-Seq-field)` — outer indexing into the Seq returns
  a composite Dynamic; `.items` reaches the inner Seq's Array+length pair
  via the type's SeqField accessors; inner indexing reads elements.
* `∀ x ∈ outer[i].items : …` unrolls over the inner Seq's pinned length.
* Sub-schema declaration: `g ∈ Group` declares both `g.items__arr` and
  `g.items__len` (the latter pinned via constraint inheritance — see #24).

What's still pending:

* **Top-level `Seq(Seq(T))`** — no native syntax. Workaround: wrap with
  a composite (`type EffectGroup(effs ∈ Seq(Effect)); xs ∈ Seq(EffectGroup)`).
  A future parser sugar could auto-generate the wrapper.
* **Set(Seq(T))** and **Set(Set(T))** — same blocker as the wrapping
  workaround above; doable once we decide on a syntax.
* **Element-level body-constraint inheritance for Seq(Composite)** —
  fixed. When `name ∈ Seq(SomeType)` is declared and SomeType has body
  Constraints, `inline_body_items_guarded` now emits per-element
  substituted versions over the Seq's pinned indices. So
  `type EffectPair(effs ∈ Seq(Effect)); #effs = 2` + `plat_effs ∈
  Seq(EffectPair); #plat_effs = 4` auto-pins each `plat_effs[i].effs`
  to length 2 — no explicit `∀ i : #plat_effs[i].effs = 2` needed.
* **Round-tripping Seq-valued composite fields through `given`** —
  `composite_value_to_dyn` returns None for SeqField; needed for
  multi-step executor frames carrying composites with Seq fields.

### 26. Subclaim invocations inside `∀` bodies — supported (was a gap)

`∀ (p, b) ∈ coindexed(platforms, plat_effs) : win.draw_rect(Rect(…),
b.effs)` now works end-to-end. `inline_body_items_guarded` recognizes
a `Forall` Constraint whose body contains a method-style subclaim
invocation and expands each iteration at AST level. The substituted
body for index `i` becomes a regular `BodyItem::Constraint(Expr::Call(
"win.draw_rect", […]))` that dispatches through `inline_subschema_call`
as usual — full solver access, internal `out = ⟨…⟩` assertions fire.

Supported range shapes:

* `coindexed(seq1, seq2, …)` — tuple binding, all seqs must have a
  pinned length. Bound vars substitute to `Index(Identifier(seq_k),
  Int(i))` per iteration.
* Bare `Identifier(seq_name)` — single-name binding, pinned length.

The substitution walks the body expression and rewrites:

* `Identifier("p")` → `Index(Identifier("platforms"), Int(i))`.
* `Identifier("p.color")` (the parser folds dots) → `Field(Index(…),
  "color")`.
* Deeper paths chain `Field`s.

`resolve_mapping` was extended to accept the `Field(Index(…), …)` chain
as a mapping value: it drills along the field path applying composite
accessors, then binds the leaf composite's leaves under the slot
prefix, or binds a SeqField as a `SeqVar`/`DatatypeSeqVar` for inner
Seqs. So `r ↦ Rect(p.color, p.aabb.pos, p.aabb.size)` after
substitution still resolves cleanly: each Rect field gets its leaves
bound by drilling through the indexed platform element.

One caller-side wrinkle remains: inner-Seq length pins from
`type Body(...)` invariants don't propagate to Seq elements (see #25
last bullet). The user has to pin `#plat_effs[i].effs = 2` explicitly
before the `∀` for the per-iteration subclaim length-pinning to be
load-bearing. Mario uses this form.

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
