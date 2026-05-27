# Runtime split — cutting the current runtime along the SMT-LIB seam

> **Status:** design doc (2026-05). Docs-only. Decides whether — and how — to
> split the EXISTING Rust runtime into (1) an Evident→SMT-LIB+metadata
> **transpiler** (front-end) and (2) an isolated **Z3-FSM engine** (SMT-LIB →
> per-tick solve → effects), and compares that split against the `new-runtime`
> greenfield rewrite. Companion to
> [`smtlib-as-compile-target.md`](smtlib-as-compile-target.md) (the north star)
> and [`smt-lib-as-ir.md`](smt-lib-as-ir.md) (the IR mapping).

## The question

The north star ([`smtlib-as-compile-target.md`](smtlib-as-compile-target.md)) says
Evident should compile to **SMT-LIB text**, Z3 should run it, and the
functionizers should optimize the resulting AST regardless of source. That
implies a clean two-part architecture:

```
                          ┌─ the SMT-LIB seam ─┐
Evident source → AST → [ FRONT-END ] → SMT-LIB text + metadata → [ ENGINE ] → effects
                       transpiler                                  per-tick solve loop
                       (no Z3, no IO)                              (Z3 + state + dispatch)
```

Three sessions probe this from three angles:
- **`behavior-contract`** — capture the current engine's semantics as
  implementation-agnostic fixtures (`runtime-contract/`), the oracle any engine
  must pass.
- **`new-runtime`** — build a *greenfield* Rust engine whose input is SMT-LIB +
  metadata (`runtime-smt/`), unconstrained by the legacy.
- **`split-plan`** (this doc) — design how to cut the *existing* runtime along
  the same seam, and decide split-vs-greenfield on evidence.

This doc is the evidence. Phase 1 surveys the current source against the seam;
Phase 2 pins the interface; Phase 3 lays out an additive migration; Phase 4
delivers the decisive recommendation.

---

## Phase 1 — Source survey

Six parallel subagents classified every module in the seam-relevant clusters as
**front-end** (Evident→AST→SMT-LIB), **engine** (SMT-LIB→solve→effects),
**entangled** (resists a clean cut), or the refinements **metadata-producer** /
**straddle** / **shared-types**. Per-cluster detail lives in
[`split-survey/`](split-survey/); this section collates.

### Collated classification (by cluster)

| Cluster | Files | LOC | Front-end | Engine | Entangled / Straddle | Survey |
|---|---|---|---|---|---|---|
| `core/` + `lexer.rs` + `parser/` | 18 | ~2,900 | 12 (parser, lexer, ast, api, seq_helpers) | 2 (z3_program, z3_types) | 1 entangled (functionizer) + 1 shared (value) + 2 infra | [core-lexer-parser](split-survey/core-lexer-parser.md) |
| `translate/` | 35 | 13,131 | 12 (~2,650) | 11 (~2,050) | **12 entangled (~8,430)** | [translate](split-survey/translate.md) |
| `effect_loop/` + `subscriptions.rs` | 9 | 1,923 | — | 4 | 2 metadata-producer + 3 entangled | [effect_loop-subscriptions](split-survey/effect_loop-subscriptions.md) |
| `functionize/` + `z3_eval.rs` | 8 | ~4,500 | 0 | **8 (all engine)** | 0 | [functionize-z3eval](split-survey/functionize-z3eval.md) |
| `runtime/` (EvidentRuntime) | 16 | 3,902 | 7 | 1 | 4 straddle + 4 support | [runtime](split-survey/runtime.md) |
| `ffi.rs`/`fti.rs`/`event_sources/`/`chc.rs`/`effect_dispatch.rs` | 13 | 3,021 | 0 | 11 | 2 entangled (minor) | [ffi-fti-eventsources-chc](split-survey/ffi-fti-eventsources-chc.md) |

