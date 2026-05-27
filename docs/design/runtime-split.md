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

`behavior-contract`'s `FORMAT.md` **has since landed** and independently
converges on this design: JSON sidecar (explicitly chosen over naming-convention
hints and `(set-info)` annotations, for the same reasons — typed, versionable,
keeps `.smt2` purely relational); the same role fields
(`state_var`/`state_next_var`/`effects_var`/`last_results_var`/`world_fields`); a
tagged-`Value` JSON encoding mirroring `core/value.rs`; the transition relation
split into `problem.smt2 ⧺ prev.smt2 ⧺ inputs.smt2` with **the consumer appending
`check-sat`** (matching Part A's "no `(check-sat)`"); and the two-engine
consumption (`CurrentRuntimeEngine` + `SmtLibEngine`) that is the
`FsmEngine`-trait of Phase-3 Step 1. It also independently confirms two survey
findings: most fixtures are `handwritten` not `transpiled` because
`translate/smtlib.rs` emits only the scalar QF subset (the same ~3.5 KLOC
text-emit gap), and a **determinism rule** (pin every input that drives a checked
output, or assert only on uniquely-forced outputs) — its handling of subtlety 1.
`FORMAT.md` pins inputs to force a unique model and adds a per-output uniqueness
check (Method B), which sidesteps the tactic-chain divergence rather than fixing
the tactic chain; this doc's point stands that **solver configuration is part of
the contract** for any fixture not fully input-pinned. The two specs should be
treated as one; `FORMAT.md`'s fixtures are the conformance suite a split engine
and the greenfield engine must both pass.

---

## Phase 3 — Migration

A flag-day rewrite is off the table: the survey prices the full-language text
emitter at ~3.2–3.5 KLOC and the engine re-architecture as genuinely new code.
The migration must be **additive and `./test.sh`-green at every step** — grow a
second (text) path beside the live one, gated by an `EVIDENT_*` env var (the
existing `smtlib.rs::is_enabled` pattern; the codebase has **no Cargo features**,
all gating is runtime env), and flip the default only after the replacement is
proven on the full demo + conformance corpus.

### The dependency reality that shapes the order

Two structural facts (from the [crate-structure probe]) constrain the sequence:

- **One crate, no workspace** (`runtime/`, `evident-runtime`). "Extract a crate"
  means first proving a clean module cut; a literal crate split is optional and
  comes last.
- **The dependency DAG seam is *not* the semantic seam.** The
  below-orchestrator cluster — `lexer`, `parser/`, `core/`, `translate/`,
  `z3_eval`, `fsm_unroll/` — has **zero back-edges** into `runtime/`/`effect_loop/`,
  so it is dependency-extractable today. But `translate/` is *semantically
  engine-side* (it builds live handles), while the *semantically* front-end AST
  passes — `desugar`/`inject`/`validate`/`generics` — live in `runtime/` +
  `portable/` (the orchestrator cluster) **and re-enter the engine** through
  `portable/`'s self-hosting back-edge (`portable/` imports
  `runtime::EvidentRuntime` + `effect_loop::run_nested`; `runtime`/`effect_loop`
  call `portable`). Self-hosting welded the front-end passes to the whole engine.
  There is also a smaller back-edge: `translate/inline/walk.rs → pretty →
  portable::pretty`.

Consequence: "lift the front-end into a standalone transpiler" is **not free** —
the front-end passes need a running Evident engine to execute. This is the
north-star bootstrap circle ("the translator needs the translator"). The
migration resolves it the same way the north star does: the transpiler keeps the
**Rust pass implementations as the bootstrap seed** (parser already stays Rust
for exactly this reason; `subscriptions` already has a Rust backup), with the
self-hosted `.ev` passes an *optional accelerator*, not a load-bearing
dependency.

### The ordered, test-green sequence

| Step | What | Additive? | Breaks which coupling | `./test.sh` gate |
|---|---|---|---|---|
| **0** (mostly done) | Land `dump-smtlib` + the 19 snapshot fixtures + sat-parity roundtrip test on trunk. The gated scalar/string text-emit slice. | Yes (gated, off default path) | — (establishes the seam exists) | Snapshots + roundtrip parity pass; default path untouched. |
| **1** | Metadata sidecar + `FsmEngine` trait + `CurrentRuntimeEngine` adapter (shared with `behavior-contract`). `serde` on `MainShape`/`AccessSets`; adapter wraps today's `EvidentRuntime`. | Yes | — (bridges FE→engine without moving code) | Trait + adapter compile; behavior-contract fixtures pass against the adapter. |
| **2** | Clarify the `core/` boundary in place: `core::frontend` (ast, value, api, seq_helpers) vs `core::engine` (z3_program, z3_types, functionizer) namespaces; keep the flat `crate::core::*` re-export for back-compat. | Yes (re-exports only) | Documents the type-level seam (`core/mod.rs` flat re-export) | Builds; no behavior change. |
| **3** | Establish "the front-end runs without the engine": confirm/secure a Rust impl for each AST pass (`desugar`/`inject`/`validate`/`generics`) usable with `portable/`'s cache disabled; decouple `pretty` from `portable::pretty` for the transpiler. | Yes | The `portable ↔ runtime ↔ effect_loop` loop + the `pretty` back-edge | A test runs the AST passes with self-hosting disabled and matches the self-hosted output. |
| **4** | Grow the text emitter to the **full language** behind the gate: records→datatype, enums→`declare-datatype`, Seq, ∀/∃ unrolling, `match`→`ite`, claim inlining, record lifting, string ops. Implement guard-satisfiability as **always-emit** (drop the prune-by-sat). | Yes (gated) | **`inline/guards.rs` mid-translation `solver.check()`** (always-emit on the text path; default path keeps the optimization) | Snapshot + roundtrip parity grows feature-by-feature; default path untouched. **The ~3.2–3.5 KLOC of real work.** |
| **5** | The **re-ingest shim** (keystone): a path that takes (SMT-LIB text + sidecar), parses it into a **fresh, scoped `Context`** (not the leaked `&'static`), and runs the *unchanged* `simplify_assertions → extract_program_partial → functionize → eval` pipeline on the re-parsed handles. | Yes (gated, parallel engine) | **The leaked `&'static Context` + `unsafe transmute` + leaked worker contexts** (the fresh scoped context sheds fragilities #1/#3/#4 — *if* deliberately scoped) | behavior-contract fixtures pass against the text-engine (engine-over-text reproduces golden). |
| **6** | Per-tick re-assertion: the text-engine holds each FSM's transition solver (parsed once) and layers prev-state + input pins each tick (Part C), instead of re-translating. | Yes (gated) | **The per-tick `query_with_pins_and_given` weld** (`scheduler.rs:277`, `nested.rs:288`) — for the text path only | Text-engine passes the full demo corpus (`cargo test --test demos`) + conformance behind the flag. |
| **7** | **Flip the default** (gate now selects the *legacy* path as a one-release fallback). Then delete the ~8.4 KLOC C-API translate core (`exprs/`, `inline/`, `declare`/`datatypes`) + the leaked-context machinery. Functionizer, `z3_eval`, `eval/`, IO kernel survive unchanged. | **No** (the only non-additive step — comes last, after proof) | Retires the in-memory-AST path entirely | Text path passes demos + conformance + fixtures *alone*. |

### The shims that bridge the transition

1. **The metadata sidecar** (Step 1) — front-end → engine without moving any code.
2. **The `EVIDENT_*` gate** (existing pattern) — both paths live simultaneously;
   no flag-day. The default path is untouched through Step 6.
3. **Re-ingest-text-into-fresh-`Context`** (Step 5) — *the keystone*. The
   [functionize survey](split-survey/functionize-z3eval.md) verified `z3_eval` is
   welded to the handle *type*, not the translate pipeline: re-parse the text into
   a `Context` and the entire optimizer + solve stack runs **unmodified**. This is
   what makes the engine a *reuse* rather than a rewrite.
4. **Rust pass impls as bootstrap seed** (Step 3) — the transpiler runs its own
   AST passes without standing up the full self-hosting engine.

### Where the in-memory-AST coupling is broken (and the honest cost)

The three couplings the survey identified, mapped to migration steps:

- **Mid-translation solver query** (`inline/guards.rs`) → **Step 4**, by emitting
  all guarded branches on the text path (correctness preserved; possibly more
  assertions). Cheap and local.
- **Per-tick translate weld** (`scheduler.rs:277`/`nested.rs:288`) → **Step 6**,
  by holding a transition solver and re-asserting pins. Genuinely new
  orchestration code.
- **Leaked `&'static Context` + transmute + worker contexts** (`mod.rs:95`,
  `query.rs:485,858`) → **Step 5**, *only if* the re-ingest path is deliberately
  built on a scoped context. **This is the migration's pivotal choice**: reuse the
  leaked pattern and the split inherits all six fragilities; build a scoped context
  and Steps 5–6 become *the same scoped-context orchestration the greenfield
  writes from scratch* — at which point the split is reusing the front-end + the
  functionizer/eval stack, but **rebuilding the orchestration layer anyway**.

That last point is the bridge to the recommendation: Steps 0–4 produce assets
(the seam, the sidecar, the full-language text emitter) that are valuable
*regardless* of whether the engine is split or greenfielded — but Steps 5–7, the
part that actually "splits the engine," converge on the greenfield's work. Phase 4
weighs exactly this.

---

## Phase 4 — Comparison + recommendation

This is the decisive section. It weighs the **split** (cut the existing runtime
along the seam, Phase 3) against the **`new-runtime` greenfield** (`runtime-smt/`,
a from-scratch SMT-LIB-input engine), grounded in the surveys *and* in what the
sibling sessions have actually built.

### The convergent evidence: three sessions, one interface

The strongest single finding is that **three independent sessions arrived at the
same seam**:

- This doc's Phase 2: SMT-LIB transition relation (no `check-sat`) + JSON
  metadata sidecar serialized from `MainShape`/`AccessSets` + per-tick pin
  protocol + decoded-`Value` results.
- `behavior-contract/runtime-contract/FORMAT.md`: identical role-based JSON
  sidecar, `problem ⧺ prev ⧺ inputs` relation, consumer-appends-`check-sat`,
  tagged-`Value` encoding, two-engine (`FsmEngine`) consumption.
- `new-runtime/runtime-smt/src/spec.rs`: the same metadata as Rust types
  (`FsmSpec`, `StateVar`, `WorldVar`, `GivenVar`, `HaltSpec`, `EffectSpec`) — a
  working tick (`tick.rs`, N1) consuming exactly this contract.

The interface is therefore **validated and durable** — it is the asset that
survives regardless of which engine wins. That alone reframes the question: the
decision is not "split vs greenfield" as rival monoliths, but **which side of the
already-agreed interface each existing subsystem should live on, and which are
worth lifting versus rebuilding.**

### Scorecard

| Axis | Split (cut the legacy) | Greenfield (`runtime-smt`) |
|---|---|---|
| **Front-end transpiler** | Reuses parser + AST passes; must still grow the ~3.5 KLOC full-language emitter (Step 4). | *Needs the same transpiler* for Evident input (its N4 stretch). **Shared cost, not a differentiator.** |
| **Functionizer / JIT** (~4,500 LOC, `functionize/` + `z3_eval`) | **Kept for free** — source-agnostic, runs unchanged on re-parsed SMT-LIB (verified). | Absent (N4 stretch (a)); must be **ported**. Per-tick re-solve until then. |
| **IO / FFI kernel** (~3,000 LOC) | Kept in place; clean engine-side already. | Absent; but value-level and un-entangled, so **ports cleanly** (not a rewrite). |
| **Eval / solve stack** (`translate/eval/`, ~1,450 LOC) | Kept; engine-side, clean. | Re-implemented (small; `runtime-smt` already has the floor). |
| **Orchestration** (tick/state/scheduler/halt) | Must be **rebuilt around a scoped context** to shed fragility (Step 5–6) — i.e. the greenfield's work, done inside an entangled `query.rs`. | **Built clean from line one** against the interface; N1 working in ~2,355 LOC total. |
| **Context lifecycle / fragility** | Inherits all six leaked-`'static`/`transmute`/`thread_local` patterns unless Steps 5–6 redesign them. | **Avoided by construction**: raw `z3-sys`, one RAII `Context` per engine, freed on `Drop` — its Cargo.toml names this as the explicit fix for "the legacy runtime's flakiness." |
| **Test isolation / multi-instance** | Blocked by the leaked context until redesigned. | Clean per-instance from the start. |
| **Incrementality / risk** | **Each step `./test.sh`-green against the live demo corpus** — continuous proof. | Validated against fixtures (the oracle), not yet the full demo corpus — risk of late-surfacing unmodeled behavior. |
| **Translate-core entanglement** | Dragged along (mid-translation solving, in-memory handles) until Step 7 deletion. | None — never inherited. |

### The decisive synthesis

The split decomposes into two halves with **opposite verdicts**, and that is the
whole answer:

1. **The below-`z3_eval` engine is clean and worth *reusing*.** The functionizer,
   the eval/solve stack, and the IO/FFI kernel are source-agnostic and
   un-entangled (the surveys verified each). Re-implementing them from scratch
   would be a large, pointless waste — the functionizer alone is ~4,500 LOC of
   hard-won Cranelift codegen the greenfield has not begun.

2. **The orchestration layer is exactly where the legacy fragility lives, and
   "splitting" it means rebuilding it.** `query.rs`'s straddle, the leaked
   `&'static Context`, the per-tick translate weld, and the in-memory-handle
   translate core are not subsystems you lift out intact — they are the
   entanglement. Cutting them cleanly (Phase 3 Steps 5–6) *is* building a
   scoped-context tick loop against the SMT-LIB interface, which is precisely what
   `new-runtime` already did in 2.3 KLOC without the entanglement to fight.

So the part that *should* be lifted is the part that is **already clean (and thus
ports easily)**, and the part that *resists* the cut is the part you would
**rebuild anyway**. A literal cut of the legacy buys nothing the greenfield
doesn't get cleaner.

### Recommendation — HYBRID: greenfield the orchestration, reuse the front-end, *port* (don't rewrite) the clean subsystems

Decisively: **do not perform the literal split of the legacy orchestration.**
Instead:

1. **Treat the SMT-LIB + metadata interface as the keystone deliverable.** It is
   already validated by three sessions; build everything to it. This is the
   "split *interface* matters even if we don't cut the legacy" outcome the plan
   anticipated — confirmed.
2. **Greenfield the orchestration engine** (tick / state threading / scheduler /
   halt / world-coordination) on a scoped RAII `Context` — i.e. continue
   `new-runtime`. It avoids all six fragilities by construction and is the layer
   that is genuinely *cleaner from scratch*.
3. **Reuse the front-end transpiler** (parser + the Rust AST passes + the grown
   full-language SMT-LIB emitter, Phase-3 Steps 0–4). This is shared work that
   pays off for the greenfield's Evident input regardless — so the split's
   Steps 0–4 are worth doing, redirected as *contributions to the shared
   front-end*, not as a prelude to cutting the legacy.
4. **Port — not re-implement — the clean engine subsystems** into the greenfield
   behind the same interface: the functionizer + `z3_eval` (source-agnostic; they
   attach to the greenfield's re-parsed Z3 AST exactly as the split's Step-5
   re-ingest shim would) and the IO/FFI kernel (value-level; lifts cleanly). The
   greenfield's re-ingest point and the split's Step-5 shim are *the same code* —
   so the JIT/IO reuse the split would have gotten "for free" is available to the
   greenfield too, as a deliberate port.

The nuance that sharpens the plan's anticipated "greenfield the engine, reuse the
front-end" answer: it is specifically the **orchestration** that should be
greenfielded (not the whole engine), and the functionizer/IO subsystems should be
**ported, not rewritten** — because they are already source-agnostic. Squandering
that by re-implementing the JIT from scratch would forfeit the split's one real
advantage.

### What this means for each session

- **`split-plan` (this doc):** Phase-3 Steps 0–4 (interface + sidecar +
  full-language transpiler + the front-end/engine decoupling) are worth doing and
  feed the greenfield. **Steps 5–7 (cut the legacy orchestration) — do not**;
  redirect that effort into `new-runtime`. The deliverable of this session is the
  decision itself plus the reusable interface + migration assets.
- **`new-runtime`:** continue — it is the recommended engine. Next milestones,
  in priority order: (a) a spike proving `z3_eval` + Cranelift attach to
  `runtime-smt`'s re-parsed `Context` (de-risks the ~4,500 LOC JIT *port* — the
  recommendation hinges on this being a port, not a rewrite); (b) port the
  value-level IO/FFI kernel; (c) wire to the shared transpiler for Evident input;
  (d) the multi-FSM scheduler (N3).