(The six clusters cover the seam-bearing core. Not separately surveyed:
`commands/` + `main.rs` (CLI dispatch, front-end-facing), `portable/` (the
self-hosted Evident passes — front-end), `fsm_unroll/` (nested-FSM symbolic
unroll — engine, same `Z3Program` weld as functionize), `pretty.rs`
(diagnostics), `decompose.rs`/`z3_profile.rs` (engine, noted by the
functionize survey). None change the picture below.)

### Where the seam actually falls

The six surveys converge on a sharp, consistent picture. The runtime divides
into **three** regions, not two — and the third is the whole story.

#### 1. The front-end is already clean and ready to cross as text

Everything from source to AST, plus every AST→AST pass, has **zero live Z3
coupling** and could move into a transpiler crate mechanically:

- **Parse**: `lexer.rs` + all 9 `parser/` files — pure `String → Program`, no
  Z3 surface ([core-lexer-parser](split-survey/core-lexer-parser.md)).
- **Pure-data core types**: `core/ast.rs`, `core/value.rs`, `core/api.rs`,
  `core/seq_helpers.rs` — `#[derive(Clone)]`, serializable, no Z3 handles.
- **AST→AST passes** (`runtime/`): `desugar.rs`, `inject.rs`, `validate.rs`,
  `introspect.rs`, `load.rs`'s loading pipeline — all run before any Z3 call
  ([runtime](split-survey/runtime.md)).
- **Metadata-producers** (`effect_loop/`): `fsm.rs::resolve_fsm` (slot
  resolution → `MainShape`) and `subscriptions.rs` (world read/write
  `AccessSets`) are pure AST static analysis whose output is exactly the
  metadata the seam must carry ([effect_loop-subscriptions](split-survey/effect_loop-subscriptions.md)).

**The AST→SMT-LIB-text boundary is expressible with front-end types alone** —
proven by `translate/smtlib.rs` and the `evident dump-smtlib` command, which
take `&SchemaDecl` (from `ast.rs`) and emit text using no `Var`/`Z3Program`/
`Context`. None of the engine-side core types are needed to *author* SMT-LIB.

#### 2. The engine below the translate core is clean and source-agnostic

The runtime's "what makes it not-just-Z3" machinery does **not** care where the
Z3 AST came from, and would survive the split largely untouched:

- **The functionizer is fully source-agnostic** ([functionize-z3eval](split-survey/functionize-z3eval.md)).
  Cranelift/symbolic/glsl/llm/satisfier all consume `&Z3Program` and registries;
  none import `translate/` or the AST. The north-star claim "functionizers
  consume Z3 AST and don't care about its source" is **verified true**.
- **`z3_eval.rs` is welded to the Z3 handle *type*, not the translate
  pipeline.** It walks `Bool<'ctx>`/`Dynamic<'ctx>` via structural inspection
  (`kind()`, `children()`, `safe_decl()`) — operations identical whether the
  handles came from the C-API translate path or from **parsing SMT-LIB text into
  the same `Context`**. Re-ingest the front-end's text via `Context::from_string`
  and the entire `simplify_assertions → extract_program_partial → compile` chain
  runs unchanged.
- **The eval/solve cluster** (`translate/eval/*`, `extract.rs`) — tactics,
  cached push/pop, model decode, UNSAT cores, decomposition — is irreducibly
  engine-side and well-bounded (~1,450 LOC) ([translate](split-survey/translate.md)).
- **The IO/FFI kernel is the clean "stays Rust forever" arm**
  ([ffi-fti-eventsources-chc](split-survey/ffi-fti-eventsources-chc.md)).
  `effect_dispatch.rs` takes a *decoded* `&Effect` (not a Z3 handle); event
  sources talk to the scheduler via `Vec<(String, Value)>` writes and
  `SchedulerEvent::Tick` wakes — entirely value-level. `chc.rs` is standalone
  (test-only, not on a live path).
- **The scheduler/state/halt machinery** (`effect_loop/scheduler.rs`,
  `state.rs`, `timing.rs`) is engine-side.

**The cross-seam wire format must be SMT-LIB *text*, not `Z3Program`.**
`Z3Program` embeds `Dynamic<'ctx>` in every computation-carrying variant
(`z3_program.rs:10-16`) — no serde, no round-trip, bound to a live `Context`. It
is an internal optimizer IR, decisively not a wire format. Text is the only
candidate, and it is sufficient: the engine re-parses it to fresh handles and the
optimizer pipeline picks up from there.

#### 3. The translate core is the wall — and two entanglements resist a clean cut

The ~8,430 LOC of **entangled** translate code is where the split is hard, and
two specific couplings are the load-bearing blockers.

**(a) The whole constraint-translation core builds live handles, not text.**
The `exprs/` cluster (~1,660 LOC) returns `Bool<'ctx>`/`Int<'ctx>`/
`Datatype<'ctx>`; `declare.rs`/`datatypes.rs` (~413 LOC) build live `Sort`/
`DatatypeSort`/consts stored in `Var<'ctx>`; `inline/*` (~1,160 LOC) materializes
consts and asserts mid-walk. `translate/smtlib.rs` emits text but only for a
**scalar subset** — it is a proof of concept, not a baseline. Making the
front-end emit SMT-LIB text for the **full language** (records→datatypes,
enums→`declare-datatype`, Seq→Array, quantifier unrolling, match→ite, claim
inlining, record lifting, string ops via raw `z3_sys`) is a **~3,200–3,500 LOC
deep rewrite of ~15 files**, not a moderate port ([translate](split-survey/translate.md)).

**(b) Translation queries the solver mid-flight.** `inline/guards.rs:17-35`
(`guard_is_satisfiable`) runs `solver.push()/assert()/check()/pop()` *inside* the
inlining loop — invoked from `inline/walk.rs:176`, `inline/calls.rs:34,122,261`,
`inline/subschema.rs:136` — to prune unsatisfiable guarded branches. A text
emitter has no solver to consult. This breaks the clean "emit text, *then*
solve" pipeline: it must either drop the optimization (always emit; correctness
preserved) or be refactored into a separate pre-solve annotation pass.

**(c) The engine re-enters translate every tick.** The scheduler
(`scheduler.rs:277`) and nested-FSM loop (`nested.rs:288`) call
`query_with_pins_and_given` per tick, which resolves through JIT / cached-solver
/ full-rebuild paths — **none accept SMT-LIB text**; all are welded to the
in-memory `SchemaDecl` + translate path ([effect_loop-subscriptions](split-survey/effect_loop-subscriptions.md)).
The cached-solver slow path (`scheduler_api.rs:54–83`) is the *closest* existing
analogue to "hand the engine a transition system once, re-assert each tick" — but
it holds a live `Solver`, not text. A split engine needs a pre-built per-FSM
**transition solver** it re-asserts prev-state + inputs into.

### The fragility a split would inherit

The `runtime/` survey found **six interlocking `'static`/thread_local invariants**
that make the current orchestration layer "fine for a CLI, one runtime per
process" but fragile for any library / multi-instance / per-test-isolation use —
exactly what the greenfield plan calls out as the legacy's flakiness:

1. `mod.rs:95` — primary `Box::leak`'d `&'static Context`, one per
   `EvidentRuntime`, never freed; **all** Z3 state hangs off it.
2. `register_enums.rs:177` — each `DatatypeSort` is `Box::leak`'d; re-declaring
   an enum name in the same context is a hard fatal error (`load.rs:107`).
3. `query.rs:485–487` — `unsafe transmute` of `Bool<'_>` → `Bool<'static>`, sound
   *only* because the context is leaked.
4. `query.rs:319–331,858–860` — leaked per-component parallel worker contexts,
   accumulating for process lifetime, behind a global setup mutex.
5. `query.rs:58–62` — `unsafe impl Send/Sync for SlowPart` over a `RefCell` + raw
   `&'static Context`, justified by a 1:1 part↔thread pairing.