- **`behavior-contract`:** the oracle both engines must pass; `FORMAT.md` is the
  shared interface. Deepen fixture coverage on the behaviors the surveys flagged
  as most entangled — **multi-FSM world threading, nested-FSM (`run`) re-entry,
  and `SpawnFsm`** — since those are where a greenfield is most likely to diverge
  from hard-won legacy semantics.

### Honest risks to the recommendation

1. **The JIT/IO reuse is a *port*, and ports can rot into rewrites.** The
   recommendation's economics depend on `z3_eval` + functionizer attaching to the
   greenfield's `Context` essentially unchanged. The functionize survey verified
   this is *structurally* true (they walk Z3 AST by `kind()`/`children()`,
   source-agnostic), but it is unproven against `runtime-smt`'s raw-`z3-sys`
   context specifically. **Spike it early** (new-runtime milestone (a)); if it
   fails, the split's "keep the JIT for free" advantage re-enters the calculus.
2. **The full-language transpiler (~3.5 KLOC + the guard-sat decision) is on the
   critical path for both** and is the single largest piece of work. It is
   unavoidable for Evident-input either way; start it early and share it.
3. **Behavior parity risk.** The greenfield validates against fixtures, not the
   live demo corpus, until late. The most entangled legacy behaviors (nested
   re-entry, world threading, spawn) must be in the fixture oracle *before* the
   greenfield claims parity — otherwise a behavior the legacy got right surfaces
   as a regression after the investment. This is the split's one durable
   edge (continuous `./test.sh` against real demos); the mitigation is deep
   `behavior-contract` coverage of exactly those behaviors.

### One-line answer

**Build the SMT-LIB + metadata interface (all three sessions already agree on
it); greenfield the orchestration engine on a scoped context (it is cleaner from
scratch and sheds the six leaked-context fragilities by construction); reuse the
front-end transpiler and *port* the source-agnostic functionizer + IO kernel
across the same interface — do not perform the literal cut of the legacy
orchestration, because the part worth lifting is already clean and the part that
resists the cut is the part the greenfield rebuilds better.**