6. `nested.rs:13` — `thread_local! { PERCOLATED_EFFECTS }`: per-thread, not
   per-instance; two runtimes on one thread silently share the channel.

A split that lifts `runtime/` as-is inherits all six. Eliminating them is not a
refactor of this layer — it is a **redesign around a scoped (non-`'static`)
context**, which is the same work the greenfield does from line one. This is the
single most important input to the Phase 4 decision, so it is flagged here.

### Phase 1 verdict (carried into later phases)

- The **front-end** and the **sub-translate engine** (functionizer, eval, IO
  kernel, scheduler) are clean and would split well.
- The **translate core** is the wall: a ~3.2–3.5 KLOC rewrite to emit
  full-language text, **plus** an architectural decision on the
  guard-satisfiability mid-translation query.
- The cross-seam IR is **SMT-LIB text + a metadata sidecar** (`MainShape` slots +
  `AccessSets`); `Z3Program` cannot cross.
- A split inherits **six leaked-context/thread_local fragilities**; the
  greenfield avoids them by construction.

These four findings frame the interface (Phase 2), the migration (Phase 3), and
the recommendation (Phase 4).

---

## Phase 2 — The interface

The seam needs an exact contract: what the transpiler **emits**, what the engine
**consumes**, and how FSM structure (state / state_next / effects / given / halt)
crosses. The contract has three parts — the **SMT-LIB transition system**
(text), the **metadata sidecar** (FSM structure), and the **per-tick wire
protocol** (how prev-state + inputs are pinned and results read back). The
companion `behavior-contract` session is defining the same boundary as
`runtime-contract/FORMAT.md`; that file is **not yet landed**, so this section
designs the minimal format and flags where it must reconcile.

### What crosses the seam (and what cannot)

| Crosses | Form | Why |
|---|---|---|
| The constraint program | **SMT-LIB text** | Serializable, solver-agnostic, re-parseable to fresh Z3 handles (verified: `z3_eval`/functionizer run unchanged on re-parsed AST). |
| FSM structure | **metadata sidecar** (JSON) | `MainShape` + `AccessSets` are pure-data structs, no Z3 (`fsm.rs:9-27`, `subscriptions.rs:9-15`). |
| Per-tick state + inputs | **`(assert (= var val))` lines** layered on the transition relation | This is exactly today's `--given` mechanism (`smtlib.rs` emit pass 3). |
| Model results | **decoded `Value`s** | `effect_dispatch` already takes decoded `&Effect`, not handles. |
| ❌ `Z3Program` | — | Embeds `Dynamic<'ctx>`; non-serializable (`z3_program.rs:10-16`). Stays engine-internal. |
| ❌ live `Bool/Dynamic<'ctx>` handles | — | Lifetime-bound to a `Context`; cannot cross a text boundary. |

### Part A — The SMT-LIB transition system (transpiler output)

The transpiler emits, **once per FSM at compile time**, a transition relation
over two generations of variables: current-tick (`state`, `world.X`,
`last_results`, `_name`…) and next-tick (`state_next`, `world_next.X`,
`effects`, `halt`). Today's `dump-smtlib` artifact (the realized scalar/string
slice) shows the shape; the full-language version adds the sort declarations the
[translate survey](split-survey/translate.md) enumerates.

```smt2
; transition relation for fsm `countdown` — generated by the transpiler
; (1) sort declarations — enums/records the FSM touches
(declare-datatype S ((Counting (n Int)) (Done)))
; (2) the two variable generations
(declare-const state      S)        ; current tick (pinned each tick)
(declare-const state_next  S)        ; next tick   (read from model)
(declare-const halt        Bool)     ; embedded-FSM halt flag (read from model)
; (3) the transition constraints (claim body, inlined + lowered)
(assert (= state_next (ite (> (n state) 0) (Counting (- (n state) 1)) Done)))
(assert (= halt (is-Done state)))
; NO (check-sat) — the engine drives solving, layering pins each tick
```

Differences from the standalone `dump-smtlib` artifact:
- **No `(check-sat)`/`(get-model)`.** Those belong to the one-shot dump; the
  engine asserts pins then calls check-sat itself, repeatedly.
- **Two variable generations are explicit.** `dump-smtlib` emits a single
  free-query namespace; the transition system must distinguish `state` from
  `state_next` (and `world.X` from `world_next.X`) so the engine can pin one and
  read the other.
- **Full-language sorts.** Records → single-constructor datatypes, enums →
  `(declare-datatype …)`, Seq → Array/seq-theory — the rows
  [`smt-lib-as-ir.md`](smt-lib-as-ir.md) marks aspirational and the survey prices
  at a ~3.2–3.5 KLOC rewrite.

### Part B — The metadata sidecar (FSM structure)

The text alone does not say which const is state vs effects vs world. That is
carried in a sidecar serialized directly from `MainShape` + `AccessSets`. **JSON
sidecar is the recommended encoding** (these are plain structs; `serde` is
trivial) over the alternatives (variable-naming convention — brittle; SMT-LIB
`(set-info)` annotations — non-standard tooling). The fields, one record per FSM:

```jsonc
{
  "fsm": "countdown",
  "state_var":        "state",        // pin each tick           (MainShape.state_var)
  "state_next_var":   "state_next",   // read from model         (MainShape.state_next_var)
  "state_type":       "S",            // seed tick-0; encode pins (MainShape.state_type)
  "effects_var":      "effects",      // Seq(Effect) to dispatch  (MainShape.effects_var)
  "last_results_var": "last_results", // inject prev dispatch     (MainShape.last_results_var)
  "world_var":        "world",        // reads prev world         (MainShape.world_var)
  "world_next_var":   "world_next",   // writes world (=> writer) (MainShape.world_next_var)
  "world_type":       "World",        //                          (MainShape.world_type)
  "halt_var":         "halt",         // embedded-FSM only; null for scheduler FSMs
  "reads":  ["pos", "score"],         // world fields read        (AccessSets.reads)
  "writes": ["pos"],                  // world fields written     (AccessSets.writes)
  "event_subscriptions": ["tick"],    // FrameTimer/Signal wakes  (MainShape.event_subscriptions)
  "fti_params": [["win","SDL_Window",…]], //                      (MainShape.fti_params)
  "effect_order": null                // Mode-2 precomputed toposort; null = Mode-1 (read effects_var)
}
```

Every field is derived from the AST by `resolve_fsm` (`fsm.rs:38-151`) and the
self-hosted `subscriptions` pass — **no Z3 dependency** — so the whole sidecar is
a front-end product. This is the realization of the plan's
"state/effects/given/halt" structure question: state↔`state_var`/`state_next_var`,
effects↔`effects_var`(+`effect_order`), given↔the pin protocol below, halt↔
`halt_var` (embedded) or the implicit rule (scheduler, see Part C).

### Part C — The per-tick wire protocol

The engine holds each FSM's transition solver (text re-parsed once into its own
`Context`) and runs the loop the [effect_loop survey](split-survey/effect_loop-subscriptions.md)
documents (13 steps), expressed against the seam:

1. **Wake-gate** — tick > 0: run the FSM only if it emitted effects last tick,
   has a pending world-read change (`reads ∩` writer's changed fields), changed
   state, or got an async event. Tick 0: all FSMs (bootstrap).
2. **Pin prev-state + inputs** — layer `(assert (= var val))` onto the
   transition solver (push a frame):
   - `state` ← prev `state_next` (threaded);
   - each `world.X` ← current world snapshot;
   - `last_results` ← prev tick's dispatch results;
   - `_name` time-shifts and `is_first_tick`.
3. **check-sat** — UNSAT ⇒ abort (logged).
4. **Read model** — `state_next_var` → next state; `effects_var` → `Seq(Effect)`
   (or, Mode-2, gather + order via `effect_order`); `world_next.X` → world writes;
   `halt_var` (if present) → halt flag.
5. **Dispatch effects** — `effect_dispatch` runs the decoded `Effect`s, returns
   `last_results` for the next tick. `Effect::Exit(code)` sets graceful shutdown.
6. **Thread forward** — `state_next` becomes next tick's `state`; writer's
   `world_next.X` updates the snapshot, waking readers whose `reads` intersect;
   pop the pin frame.
7. **Halt** — embedded FSMs: `halt_var` true. Scheduler FSMs: **implicit** — no
   FSM scheduled in a tick (and no pending world writes / live event source), or
   any `Effect::Exit`.

**Tick-0 seeding**: the engine constructs the first nullary variant of
`state_type` (`scheduler.rs:39-62`); a payload-only first variant is left
unconstrained for Z3 to pick. Spawned FSMs can seed a first variant with an Int
spawn-arg.

### Three interface subtleties (the honest caveats)

These are where "same SMT-LIB ⇒ same behavior" is **not** automatically true —
each is a real obligation the contract must state, surfaced by the surveys and
the equivalence tests.

1. **The answer depends on the tactic chain, not just the text.** The
   `smtlib_roundtrip` test pins a *documented divergence*: `x>0 ∧ x*x=2` is SAT
   via Z3's default `nlsat` (the SMT-LIB path) but the C-API path's tuned tactic
   chain returns `Unknown` → mapped to false. So the metadata (or an engine
   policy) **must also fix the tactic chain** (`translate/eval/solver.rs`'s
   configuration) — otherwise two conformant engines disagree. The contract is
   *SMT-LIB text + metadata + solver configuration*, not text alone.

2. **Pinning enum/record-valued state needs the datatype term, not a scalar.**
   `(assert (= state (Counting 5)))` requires the engine to know `S`'s
   constructors — which it does, from the sort declarations in Part A. But
   `encode_state_value` (`state.rs:37`) today only handles enum-typed state;
   **record-valued state is pinned as flattened scalar fields**
   (`state.field → scalar`), and `Value::Composite`/`SeqComposite`/`SeqEnum`
   states return `None`. The contract inherits exactly this support envelope —
   the metadata's `state_type` plus the Part-A sort decls are sufficient *for the
   shapes the current engine pins*; richer state shapes are a contract TODO, not
   a silent gap.

3. **Effect ordering, nested FSMs, and spawn need precomputed metadata.**
   - *Effect ordering*: Mode-1 (`effects ∈ Seq(Effect)` declared) reads the list
     in order — clean. Mode-2 gathers loose `Effect` bindings and **toposorts**
     them; the result is memoized by stable shape (`DISPATCH_ORDER_CACHE`), so it
     can be precomputed into `effect_order` at compile time
     ([effect_loop survey](split-survey/effect_loop-subscriptions.md)).
   - *Nested FSMs* (`run(F,init)`): each becomes its own transition system in the
     sidecar, addressed by name; the engine runs a sub-loop against it.
   - *`SpawnFsm`*: the one genuine per-tick schema lookup (`scheduler.rs:398`).
     Spawn targets are a closed set resolvable at compile time → serialize all
     reachable `MainShape`s into the sidecar (a **spawn registry**), so the engine
     never calls back into the front-end.

### Reconciliation with `runtime-contract/FORMAT.md`

`behavior-contract` Phase 2 will choose the canonical metadata encoding; this doc
recommends the **JSON sidecar above** (direct `MainShape`/`AccessSets`
serialization) and asserts the same three-part split (transition-system text +
sidecar + pin protocol). The one addition this survey forces onto `FORMAT.md`
that the behavior-contract plan does not yet name: **solver configuration is part
of the contract** (subtlety 1). When `FORMAT.md` lands, this section should be
diffed against it and the union taken; the fixtures it produces are the
conformance suite a split engine and the greenfield engine must both pass.
